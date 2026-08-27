//! SPHIN-5: enrich generated keywords with per-site suggestions pulled from
//! official stock site APIs (Shutterstock, Adobe Stock) rather than
//! scraping.
//!
//! Neither site publishes a dedicated "suggest keywords" endpoint, so each
//! connector mirrors how a contributor would explore related tags by hand:
//! search the seed term via the site's public search API and aggregate the
//! `keywords` field across the top results, ranked by how often each term
//! recurs ([`rank_keywords`]). Request shaping and response parsing are
//! unit-tested offline; no test in this module touches the network.
//!
//! The local cache that keeps repeated lookups from burning through each
//! API's rate limit (SPHIN-24) lives in [`crate::db`] rather than here, the
//! same way [`crate::db`] -- not [`crate::analysis`] -- owns stored
//! analysis results.

mod adobe_stock;
mod http;
mod shutterstock;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub use adobe_stock::AdobeStockProvider;
pub use shutterstock::ShutterstockProvider;

use crate::error::Result;

/// Per-project keyword-enrichment configuration. Both connectors are
/// optional and independent -- either, both, or neither can be enabled, and
/// suggestions from every enabled connector are merged by the caller.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeywordConfig {
    #[serde(default)]
    pub shutterstock: Option<SiteCredentials>,
    #[serde(default)]
    pub adobe_stock: Option<SiteCredentials>,
}

/// API credentials for one stock site's keyword API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteCredentials {
    pub api_key: String,
    /// Base URL override (proxy, test double); empty means "use the
    /// provider default".
    #[serde(default)]
    pub base_url: String,
}

/// A connector that turns a seed keyword/phrase into related suggestions.
pub trait KeywordProvider {
    fn suggest(&self, seed: &str, limit: usize) -> Result<Vec<String>>;
    fn name(&self) -> &'static str;
}

/// `base_url` field as `Some(trimmed)` unless it's blank.
pub(crate) fn non_empty(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

/// Aggregate keyword lists by frequency (case-insensitive), excluding the
/// seed term itself and blank entries, and return the top `limit` most
/// common terms (ties broken alphabetically for a stable order). The first
/// casing seen for a term is preserved in the output.
pub(crate) fn rank_keywords(
    lists: impl Iterator<Item = Vec<String>>,
    seed: &str,
    limit: usize,
) -> Vec<String> {
    let seed_lower = seed.trim().to_lowercase();
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut original_case: HashMap<String, String> = HashMap::new();

    for list in lists {
        for kw in list {
            let trimmed = kw.trim();
            if trimmed.is_empty() {
                continue;
            }
            let lower = trimmed.to_lowercase();
            if lower == seed_lower {
                continue;
            }
            *counts.entry(lower.clone()).or_insert(0) += 1;
            original_case.entry(lower).or_insert_with(|| trimmed.to_string());
        }
    }

    let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
        .into_iter()
        .take(limit)
        .map(|(lower, _)| original_case.remove(&lower).unwrap_or(lower))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_by_frequency_ties_broken_alphabetically() {
        let lists = vec![
            vec!["dog".to_string(), "park".to_string(), "Seed".to_string()],
            vec!["dog".to_string(), "outdoor".to_string()],
            vec!["park".to_string()],
        ];
        let ranked = rank_keywords(lists.into_iter(), "seed", 10);
        assert_eq!(ranked, vec!["dog", "park", "outdoor"]);
    }

    #[test]
    fn dedupes_case_insensitively_preserving_first_casing() {
        let lists = vec![vec!["Dog".to_string(), "dog".to_string(), "DOG".to_string()]];
        let ranked = rank_keywords(lists.into_iter(), "unrelated", 10);
        assert_eq!(ranked, vec!["Dog"]);
    }

    #[test]
    fn respects_the_limit() {
        let lists = vec![vec!["a".to_string(), "b".to_string(), "c".to_string()]];
        assert_eq!(rank_keywords(lists.into_iter(), "seed", 2).len(), 2);
    }

    #[test]
    fn non_empty_treats_whitespace_as_blank() {
        assert_eq!(non_empty("  "), None);
        assert_eq!(non_empty(" x "), Some("x"));
    }
}
