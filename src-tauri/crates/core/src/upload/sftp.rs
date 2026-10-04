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
use std::time::Duration;

use russh::keys::key::PublicKey;
use russh_sftp::client::SftpSession;
use tokio::io::AsyncWriteExt;

use crate::error::{CoreError, Result};

use super::{classify_error, SftpProfile};

/// Ceiling on the TCP connect + SSH handshake + auth. `russh::client::Config`
/// sets no timeout of its own, so without this a hung/firewalled host blocks
/// the caller (and, when called from the job worker, the whole queue behind
/// it) forever instead of surfacing an error.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// Ceiling on each individual SFTP subsystem/file operation once connected.
const IO_TIMEOUT: Duration = Duration::from_secs(30);

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

/// The host key fingerprint seen on a successful connection, so the caller
/// can pin it (first connection) or confirm it matched what was already
/// saved. Returned by both a real upload and a connection test.
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

/// Connect and authenticate only -- no file transfer -- so a saved profile's
/// credentials and reachability can be verified before (or without) a real
/// upload.
pub fn test_connection(profile: &SftpProfile, password: &str) -> Result<UploadOutcome> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| CoreError::Sftp(format!("could not start async runtime: {e}")))?;
    rt.block_on(run_test_connection(profile, password))
}

/// Connect, authenticate, and open the SFTP subsystem, timing out each step
/// so a hung/firewalled host can never block the caller forever. Shared by
/// [`run_upload`] and [`run_test_connection`].
async fn connect_and_open_sftp(
    profile: &SftpProfile,
    password: &str,
) -> Result<(russh::client::Handle<TofuHandler>, SftpSession, Arc<Mutex<Option<String>>>)> {
    let seen_fingerprint = Arc::new(Mutex::new(None));
    let handler = TofuHandler {
        expected: profile.host_key_fingerprint.clone(),
        seen: seen_fingerprint.clone(),
    };
    let config = Arc::new(russh::client::Config::default());

    let mut session = timeout(
        CONNECT_TIMEOUT,
        profile,
        russh::client::connect(config, (profile.host.as_str(), profile.port), handler),
    )
    .await?
    .map_err(|e| err(profile, &e))?;

    let authenticated = timeout(
        CONNECT_TIMEOUT,
        profile,
        session.authenticate_password(&profile.username, password),
    )
    .await?
    .map_err(|e| err(profile, &e))?;
    if !authenticated {
        return Err(CoreError::Sftp(classify_error(
            profile.site,
            "authentication failed (check the username and password)",
        )));
    }

    let channel = timeout(IO_TIMEOUT, profile, session.channel_open_session())
        .await?
        .map_err(|e| err(profile, &e))?;
    timeout(IO_TIMEOUT, profile, channel.request_subsystem(true, "sftp"))
        .await?
        .map_err(|e| err(profile, &e))?;
    let sftp = timeout(IO_TIMEOUT, profile, SftpSession::new(channel.into_stream()))
        .await?
        .map_err(|e| CoreError::Sftp(classify_error(profile.site, &e.to_string())))?;

    Ok((session, sftp, seen_fingerprint))
}

async fn run_upload(
    profile: &SftpProfile,
    password: &str,
    bytes: &[u8],
    remote_filename: &str,
) -> Result<UploadOutcome> {
    let (_session, sftp, seen_fingerprint) = connect_and_open_sftp(profile, password).await?;

    let remote_path = format!("{}/{}", profile.remote_dir.trim_end_matches('/'), remote_filename);
    let mut file = timeout(IO_TIMEOUT, profile, sftp.create(remote_path))
        .await?
        .map_err(|e| CoreError::Sftp(classify_error(profile.site, &e.to_string())))?;
    timeout(IO_TIMEOUT, profile, file.write_all(bytes))
        .await?
        .map_err(|e| CoreError::Sftp(classify_error(profile.site, &e.to_string())))?;
    timeout(IO_TIMEOUT, profile, file.shutdown())
        .await?
        .map_err(|e| CoreError::Sftp(classify_error(profile.site, &e.to_string())))?;

    Ok(outcome(&seen_fingerprint))
}

async fn run_test_connection(profile: &SftpProfile, password: &str) -> Result<UploadOutcome> {
    let (_session, _sftp, seen_fingerprint) = connect_and_open_sftp(profile, password).await?;
    Ok(outcome(&seen_fingerprint))
}

fn outcome(seen_fingerprint: &Arc<Mutex<Option<String>>>) -> UploadOutcome {
    let fingerprint = seen_fingerprint
        .lock()
        .expect("fingerprint mutex poisoned")
        .clone()
        .unwrap_or_default();
    UploadOutcome {
        host_key_fingerprint: fingerprint,
    }
}

/// Runs `fut` with a deadline, turning an expiry into the same kind of
/// `CoreError::Sftp` a real connection failure would produce -- the caller
/// still gets a plain `Result<T>` to `?` against, just with one extra layer
/// (`await?` unwraps the timeout, the inner value is the original result).
async fn timeout<T, F>(duration: Duration, profile: &SftpProfile, fut: F) -> Result<T>
where
    F: std::future::Future<Output = T>,
{
    tokio::time::timeout(duration, fut).await.map_err(|_| {
        CoreError::Sftp(classify_error(
            profile.site,
            &format!(
                "connection to {}:{} timed out after {}s -- check the host/port and that the \
                 server is reachable",
                profile.host,
                profile.port,
                duration.as_secs()
            ),
        ))
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
