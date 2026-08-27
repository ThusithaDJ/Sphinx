//! Shared HTTP plumbing for keyword-suggestion connectors: one blocking
//! client, a `GET -> json` helper, uniform mapping of transport / non-2xx /
//! bad-body failures onto [`CoreError`]. Mirrors `analysis::http`, kept
//! separate since GET-with-query-params is a different shape than the POST
//! JSON bodies the vision providers send.

use std::time::Duration;

use serde_json::Value;

use crate::error::{CoreError, Result};

/// Build a blocking client with a connect + total timeout.
pub fn client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .user_agent(concat!("sphinx/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|source| CoreError::Http {
            provider: "http".to_string(),
            source,
        })
}

/// GET `url` with `query` params and `headers`, returning the parsed JSON
/// response. A non-2xx status becomes [`CoreError::Provider`] carrying a
/// trimmed excerpt of the error body.
pub fn get_json(
    client: &reqwest::blocking::Client,
    provider: &'static str,
    url: &str,
    query: &[(&str, &str)],
    headers: &[(&str, &str)],
) -> Result<Value> {
    let mut req = client.get(url).query(query);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }

    let resp = req.send().map_err(|source| CoreError::Http {
        provider: provider.to_string(),
        source,
    })?;

    let status = resp.status();
    let text = resp.text().map_err(|source| CoreError::Http {
        provider: provider.to_string(),
        source,
    })?;

    if !status.is_success() {
        return Err(CoreError::Provider {
            provider: provider.to_string(),
            status: status.as_u16(),
            message: excerpt(&text, 400),
        });
    }

    serde_json::from_str(&text).map_err(|e| CoreError::BadResponse {
        provider: provider.to_string(),
        message: format!("response was not JSON: {e}"),
    })
}

fn excerpt(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "…"
    }
}
