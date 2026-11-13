//! Read-only Upstox public fundamentals. Only the credential is persisted.
use super::company::{decode, source_client, SLOT};
use crate::{secrets::SecretStore, Error, Result};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// Reuse the already reserved internal namespace. Previous runtimes also block
// generic reads of this logical key, including during a runtime rollback.
pub const CREDENTIAL_KEY: &str = "profile:research:upstox_analytics_v1";

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionRequest {
    pub token: String,
    pub expires_on: String,
    pub free_personal_confirmed: bool,
    pub private_dashboard_confirmed: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub configured: bool,
    pub expires_on: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchRequest {
    pub query: String,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Basis {
    Consolidated,
    Standalone,
}
impl Basis {
    fn name(&self) -> &'static str {
        match self {
            Self::Consolidated => "consolidated",
            Self::Standalone => "standalone",
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalyseRequest {
    pub instrument_key: String,
    pub basis: Basis,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Listing {
    pub name: String,
    pub symbol: String,
    pub isin: String,
    pub key: String,
    pub exchange: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub listings: Vec<Listing>,
    pub retrieved_at: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinancialData {
    pub listing: Listing,
    pub basis: Basis,
    pub income: Value,
    pub cash: Value,
    pub balance: Value,
    pub retrieved_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RatiosRequest {
    pub instrument_key: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RatioEntry {
    pub name: String,
    pub company: Option<f64>,
    pub sector: Option<f64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RatiosData {
    pub listing: Listing,
    pub ratios: Vec<RatioEntry>,
    pub retrieved_at: String,
}

fn ratio_number(value: Option<&Value>, percentage: bool) -> Result<Option<f64>> {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let text = value
        .as_str()
        .filter(|s| s.len() <= 32)
        .ok_or_else(|| failure("INDIA_RATIOS_INVALID"))?
        .trim();
    if ["", "-", "--", "NA", "N/A"].contains(&text) {
        return Ok(None);
    }
    let text = if percentage {
        text.strip_suffix('%').unwrap_or(text)
    } else {
        text
    };
    if !text
        .bytes()
        .all(|b| b.is_ascii_digit() || b".-+".contains(&b))
    {
        return Err(failure("INDIA_RATIOS_INVALID"));
    }
    let number = text
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && v.abs() <= 1e9)
        .ok_or_else(|| failure("INDIA_RATIOS_INVALID"))?;
    Ok(Some(number))
}

fn ratio_entries(raw: &Value) -> Result<Vec<RatioEntry>> {
    let entries = raw
        .get("data")
        .and_then(Value::as_array)
        .filter(|rows| rows.len() <= 12)
        .ok_or_else(|| failure("INDIA_RATIOS_INVALID"))?;
    let mut result: Vec<RatioEntry> = Vec::new();
    for entry in entries {
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| failure("INDIA_RATIOS_INVALID"))?;
        if !["P/E", "P/B", "ROA", "ROE", "ROCE", "EV/EBITDA"].contains(&name) {
            continue;
        }
        if result.iter().any(|row| row.name == name) {
            return Err(failure("INDIA_RATIOS_INVALID"));
        }
        let percentage = ["ROA", "ROE", "ROCE"].contains(&name);
        result.push(RatioEntry {
            name: name.into(),
            company: ratio_number(entry.get("company_value"), percentage)?,
            sector: ratio_number(entry.get("sector_value"), percentage)?,
        });
    }
    Ok(result)
}

/// Explicit public-ratio lookup; no basis or valuation date is inferred.
pub async fn ratios(store: &dyn SecretStore, request: RatiosRequest) -> Result<RatiosData> {
    let (exchange, isin) = request
        .instrument_key
        .split_once("_EQ|")
        .ok_or_else(|| failure("INDIA_IDENTITY_INVALID"))?;
    if !["NSE", "BSE"].contains(&exchange) || !valid_isin(isin) {
        return Err(failure("INDIA_IDENTITY_INVALID"));
    }
    admitted()?;
    let _permit = SLOT
        .try_acquire()
        .map_err(|_| failure("COMPANY_REQUEST_BUSY"))?;
    let config = connection(store)?;
    let client = source_client()
        .user_agent("WealthCompass private Indian research")
        .build()
        .map_err(|_| failure("INDIA_SOURCE_UNAVAILABLE"))?;
    let operation = async {
        let listing = find(&client, &config.token, isin)
            .await?
            .into_iter()
            .find(|row| row.key == request.instrument_key)
            .ok_or_else(|| failure("INDIA_IDENTITY_INVALID"))?;
        let url = reqwest::Url::parse(&format!(
            "https://api.upstox.com/v2/fundamentals/{isin}/key-ratios"
        ))
        .map_err(|_| failure("INDIA_SOURCE_UNAVAILABLE"))?;
        let raw = get(&client, &config.token, url).await?;
        Ok(RatiosData {
            listing,
            ratios: ratio_entries(&raw)?,
            retrieved_at: Utc::now().to_rfc3339(),
        })
    };
    tokio::time::timeout(std::time::Duration::from_secs(35), operation)
        .await
        .map_err(|_| failure("INDIA_REQUEST_TIMEOUT"))?
}

fn failure(code: &str) -> Error {
    Error::ConstraintViolation(code.into())
}
fn valid_connection(value: &ConnectionRequest) -> bool {
    let today = Utc::now()
        .with_timezone(&chrono_tz::Asia::Kolkata)
        .date_naive();
    value.free_personal_confirmed
        && value.private_dashboard_confirmed
        && (20..=4096).contains(&value.token.len())
        && value
            .token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._~-".contains(&b))
        && NaiveDate::parse_from_str(&value.expires_on, "%Y-%m-%d")
            .is_ok_and(|date| date > today && date.to_string() == value.expires_on)
}
fn connection(store: &dyn SecretStore) -> Result<ConnectionRequest> {
    let raw = store
        .get_secret(CREDENTIAL_KEY)
        .map_err(|_| failure("INDIA_CREDENTIAL_UNAVAILABLE"))?
        .ok_or_else(|| failure("INDIA_CONNECTION_REQUIRED"))?;
    let value: ConnectionRequest =
        serde_json::from_str(&raw).map_err(|_| failure("INDIA_CONNECTION_REQUIRED"))?;
    if !valid_connection(&value) {
        return Err(failure("INDIA_CONNECTION_REQUIRED"));
    }
    Ok(value)
}
pub fn connection_status(store: &dyn SecretStore) -> Result<ConnectionStatus> {
    let raw = store
        .get_secret(CREDENTIAL_KEY)
        .map_err(|_| failure("INDIA_CREDENTIAL_UNAVAILABLE"))?;
    let parsed = raw.and_then(|s| serde_json::from_str::<ConnectionRequest>(&s).ok());
    Ok(ConnectionStatus {
        configured: parsed.as_ref().is_some_and(valid_connection),
        expires_on: parsed.map(|s| s.expires_on),
    })
}
pub fn save_connection(
    store: &dyn SecretStore,
    value: ConnectionRequest,
) -> Result<ConnectionStatus> {
    if !valid_connection(&value) {
        return Err(failure("INDIA_CONNECTION_INVALID"));
    }
    let raw = serde_json::to_string(&value).map_err(|_| failure("INDIA_CONNECTION_INVALID"))?;
    store
        .set_secret(CREDENTIAL_KEY, &raw)
        .map_err(|_| failure("INDIA_CREDENTIAL_UNAVAILABLE"))?;
    Ok(ConnectionStatus {
        configured: true,
        expires_on: Some(value.expires_on),
    })
}
pub fn remove_connection(store: &dyn SecretStore) -> Result<ConnectionStatus> {
    store
        .delete_secret(CREDENTIAL_KEY)
        .map_err(|_| failure("INDIA_CREDENTIAL_UNAVAILABLE"))?;
    Ok(ConnectionStatus {
        configured: false,
        expires_on: None,
    })
}
pub fn valid_isin(value: &str) -> bool {
    if value.len() != 12
        || !value.starts_with("INE")
        || !value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        || !value.as_bytes()[11].is_ascii_digit()
    {
        return false;
    }
    let digits = value
        .bytes()
        .flat_map(|b| {
            if b.is_ascii_digit() {
                vec![b - b'0']
            } else {
                vec![(b - b'A' + 10) / 10, (b - b'A' + 10) % 10]
            }
        })
        .collect::<Vec<_>>();
    let total: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, d)| {
            let n = if i % 2 == 1 { *d * 2 } else { *d };
            u32::from(n / 10 + n % 10)
        })
        .sum();
    total.is_multiple_of(10)
}
fn query(value: &str) -> Result<String> {
    let value = value.trim().to_ascii_uppercase();
    if value.is_empty()
        || value.len() > 40
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b" .&^_-".contains(&b))
    {
        return Err(failure("INDIA_QUERY_INVALID"));
    }
    Ok(value)
}
fn listing(row: &Value) -> Option<Listing> {
    let name = row.get("name")?.as_str()?;
    let symbol = row.get("trading_symbol")?.as_str()?;
    let isin = row.get("isin")?.as_str()?;
    let key = row.get("instrument_key")?.as_str()?;
    let exchange = row.get("exchange")?.as_str()?;
    if !["NSE", "BSE"].contains(&exchange)
        || !valid_isin(isin)
        || name.is_empty()
        || name.len() > 200
        || symbol.is_empty()
        || symbol.len() > 30
        || row.get("segment")?.as_str()? != format!("{exchange}_EQ")
        || key != format!("{exchange}_EQ|{isin}")
        || !symbol
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".&^_-".contains(&b))
    {
        return None;
    }
    Some(Listing {
        name: name.into(),
        symbol: symbol.into(),
        isin: isin.into(),
        key: key.into(),
        exchange: exchange.into(),
    })
}
fn admitted() -> Result<()> {
    if std::env::var_os("WEALTHFOLIO_E2E").is_some()
        && std::env::var("WF_RESEARCH_PUBLIC_REQUESTS_ENABLED").as_deref() != Ok("true")
    {
        return Err(failure("INDIA_NETWORK_DISABLED_IN_TEST"));
    }
    Ok(())
}
async fn get(client: &reqwest::Client, token: &str, url: reqwest::Url) -> Result<Value> {
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let response = client
        .get(url)
        .header("Accept", "application/json")
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| failure("INDIA_SOURCE_UNAVAILABLE"))?;
    if matches!(response.status().as_u16(), 401 | 403) {
        return Err(failure("INDIA_AUTH_REQUIRED"));
    }
    let raw = decode(response, 1024 * 1024).await?;
    if raw.get("status").and_then(Value::as_str) != Some("success") {
        return Err(failure("INDIA_SOURCE_INVALID"));
    }
    Ok(raw)
}
async fn find(client: &reqwest::Client, token: &str, value: &str) -> Result<Vec<Listing>> {
    let mut url = reqwest::Url::parse("https://api.upstox.com/v2/instruments/search")
        .map_err(|_| failure("INDIA_SOURCE_UNAVAILABLE"))?;
    url.query_pairs_mut().extend_pairs([
        ("query", value),
        ("exchanges", "NSE,BSE"),
        ("segments", "EQ"),
        ("page_number", "1"),
        ("records", "20"),
    ]);
    let raw = get(client, token, url).await?;
    let rows = raw
        .get("data")
        .and_then(Value::as_array)
        .filter(|rows| rows.len() <= 20)
        .ok_or_else(|| failure("INDIA_SOURCE_INVALID"))?;
    Ok(rows.iter().filter_map(listing).collect())
}
fn summary(data: &Value, endpoint: &str) -> Result<Value> {
    fn period(row: &Value) -> Result<Value> {
        let value = row
            .get("period")
            .and_then(Value::as_str)
            .ok_or_else(|| failure("INDIA_SOURCE_INVALID"))?;
        let months = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        if value.len() != 8
            || !months.contains(&value.get(..3).unwrap_or_default())
            || value.as_bytes()[3] != b' '
            || !value.as_bytes()[4..].iter().all(u8::is_ascii_digit)
        {
            return Err(failure("INDIA_SOURCE_INVALID"));
        }
        Ok(Value::String(value.into()))
    }
    fn amount(row: &Value, key: &str) -> Value {
        row.get(key)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite() && v.abs() <= 1e12)
            .map_or(Value::Null, |v| serde_json::json!(v))
    }
    let mut result =
        serde_json::json!({"type":data["type"],"time_period":"yearly","units_in":"crore"});
    if endpoint == "balance-sheet" {
        let rows = data
            .get("history")
            .and_then(Value::as_array)
            .filter(|v| v.len() <= 30)
            .ok_or_else(|| failure("INDIA_SOURCE_INVALID"))?;
        result["history"]=Value::Array(rows.iter().map(|row|Ok(serde_json::json!({"period":period(row)?,"total_asset":amount(row,"total_asset"),"total_liability":amount(row,"total_liability")}))).collect::<Result<Vec<_>>>()?);
    } else {
        let key = if endpoint == "income-statement" {
            "income_statement"
        } else {
            "cash_flow"
        };
        let categories = if endpoint == "income-statement" {
            ["revenue", "operating_profit", "net_profit"]
        } else {
            ["operating", "investing", "financing"]
        };
        let rows = data
            .get(key)
            .and_then(Value::as_array)
            .filter(|v| v.len() <= 30)
            .ok_or_else(|| failure("INDIA_SOURCE_INVALID"))?;
        let mut output = Vec::new();
        for row in rows {
            let Some(category) = row
                .get("category")
                .and_then(Value::as_str)
                .filter(|v| categories.contains(v))
            else {
                continue;
            };
            let history = row
                .get("history")
                .and_then(Value::as_array)
                .filter(|v| v.len() <= 30)
                .ok_or_else(|| failure("INDIA_SOURCE_INVALID"))?;
            let history = history
                .iter()
                .map(|row| {
                    Ok(serde_json::json!({"period":period(row)?,"value":amount(row,"value")}))
                })
                .collect::<Result<Vec<_>>>()?;
            output.push(serde_json::json!({"category":category,"history":history}));
        }
        result[key] = Value::Array(output);
    }
    Ok(result)
}
pub async fn search(store: &dyn SecretStore, request: SearchRequest) -> Result<SearchResults> {
    let value = query(&request.query)?;
    admitted()?;
    let _permit = SLOT
        .try_acquire()
        .map_err(|_| failure("COMPANY_REQUEST_BUSY"))?;
    let config = connection(store)?;
    let client = source_client()
        .user_agent("WealthCompass private Indian research")
        .build()
        .map_err(|_| failure("INDIA_SOURCE_UNAVAILABLE"))?;
    let listings = find(&client, &config.token, &value).await?;
    Ok(SearchResults {
        listings,
        retrieved_at: Utc::now().to_rfc3339(),
    })
}
pub async fn analyse(store: &dyn SecretStore, request: AnalyseRequest) -> Result<FinancialData> {
    let (exchange, isin) = request
        .instrument_key
        .split_once("_EQ|")
        .ok_or_else(|| failure("INDIA_IDENTITY_INVALID"))?;
    if !["NSE", "BSE"].contains(&exchange) || !valid_isin(isin) {
        return Err(failure("INDIA_IDENTITY_INVALID"));
    }
    admitted()?;
    let _permit = SLOT
        .try_acquire()
        .map_err(|_| failure("COMPANY_REQUEST_BUSY"))?;
    let config = connection(store)?;
    let client = source_client()
        .user_agent("WealthCompass private Indian research")
        .build()
        .map_err(|_| failure("INDIA_SOURCE_UNAVAILABLE"))?;
    let operation = async {
        let listing = find(&client, &config.token, isin)
            .await?
            .into_iter()
            .find(|row| row.key == request.instrument_key)
            .ok_or_else(|| failure("INDIA_IDENTITY_INVALID"))?;
        let mut values = Vec::new();
        for endpoint in ["income-statement", "cash-flow", "balance-sheet"] {
            let mut url = reqwest::Url::parse(&format!(
                "https://api.upstox.com/v2/fundamentals/{isin}/{endpoint}"
            ))
            .map_err(|_| failure("INDIA_SOURCE_UNAVAILABLE"))?;
            url.query_pairs_mut()
                .append_pair("type", request.basis.name());
            if endpoint == "income-statement" {
                url.query_pairs_mut().append_pair("time_period", "yearly");
            }
            let raw = get(&client, &config.token, url).await?;
            let data = raw
                .get("data")
                .filter(|data| {
                    data.get("type").and_then(Value::as_str) == Some(request.basis.name())
                        && data.get("units_in").and_then(Value::as_str) == Some("crore")
                        && data.get("time_period").and_then(Value::as_str) == Some("yearly")
                })
                .ok_or_else(|| failure("INDIA_BASIS_OR_UNITS_INVALID"))?;
            values.push(summary(data, endpoint)?);
        }
        let mut values = values.into_iter();
        Ok(FinancialData {
            listing,
            basis: request.basis,
            income: values
                .next()
                .ok_or_else(|| failure("INDIA_SOURCE_INVALID"))?,
            cash: values
                .next()
                .ok_or_else(|| failure("INDIA_SOURCE_INVALID"))?,
            balance: values
                .next()
                .ok_or_else(|| failure("INDIA_SOURCE_INVALID"))?,
            retrieved_at: Utc::now().to_rfc3339(),
        })
    };
    tokio::time::timeout(std::time::Duration::from_secs(55), operation)
        .await
        .map_err(|_| failure("INDIA_REQUEST_TIMEOUT"))?
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChartRequest {
    pub instrument_key: String,
    pub range: ChartRange,
}
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChartRange {
    Day,
    Month,
    Year,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartCandle {
    pub time: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartQuote {
    pub price: f64,
    pub last_trade_at: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartData {
    pub listing: Listing,
    pub range: ChartRange,
    pub interval: String,
    pub candles: Vec<ChartCandle>,
    pub quote: ChartQuote,
    pub retrieved_at: String,
}
fn chart_candles(raw: &Value, now: chrono::DateTime<Utc>) -> Result<Vec<ChartCandle>> {
    let rows = raw
        .pointer("/data/candles")
        .and_then(Value::as_array)
        .filter(|rows| rows.len() <= 400)
        .ok_or_else(|| failure("INDIA_CHART_INVALID"))?;
    let mut output = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in rows {
        let row = row
            .as_array()
            .filter(|row| (6..=7).contains(&row.len()))
            .ok_or_else(|| failure("INDIA_CHART_INVALID"))?;
        let time = row[0]
            .as_str()
            .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
            .filter(|v| v.timestamp() > 0 && v.timestamp() <= now.timestamp() + 60)
            .ok_or_else(|| failure("INDIA_CHART_INVALID"))?;
        if !seen.insert(time.timestamp()) {
            return Err(failure("INDIA_CHART_INVALID"));
        }
        let number = |i: usize| {
            row[i]
                .as_f64()
                .filter(|v| v.is_finite() && *v >= 0.0 && *v <= 1e15)
                .ok_or_else(|| failure("INDIA_CHART_INVALID"))
        };
        let (open, high, low, close, volume) =
            (number(1)?, number(2)?, number(3)?, number(4)?, number(5)?);
        if low <= 0.0
            || high < low
            || high < open.max(close)
            || low > open.min(close)
            || volume.fract() != 0.0
        {
            return Err(failure("INDIA_CHART_INVALID"));
        }
        output.push(ChartCandle {
            time: time.with_timezone(&Utc).to_rfc3339(),
            open,
            high,
            low,
            close,
            volume,
        });
    }
    output.sort_by(|a, b| a.time.cmp(&b.time));
    Ok(output)
}
fn chart_quote(raw: &Value, key: &str, now: chrono::DateTime<Utc>) -> Result<ChartQuote> {
    let values = raw
        .get("data")
        .and_then(Value::as_object)
        .filter(|v| v.len() == 1)
        .ok_or_else(|| failure("INDIA_CHART_INVALID"))?;
    let row = values
        .values()
        .next()
        .ok_or_else(|| failure("INDIA_CHART_INVALID"))?;
    if row.get("instrument_token").and_then(Value::as_str) != Some(key) {
        return Err(failure("INDIA_IDENTITY_INVALID"));
    }
    let price = row
        .get("last_price")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && *v > 0.0 && *v <= 1e12)
        .ok_or_else(|| failure("INDIA_CHART_INVALID"))?;
    let time = row
        .get("last_trade_time")
        .and_then(Value::as_str)
        .and_then(|v| v.parse::<i64>().ok())
        .and_then(chrono::DateTime::<Utc>::from_timestamp_millis)
        .filter(|v| v.timestamp() > 0 && v.timestamp() <= now.timestamp() + 60)
        .ok_or_else(|| failure("INDIA_CHART_INVALID"))?;
    Ok(ChartQuote {
        price,
        last_trade_at: time.to_rfc3339(),
    })
}
pub async fn chart(store: &dyn SecretStore, request: ChartRequest) -> Result<ChartData> {
    let (exchange, isin) = request
        .instrument_key
        .split_once("_EQ|")
        .ok_or_else(|| failure("INDIA_IDENTITY_INVALID"))?;
    if !["NSE", "BSE"].contains(&exchange) || !valid_isin(isin) {
        return Err(failure("INDIA_IDENTITY_INVALID"));
    }
    admitted()?;
    let _permit = SLOT
        .try_acquire()
        .map_err(|_| failure("COMPANY_REQUEST_BUSY"))?;
    let config = connection(store)?;
    let client = source_client()
        .user_agent("WealthCompass private price chart")
        .build()
        .map_err(|_| failure("INDIA_SOURCE_UNAVAILABLE"))?;
    let operation = async {
        let listing = find(&client, &config.token, isin)
            .await?
            .into_iter()
            .find(|row| row.key == request.instrument_key)
            .ok_or_else(|| failure("INDIA_IDENTITY_INVALID"))?;
        let today = Utc::now()
            .with_timezone(&chrono_tz::Asia::Kolkata)
            .date_naive();
        let mut url = reqwest::Url::parse("https://api.upstox.com/v3/historical-candle/")
            .map_err(|_| failure("INDIA_SOURCE_UNAVAILABLE"))?;
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| failure("INDIA_SOURCE_UNAVAILABLE"))?;
            path.pop_if_empty();
            match request.range {
                ChartRange::Day => {
                    path.push("intraday")
                        .push(&request.instrument_key)
                        .push("minutes")
                        .push("5");
                }
                ChartRange::Month | ChartRange::Year => {
                    let days = if matches!(request.range, ChartRange::Month) {
                        31
                    } else {
                        366
                    };
                    let start = today - chrono::Duration::days(days);
                    path.push(&request.instrument_key)
                        .push("days")
                        .push("1")
                        .push(&today.to_string())
                        .push(&start.to_string());
                }
            }
        }
        let raw = get(&client, &config.token, url).await?;
        let now = Utc::now();
        let candles = chart_candles(&raw, now)?;
        let mut url = reqwest::Url::parse("https://api.upstox.com/v2/market-quote/quotes")
            .map_err(|_| failure("INDIA_SOURCE_UNAVAILABLE"))?;
        url.query_pairs_mut()
            .append_pair("instrument_key", &request.instrument_key);
        let raw = get(&client, &config.token, url).await?;
        let quote = chart_quote(&raw, &request.instrument_key, Utc::now())?;
        Ok(ChartData {
            listing,
            range: request.range,
            interval: if matches!(request.range, ChartRange::Day) {
                "5 minutes"
            } else {
                "1 day"
            }
            .into(),
            candles,
            quote,
            retrieved_at: Utc::now().to_rfc3339(),
        })
    };
    tokio::time::timeout(std::time::Duration::from_secs(35), operation)
        .await
        .map_err(|_| failure("INDIA_REQUEST_TIMEOUT"))?
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "isolated credential canaries, no real credentials"
    )]
    use super::*;
    #[test]
    fn ratios_preserve_percent_units_missing_values_and_negative_values() {
        let raw = serde_json::json!({"data":[
            {"name":"P/E","company_value":"20.15","sector_value":"--"},
            {"name":"ROE","company_value":"-4.39%","sector_value":"7.54%"},
            {"name":"P/B","company_value":null},
            {"name":"Future ratio","company_value":"ignored"}
        ]});
        let rows = ratio_entries(&raw).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].company, Some(20.15));
        assert_eq!(rows[0].sector, None);
        assert_eq!(rows[1].company, Some(-4.39));
        assert_eq!(rows[1].sector, Some(7.54));
        assert_eq!(rows[2].company, None);
        assert!(ratio_entries(&serde_json::json!({"data":[]}))
            .unwrap()
            .is_empty());
    }
    #[test]
    fn ratios_reject_ambiguous_units_duplicates_and_nonfinite_values() {
        for value in ["NaN", "Infinity", "1e5", "1,000", "5%", "1000000001"] {
            assert!(ratio_number(Some(&serde_json::json!(value)), false).is_err());
        }
        assert!(ratio_number(Some(&serde_json::json!(5)), false).is_err());
        let row = serde_json::json!({"name":"P/E","company_value":"5"});
        assert!(ratio_entries(&serde_json::json!({"data":[row.clone(),row]})).is_err());
        assert!(ratio_entries(&serde_json::json!({"data":null})).is_err());
    }
    #[test]
    fn chart_prices_reject_duplicate_future_and_incoherent_candles() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-27T08:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let candle = serde_json::json!(["2026-10-23T00:00:00+05:30", 10, 12, 9, 11, 100, 0]);
        assert_eq!(
            chart_candles(
                &serde_json::json!({"data":{"candles":[candle.clone()]}}),
                now
            )
            .unwrap()[0]
                .close,
            11.0
        );
        assert!(chart_candles(
            &serde_json::json!({"data":{"candles":[candle.clone(),candle.clone()]}}),
            now
        )
        .is_err());
        for (index, value) in [
            (0, serde_json::json!("2027-01-01T00:00:00Z")),
            (2, serde_json::json!(8)),
            (4, serde_json::json!(-1)),
            (5, serde_json::json!(1.5)),
        ] {
            let mut bad = candle.clone();
            bad[index] = value;
            assert!(chart_candles(&serde_json::json!({"data":{"candles":[bad]}}), now).is_err());
        }
        assert!(
            chart_candles(&serde_json::json!({"data":{"candles":[]}}), now)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn chart_quote_binds_the_key_and_preserves_trade_time() {
        let now = Utc::now();
        let key = "NSE_EQ|INE213A01029";
        let raw = serde_json::json!({"data":{"NSE_EQ:ONGC":{"instrument_token":key,"last_price":220.0,"last_trade_time":"1697624972130"}}});
        assert_eq!(
            chart_quote(&raw, key, now).unwrap().last_trade_at,
            "2023-10-18T10:29:32.130+00:00"
        );
        assert!(chart_quote(&raw, "BSE_EQ|INE213A01029", now).is_err());
        let mut bad = raw.clone();
        bad["data"]["NSE_EQ:ONGC"]["last_trade_time"] = serde_json::json!("not-a-date");
        assert!(chart_quote(&bad, key, now).is_err());
        assert!(serde_json::from_value::<ChartRequest>(
            serde_json::json!({"instrumentKey":key,"range":"week"})
        )
        .is_err());
        assert!(serde_json::from_value::<ChartRequest>(
            serde_json::json!({"instrumentKey":key,"range":"day","token":"private-canary"})
        )
        .is_err());
    }
    #[test]
    fn credentials_use_a_previously_reserved_namespace_and_no_debug_output() {
        for key in [CREDENTIAL_KEY.to_string(), CREDENTIAL_KEY.to_uppercase()] {
            assert!(crate::secrets::validate_unscoped_secret_service_id(&key).is_err());
        }
        let status = ConnectionStatus {
            configured: true,
            expires_on: Some("2027-01-01".into()),
        };
        let encoded = serde_json::to_value(status).expect("serializable status");
        assert_eq!(encoded.as_object().expect("object").len(), 2);
        assert!(encoded.get("token").is_none());
    }
    #[test]
    fn stock_identifiers_reject_funds_invalid_checksums_and_urls() {
        assert!(valid_isin("INE213A01029"));
        assert!(valid_isin("INE002A01018"));
        for value in ["INE213A01028", "INF213A01029", "../../secret", ""] {
            assert!(!valid_isin(value));
        }
        assert!(query("ONGC").is_ok());
        assert!(query("https://example.com").is_err());
        assert!(serde_json::from_value::<AnalyseRequest>(serde_json::json!({"instrumentKey":"NSE_EQ|INE213A01029","basis":"consolidated","token":"secret"})).is_err());
    }
    #[test]
    fn listing_identity_must_match_the_equity_exchange_and_isin() {
        let mut row = serde_json::json!({"name":"Fictional issuer","trading_symbol":"TEST","isin":"INE213A01029","instrument_key":"NSE_EQ|INE213A01029","exchange":"NSE","segment":"NSE_EQ"});
        assert!(listing(&row).is_some());
        row["instrument_key"] = Value::String("BSE_EQ|INE213A01029".into());
        assert!(listing(&row).is_none());
    }
    #[test]
    fn only_declared_public_summary_fields_leave_the_host() {
        let data = serde_json::json!({"type":"consolidated","token":"private-token-canary","income_statement":[{"category":"revenue","privateNote":"private-note-canary","history":[{"period":"Mar 2026","value":10,"token":"nested-canary"}]}],"full_statement":[{"secret":"full-canary"}]});
        let output = summary(&data, "income-statement")
            .expect("valid public summary")
            .to_string();
        assert!(!output.contains("canary"));
        assert!(!output.contains("token"));
        assert!(!output.contains("full_statement"));
    }
    #[test]
    fn stored_credentials_are_isolated_by_the_real_profile_store() {
        use crate::profiles::{Profile, ScopedSecretStore};
        use std::collections::BTreeMap;
        use std::sync::{Arc, Mutex};
        #[derive(Default)]
        struct Memory(Mutex<BTreeMap<String, String>>);
        impl SecretStore for Memory {
            fn get_secret(&self, key: &str) -> Result<Option<String>> {
                Ok(self.0.lock().unwrap().get(key).cloned())
            }
            fn set_secret(&self, key: &str, value: &str) -> Result<()> {
                self.0.lock().unwrap().insert(key.into(), value.into());
                Ok(())
            }
            fn delete_secret(&self, key: &str) -> Result<()> {
                self.0.lock().unwrap().remove(key);
                Ok(())
            }
        }
        let raw = Arc::new(Memory::default());
        let root = tempfile::tempdir().unwrap();
        let gate = Arc::new(Mutex::new(()));
        let profile = |name: &str| Profile {
            id: uuid::Uuid::new_v4(),
            name: name.into(),
            avatar_id: "test".into(),
            lock_enabled: false,
            never_protected: true,
            legacy_database: None,
            legacy_addons_root: None,
            connect: None,
        };
        let a = profile("A");
        let b = profile("B");
        let sa =
            ScopedSecretStore::new(raw.clone(), &a, root.path().join("a.deleted"), gate.clone());
        let sb = ScopedSecretStore::new(raw, &b, root.path().join("b.deleted"), gate);
        let credential = || ConnectionRequest {
            token: "fictional-credential-canary-123456".into(),
            expires_on: "2099-01-01".into(),
            free_personal_confirmed: true,
            private_dashboard_confirmed: true,
        };
        save_connection(&sa, credential()).unwrap();
        assert!(connection_status(&sa).unwrap().configured);
        assert!(!connection_status(&sb).unwrap().configured);
        save_connection(&sb, credential()).unwrap();
        remove_connection(&sa).unwrap();
        assert!(!connection_status(&sa).unwrap().configured);
        assert!(connection_status(&sb).unwrap().configured);
    }
}
