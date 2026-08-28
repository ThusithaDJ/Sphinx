//! SPHIN-16: the structured content-description prompt.
//!
//! One prompt, shared across every provider, that asks for strict JSON matching
//! [`crate::analysis::AnalysisResult`]. Kept in its own module so the wording
//! can be iterated on (and diffed) without touching provider code.

/// Sent as the system / developer message.
pub const SYSTEM: &str = "You are a stock-media metadata expert. You examine an \
image and report only factual, verifiable observations useful for cataloguing \
and licensing it on stock marketplaces such as Shutterstock, Adobe Stock and \
Getty. You never invent details you cannot actually see, and you never add \
marketing or promotional language.";

/// The task instruction, sent alongside the image. Describes the exact JSON
/// shape expected.
pub const INSTRUCTION: &str = r#"Analyze the attached image for stock licensing.

Respond with a SINGLE JSON object and nothing else — no prose, no code fence.
It must match exactly this shape:

{
  "description": string,   // one or two factual, present-tense sentences; no marketing words
  "subjects": [string],    // concrete things visible, most prominent first, 1-8 items
  "scene": string,         // the setting or environment, a few words
  "mood": string,          // overall mood/atmosphere, one or two words
  "colors": [string],      // dominant colours as plain names, 1-5 items
  "keywords": [string],    // 15-40 lowercase search tags a buyer might use; one or two words each; concrete terms before abstract ones; no duplicates, no sentences
  "editorial": boolean,    // true if it shows recognizable real people, logos, brands, artwork or private property that would restrict commercial use
  "text_content": string   // any clearly legible text in the image, else ""
}

Rules:
- Describe only what is actually visible in this image.
- Every keyword must be relevant enough that someone searching that term would be satisfied to find this image.
- Do not include camera settings, file names, resolutions or watermark text."#;

/// The JSON shape both the image and video instructions ask for -- kept in
/// one place so the two prompts can't quietly drift apart.
const JSON_SHAPE: &str = r#"{
  "description": string,   // one or two factual, present-tense sentences; no marketing words
  "subjects": [string],    // concrete things visible, most prominent first, 1-8 items
  "scene": string,         // the setting or environment, a few words
  "mood": string,          // overall mood/atmosphere, one or two words
  "colors": [string],      // dominant colours as plain names, 1-5 items
  "keywords": [string],    // 15-40 lowercase search tags a buyer might use; one or two words each; concrete terms before abstract ones; no duplicates, no sentences
  "editorial": boolean,    // true if it shows recognizable real people, logos, brands, artwork or private property that would restrict commercial use
  "text_content": string   // any clearly legible text visible, else ""
}"#;

/// System message for video analysis (SPHIN-33): the model is shown a
/// sequence of keyframes plus an optional transcript, not a single image.
pub const VIDEO_SYSTEM: &str = "You are a stock-media metadata expert. You examine a sequence of \
keyframes sampled sequentially from a single video, plus an optional transcript of its audio, \
and report only factual, verifiable observations useful for cataloguing and licensing it on \
stock video marketplaces such as Shutterstock, Adobe Stock and Getty. You never invent details \
you cannot actually see or hear, and you never add marketing or promotional language.";

/// Build the full instruction for a project, appending any project-specific
/// guidance (`AnalysisConfig::prompt_extra`).
pub fn build_instruction(prompt_extra: &str) -> String {
    let extra = prompt_extra.trim();
    if extra.is_empty() {
        INSTRUCTION.to_string()
    } else {
        format!("{INSTRUCTION}\n\nProject-specific guidance:\n{extra}")
    }
}

/// Build the full instruction for a video analysis request: `frame_count`
/// keyframes plus an optional transcript, followed by the same JSON-shape
/// contract the image prompt uses, plus any project-specific guidance.
pub fn build_video_instruction(
    prompt_extra: &str,
    frame_count: usize,
    transcript: Option<&str>,
) -> String {
    let mut s = format!(
        "You are given {frame_count} keyframes sampled sequentially from a single video, in \
chronological order. Analyze the video as a whole (not each frame individually) for stock \
licensing.\n\nRespond with a SINGLE JSON object and nothing else — no prose, no code fence. \
It must match exactly this shape:\n\n{JSON_SHAPE}\n\nRules:\n\
- Describe only what is actually visible across the frames, or audible in the transcript if one is provided.\n\
- Every keyword must be relevant enough that someone searching that term would be satisfied to find this video.\n\
- Do not include camera settings, file names, resolutions or watermark text."
    );

    if let Some(t) = transcript {
        let t = t.trim();
        if !t.is_empty() {
            s.push_str(&format!(
                "\n\nAudio transcript of the video:\n\"\"\"\n{t}\n\"\"\""
            ));
        }
    }

    let extra = prompt_extra.trim();
    if !extra.is_empty() {
        s.push_str(&format!("\n\nProject-specific guidance:\n{extra}"));
    }

    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instruction_is_returned_verbatim_without_extra() {
        assert_eq!(build_instruction("   "), INSTRUCTION);
    }

    #[test]
    fn extra_guidance_is_appended() {
        let out = build_instruction("Prefer species names.");
        assert!(out.starts_with(INSTRUCTION));
        assert!(out.contains("Project-specific guidance:\nPrefer species names."));
    }

    #[test]
    fn video_instruction_mentions_the_frame_count_and_json_shape() {
        let out = build_video_instruction("", 5, None);
        assert!(out.contains("5 keyframes"));
        assert!(out.contains("\"keywords\": [string]"));
        assert!(!out.contains("Audio transcript"));
    }

    #[test]
    fn video_instruction_includes_a_non_empty_transcript() {
        let out = build_video_instruction("", 3, Some("Hello world."));
        assert!(out.contains("Audio transcript of the video"));
        assert!(out.contains("Hello world."));
    }

    #[test]
    fn video_instruction_omits_a_blank_transcript() {
        let out = build_video_instruction("", 3, Some("   "));
        assert!(!out.contains("Audio transcript"));
    }

    #[test]
    fn video_instruction_appends_project_guidance() {
        let out = build_video_instruction("Prefer species names.", 4, None);
        assert!(out.contains("Project-specific guidance:\nPrefer species names."));
    }
}
