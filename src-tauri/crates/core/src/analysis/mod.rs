//! SPHIN-2: media analysis via cloud vision models.
//!
//! One image in, one [`AnalysisResult`] out. Providers ([`ProviderKind`]) each
//! wrap a hosted vision model (OpenAI, Google Gemini, Anthropic Claude) behind
//! the [`VisionProvider`] trait. Request shaping and response parsing are
//! unit-tested offline; no test in this module touches the network.
//!
//! Everything here is synchronous and blocking on purpose — the async job queue
//! (SPHIN-7) will drive these on worker threads.

mod anthropic;
pub mod config;
mod gemini;
mod http;
mod openai;
pub mod prompt;

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

pub use config::{AnalysisConfig, ProviderKind};

/// An image handed to a provider: owned bytes plus a MIME type. Providers never
/// touch the filesystem, which keeps all IO in [`ImageInput::from_path`] and
/// makes the providers trivially testable.
#[derive(Clone)]
pub struct ImageInput {
    pub bytes: Vec<u8>,
    pub mime: String,
}

impl ImageInput {
    /// Read an image file and infer its MIME type from the extension.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mime = mime_for_extension(
            path.extension().and_then(|e| e.to_str()).unwrap_or(""),
        )
        .ok_or_else(|| CoreError::UnsupportedMedia(path.display().to_string()))?;
        let bytes = std::fs::read(path)?;
        Ok(Self {
            bytes,
            mime: mime.to_string(),
        })
    }

    /// Base64 (standard alphabet, padded) — the encoding every provider wants
    /// for inline image data.
    pub fn base64(&self) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(&self.bytes)
    }

    /// `data:` URL form, used by the OpenAI chat API.
    pub fn data_url(&self) -> String {
        format!("data:{};base64,{}", self.mime, self.base64())
    }
}

/// Map a file extension to the MIME type to send to a vision provider. Only
/// formats the major providers actually accept are listed; anything else is an
/// unsupported-media error rather than a silently-wrong guess.
pub fn mime_for_extension(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

/// The structured, factual content description a vision model returns for an
/// asset (SPHIN-16). Downstream epics turn this into marketable
/// title/description/keywords (SPHIN-3) and embed it (SPHIN-4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisResult {
    /// One or two factual, present-tense sentences describing what is visible.
    pub description: String,
    /// Concrete things visible, most prominent first.
    pub subjects: Vec<String>,
    /// The setting / environment in a few words.
    pub scene: String,
    /// Overall mood or atmosphere, one or two words.
    pub mood: String,
    /// Dominant colours as plain names.
    pub colors: Vec<String>,
    /// Candidate search tags, lowercase, deduped — raw material for SPHIN-3.
    pub keywords: Vec<String>,
    /// True if the image contains recognizable people, logos, brands, artwork
    /// or property that would restrict commercial licensing.
    pub editorial: bool,
    /// Any legible text in the image (empty string if none).
    #[serde(default)]
    pub text_content: String,
    /// The model that produced this result (provenance for debugging / re-runs).
    #[serde(default)]
    pub model: String,
    /// Which provider produced it.
    #[serde(default)]
    pub provider: String,
}

/// The half of [`AnalysisResult`] we ask the model to return as JSON. `model`
/// and `provider` are filled in by [`VisionProvider::analyze`], not the model.
#[derive(Debug, Clone, Deserialize)]
struct ModelAnalysis {
    #[serde(default)]
    description: String,
    #[serde(default)]
    subjects: Vec<String>,
    #[serde(default)]
    scene: String,
    #[serde(default)]
    mood: String,
    #[serde(default)]
    colors: Vec<String>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    editorial: bool,
    #[serde(default)]
    text_content: String,
}

/// A hosted vision model behind a uniform interface.
pub trait VisionProvider {
    /// Analyze one image, returning the structured description or a
    /// [`CoreError::Provider`] / [`CoreError::BadResponse`] on failure.
    fn analyze(&self, image: &ImageInput) -> Result<AnalysisResult>;

    /// Analyze a sequence of keyframes sampled from a video, plus an
    /// optional transcript of its audio (SPHIN-33). Returns the same
    /// [`AnalysisResult`] shape as [`Self::analyze`] so downstream metadata
    /// generation (SPHIN-3), embedding (SPHIN-4) and keyword enrichment
    /// (SPHIN-5) need no video-specific handling.
    fn analyze_video(&self, frames: &[ImageInput], transcript: Option<&str>) -> Result<AnalysisResult>;

    /// A short, stable name for logs and stored provenance ("openai", ...).
    fn name(&self) -> &'static str;

    /// The model id requests are being sent to.
    fn model(&self) -> &str;
}

/// Build the configured provider. Fails fast if the config has no API key,
/// so callers get a clear "not configured" error before any network call.
pub fn provider_for(config: &AnalysisConfig) -> Result<Box<dyn VisionProvider>> {
    if config.api_key.trim().is_empty() {
        return Err(CoreError::Config(format!(
            "{} provider selected but no API key set",
            config.provider.as_str()
        )));
    }
    Ok(match config.provider {
        ProviderKind::OpenAi => {
            Box::new(openai::OpenAiProvider::new(config)?) as Box<dyn VisionProvider>
        }
        ProviderKind::Gemini => Box::new(gemini::GeminiProvider::new(config)?),
        ProviderKind::Anthropic => Box::new(anthropic::AnthropicProvider::new(config)?),
    })
}

/// Read an image from disk and run the configured provider against it.
pub fn analyze_file(config: &AnalysisConfig, path: impl AsRef<Path>) -> Result<AnalysisResult> {
    let image = ImageInput::from_path(path)?;
    provider_for(config)?.analyze(&image)
}

/// SPHIN-33: sample keyframes (and, if enabled, a transcript) from a video and
/// run the configured vision provider against them as a single request.
///
/// Transcription is best-effort: if it isn't enabled, has no key configured,
/// the video has no audio track, or the transcription call fails, analysis
/// proceeds on keyframes alone rather than failing outright -- a transcript
/// is a bonus signal, not a requirement, and the frames are usually
/// sufficient on their own.
pub fn analyze_video_file(
    vision_config: &AnalysisConfig,
    video_config: &crate::video::VideoConfig,
    transcription_config: Option<&crate::transcribe::TranscriptionConfig>,
    path: impl AsRef<Path>,
) -> Result<AnalysisResult> {
    let provider = provider_for(vision_config)?;
    let path = path.as_ref();

    let tmp = tempfile::tempdir()?;
    let frame_paths = crate::video::extract_keyframes(video_config, path, tmp.path())?;
    let frames = frame_paths
        .iter()
        .map(ImageInput::from_path)
        .collect::<Result<Vec<_>>>()?;

    let transcript = transcription_config
        .filter(|tc| tc.enabled)
        .and_then(|tc| {
            let audio_path = tmp.path().join("audio.wav");
            crate::video::extract_audio(video_config, path, &audio_path).ok()?;
            crate::transcribe::transcribe(tc, &audio_path).ok()
        });

    provider.analyze_video(&frames, transcript.as_deref())
}

/// Turn the model's text output into an [`AnalysisResult`]. Tolerates the model
/// wrapping the JSON in prose or a ```` ```json ```` fence (common even when
/// asked not to) by extracting the outermost `{...}` span.
fn parse_analysis_json(
    provider: &'static str,
    model: &str,
    raw: &str,
) -> Result<AnalysisResult> {
    let json = extract_json_object(raw).ok_or_else(|| CoreError::BadResponse {
        provider: provider.to_string(),
        message: format!("no JSON object in model output: {}", truncate(raw, 200)),
    })?;

    let parsed: ModelAnalysis =
        serde_json::from_str(json).map_err(|e| CoreError::BadResponse {
            provider: provider.to_string(),
            message: format!("{e}; output was: {}", truncate(json, 200)),
        })?;

    Ok(AnalysisResult {
        description: parsed.description.trim().to_string(),
        subjects: clean_list(parsed.subjects, false),
        scene: parsed.scene.trim().to_string(),
        mood: parsed.mood.trim().to_string(),
        colors: clean_list(parsed.colors, true),
        keywords: clean_list(parsed.keywords, true),
        editorial: parsed.editorial,
        text_content: parsed.text_content.trim().to_string(),
        model: model.to_string(),
        provider: provider.to_string(),
    })
}

/// Extract the outermost balanced `{...}` span from arbitrary text.
fn extract_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, ch) in s[start..].char_indices() {
        if in_string {
            match ch {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..start + i + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Trim, drop blanks, dedupe (case-insensitively), optionally lowercase.
fn clean_list(items: Vec<String>, lowercase: bool) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in items {
        let v = item.trim();
        if v.is_empty() {
            continue;
        }
        let v = if lowercase { v.to_lowercase() } else { v.to_string() };
        if seen.insert(v.to_lowercase()) {
            out.push(v);
        }
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_detection_is_conservative() {
        assert_eq!(mime_for_extension("JPG"), Some("image/jpeg"));
        assert_eq!(mime_for_extension("png"), Some("image/png"));
        assert_eq!(mime_for_extension("tiff"), None);
        assert_eq!(mime_for_extension("mp4"), None);
    }

    #[test]
    fn image_input_encodes_data_url() {
        let img = ImageInput {
            bytes: b"hi".to_vec(),
            mime: "image/png".into(),
        };
        assert_eq!(img.base64(), "aGk=");
        assert_eq!(img.data_url(), "data:image/png;base64,aGk=");
    }

    #[test]
    fn parses_clean_json() {
        let raw = r#"{"description":"A dog runs.","subjects":["dog"],"scene":"park",
            "mood":"playful","colors":["green"],"keywords":["Dog","dog","running"],
            "editorial":false,"text_content":""}"#;
        let r = parse_analysis_json("openai", "gpt-4o", raw).unwrap();
        assert_eq!(r.description, "A dog runs.");
        assert_eq!(r.keywords, vec!["dog", "running"]); // deduped + lowercased
        assert_eq!(r.model, "gpt-4o");
        assert_eq!(r.provider, "openai");
        assert!(!r.editorial);
    }

    #[test]
    fn parses_json_wrapped_in_a_fence_and_prose() {
        let raw = "Sure! Here is the analysis:\n```json\n{\"description\":\"x\",\
            \"editorial\":true}\n```\nLet me know if you need more.";
        let r = parse_analysis_json("anthropic", "claude", raw).unwrap();
        assert_eq!(r.description, "x");
        assert!(r.editorial);
        assert!(r.subjects.is_empty());
    }

    #[test]
    fn braces_inside_strings_dont_confuse_the_extractor() {
        let raw = r#"{"description":"a sign reading {OPEN}","text_content":"{OPEN}"}"#;
        let r = parse_analysis_json("gemini", "g", raw).unwrap();
        assert_eq!(r.description, "a sign reading {OPEN}");
        assert_eq!(r.text_content, "{OPEN}");
    }

    #[test]
    fn non_json_output_is_a_bad_response_error() {
        let err = parse_analysis_json("openai", "gpt-4o", "I can't help with that.")
            .unwrap_err();
        assert!(matches!(err, CoreError::BadResponse { .. }));
    }

    #[test]
    fn provider_for_requires_an_api_key() {
        let cfg = AnalysisConfig::new(ProviderKind::OpenAi);
        assert!(matches!(provider_for(&cfg), Err(CoreError::Config(_))));
    }
}
