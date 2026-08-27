//! SPHIN-3: turn a [`crate::analysis::AnalysisResult`] into marketable
//! title/description/keywords that respect a stock site's [`LimiterProfile`].
//!
//! Deliberately template-based rather than another model call: the vision
//! analysis (SPHIN-2) already extracted every fact worth saying, so this step
//! is pure, offline, and instant -- no API key needed to go from analysis to
//! submittable metadata.

mod limiter;

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

pub use limiter::LimiterProfile;

use crate::analysis::AnalysisResult;

/// Generated title/description/keywords, ready to embed (SPHIN-4) or upload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratedMetadata {
    pub title: String,
    pub description: String,
    pub keywords: Vec<String>,
    /// Name of the [`LimiterProfile`] this was generated for.
    pub profile: String,
    /// False if the analysis didn't yield enough distinct keywords to reach
    /// the profile's `min_keywords` -- surfaced so the UI can prompt the user
    /// to add more manually rather than silently under-submitting.
    pub meets_minimum_keywords: bool,
}

/// Generate metadata for `analysis`, fit to `profile`'s limits.
pub fn generate(analysis: &AnalysisResult, profile: &LimiterProfile) -> GeneratedMetadata {
    let title = build_title(analysis, profile.max_title_chars);
    let description = build_description(analysis, profile.max_description_chars);
    let keywords = build_keywords(analysis, profile);
    let meets_minimum_keywords = keywords.len() >= profile.min_keywords;

    GeneratedMetadata {
        title,
        description,
        keywords,
        profile: profile.name.clone(),
        meets_minimum_keywords,
    }
}

/// A short, marketable title: the factual description, trimmed to fit.
fn build_title(analysis: &AnalysisResult, max_chars: usize) -> String {
    let base = if !analysis.description.trim().is_empty() {
        analysis.description.clone()
    } else {
        fallback_phrase(analysis)
    };
    let cleaned = base.trim().trim_end_matches('.').to_string();
    truncate_at_word_boundary(&cleaned, max_chars)
}

/// The full description, extended with scene/mood when the analysis
/// sentence alone leaves room, trimmed to fit.
fn build_description(analysis: &AnalysisResult, max_chars: usize) -> String {
    let mut parts = vec![analysis.description.trim().to_string()];
    if !analysis.scene.trim().is_empty() {
        parts.push(format!("Set in {}.", analysis.scene.trim()));
    }
    if !analysis.mood.trim().is_empty() {
        parts.push(format!("A {} atmosphere.", analysis.mood.trim()));
    }
    let full = parts
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let full = if full.is_empty() {
        fallback_phrase(analysis)
    } else {
        full
    };
    truncate_at_word_boundary(&full, max_chars)
}

/// Merge every candidate keyword source, most-relevant first, deduped
/// case-insensitively, and fit to the profile's count/length limits.
fn build_keywords(analysis: &AnalysisResult, profile: &LimiterProfile) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    let candidates = analysis
        .keywords
        .iter()
        .chain(analysis.subjects.iter())
        .chain(analysis.colors.iter())
        .chain(std::iter::once(&analysis.scene))
        .chain(std::iter::once(&analysis.mood));

    for candidate in candidates {
        for word in split_candidate(candidate) {
            let trimmed = word.trim().to_lowercase();
            if trimmed.is_empty() || trimmed.chars().count() > profile.max_keyword_chars {
                continue;
            }
            if seen.insert(trimmed.clone()) {
                out.push(trimmed);
                if out.len() >= profile.max_keywords {
                    return out;
                }
            }
        }
    }
    out
}

/// Scene/mood can be multi-word ("busy city street"); emit the phrase itself
/// plus its individual words so both broad and specific search terms are
/// covered by the keyword list.
fn split_candidate(s: &str) -> Vec<String> {
    let s = s.trim();
    if s.is_empty() {
        return Vec::new();
    }
    let mut out = vec![s.to_string()];
    if s.contains(' ') {
        out.extend(s.split_whitespace().map(|w| w.to_string()));
    }
    out
}

fn fallback_phrase(analysis: &AnalysisResult) -> String {
    let mut parts = analysis.subjects.clone();
    if !analysis.scene.trim().is_empty() {
        parts.push(analysis.scene.clone());
    }
    if parts.is_empty() {
        "Untitled".to_string()
    } else {
        parts.join(", ")
    }
}

/// Truncate to at most `max_chars` characters without cutting a word in half,
/// unless a single word alone exceeds the limit (then hard-cut that word).
fn truncate_at_word_boundary(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out = String::new();
    for word in s.split_whitespace() {
        let candidate_len = if out.is_empty() {
            word.chars().count()
        } else {
            out.chars().count() + 1 + word.chars().count()
        };
        if candidate_len > max_chars {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.is_empty() {
        out = s.chars().take(max_chars.max(1)).collect();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_analysis() -> AnalysisResult {
        AnalysisResult {
            description: "A golden retriever runs across a sunlit park.".to_string(),
            subjects: vec!["golden retriever".to_string(), "park".to_string()],
            scene: "sunny park".to_string(),
            mood: "playful".to_string(),
            colors: vec!["green".to_string(), "gold".to_string()],
            keywords: vec!["dog".to_string(), "running".to_string(), "outdoors".to_string()],
            editorial: false,
            text_content: String::new(),
            model: "gpt-4o".to_string(),
            provider: "openai".to_string(),
        }
    }

    #[test]
    fn generates_title_description_and_keywords() {
        let analysis = sample_analysis();
        let profile = LimiterProfile::custom("Test");
        let meta = generate(&analysis, &profile);

        assert_eq!(meta.title, "A golden retriever runs across a sunlit park");
        assert!(meta.description.contains("sunlit park"));
        assert!(meta.description.contains("Set in sunny park."));
        assert!(meta.description.contains("playful atmosphere"));
        assert!(meta.keywords.contains(&"dog".to_string()));
        assert!(meta.keywords.contains(&"golden retriever".to_string()));
        assert!(meta.keywords.contains(&"green".to_string()));
        assert_eq!(meta.profile, "Test");
    }

    #[test]
    fn title_respects_max_chars_at_a_word_boundary() {
        let analysis = sample_analysis();
        let mut profile = LimiterProfile::custom("Tight");
        profile.max_title_chars = 20;
        let meta = generate(&analysis, &profile);
        assert!(meta.title.chars().count() <= 20);
        assert!(!meta.title.ends_with(' '));
        // Should not have chopped a word in half.
        assert_eq!(meta.title, "A golden retriever");
    }

    #[test]
    fn a_single_overlong_word_is_hard_cut() {
        assert_eq!(truncate_at_word_boundary("Supercalifragilisticexpialidocious", 10).chars().count(), 10);
    }

    #[test]
    fn keywords_are_deduped_case_insensitively_and_capped() {
        let mut analysis = sample_analysis();
        analysis.keywords = vec!["Dog".to_string(), "dog".to_string(), "DOG".to_string()];
        let mut profile = LimiterProfile::custom("Capped");
        profile.max_keywords = 3;
        let meta = generate(&analysis, &profile);
        assert_eq!(meta.keywords.iter().filter(|k| *k == "dog").count(), 1);
        assert_eq!(meta.keywords.len(), 3);
    }

    #[test]
    fn overlong_keywords_are_dropped() {
        let mut analysis = sample_analysis();
        analysis.keywords = vec!["a".repeat(60)];
        let profile = LimiterProfile::custom("Test");
        let meta = generate(&analysis, &profile);
        assert!(!meta.keywords.iter().any(|k| k.len() > profile.max_keyword_chars));
    }

    #[test]
    fn meets_minimum_keywords_flag_reflects_the_profile_floor() {
        let mut analysis = sample_analysis();
        analysis.keywords = vec![];
        analysis.subjects = vec![];
        analysis.colors = vec![];
        analysis.scene = String::new();
        analysis.mood = String::new();
        let mut profile = LimiterProfile::custom("Strict");
        profile.min_keywords = 5;
        let meta = generate(&analysis, &profile);
        assert!(meta.keywords.is_empty());
        assert!(!meta.meets_minimum_keywords);
    }

    #[test]
    fn falls_back_to_subjects_and_scene_when_description_is_empty() {
        let mut analysis = sample_analysis();
        analysis.description = String::new();
        let profile = LimiterProfile::custom("Test");
        let meta = generate(&analysis, &profile);
        assert_eq!(meta.title, "golden retriever, park, sunny park");
    }
}
