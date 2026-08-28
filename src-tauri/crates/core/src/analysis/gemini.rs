//! Google Gemini vision provider (SPHIN-15): `generateContent` with an inline
//! image part.

use serde_json::{json, Value};

use crate::error::Result;

use super::http;
use super::prompt;
use super::{parse_analysis_json, AnalysisConfig, AnalysisResult, ImageInput, VisionProvider};

pub struct GeminiProvider {
    client: reqwest::blocking::Client,
    api_key: String,
    model: String,
    base_url: String,
    prompt_extra: String,
}

impl GeminiProvider {
    pub fn new(config: &AnalysisConfig) -> Result<Self> {
        Ok(Self {
            client: http::client(config.timeout())?,
            api_key: config.api_key.clone(),
            model: config.model_or_default(),
            base_url: config.base_url_or_default(),
            prompt_extra: config.prompt_extra.clone(),
        })
    }

    /// Model goes in the path; the key goes in the `x-goog-api-key` header
    /// (Gemini also accepts `?key=`, but keeping it out of the URL keeps it out
    /// of logs and proxies).
    fn endpoint(&self) -> String {
        format!("{}/models/{}:generateContent", self.base_url, self.model)
    }

    fn build_body(&self, system: &str, instruction: &str, images: &[ImageInput]) -> Value {
        let mut parts = vec![json!({ "text": instruction })];
        for image in images {
            parts.push(json!({ "inline_data": { "mime_type": image.mime, "data": image.base64() } }));
        }
        json!({
            "systemInstruction": { "parts": [ { "text": system } ] },
            "contents": [ { "role": "user", "parts": parts } ],
            "generationConfig": {
                "temperature": 0.2,
                "responseMimeType": "application/json"
            }
        })
    }

    fn send(&self, body: &Value) -> Result<AnalysisResult> {
        let resp = http::post_json(
            &self.client,
            self.name(),
            &self.endpoint(),
            &[("x-goog-api-key", self.api_key.as_str())],
            body,
        )?;
        let content = http::dig_str(
            self.name(),
            &resp,
            &["candidates", "0", "content", "parts", "0", "text"],
        )?;
        parse_analysis_json(self.name(), &self.model, content)
    }
}

impl VisionProvider for GeminiProvider {
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
        "gemini"
    }

    fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::ProviderKind;

    fn provider() -> GeminiProvider {
        let mut cfg = AnalysisConfig::new(ProviderKind::Gemini);
        cfg.api_key = "AIza-test".into();
        GeminiProvider::new(&cfg).unwrap()
    }

    #[test]
    fn endpoint_carries_model_in_path_and_no_key() {
        let url = provider().endpoint();
        assert_eq!(
            url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-1.5-pro:generateContent"
        );
        assert!(!url.contains("AIza-test"));
    }

    #[test]
    fn body_has_inline_image_and_json_mime() {
        let img = ImageInput {
            bytes: b"x".to_vec(),
            mime: "image/webp".into(),
        };
        let body = provider().build_body(prompt::SYSTEM, "instruction", &[img]);
        let parts = &body["contents"][0]["parts"];
        assert_eq!(parts[0]["text"].as_str().unwrap().is_empty(), false);
        assert_eq!(parts[1]["inline_data"]["mime_type"], "image/webp");
        assert_eq!(parts[1]["inline_data"]["data"], "eA==");
        assert_eq!(
            body["generationConfig"]["responseMimeType"],
            "application/json"
        );
    }

    #[test]
    fn video_body_carries_every_frame_as_inline_data() {
        let frames = vec![
            ImageInput { bytes: b"a".to_vec(), mime: "image/jpeg".into() },
            ImageInput { bytes: b"b".to_vec(), mime: "image/jpeg".into() },
        ];
        let body = provider().build_body(prompt::VIDEO_SYSTEM, "instruction", &frames);
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], prompt::VIDEO_SYSTEM);
        let parts = &body["contents"][0]["parts"];
        assert_eq!(parts.as_array().unwrap().len(), 3); // text + 2 frames
        assert_eq!(parts[1]["inline_data"]["data"], "YQ==");
        assert_eq!(parts[2]["inline_data"]["data"], "Yg==");
    }
}
