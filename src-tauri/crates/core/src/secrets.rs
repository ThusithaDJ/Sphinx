//! SPHIN-25: SFTP passwords -- and the AI / keyword-provider API keys --
//! never touch `sphinx.db`. Each is stored in the
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

/// SFTP/FTPS passwords, keyed by `sftp-profile-<id>`.
const SERVICE: &str = "sphinx-sftp";
/// AI-provider, transcription and keyword-API keys, keyed by
/// `project-<id>-<purpose>` (see the Tauri layer's `api_key_name`).
const API_KEY_SERVICE: &str = "sphinx-api-keys";

fn entry(service: &str, key: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(service, key)
        .map_err(|e| CoreError::Config(format!("credential store error: {e}")))
}

fn set_in(service: &str, key: &str, secret: &str) -> Result<()> {
    entry(service, key)?
        .set_password(secret)
        .map_err(|e| CoreError::Config(format!("could not save credential: {e}")))
}

fn get_in(service: &str, key: &str) -> Result<Option<String>> {
    match entry(service, key)?.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(CoreError::Config(format!("could not read credential: {e}"))),
    }
}

fn delete_in(service: &str, key: &str) -> Result<()> {
    match entry(service, key)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(CoreError::Config(format!("could not delete credential: {e}"))),
    }
}

/// Store `secret` under `key` in the OS credential store, replacing any
/// existing value.
pub fn set_secret(key: &str, secret: &str) -> Result<()> {
    set_in(SERVICE, key, secret)
}

/// The secret stored under `key`, or `None` if nothing has been saved yet.
pub fn get_secret(key: &str) -> Result<Option<String>> {
    get_in(SERVICE, key)
}

/// Remove the secret stored under `key`. Not finding one is not an error.
pub fn delete_secret(key: &str) -> Result<()> {
    delete_in(SERVICE, key)
}

/// Store an API key under `name`, replacing any existing value.
pub fn set_api_key(name: &str, key: &str) -> Result<()> {
    set_in(API_KEY_SERVICE, name, key)
}

/// The API key stored under `name`, or `None` if none has been saved.
pub fn get_api_key(name: &str) -> Result<Option<String>> {
    get_in(API_KEY_SERVICE, name)
}

/// Remove the API key stored under `name`. Not finding one is not an error.
pub fn delete_api_key(name: &str) -> Result<()> {
    delete_in(API_KEY_SERVICE, name)
}
