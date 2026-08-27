//! Anthropic Claude vision provider (SPHIN-15): the messages API with an image
//! content block.

use serde_json::{json, Value};

use crate::error::Result;

use super::http;
use super::prompt;
use super::{parse_analysis_json, AnalysisConfig, AnalysisResult, ImageInput, VisionProvider};

/// Pinned messages-API version (Anthropic requires the `anthropic-version`
/// header; this value is stable and documented).
const API_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    client: reqwest::blocking::Client,
    api_key: String,
    model: String,
    base_url: String,
    instruction: String,
}

impl AnthropicProvider {
    pub fn new(config: &AnalysisConfig) -> Result<Self> {
        Ok(Self {
            client: http::client(config.timeout())?,
            api_key: config.api_key.clone(),
            model: config.model_or_default(),
            base_url: config.base_url_or_default(),
            instruction: prompt::build_instruction(&config.prompt_extra),
        })
    }

    fn endpoint(&self) -> String {
        format!("{}/messages", self.base_url)
    }

    fn build_body(&self, image: &ImageInput) -> Value {
        json!({
            "model": self.model,
            "max_tokens": 1024,
            "temperature": 0.2,
            "system": prompt::SYSTEM,
            "messages": [
                { "role": "user", "content": [
                    { "type": "image", "source": {
                        "type": "base64",
                        "media_type": image.mime,
                        "data": image.base64()
                    }},
                    { "type": "text", "text": self.instruction }
                ]}
            ]
        })
    }
}

impl VisionProvider for AnthropicProvider {
    fn analyze(&self, image: &ImageInput) -> Result<AnalysisResult> {
        let resp = http::post_json(
            &self.client,
            self.name(),
            &self.endpoint(),
            &[
                ("x-api-key", self.api_key.as_str()),
                ("anthropic-version", API_VERSION),
            ],
            &self.build_body(image),
        )?;
        let content = http::dig_str(self.name(), &resp, &["content", "0", "text"])?;
        parse_analysis_json(self.name(), &self.model, content)
    }

    fn name(&self) -> &'static str {
        "anthropic"
    }

    fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::ProviderKind;

    #[test]
    fn body_uses_base64_image_source_block() {
        let mut cfg = AnalysisConfig::new(ProviderKind::Anthropic);
        cfg.api_key = "k".into();
        let p = AnthropicProvider::new(&cfg).unwrap();
        let img = ImageInput {
            bytes: b"x".to_vec(),
            mime: "image/jpeg".into(),
        };
        let body = p.build_body(&img);
        assert_eq!(body["model"], "claude-3-5-sonnet-latest");
        let src = &body["messages"][0]["content"][0]["source"];
        assert_eq!(src["type"], "base64");
        assert_eq!(src["media_type"], "image/jpeg");
        assert_eq!(src["data"], "eA==");
        assert_eq!(body["messages"][0]["content"][1]["type"], "text");
    }

    #[test]
    fn endpoint_is_under_the_configured_base() {
        let mut cfg = AnalysisConfig::new(ProviderKind::Anthropic);
        cfg.api_key = "k".into();
        let p = AnthropicProvider::new(&cfg).unwrap();
        assert_eq!(p.endpoint(), "https://api.anthropic.com/v1/messages");
    }
}
