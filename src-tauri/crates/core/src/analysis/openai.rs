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
    prompt_extra: String,
}

impl OpenAiProvider {
    pub fn new(config: &AnalysisConfig) -> Result<Self> {
        Ok(Self {
            client: http::client(config.timeout())?,
            api_key: config.api_key.clone(),
            model: config.model_or_default(),
            base_url: config.base_url_or_default(),
            prompt_extra: config.prompt_extra.clone(),
        })
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }

    /// The request body for one or more images sharing a single instruction.
    /// Pure so it can be asserted on without a network call.
    fn build_body(&self, system: &str, instruction: &str, images: &[ImageInput]) -> Value {
        let mut content = vec![json!({ "type": "text", "text": instruction })];
        for image in images {
            content.push(json!({ "type": "image_url", "image_url": { "url": image.data_url() } }));
        }
        json!({
            "model": self.model,
            "temperature": 0.2,
            "response_format": { "type": "json_object" },
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": content }
            ]
        })
    }

    fn send(&self, body: &Value) -> Result<AnalysisResult> {
        let auth = format!("Bearer {}", self.api_key);
        let resp = http::post_json(
            &self.client,
            self.name(),
            &self.endpoint(),
            &[("authorization", auth.as_str())],
            body,
        )?;
        let content = http::dig_str(
            self.name(),
            &resp,
            &["choices", "0", "message", "content"],
        )?;
        parse_analysis_json(self.name(), &self.model, content)
    }
}

impl VisionProvider for OpenAiProvider {
    fn analyze(&self, image: &ImageInput) -> Result<AnalysisResult> {
        let instruction = prompt::build_instruction(&self.prompt_extra);
        let body = self.build_body(prompt::SYSTEM, &instruction, std::slice::from_ref(image));
        self.send(&body)
    }

    fn analyze_video(&self, frames: &[ImageInput], transcript: Option<&str>) -> Result<AnalysisResult> {
        let instruction = prompt::build_video_instruction(&self.prompt_extra, frames.len(), transcript);
        let body = self.build_body(prompt::VIDEO_SYSTEM, &instruction, frames);
        self.send(&body)
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
        let body = provider().build_body(prompt::SYSTEM, "instruction", &[img]);
        assert_eq!(body["model"], "gpt-4o");
        assert_eq!(body["response_format"]["type"], "json_object");
        assert_eq!(body["messages"][0]["content"], prompt::SYSTEM);
        let parts = &body["messages"][1]["content"];
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(
            parts[1]["image_url"]["url"],
            "data:image/png;base64,eA=="
        );
    }

    #[test]
    fn video_body_carries_every_frame_and_the_video_system_message() {
        let frames = vec![
            ImageInput { bytes: b"a".to_vec(), mime: "image/jpeg".into() },
            ImageInput { bytes: b"b".to_vec(), mime: "image/jpeg".into() },
        ];
        let body = provider().build_body(prompt::VIDEO_SYSTEM, "instruction", &frames);
        assert_eq!(body["messages"][0]["content"], prompt::VIDEO_SYSTEM);
        let parts = &body["messages"][1]["content"];
        assert_eq!(parts.as_array().unwrap().len(), 3); // text + 2 frames
        assert_eq!(parts[1]["image_url"]["url"], "data:image/jpeg;base64,YQ==");
        assert_eq!(parts[2]["image_url"]["url"], "data:image/jpeg;base64,Yg==");
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
