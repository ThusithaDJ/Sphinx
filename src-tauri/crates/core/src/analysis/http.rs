//! Shared HTTP plumbing for the vision providers: one blocking client per
//! provider instance, a `POST json -> json` helper, and uniform mapping of
//! transport / non-2xx / bad-body failures onto [`CoreError`].

use std::time::Duration;

use serde_json::Value;

use crate::error::{CoreError, Result};

/// Build a blocking client with a connect + total timeout.
pub fn client(timeout: Duration) -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(15))
        .user_agent(concat!("sphinx/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|source| CoreError::Http {
            provider: "http".to_string(),
            source,
        })
}

/// POST `body` as JSON to `url` with extra `headers`, returning the parsed JSON
/// response. A non-2xx status becomes [`CoreError::Provider`] carrying a trimmed
/// excerpt of the error body.
pub fn post_json(
    client: &reqwest::blocking::Client,
    provider: &'static str,
    url: &str,
    headers: &[(&str, &str)],
    body: &Value,
) -> Result<Value> {
    let mut req = client.post(url).json(body);
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

/// Pull a required string out of a JSON response by path, e.g.
/// `&["choices", "0", "message", "content"]`. Numeric segments index arrays.
pub fn dig_str<'a>(
    provider: &'static str,
    value: &'a Value,
    path: &[&str],
) -> Result<&'a str> {
    let mut cur = value;
    for seg in path {
        cur = match seg.parse::<usize>() {
            Ok(idx) => cur.get(idx),
            Err(_) => cur.get(*seg),
        }
        .ok_or_else(|| CoreError::BadResponse {
            provider: provider.to_string(),
            message: format!(
                "missing `{}` in response: {}",
                path.join("."),
                excerpt(&value.to_string(), 300)
            ),
        })?;
    }
    cur.as_str().ok_or_else(|| CoreError::BadResponse {
        provider: provider.to_string(),
        message: format!("`{}` was not a string", path.join(".")),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dig_str_walks_objects_and_arrays() {
        let v = json!({"choices":[{"message":{"content":"hello"}}]});
        assert_eq!(
            dig_str("openai", &v, &["choices", "0", "message", "content"]).unwrap(),
            "hello"
        );
    }

    #[test]
    fn dig_str_reports_a_helpful_missing_path() {
        let v = json!({"choices":[]});
        let err = dig_str("openai", &v, &["choices", "0", "message"]).unwrap_err();
        match err {
            CoreError::BadResponse { message, .. } => assert!(message.contains("choices.0.message")),
            other => panic!("unexpected: {other:?}"),
        }
    }
}
