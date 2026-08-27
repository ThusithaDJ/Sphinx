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
}
