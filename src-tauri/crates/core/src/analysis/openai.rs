//! OpenAI vision provider (SPHIN-15): chat completions with an image part.

use serde_json::{json, Value};

use crate::error::Result;

use super::http;
use super::prompt;
use super::{parse_analysis_json, AnalysisConfig, AnalysisResult, ImageInput, VisionProvider};

pub struct OpenAiProvider {
    client: reqwest::blocking::Client,
    api_key: String,
    model: String,
    base_url: String,
    instruction: String,
}

impl OpenAiProvider {
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
        format!("{}/chat/completions", self.base_url)
    }

    /// The request body. Pure so it can be asserted on without a network call.
    fn build_body(&self, image: &ImageInput) -> Value {
        json!({
            "model": self.model,
            "temperature": 0.2,
            "response_format": { "type": "json_object" },
            "messages": [
                { "role": "system", "content": prompt::SYSTEM },
                { "role": "user", "content": [
                    { "type": "text", "text": self.instruction },
                    { "type": "image_url", "image_url": { "url": image.data_url() } }
                ]}
            ]
        })
    }
}

impl VisionProvider for OpenAiProvider {
    fn analyze(&self, image: &ImageInput) -> Result<AnalysisResult> {
        let auth = format!("Bearer {}", self.api_key);
        let resp = http::post_json(
            &self.client,
            self.name(),
            &self.endpoint(),
            &[("authorization", auth.as_str())],
            &self.build_body(image),
        )?;
        let content = http::dig_str(
            self.name(),
            &resp,
            &["choices", "0", "message", "content"],
        )?;
        parse_analysis_json(self.name(), &self.model, content)
    }

    fn name(&self) -> &'static str {
        "openai"
    }

    fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::ProviderKind;

    fn provider() -> OpenAiProvider {
        let mut cfg = AnalysisConfig::new(ProviderKind::OpenAi);
        cfg.api_key = "sk-test".into();
        OpenAiProvider::new(&cfg).unwrap()
    }

    #[test]
    fn body_has_image_and_json_mode() {
        let img = ImageInput {
            bytes: b"x".to_vec(),
            mime: "image/png".into(),
        };
        let body = provider().build_body(&img);
        assert_eq!(body["model"], "gpt-4o");
        assert_eq!(body["response_format"]["type"], "json_object");
        let parts = &body["messages"][1]["content"];
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(
            parts[1]["image_url"]["url"],
            "data:image/png;base64,eA=="
        );
    }

    #[test]
    fn endpoint_respects_base_url_override() {
        let mut cfg = AnalysisConfig::new(ProviderKind::OpenAi);
        cfg.api_key = "k".into();
        cfg.base_url = "https://gateway.local/openai/v1/".into();
        let p = OpenAiProvider::new(&cfg).unwrap();
        assert_eq!(p.endpoint(), "https://gateway.local/openai/v1/chat/completions");
    }
}
