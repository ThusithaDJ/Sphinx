//! SPHIN-19: configurable limiter profiles per stock site (length/count).
//!
//! Every stock site enforces its own title/description length and keyword
//! count limits. A [`LimiterProfile`] captures those limits so metadata
//! generation ([`crate::metadata::generate`]) can produce output that fits
//! without the caller needing to special-case which site is being targeted.

use serde::{Deserialize, Serialize};

/// Length/count limits enforced when generating metadata for a stock site.
///
/// The built-in presets are a reasonable starting point based on each site's
/// publicly documented submission limits at the time of writing, not a
/// guarantee they match current requirements -- every field is a plain
/// number the user can override per project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LimiterProfile {
    /// Display name, e.g. "Shutterstock".
    pub name: String,
    pub max_title_chars: usize,
    pub max_description_chars: usize,
    pub min_keywords: usize,
    pub max_keywords: usize,
    pub max_keyword_chars: usize,
}

impl LimiterProfile {
    /// A blank starting point for a user-defined profile.
    pub fn custom(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            max_title_chars: 200,
            max_description_chars: 200,
            min_keywords: 0,
            max_keywords: 50,
            max_keyword_chars: 50,
        }
    }

    pub fn shutterstock() -> Self {
        Self {
            name: "Shutterstock".to_string(),
            max_title_chars: 200,
            max_description_chars: 200,
            min_keywords: 7,
            max_keywords: 50,
            max_keyword_chars: 50,
        }
    }

    pub fn adobe_stock() -> Self {
        Self {
            name: "Adobe Stock".to_string(),
            max_title_chars: 200,
            max_description_chars: 200,
            min_keywords: 1,
            max_keywords: 49,
            max_keyword_chars: 50,
        }
    }

    pub fn istock() -> Self {
        Self {
            name: "iStock".to_string(),
            max_title_chars: 200,
            max_description_chars: 200,
            min_keywords: 5,
            max_keywords: 50,
            max_keyword_chars: 50,
        }
    }

    pub fn dreamstime() -> Self {
        Self {
            name: "Dreamstime".to_string(),
            max_title_chars: 100,
            max_description_chars: 500,
            min_keywords: 3,
            max_keywords: 50,
            max_keyword_chars: 50,
        }
    }

    pub fn rf_123() -> Self {
        Self {
            name: "123RF".to_string(),
            max_title_chars: 100,
            max_description_chars: 500,
            min_keywords: 3,
            max_keywords: 50,
            max_keyword_chars: 50,
        }
    }

    pub fn pond5() -> Self {
        Self {
            name: "Pond5".to_string(),
            max_title_chars: 80,
            max_description_chars: 500,
            min_keywords: 3,
            max_keywords: 50,
            max_keyword_chars: 50,
        }
    }

    /// Every built-in preset, in the order they should appear in a picker.
    pub fn built_ins() -> Vec<LimiterProfile> {
        vec![
            Self::shutterstock(),
            Self::adobe_stock(),
            Self::istock(),
            Self::dreamstime(),
            Self::rf_123(),
            Self::pond5(),
        ]
    }
}

impl Default for LimiterProfile {
    /// Shutterstock's limits: the most commonly targeted site and a
    /// reasonably strict middle ground among the presets.
    fn default() -> Self {
        Self::shutterstock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_ins_have_distinct_names_and_sane_limits() {
        let profiles = LimiterProfile::built_ins();
        assert_eq!(profiles.len(), 6);
        let mut names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 6, "built-in profile names must be unique");
        for p in &profiles {
            assert!(p.max_title_chars > 0);
            assert!(p.max_description_chars > 0);
            assert!(p.max_keywords >= p.min_keywords);
        }
    }

    #[test]
    fn default_is_shutterstock() {
        assert_eq!(LimiterProfile::default().name, "Shutterstock");
    }

    #[test]
    fn custom_starts_permissive() {
        let p = LimiterProfile::custom("My Site");
        assert_eq!(p.name, "My Site");
        assert_eq!(p.min_keywords, 0);
    }
}
