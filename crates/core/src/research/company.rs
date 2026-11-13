//! Explicit, temporary public-company retrieval. Never reads a portfolio or writes storage.
use std::time::Duration;

use chrono::Utc;
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Semaphore;

use crate::{Error, Result};

const MAX_BYTES: usize = 12 * 1024 * 1024;
pub(super) static SLOT: Semaphore = Semaphore::const_new(1);
const AGENT: &str = "Oiloftrop/0.1 https://github.com/garvgulati/oiloftrop";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompanyRequest {
    pub symbol: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanyData {
    pub submissions: Value,
    pub facts: Value,
    pub retrieved_at: String,
}

fn failure(code: &str) -> Error {
    Error::ConstraintViolation(code.into())
}

fn identifier(raw: &str) -> Result<String> {
    let value = raw.trim().to_ascii_uppercase();
    if value.is_empty()
        || value.len() > 20
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        || !value.bytes().any(|b| b.is_ascii_alphanumeric())
        || (value.bytes().all(|b| b.is_ascii_digit())
            && (value.len() > 10 || value.parse::<u64>().unwrap_or(0) == 0))
    {
        return Err(failure("COMPANY_IDENTIFIER_INVALID"));
    }
    Ok(value)
}

pub(super) fn source_client() -> reqwest::ClientBuilder {
    wealthfolio_http::client_builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .user_agent(AGENT)
}

pub(super) async fn decode(mut response: Response, limit: usize) -> Result<Value> {
    if !response.status().is_success() {
        return Err(failure(match response.status().as_u16() {
            404 => "COMPANY_SOURCE_NOT_FOUND",
            429 => "COMPANY_SOURCE_RATE_LIMITED",
            _ => "COMPANY_SOURCE_UNAVAILABLE",
        }));
    }
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(failure("COMPANY_SOURCE_TOO_LARGE"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| failure("COMPANY_SOURCE_UNAVAILABLE"))?
    {
        if bytes.len() + chunk.len() > limit {
            return Err(failure("COMPANY_SOURCE_TOO_LARGE"));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| failure("COMPANY_SOURCE_INVALID"))
}

async fn fetch(client: &Client, url: String, limit: usize) -> Result<Value> {
    // URLs are host-constructed below, never supplied by the browser.
    tokio::time::sleep(Duration::from_millis(150)).await;
    let response = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|_| failure("COMPANY_SOURCE_UNAVAILABLE"))?;
    decode(response, limit).await
}

fn resolve_ticker(directory: &Value, symbol: &str) -> Result<String> {
    directory
        .as_object()
        .and_then(|rows| {
            rows.values().find_map(|row| {
                if row.get("ticker")?.as_str()? != symbol {
                    return None;
                }
                let cik = row.get("cik_str")?.as_u64()?;
                (cik > 0 && cik < 10_000_000_000).then(|| format!("{cik:010}"))
            })
        })
        .ok_or_else(|| failure("COMPANY_TICKER_NOT_FOUND"))
}

/// A single bounded request chain: no retries, cache, background refresh or DB access.
pub async fn retrieve_company(request: CompanyRequest) -> Result<CompanyData> {
    let symbol = identifier(&request.symbol)?;
    if std::env::var_os("WEALTHFOLIO_E2E").is_some()
        && std::env::var("WF_RESEARCH_PUBLIC_REQUESTS_ENABLED").as_deref() != Ok("true")
    {
        return Err(failure("COMPANY_NETWORK_DISABLED_IN_TEST"));
    }
    let _permit = SLOT
        .try_acquire()
        .map_err(|_| failure("COMPANY_REQUEST_BUSY"))?;
    let client = source_client()
        .build()
        .map_err(|_| failure("COMPANY_SOURCE_UNAVAILABLE"))?;
    let operation = async {
        let cik = if symbol.bytes().all(|b| b.is_ascii_digit()) {
            format!("{:0>10}", symbol)
        } else {
            let directory = fetch(
                &client,
                "https://www.sec.gov/files/company_tickers.json".into(),
                2 * 1024 * 1024,
            )
            .await?;
            resolve_ticker(&directory, &symbol)?
        };
        let submissions = fetch(
            &client,
            format!("https://data.sec.gov/submissions/CIK{cik}.json"),
            MAX_BYTES,
        )
        .await?;
        let facts = fetch(
            &client,
            format!("https://data.sec.gov/api/xbrl/companyfacts/CIK{cik}.json"),
            MAX_BYTES,
        )
        .await?;
        if submissions
            .get("cik")
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<u64>().ok())
            != cik.parse::<u64>().ok()
            || facts.get("cik").and_then(Value::as_u64) != cik.parse::<u64>().ok()
            || !facts.get("facts").is_some_and(Value::is_object)
        {
            return Err(failure("COMPANY_IDENTITY_MISMATCH"));
        }
        Ok(CompanyData {
            submissions,
            facts,
            retrieved_at: Utc::now().to_rfc3339(),
        })
    };
    tokio::time::timeout(Duration::from_secs(48), operation)
        .await
        .map_err(|_| failure("COMPANY_REQUEST_TIMEOUT"))?
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "synthetic public source fixtures")]
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn identifier_and_public_request_exclude_urls_and_private_fields() {
        for value in [
            "https://example.com",
            "ONGC/../../",
            "",
            "0",
            "12345678901",
            "---",
            "A:B",
        ] {
            assert!(identifier(value).is_err());
        }
        assert_eq!(identifier(" msft ").unwrap(), "MSFT");
        assert_eq!(identifier("BRK-B").unwrap(), "BRK-B");
        assert!(serde_json::from_value::<CompanyRequest>(
            serde_json::json!({"symbol":"MSFT","portfolio":{"cash":100}})
        )
        .is_err());
        assert!(serde_json::from_value::<CompanyRequest>(
            serde_json::json!({"symbol":"MSFT","apiKey":"canary"})
        )
        .is_err());
    }

    #[test]
    fn ticker_resolution_requires_exact_symbol_and_valid_cik() {
        let rows = serde_json::json!({"0":{"ticker":"MSFT","cik_str":789019},"1":{"ticker":"BAD","cik_str":0}});
        assert_eq!(resolve_ticker(&rows, "MSFT").unwrap(), "0000789019");
        assert!(resolve_ticker(&rows, "MS").is_err());
        assert!(resolve_ticker(&rows, "BAD").is_err());
    }

    async fn response(raw: &'static str) -> Response {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request).await;
            stream.write_all(raw.as_bytes()).await.unwrap();
        });
        source_client()
            .https_only(false)
            .no_proxy()
            .build()
            .unwrap()
            .get(format!("http://{address}"))
            .send()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn rejects_redirects_errors_invalid_and_oversized_sources() {
        for raw in [
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/\r\nContent-Length: 0\r\n\r\n",
            "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 100000\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nbad",
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n9\r\n123456789\r\n0\r\n\r\n",
        ] {
            assert!(decode(response(raw).await, 8).await.is_err());
        }
        assert_eq!(
            decode(
                response("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}").await,
                8
            )
            .await
            .unwrap(),
            serde_json::json!({})
        );
    }
}
