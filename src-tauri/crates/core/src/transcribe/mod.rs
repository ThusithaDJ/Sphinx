//! SPHIN-32: optional audio transcription via an OpenAI-Whisper-compatible
//! HTTP API.
//!
//! Deliberately a cloud call rather than a bundled local Whisper model
//! (whisper.cpp etc.) -- it reuses the same "shape the request, don't test
//! the transport" pattern already used for the vision providers
//! ([`crate::analysis`]) and keyword connectors ([`crate::keywords`]), and
//! avoids adding a second native-toolchain / large-model-download dependency
//! to a project that has already deliberately avoided those (see
//! [`crate::upload`]'s choice of `russh` over `ssh2`). Request shaping and
//! response parsing are unit-tested offline; the actual HTTP call is not (no
//! credentials or network access in this sandbox).

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{CoreError, Result};

/// Per-project transcription configuration. Off by default -- a project only
/// pays the extra API call and latency if it opts in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub api_key: String,
    /// Empty string => `https://api.openai.com/v1`. Lets self-hosted
    /// Whisper-compatible gateways (e.g. faster-whisper servers) be used
    /// instead.
    #[serde(default)]
    pub base_url: String,
    #[serde(default = "default_model")]
    pub model: String,
}

fn default_model() -> String {
    "whisper-1".to_string()
}

fn default_base_url() -> &'static str {
    "https://api.openai.com/v1"
}

impl Default for TranscriptionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_key: String::new(),
            base_url: String::new(),
            model: default_model(),
        }
    }
}

impl TranscriptionConfig {
    fn base_url_or_default(&self) -> String {
        let b = self.base_url.trim().trim_end_matches('/');
        if b.is_empty() {
            default_base_url().to_string()
        } else {
            b.to_string()
        }
    }
}

/// Transcribe the audio file at `audio_path` and return its text. Fails fast
/// with [`CoreError::Config`] if no API key is set, before any network call.
pub fn transcribe(config: &TranscriptionConfig, audio_path: impl AsRef<Path>) -> Result<String> {
    if config.api_key.trim().is_empty() {
        return Err(CoreError::Config(
            "transcription enabled but no API key set".to_string(),
        ));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(15))
        .user_agent(concat!("sphinx/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|source| CoreError::Http {
            provider: "whisper".to_string(),
            source,
        })?;

    let form = reqwest::blocking::multipart::Form::new()
        .text("model", config.model.clone())
        .file("file", audio_path.as_ref())
        .map_err(CoreError::Io)?;

    let url = format!("{}/audio/transcriptions", config.base_url_or_default());
    let resp = client
        .post(&url)
        .header("authorization", format!("Bearer {}", config.api_key))
        .multipart(form)
        .send()
        .map_err(|source| CoreError::Http {
            provider: "whisper".to_string(),
            source,
        })?;

    let status = resp.status();
    let text = resp.text().map_err(|source| CoreError::Http {
        provider: "whisper".to_string(),
        source,
    })?;

    if !status.is_success() {
        return Err(CoreError::Provider {
            provider: "whisper".to_string(),
            status: status.as_u16(),
            message: excerpt(&text, 400),
        });
    }

    parse_transcription_response(&text)
}

/// Pull the transcript text out of `{"text": "..."}` (the shape returned by
/// both OpenAI's `whisper-1` and its common self-hosted-compatible servers).
fn parse_transcription_response(body: &str) -> Result<String> {
    let v: Value = serde_json::from_str(body).map_err(|e| CoreError::BadResponse {
        provider: "whisper".to_string(),
        message: format!("response was not JSON: {e}"),
    })?;
    v.get("text")
        .and_then(|t| t.as_str())
        .map(|s| s.trim().to_string())
        .ok_or_else(|| CoreError::BadResponse {
            provider: "whisper".to_string(),
            message: format!("missing `text` in response: {}", excerpt(body, 300)),
        })
}

fn excerpt(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_falls_back_to_openai() {
        let cfg = TranscriptionConfig::default();
        assert_eq!(cfg.base_url_or_default(), "https://api.openai.com/v1");
    }

    #[test]
    fn base_url_override_is_normalized() {
        let cfg = TranscriptionConfig {
            base_url: "https://gateway.local/v1/".to_string(),
            ..TranscriptionConfig::default()
        };
        assert_eq!(cfg.base_url_or_default(), "https://gateway.local/v1");
    }

    #[test]
    fn default_model_is_whisper_1() {
        assert_eq!(TranscriptionConfig::default().model, "whisper-1");
        assert!(!TranscriptionConfig::default().enabled);
    }

    #[test]
    fn parses_clean_response() {
        let text = parse_transcription_response(r#"{"text":"Hello there."}"#).unwrap();
        assert_eq!(text, "Hello there.");
    }

    #[test]
    fn missing_text_field_is_a_bad_response_error() {
        let err = parse_transcription_response(r#"{"error":"nope"}"#).unwrap_err();
        assert!(matches!(err, CoreError::BadResponse { .. }));
    }

    #[test]
    fn non_json_body_is_a_bad_response_error() {
        let err = parse_transcription_response("not json").unwrap_err();
        assert!(matches!(err, CoreError::BadResponse { .. }));
    }

    #[test]
    fn transcribe_requires_an_api_key_before_touching_the_network() {
        let cfg = TranscriptionConfig::default();
        let err = transcribe(&cfg, "nonexistent.wav").unwrap_err();
        assert!(matches!(err, CoreError::Config(_)));
    }
}
