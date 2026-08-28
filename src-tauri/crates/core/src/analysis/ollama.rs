//! Ollama vision provider (SPHIN-34): a local Ollama server running a
//! vision-capable model (LLaVA, Qwen2-VL, ...) via `/api/chat`. Images are
//! sent as raw base64 (no `data:` prefix, unlike the cloud providers), and
//! the request asks for `"format": "json"` so the model returns strict JSON
//! the same way the cloud providers are prompted to.

use serde_json::{json, Value};

use crate::error::{CoreError, Result};

use super::http;
use super::prompt;
use super::{parse_analysis_json, AnalysisConfig, AnalysisResult, ImageInput, VisionProvider};

pub struct OllamaProvider {
    client: reqwest::blocking::Client,
    api_key: String,
    model: String,
    base_url: String,
    prompt_extra: String,
}

impl OllamaProvider {
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
        format!("{}/api/chat", self.base_url)
    }

    fn build_body(&self, system: &str, instruction: &str, images: &[ImageInput]) -> Value {
        json!({
            "model": self.model,
            "stream": false,
            "format": "json",
            "options": { "temperature": 0.2 },
            "messages": [
                { "role": "system", "content": system },
                {
                    "role": "user",
                    "content": instruction,
                    "images": images.iter().map(ImageInput::base64).collect::<Vec<_>>()
                }
            ]
        })
    }

    fn send(&self, body: &Value) -> Result<AnalysisResult> {
        // The key is optional and empty by default (a local Ollama server
        // needs no auth); when set, it's for a proxied/authenticated server
        // and goes out as a normal Bearer token.
        let auth = format!("Bearer {}", self.api_key);
        let headers: &[(&str, &str)] = if self.api_key.trim().is_empty() {
            &[]
        } else {
            &[("authorization", auth.as_str())]
        };
        let resp = http::post_json(&self.client, self.name(), &self.endpoint(), headers, body)?;
        let content = http::dig_str(self.name(), &resp, &["message", "content"])?;
        parse_analysis_json(self.name(), &self.model, content)
    }
}

impl VisionProvider for OllamaProvider {
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
        "ollama"
    }

    fn model(&self) -> &str {
        &self.model
    }
}

/// Probe a local Ollama server for reachability, returning a short summary of
/// the models it has pulled (SPHIN-34/36). Used by the Settings screen's
/// "Local tools" status row, the same way `check_ffmpeg`/`check_exiftool`
/// check their respective external tools.
pub fn check_ollama(config: &AnalysisConfig) -> Result<String> {
    let base_url = config.base_url_or_default();
    let client = http::client(std::time::Duration::from_secs(5))?;
    let resp = client
        .get(format!("{base_url}/api/tags"))
        .send()
        .map_err(|source| CoreError::Http {
            provider: "ollama".to_string(),
            source,
        })?;

    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if !status.is_success() {
        return Err(CoreError::Provider {
            provider: "ollama".to_string(),
            status: status.as_u16(),
            message: text.chars().take(200).collect(),
        });
    }

    let body: Value = serde_json::from_str(&text).map_err(|e| CoreError::BadResponse {
        provider: "ollama".to_string(),
        message: format!("response was not JSON: {e}"),
    })?;
    let names: Vec<String> = body["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["name"].as_str().map(str::to_string))
        .collect();

    Ok(if names.is_empty() {
        "reachable, no models pulled yet".to_string()
    } else {
        format!("{} model(s): {}", names.len(), names.join(", "))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::ProviderKind;

    fn provider() -> OllamaProvider {
        let cfg = AnalysisConfig::new(ProviderKind::Ollama);
        OllamaProvider::new(&cfg).unwrap()
    }

    #[test]
    fn endpoint_defaults_to_localhost() {
        assert_eq!(provider().endpoint(), "http://localhost:11434/api/chat");
    }

    #[test]
    fn body_sends_raw_base64_images_and_json_format() {
        let img = ImageInput {
            bytes: b"x".to_vec(),
            mime: "image/png".into(),
        };
        let body = provider().build_body(prompt::SYSTEM, "instruction", &[img]);
        assert_eq!(body["model"], "llava");
        assert_eq!(body["format"], "json");
        assert_eq!(body["messages"][0]["content"], prompt::SYSTEM);
        assert_eq!(body["messages"][1]["content"], "instruction");
        assert_eq!(body["messages"][1]["images"][0], "eA==");
    }

    #[test]
    fn video_body_carries_every_frame() {
        let frames = vec![
            ImageInput { bytes: b"a".to_vec(), mime: "image/jpeg".into() },
            ImageInput { bytes: b"b".to_vec(), mime: "image/jpeg".into() },
        ];
        let body = provider().build_body(prompt::VIDEO_SYSTEM, "instruction", &frames);
        let images = body["messages"][1]["images"].as_array().unwrap();
        assert_eq!(images.len(), 2);
        assert_eq!(images[0], "YQ==");
        assert_eq!(images[1], "Yg==");
    }

    #[test]
    fn no_api_key_is_required_to_construct_the_provider() {
        let cfg = AnalysisConfig::new(ProviderKind::Ollama);
        assert!(cfg.api_key.is_empty());
        assert!(OllamaProvider::new(&cfg).is_ok());
    }
}
