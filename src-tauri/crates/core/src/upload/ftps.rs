//! The FTPS (explicit FTP over TLS) transport -- some stock sites (e.g.
//! Shutterstock) only offer this, not SFTP. Isolated in its own file for the
//! same reason [`super::sftp`] is: it's the one piece that can't be
//! exercised without a real server.
//!
//! Built on `suppaftp`'s sync blocking client with its `rustls` backend
//! (pure Rust, no new build toolchain). Deliberately rustls rather than
//! native-tls: many FTPS servers (Shutterstock's included -- this is what
//! its "unexpected EOF during handshake" on upload, after a successful
//! connection test, turned out to mean) require the data-channel TLS
//! connection to resume the control channel's TLS session
//! ("require_ssl_reuse") and silently drop it otherwise. `suppaftp` reuses
//! one [`RustlsConnector`] (backed by one `Arc<ClientConfig>`, which carries
//! rustls's own session cache) across both connections, so the data channel
//! resumes automatically; its native-tls backend does not guarantee that.
//!
//! Unlike SSH, TLS already validates the server's certificate against a
//! trusted root store on every connection, so there's no trust-on-first-use
//! step here: a successful connection is inherently verified, and
//! [`UploadOutcome::host_key_fingerprint`] is just a fixed "connected at
//! least once" marker for the profile-pinning logic in `lib.rs` to key off
//! of, not an actual fingerprint.

use std::io::Cursor;
use std::net::ToSocketAddrs;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use suppaftp::rustls::{ClientConfig, RootCertStore};
use suppaftp::{RustlsConnector, RustlsFtpStream};

use crate::error::{CoreError, Result};

use super::{classify_error, SftpProfile, UploadOutcome};

/// Mozilla's root certificates, built once and reused for every connection
/// in the process -- the point of `rustls`'s `ClientConfig` here isn't just
/// avoiding rebuilding a root store, it's that its session cache (which
/// makes data-channel TLS session resumption work, see the module doc)
/// lives on this same shared config.
fn tls_config() -> Arc<ClientConfig> {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let mut roots = RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth())
        })
        .clone()
}

/// Ceiling on the TCP connect + TLS handshake + login. Mirrors
/// [`super::sftp::CONNECT_TIMEOUT`].
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// Ceiling on each individual read/write once connected. Mirrors
/// [`super::sftp::IO_TIMEOUT`].
const IO_TIMEOUT: Duration = Duration::from_secs(30);

/// Marker returned in place of a real fingerprint -- see the module doc.
const VERIFIED_MARKER: &str = "tls-verified";

/// Connect to `profile`'s FTPS endpoint, authenticate with `password`, and
/// upload `local_path`'s contents to `<remote_dir>/<remote_filename>`.
pub fn upload_file(
    profile: &SftpProfile,
    password: &str,
    local_path: &Path,
    remote_filename: &str,
) -> Result<UploadOutcome> {
    let bytes = std::fs::read(local_path)?;
    let mut stream = connect_and_login(profile, password)?;

    // The FTP default transfer type is ASCII, which mangles (and some
    // servers, like Shutterstock's, outright reject) anything that isn't
    // plain text. Every asset here is a binary image/video file.
    stream
        .transfer_type(suppaftp::types::FileType::Binary)
        .map_err(|e| err(profile, &e))?;
    stream
        .cwd(&profile.remote_dir)
        .map_err(|e| err(profile, &e))?;
    stream
        .put_file(remote_filename, &mut Cursor::new(bytes))
        .map_err(|e| err(profile, &e))?;
    let _ = stream.quit();

    Ok(UploadOutcome {
        host_key_fingerprint: VERIFIED_MARKER.to_string(),
    })
}

/// Connect and authenticate only -- no file transfer -- so a saved profile's
/// credentials and reachability can be verified before (or without) a real
/// upload.
pub fn test_connection(profile: &SftpProfile, password: &str) -> Result<UploadOutcome> {
    let mut stream = connect_and_login(profile, password)?;
    let _ = stream.quit();
    Ok(UploadOutcome {
        host_key_fingerprint: VERIFIED_MARKER.to_string(),
    })
}

/// Connect, upgrade to explicit TLS, and log in, bounding every step so a
/// hung/firewalled host can never block the caller forever.
fn connect_and_login(profile: &SftpProfile, password: &str) -> Result<RustlsFtpStream> {
    let addr = format!("{}:{}", profile.host, profile.port)
        .to_socket_addrs()
        .map_err(|e| CoreError::Ftps(format!("could not resolve {}:{}: {e}", profile.host, profile.port)))?
        .next()
        .ok_or_else(|| CoreError::Ftps(format!("could not resolve {}:{}", profile.host, profile.port)))?;

    let stream = RustlsFtpStream::connect_timeout(addr, CONNECT_TIMEOUT).map_err(|e| err(profile, &e))?;
    stream.get_ref().set_read_timeout(Some(IO_TIMEOUT)).ok();
    stream.get_ref().set_write_timeout(Some(IO_TIMEOUT)).ok();

    // Same connector (and so the same session cache) is reused by suppaftp
    // for the data-channel connection opened later -- see the module doc.
    let connector = RustlsConnector::from(tls_config());
    let mut stream = stream
        .into_secure(connector, &profile.host)
        .map_err(|e| err(profile, &e))?;

    stream
        .login(profile.username.as_str(), password)
        .map_err(|e| err(profile, &e))?;

    Ok(stream)
}

fn err(profile: &SftpProfile, e: &suppaftp::FtpError) -> CoreError {
    CoreError::Ftps(classify_error(profile.site, &e.to_string()))
}
