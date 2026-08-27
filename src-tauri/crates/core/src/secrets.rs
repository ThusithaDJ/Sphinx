//! SPHIN-25: SFTP passwords never touch `sphinx.db`. Each is stored in the
//! OS credential store (Windows Credential Manager, via the `keyring`
//! crate) under a per-profile key; only that key -- never the secret
//! itself -- is persisted in SQLite. This is a stronger boundary than
//! encrypting the secret and storing the ciphertext in the database: even a
//! full copy of `sphinx.db` leaking contains no recoverable credential.
//!
//! This module is a thin, deliberately untested wrapper around a real OS
//! API, the same way [`crate::embed`]'s actual exiftool subprocess call and
//! [`crate::analysis`]'s actual network calls aren't unit-tested -- there's
//! nothing to usefully assert offline, and exercising it for real would
//! read and write the current user's actual credential store as a side
//! effect of running `cargo test`.

use crate::error::{CoreError, Result};

const SERVICE: &str = "sphinx-sftp";

fn entry(key: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, key)
        .map_err(|e| CoreError::Config(format!("credential store error: {e}")))
}

/// Store `secret` under `key` in the OS credential store, replacing any
/// existing value.
pub fn set_secret(key: &str, secret: &str) -> Result<()> {
    entry(key)?
        .set_password(secret)
        .map_err(|e| CoreError::Config(format!("could not save credential: {e}")))
}

/// The secret stored under `key`, or `None` if nothing has been saved yet.
pub fn get_secret(key: &str) -> Result<Option<String>> {
    match entry(key)?.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(CoreError::Config(format!("could not read credential: {e}"))),
    }
}

/// Remove the secret stored under `key`. Not finding one is not an error.
pub fn delete_secret(key: &str) -> Result<()> {
    match entry(key)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(CoreError::Config(format!("could not delete credential: {e}"))),
    }
}
