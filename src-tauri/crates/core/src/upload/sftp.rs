//! The actual SSH/SFTP transport, isolated in its own file since it's the
//! one piece of this module that can't be exercised without a real server
//! -- the same boundary [`crate::embed`] draws around its exiftool
//! subprocess call and [`crate::analysis`] draws around its network calls.
//!
//! Built on `russh` + `russh-sftp` (pure Rust, no OpenSSL/libssh2/vcpkg
//! toolchain needed on Windows). Both crates are async; this wraps them in
//! a small single-threaded Tokio runtime so the rest of sphinx-core can
//! stay synchronous, the same way `reqwest::blocking` wraps hyper.

use std::path::Path;
use std::sync::{Arc, Mutex};

use russh::keys::key::PublicKey;
use russh_sftp::client::SftpSession;
use tokio::io::AsyncWriteExt;

use crate::error::{CoreError, Result};

use super::{classify_error, SftpProfile};

/// Accepts the server's host key on the first connection (`expected` is
/// `None`) and records its fingerprint in `seen`; on every later connection
/// requires an exact match, rejecting anything else. This is the only line
/// of defense against a spoofed or MITM'd server after the first connect.
struct TofuHandler {
    expected: Option<String>,
    seen: Arc<Mutex<Option<String>>>,
}

#[async_trait::async_trait]
impl russh::client::Handler for TofuHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        let fingerprint = server_public_key.fingerprint();
        *self.seen.lock().expect("fingerprint mutex poisoned") = Some(fingerprint.clone());
        Ok(match &self.expected {
            None => true,
            Some(expected) => *expected == fingerprint,
        })
    }
}

/// The host key fingerprint seen on a successful upload, so the caller can
/// pin it (first connection) or confirm it matched what was already saved.
pub struct UploadOutcome {
    pub host_key_fingerprint: String,
}

/// Connect to `profile`'s SFTP endpoint, authenticate with `password`, and
/// upload `local_path`'s contents to `<remote_dir>/<remote_filename>`.
pub fn upload_file(
    profile: &SftpProfile,
    password: &str,
    local_path: &Path,
    remote_filename: &str,
) -> Result<UploadOutcome> {
    let bytes = std::fs::read(local_path)?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| CoreError::Sftp(format!("could not start async runtime: {e}")))?;
    rt.block_on(run_upload(profile, password, &bytes, remote_filename))
}

async fn run_upload(
    profile: &SftpProfile,
    password: &str,
    bytes: &[u8],
    remote_filename: &str,
) -> Result<UploadOutcome> {
    let seen_fingerprint = Arc::new(Mutex::new(None));
    let handler = TofuHandler {
        expected: profile.host_key_fingerprint.clone(),
        seen: seen_fingerprint.clone(),
    };
    let config = Arc::new(russh::client::Config::default());

    let mut session =
        russh::client::connect(config, (profile.host.as_str(), profile.port), handler)
            .await
            .map_err(|e| err(profile, &e))?;

    let authenticated = session
        .authenticate_password(&profile.username, password)
        .await
        .map_err(|e| err(profile, &e))?;
    if !authenticated {
        return Err(CoreError::Sftp(classify_error(
            profile.site,
            "authentication failed (check the username and password)",
        )));
    }

    let channel = session.channel_open_session().await.map_err(|e| err(profile, &e))?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| err(profile, &e))?;
    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| CoreError::Sftp(classify_error(profile.site, &e.to_string())))?;

    let remote_path = format!("{}/{}", profile.remote_dir.trim_end_matches('/'), remote_filename);
    let mut file = sftp
        .create(remote_path)
        .await
        .map_err(|e| CoreError::Sftp(classify_error(profile.site, &e.to_string())))?;
    file.write_all(bytes)
        .await
        .map_err(|e| CoreError::Sftp(classify_error(profile.site, &e.to_string())))?;
    file.shutdown()
        .await
        .map_err(|e| CoreError::Sftp(classify_error(profile.site, &e.to_string())))?;

    let fingerprint = seen_fingerprint
        .lock()
        .expect("fingerprint mutex poisoned")
        .clone()
        .unwrap_or_default();
    Ok(UploadOutcome {
        host_key_fingerprint: fingerprint,
    })
}

fn err(profile: &SftpProfile, e: &russh::Error) -> CoreError {
    if matches!(e, russh::Error::UnknownKey) {
        CoreError::Sftp(format!(
            "host key for {}:{} does not match the one on file -- this could mean the server's key \
             was legitimately rotated, or that the connection is being intercepted. Verify out of \
             band before clearing the saved fingerprint and reconnecting.",
            profile.host, profile.port
        ))
    } else {
        CoreError::Sftp(classify_error(profile.site, &e.to_string()))
    }
}
