//! Adobe Stock keyword suggestions (SPHIN-23): search the seed term via the
//! Adobe Stock Search/Files API, requesting the `keywords` result column,
//! and rank that field across the top results the same way the
//! Shutterstock connector does (see [`super::rank_keywords`]).

use serde::Deserialize;

use crate::error::{CoreError, Result};

use super::{http, non_empty, rank_keywords, KeywordProvider};

const DEFAULT_BASE_URL: &str = "https://stock.adobe.io/Rest/Media/1";

pub struct AdobeStockProvider {
    client: reqwest::blocking::Client,
    api_key: String,
    base_url: String,
}

impl AdobeStockProvider {
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
        format!("{}/Search/Files", self.base_url)
    }

    /// The query params for a search, requesting the `keywords` result
    /// column. Pure so it's assertable without a network call.
    fn query(&self, seed: &str, limit: usize) -> Vec<(String, String)> {
        vec![
            ("search_parameters[words]".to_string(), seed.to_string()),
            (
                "search_parameters[limit]".to_string(),
                limit.clamp(1, 64).to_string(),
            ),
            ("result_columns[]".to_string(), "keywords".to_string()),
        ]
    }
}

#[derive(Debug, Default, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    files: Vec<FileResult>,
}

#[derive(Debug, Deserialize)]
struct FileResult {
    #[serde(default)]
    keywords: Vec<String>,
}

impl KeywordProvider for AdobeStockProvider {
    fn suggest(&self, seed: &str, limit: usize) -> Result<Vec<String>> {
        let query = self.query(seed, limit.max(20));
        let query_refs: Vec<(&str, &str)> =
            query.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        let body = http::get_json(
            &self.client,
            self.name(),
            &self.endpoint(),
            &query_refs,
            &[
                ("x-api-key", self.api_key.as_str()),
                ("x-product", "Sphinx/1.0"),
            ],
        )?;
        let parsed: SearchResponse =
            serde_json::from_value(body).map_err(|e| CoreError::BadResponse {
                provider: self.name().to_string(),
                message: e.to_string(),
            })?;
        Ok(rank_keywords(parsed.files.into_iter().map(|f| f.keywords), seed, limit))
    }

    fn name(&self) -> &'static str {
        "adobe_stock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> AdobeStockProvider {
        AdobeStockProvider::new("key123", None).unwrap()
    }

    #[test]
    fn endpoint_defaults_to_the_public_api() {
        assert_eq!(
            provider().endpoint(),
            "https://stock.adobe.io/Rest/Media/1/Search/Files"
        );
    }

    #[test]
    fn endpoint_respects_base_url_override() {
        let p = AdobeStockProvider::new("k", Some("https://gateway.local/adobe/")).unwrap();
        assert_eq!(p.endpoint(), "https://gateway.local/adobe/Search/Files");
    }

    #[test]
    fn query_requests_the_keywords_column_and_carries_the_seed() {
        let q = provider().query("dog", 5);
        assert!(q.contains(&("result_columns[]".to_string(), "keywords".to_string())));
        assert!(q.contains(&("search_parameters[words]".to_string(), "dog".to_string())));
    }

    #[test]
    fn query_clamps_the_limit() {
        let q = provider().query("dog", 500);
        assert!(q.contains(&("search_parameters[limit]".to_string(), "64".to_string())));
    }

    #[test]
    fn response_parsing_extracts_keywords_from_each_file() {
        let raw = r#"{"files":[{"keywords":["dog","park"]},{"keywords":["dog"]}]}"#;
        let parsed: SearchResponse = serde_json::from_str(raw).unwrap();
        let ranked = rank_keywords(parsed.files.into_iter().map(|f| f.keywords), "unrelated", 10);
        assert_eq!(ranked[0], "dog");
    }
}
