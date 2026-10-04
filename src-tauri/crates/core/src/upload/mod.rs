//! SPHIN-6: SFTP upload to a stock site's contributor portal.
//!
//! Password-only for now -- every major stock site's SFTP endpoint
//! documents password auth, and a private-key option can be added to
//! [`SftpProfile`] later without changing this module's shape. The
//! password itself never touches `sphinx.db`; see [`crate::secrets`].
//!
//! The host key is pinned on first successful connection ("trust on first
//! use"): [`SftpProfile::host_key_fingerprint`] starts `None`, gets filled
//! in after the first connect, and a later connection presenting a
//! different key is rejected rather than silently trusted -- that's the
//! only thing standing between this and a MITM'd or spoofed server on
//! every connection after the first.

mod ftps;
mod sftp;

use std::path::Path;

use serde::{Deserialize, Serialize};

pub use sftp::UploadOutcome;

use crate::error::Result;

/// Which stock site a profile targets -- purely to tailor error messaging
/// (SPHIN-27); it has no bearing on the transport protocol itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SftpSite {
    Generic,
    AdobeStock,
}

impl Default for SftpSite {
    fn default() -> Self {
        SftpSite::Generic
    }
}

/// Which wire protocol a profile connects with. Some stock sites (e.g.
/// Shutterstock) only offer FTPS, not SFTP, so a profile has to pick one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportProtocol {
    Sftp,
    Ftps,
}

impl Default for TransportProtocol {
    fn default() -> Self {
        TransportProtocol::Sftp
    }
}

/// Upload `local_path`'s contents to `profile`'s remote directory, dispatching
/// to the SFTP or FTPS transport per [`SftpProfile::protocol`].
pub fn upload_file(
    profile: &SftpProfile,
    password: &str,
    local_path: &Path,
    remote_filename: &str,
) -> Result<UploadOutcome> {
    match profile.protocol {
        TransportProtocol::Sftp => sftp::upload_file(profile, password, local_path, remote_filename),
        TransportProtocol::Ftps => ftps::upload_file(profile, password, local_path, remote_filename),
    }
}

/// Connect and authenticate against `profile` without transferring a file,
/// dispatching to the SFTP or FTPS transport per [`SftpProfile::protocol`].
pub fn test_connection(profile: &SftpProfile, password: &str) -> Result<UploadOutcome> {
    match profile.protocol {
        TransportProtocol::Sftp => sftp::test_connection(profile, password),
        TransportProtocol::Ftps => ftps::test_connection(profile, password),
    }
}

/// A named SFTP connection profile for one stock site. The password lives
/// in the OS credential store under `credential_key` (see
/// [`crate::secrets`]), never in this struct once saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SftpProfile {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub site: SftpSite,
    #[serde(default)]
    pub protocol: TransportProtocol,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub remote_dir: String,
    pub credential_key: String,
    /// SHA-256 fingerprint of the host key seen on the first successful
    /// connection; `None` until then.
    #[serde(default)]
    pub host_key_fingerprint: Option<String>,
}

/// Rewrite a raw SFTP/SSH error into something actionable, special-casing
/// Adobe Stock's contributor "qualification" gate (SPHIN-27): Adobe doesn't
/// grant SFTP access until an account has enough approved submissions, and
/// a not-yet-qualified account's connection attempt looks exactly like an
/// ordinary permission/auth failure. There's no official machine-readable
/// signal for this -- Adobe's SFTP just refuses the connection the same way
/// it would for bad credentials -- so this is a heuristic on the error
/// text, not a certainty, and says "likely" for exactly that reason.
pub(crate) fn classify_error(site: SftpSite, raw: &str) -> String {
    let lower = raw.to_lowercase();
    let looks_like_permission_failure = lower.contains("denied")
        || lower.contains("not authorized")
        || lower.contains("forbidden")
        || lower.contains("authentication failed");

    if site == SftpSite::AdobeStock && looks_like_permission_failure {
        format!(
            "{raw} -- if these credentials are otherwise correct, this is likely because Adobe \
             Stock SFTP access isn't enabled yet: Adobe only grants it once a contributor account \
             reaches their submission-based qualification threshold. Upload via the Adobe Stock \
             Contributor portal until qualified."
        )
    } else {
        raw.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adobe_permission_errors_get_the_qualification_hint() {
        let msg = classify_error(SftpSite::AdobeStock, "SFTP error: Permission denied");
        assert!(msg.contains("qualification"));
        assert!(msg.contains("Permission denied"));
    }

    #[test]
    fn generic_sites_are_not_annotated() {
        let msg = classify_error(SftpSite::Generic, "SFTP error: Permission denied");
        assert_eq!(msg, "SFTP error: Permission denied");
    }

    #[test]
    fn adobe_non_permission_errors_are_left_alone() {
        let msg = classify_error(SftpSite::AdobeStock, "connection timed out");
        assert_eq!(msg, "connection timed out");
    }

    #[test]
    fn default_site_is_generic() {
        assert_eq!(SftpSite::default(), SftpSite::Generic);
    }

    #[test]
    fn default_protocol_is_sftp() {
        assert_eq!(TransportProtocol::default(), TransportProtocol::Sftp);
    }

    #[test]
    fn protocol_serde_round_trips_snake_case() {
        assert_eq!(serde_json::to_string(&TransportProtocol::Ftps).unwrap(), "\"ftps\"");
        assert_eq!(
            serde_json::from_str::<TransportProtocol>("\"sftp\"").unwrap(),
            TransportProtocol::Sftp
        );
    }
}
