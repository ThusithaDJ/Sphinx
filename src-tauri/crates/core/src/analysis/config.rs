//! SPHIN-17: per-project vision-provider configuration.
//!
//! Stored as JSON in `project_settings` (see `db::get_analysis_config`), so
//! every project can point at a different provider / model / key.

use serde::{Deserialize, Serialize};

/// Which hosted vision model a project uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    /// OpenAI chat completions with an image part (GPT-4o and successors).
    OpenAi,
    /// Google Gemini `generateContent`.
    Gemini,
    /// Anthropic Claude messages API.
    Anthropic,
    /// A local Ollama server running a vision-capable model (LLaVA,
    /// Qwen2-VL, ...) (SPHIN-34). No API key required by default.
    Ollama,
}

impl ProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::OpenAi => "openai",
            ProviderKind::Gemini => "gemini",
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::Ollama => "ollama",
        }
    }

    /// The model id used when a project hasn't overridden it. Deliberately a
    /// current, widely-available multimodal model per provider; users can point
    /// at anything newer via [`AnalysisConfig::model`].
    pub fn default_model(&self) -> &'static str {
        match self {
            ProviderKind::OpenAi => "gpt-4o",
            ProviderKind::Gemini => "gemini-1.5-pro",
            ProviderKind::Anthropic => "claude-3-5-sonnet-latest",
            ProviderKind::Ollama => "llava",
        }
    }

    /// The API base URL used when a project hasn't overridden it.
    pub fn default_base_url(&self) -> &'static str {
        match self {
            ProviderKind::OpenAi => "https://api.openai.com/v1",
            ProviderKind::Gemini => "https://generativelanguage.googleapis.com/v1beta",
            ProviderKind::Anthropic => "https://api.anthropic.com/v1",
            ProviderKind::Ollama => "http://localhost:11434",
        }
    }

    /// Whether this provider needs an API key to be usable. Only false for
    /// [`ProviderKind::Ollama`], which talks to a local, unauthenticated
    /// server by default.
    pub fn requires_api_key(&self) -> bool {
        !matches!(self, ProviderKind::Ollama)
    }
}

/// A project's analysis configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    pub provider: ProviderKind,
    /// The provider API key. Stored as-is for now; SPHIN-25 introduces
    /// encrypted credential storage that this will move behind.
    #[serde(default)]
    pub api_key: String,
    /// Model id; empty string means "use the provider default".
    #[serde(default)]
    pub model: String,
    /// API base URL override (self-hosted gateways, proxies, tests); empty
    /// means "use the provider default".
    #[serde(default)]
    pub base_url: String,
    /// Extra project-specific guidance appended to the analysis prompt, e.g.
    /// "This project is fine-art nature photography; prefer species names."
    #[serde(default)]
    pub prompt_extra: String,
    /// Request timeout in seconds (vision calls on large images can be slow).
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_timeout_secs() -> u64 {
    90
}

impl AnalysisConfig {
    /// A config for `provider` with every other field left at its default.
    pub fn new(provider: ProviderKind) -> Self {
        Self {
            provider,
            api_key: String::new(),
            model: String::new(),
            base_url: String::new(),
            prompt_extra: String::new(),
            timeout_secs: default_timeout_secs(),
        }
    }

    /// The model id to actually use (override, else provider default).
    pub fn model_or_default(&self) -> String {
        let m = self.model.trim();
        if m.is_empty() {
            self.provider.default_model().to_string()
        } else {
            m.to_string()
        }
    }

    /// The base URL to actually use, with any trailing slash removed.
    pub fn base_url_or_default(&self) -> String {
        let b = self.base_url.trim().trim_end_matches('/');
        if b.is_empty() {
            self.provider.default_base_url().to_string()
        } else {
            b.to_string()
        }
    }

    pub fn timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.timeout_secs.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_provider_defaults() {
        let cfg = AnalysisConfig::new(ProviderKind::Gemini);
        assert_eq!(cfg.model_or_default(), "gemini-1.5-pro");
        assert_eq!(
            cfg.base_url_or_default(),
            "https://generativelanguage.googleapis.com/v1beta"
        );
        assert_eq!(cfg.timeout_secs, 90);
    }

    #[test]
    fn overrides_win_and_base_url_is_normalized() {
        let mut cfg = AnalysisConfig::new(ProviderKind::OpenAi);
        cfg.model = "  gpt-5  ".into();
        cfg.base_url = "https://proxy.internal/v1/".into();
        assert_eq!(cfg.model_or_default(), "gpt-5");
        assert_eq!(cfg.base_url_or_default(), "https://proxy.internal/v1");
    }

    #[test]
    fn only_ollama_skips_the_api_key_requirement() {
        assert!(!ProviderKind::Ollama.requires_api_key());
        assert!(ProviderKind::OpenAi.requires_api_key());
        assert!(ProviderKind::Gemini.requires_api_key());
        assert!(ProviderKind::Anthropic.requires_api_key());
    }

    #[test]
    fn deserializes_with_missing_optional_fields() {
        let cfg: AnalysisConfig =
            serde_json::from_str(r#"{"provider":"anthropic","api_key":"k"}"#).unwrap();
        assert_eq!(cfg.provider, ProviderKind::Anthropic);
        assert_eq!(cfg.timeout_secs, 90);
        assert_eq!(cfg.model_or_default(), "claude-3-5-sonnet-latest");
    }
}
