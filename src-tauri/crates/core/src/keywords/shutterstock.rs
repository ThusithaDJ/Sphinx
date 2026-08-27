//! Shutterstock keyword suggestions (SPHIN-22): search the seed term via
//! Shutterstock's public Images Search API and rank the `keywords` field
//! across the top results (see [`super::rank_keywords`]).

use serde::Deserialize;

use crate::error::{CoreError, Result};

use super::{http, non_empty, rank_keywords, KeywordProvider};

const DEFAULT_BASE_URL: &str = "https://api.shutterstock.com/v2";

pub struct ShutterstockProvider {
    client: reqwest::blocking::Client,
    api_key: String,
    base_url: String,
}

impl ShutterstockProvider {
    pub fn new(api_key: impl Into<String>, base_url: Option<&str>) -> Result<Self> {
        Ok(Self {
            client: http::client()?,
            api_key: api_key.into(),
            base_url: base_url
                .and_then(non_empty)
                .map(|s| s.trim_end_matches('/').to_string())
                .unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
        })
    }

    fn endpoint(&self) -> String {
        format!("{}/images/search", self.base_url)
    }

    /// The query params for a search. Pure so it's assertable without a
    /// network call.
    fn query(&self, seed: &str, per_page: usize) -> Vec<(String, String)> {
        vec![
            ("query".to_string(), seed.to_string()),
            ("per_page".to_string(), per_page.clamp(1, 100).to_string()),
        ]
    }
}

#[derive(Debug, Default, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    data: Vec<ImageResult>,
}

#[derive(Debug, Deserialize)]
struct ImageResult {
    #[serde(default)]
    keywords: Vec<String>,
}

impl KeywordProvider for ShutterstockProvider {
    fn suggest(&self, seed: &str, limit: usize) -> Result<Vec<String>> {
        let query = self.query(seed, limit.max(20));
        let query_refs: Vec<(&str, &str)> =
            query.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let auth = format!("Bearer {}", self.api_key);

        let body = http::get_json(
            &self.client,
            self.name(),
            &self.endpoint(),
            &query_refs,
            &[("authorization", auth.as_str())],
        )?;
        let parsed: SearchResponse =
            serde_json::from_value(body).map_err(|e| CoreError::BadResponse {
                provider: self.name().to_string(),
                message: e.to_string(),
            })?;
        Ok(rank_keywords(parsed.data.into_iter().map(|r| r.keywords), seed, limit))
    }

    fn name(&self) -> &'static str {
        "shutterstock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> ShutterstockProvider {
        ShutterstockProvider::new("token123", None).unwrap()
    }

    #[test]
    fn endpoint_defaults_to_the_public_api() {
        assert_eq!(
            provider().endpoint(),
            "https://api.shutterstock.com/v2/images/search"
        );
    }

    #[test]
    fn endpoint_respects_base_url_override() {
        let p = ShutterstockProvider::new("t", Some("https://gateway.local/shutterstock/")).unwrap();
        assert_eq!(p.endpoint(), "https://gateway.local/shutterstock/images/search");
    }

    #[test]
    fn query_carries_the_seed_and_a_clamped_page_size() {
        let q = provider().query("dog park", 500);
        assert!(q.contains(&("query".to_string(), "dog park".to_string())));
        assert!(q.contains(&("per_page".to_string(), "100".to_string())));
    }

    #[test]
    fn response_parsing_extracts_keywords_from_each_result() {
        let raw = r#"{"data":[{"keywords":["dog","park"]},{"keywords":["dog","sunny"]}]}"#;
        let parsed: SearchResponse = serde_json::from_str(raw).unwrap();
        let ranked = rank_keywords(parsed.data.into_iter().map(|r| r.keywords), "unrelated", 10);
        assert_eq!(ranked[0], "dog");
    }
}
