//! Strict research reads use one SQLite transaction and preserve decimal strings.
pub mod plan;
pub mod runs;

use std::{collections::BTreeMap, str::FromStr, sync::Arc};

use chrono::{DateTime, NaiveDate, Utc};
use diesel::{prelude::*, sql_types::Text};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use wealthfolio_core::{
    portfolio::projection::MarkerScope,
    research::{
        ResearchAccount, ResearchContext, ResearchContextRepository, ResearchFxQuote,
        ResearchIssue, ResearchPosition, ResearchQuote,
    },
    Error, Result,
};

use crate::{db::get_connection, errors::StorageError, DbPool};

const MAX_ACCOUNTS: usize = 200;
const MAX_POSITIONS: usize = 200;
const MAX_JSON_BYTES: usize = 2 * 1024 * 1024;

pub struct ResearchRepository {
    pool: Arc<DbPool>,
}

impl ResearchRepository {
    pub fn new(pool: Arc<DbPool>) -> Self {
        Self { pool }
    }
}

#[derive(QueryableByName)]
struct JsonRow {
    #[diesel(sql_type = Text)]
    data: String,
}

fn rows(conn: &mut SqliteConnection, sql: &str) -> Result<Vec<Value>> {
    diesel::sql_query(sql)
        .load::<JsonRow>(conn)
        .map_err(StorageError::from)?
        .into_iter()
        .map(|row| {
            serde_json::from_str(&row.data)
                .map_err(|_| Error::ConstraintViolation("RESEARCH_ROW_INVALID".into()))
        })
        .collect()
}

fn text(row: &Value, key: &str) -> Option<String> {
    row.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn issue(
    issues: &mut Vec<ResearchIssue>,
    code: &str,
    account: Option<&str>,
    asset: Option<&str>,
    field: Option<&str>,
) {
    issues.push(ResearchIssue {
        code: code.into(),
        account_id: account.map(str::to_owned),
        asset_id: asset.map(str::to_owned),
        field: field.map(str::to_owned),
    });
}

fn decimal(
    value: Option<&Value>,
    issues: &mut Vec<ResearchIssue>,
    account: Option<&str>,
    asset: Option<&str>,
    field: &str,
) -> Option<String> {
    let Some(value) = value else {
        issue(issues, "MISSING_DECIMAL", account, asset, Some(field));
        return None;
    };
    let Some(raw) = value.as_str() else {
        issue(
            issues,
            if value.is_number() {
                "LEGACY_NUMBER_PRECISION_UNVERIFIED"
            } else {
                "INVALID_DECIMAL"
            },
            account,
            asset,
            Some(field),
        );
        return None;
    };
    match Decimal::from_str(raw) {
        Ok(number) => Some(number.normalize().to_string()),
        Err(_) => {
            issue(issues, "INVALID_DECIMAL", account, asset, Some(field));
            None
        }
    }
}

fn object(
    raw: Option<String>,
    issues: &mut Vec<ResearchIssue>,
    account: &str,
    field: &str,
) -> Option<BTreeMap<String, Value>> {
    match raw.and_then(|raw| serde_json::from_str::<BTreeMap<String, Value>>(&raw).ok()) {
        Some(value) => Some(value),
        None => {
            issue(issues, "CORRUPT_SNAPSHOT", Some(account), None, Some(field));
            None
        }
    }
}

fn quote(
    row: &Value,
    day: NaiveDate,
    issues: &mut Vec<ResearchIssue>,
    asset: &str,
) -> ResearchQuote {
    let observation_date = text(row, "day").unwrap_or_default();
    let observed_at = text(row, "timestamp").unwrap_or_default();
    let source = text(row, "source").unwrap_or_default();
    let valid_day = NaiveDate::parse_from_str(&observation_date, "%Y-%m-%d");
    let valid_time = DateTime::parse_from_rfc3339(&observed_at);
    if valid_day.as_ref().map_or(true, |value| *value > day)
        || valid_time.as_ref().map_or(true, |value| {
            value.date_naive() > day || value.with_timezone(&Utc) > Utc::now()
        })
        || source.is_empty()
    {
        issue(issues, "QUOTE_PROVENANCE_INVALID", None, Some(asset), None);
    } else if valid_day.is_ok_and(|value| value < day) {
        issue(issues, "QUOTE_NOT_CURRENT", None, Some(asset), None);
    }
    let price = decimal(row.get("close"), issues, None, Some(asset), "close");
    if price
        .as_deref()
        .and_then(|v| Decimal::from_str(v).ok())
        .is_some_and(|v| v <= Decimal::ZERO)
    {
        issue(issues, "NONPOSITIVE_QUOTE", None, Some(asset), None);
    }
    let adjusted_price = row
        .get("adjclose")
        .filter(|v| !v.is_null())
        .and_then(|v| decimal(Some(v), issues, None, Some(asset), "adjclose"));
    ResearchQuote {
        price,
        adjusted_price,
        currency: text(row, "currency").unwrap_or_default(),
        observation_date,
        observed_at,
        source,
        price_basis: "UNVERIFIED_RAW_AND_ADJUSTED_CLOSE".into(),
    }
}

fn latest_quote(conn: &mut SqliteConnection, asset: &str) -> Result<Option<Value>> {
    let found = diesel::sql_query("SELECT json_object('id',id,'day',day,'timestamp',timestamp,'source',source,'close',close,'adjclose',adjclose,'currency',currency) AS data FROM quotes WHERE asset_id = ? ORDER BY day DESC, CASE source WHEN 'MANUAL' THEN 1 WHEN 'BROKER' THEN 3 ELSE 2 END, timestamp DESC, id ASC LIMIT 1")
        .bind::<Text, _>(asset).load::<JsonRow>(conn).map_err(StorageError::from)?;
    found
        .into_iter()
        .next()
        .map(|row| {
            serde_json::from_str(&row.data)
                .map_err(|_| Error::ConstraintViolation("RESEARCH_QUOTE_INVALID".into()))
        })
        .transpose()
}

/// Reads on the supplied transaction connection so review/save can reuse this exact boundary.
pub fn load_context_on_connection(
    conn: &mut SqliteConnection,
    day: NaiveDate,
    base_currency: &str,
) -> Result<ResearchContext> {
    load_context_for_mode_on_connection(
        conn,
        day,
        base_currency,
        wealthfolio_core::research::plan::ResearchMode::Personal,
    )
}

pub fn load_context_for_mode_on_connection(
    conn: &mut SqliteConnection,
    day: NaiveDate,
    base_currency: &str,
    mode: wealthfolio_core::research::plan::ResearchMode,
) -> Result<ResearchContext> {
    let policy = rows(conn, "SELECT json_object('key',setting_key,'value',setting_value) AS data FROM app_settings WHERE setting_key IN ('base_currency','timezone') ORDER BY setting_key")?;
    let stored_base = policy
        .iter()
        .find(|row| row["key"] == "base_currency")
        .and_then(|row| text(row, "value"));
    let expected_base = base_currency;
    let base_currency = stored_base.as_deref().unwrap_or(expected_base);
    let accounts = rows(conn, "SELECT json_object('id',id,'name',name,'currency',currency,'active',is_active,'archived',is_archived,'updatedAt',updated_at,'trackingMode',tracking_mode) AS data FROM accounts ORDER BY id LIMIT 201")?;
    if accounts.len() > MAX_ACCOUNTS {
        return Err(Error::ConstraintViolation("RESEARCH_ACCOUNT_LIMIT".into()));
    }
    // Check the aggregate stored byte size before loading JSON: 200 individually
    // bounded rows must not allocate hundreds of megabytes on a modest PC.
    let sizes = rows(conn, "SELECT json_object('bytes',COALESCE(SUM(length(CAST(s.positions AS BLOB))+length(CAST(s.cash_balances AS BLOB))),0)) AS data FROM holdings_snapshots s JOIN accounts a ON a.id=s.account_id WHERE a.is_active=1 AND s.id=(SELECT h.id FROM holdings_snapshots h WHERE h.account_id=a.id ORDER BY h.snapshot_date DESC,h.id ASC LIMIT 1)")?;
    if sizes
        .first()
        .and_then(|row| row["bytes"].as_u64())
        .is_none_or(|bytes| bytes > MAX_JSON_BYTES as u64)
    {
        return Err(Error::ConstraintViolation(
            "RESEARCH_CONTEXT_SIZE_LIMIT".into(),
        ));
    }
    let snapshots = rows(conn, "SELECT json_object('id',s.id,'accountId',s.account_id,'date',s.snapshot_date,'currency',s.currency,'positions',CASE WHEN length(s.positions)<=2097152 THEN s.positions ELSE NULL END,'cash',CASE WHEN length(s.cash_balances)<=2097152 THEN s.cash_balances ELSE NULL END,'source',s.source,'calculatedAt',s.calculated_at) AS data FROM holdings_snapshots s JOIN accounts a ON a.id=s.account_id WHERE a.is_active=1 AND s.id=(SELECT h.id FROM holdings_snapshots h WHERE h.account_id=a.id ORDER BY h.snapshot_date DESC,h.id ASC LIMIT 1) ORDER BY s.account_id LIMIT 201")?;
    if snapshots.iter().map(|v| v.to_string().len()).sum::<usize>() > MAX_JSON_BYTES {
        return Err(Error::ConstraintViolation(
            "RESEARCH_CONTEXT_SIZE_LIMIT".into(),
        ));
    }
    let markers = rows(conn, "SELECT json_object('scope',scope,'dirtyFrom',dirty_from,'version',CAST(version AS TEXT),'activityIssues',activity_issues) AS data FROM projection_state ORDER BY scope LIMIT 2001")?;
    if markers.len() > 2000 {
        return Err(Error::ConstraintViolation("RESEARCH_MARKER_LIMIT".into()));
    }
    let mut issues = Vec::new();
    if stored_base.is_none() {
        issue(&mut issues, "BASE_CURRENCY_NOT_RECORDED", None, None, None);
    } else if base_currency != expected_base {
        issue(&mut issues, "SETTINGS_CHANGED_RELOAD", None, None, None);
    }
    if !matches!(base_currency, "USD" | "INR") {
        issue(&mut issues, "UNSUPPORTED_BASE_CURRENCY", None, None, None);
    }
    let mut output_accounts = Vec::new();
    let mut excluded = Vec::new();
    let mut positions = Vec::new();
    let mut fingerprint_assets = BTreeMap::new();
    let mut fingerprint_quotes = BTreeMap::new();
    for account in &accounts {
        let id = text(account, "id").unwrap_or_default();
        if account["active"] != 1 {
            excluded.push(id);
            continue;
        }
        let snapshot = snapshots.iter().find(|s| s["accountId"] == id);
        let mut cash = BTreeMap::new();
        if let Some(snapshot) = snapshot {
            let date = text(snapshot, "date").unwrap_or_default();
            if snapshot["currency"] != account["currency"] {
                issue(
                    &mut issues,
                    "SNAPSHOT_CURRENCY_CONFLICT",
                    Some(&id),
                    None,
                    None,
                );
            }
            if NaiveDate::parse_from_str(&date, "%Y-%m-%d") != Ok(day) {
                issue(&mut issues, "SNAPSHOT_NOT_CURRENT", Some(&id), None, None);
            }
            if let Some(values) = object(text(snapshot, "cash"), &mut issues, &id, "cash") {
                for (currency, value) in values {
                    if !matches!(currency.as_str(), "USD" | "INR") {
                        issue(
                            &mut issues,
                            "UNSUPPORTED_CURRENCY",
                            Some(&id),
                            None,
                            Some("cash"),
                        );
                    }
                    let amount = decimal(Some(&value), &mut issues, Some(&id), None, "cash");
                    if amount
                        .as_deref()
                        .and_then(|v| Decimal::from_str(v).ok())
                        .is_some_and(|v| v < Decimal::ZERO)
                    {
                        issue(
                            &mut issues,
                            "NEGATIVE_CASH_UNSUPPORTED",
                            Some(&id),
                            None,
                            Some("cash"),
                        );
                    }
                    cash.insert(currency, amount);
                }
            }
            if let Some(values) = object(text(snapshot, "positions"), &mut issues, &id, "positions")
            {
                if positions.len() + values.len() > MAX_POSITIONS {
                    return Err(Error::ConstraintViolation("RESEARCH_POSITION_LIMIT".into()));
                }
                for (asset, value) in values {
                    let asset_rows = diesel::sql_query("SELECT json_object('id',id,'kind',kind,'currency',quote_ccy,'type',instrument_type,'symbol',instrument_symbol,'mic',instrument_exchange_mic,'key',instrument_key,'updatedAt',updated_at) AS data FROM assets WHERE id=?")
                        .bind::<Text,_>(&asset).load::<JsonRow>(conn).map_err(StorageError::from)?;
                    let identity = asset_rows
                        .first()
                        .and_then(|row| serde_json::from_str::<Value>(&row.data).ok())
                        .unwrap_or(Value::Null);
                    fingerprint_assets.insert(asset.clone(), identity.clone());
                    let raw_quote = latest_quote(conn, &asset)?;
                    fingerprint_quotes.insert(asset.clone(), raw_quote.clone());
                    let quote = raw_quote
                        .as_ref()
                        .map(|q| quote(q, day, &mut issues, &asset));
                    if quote.is_none() {
                        issue(&mut issues, "QUOTE_MISSING", Some(&id), Some(&asset), None);
                    }
                    let quantity = decimal(
                        value.get("quantity"),
                        &mut issues,
                        Some(&id),
                        Some(&asset),
                        "quantity",
                    );
                    if quantity
                        .as_deref()
                        .and_then(|v| Decimal::from_str(v).ok())
                        .is_some_and(|v| v < Decimal::ZERO)
                    {
                        issue(
                            &mut issues,
                            "SHORT_UNSUPPORTED",
                            Some(&id),
                            Some(&asset),
                            None,
                        );
                    }
                    let currency = text(&value, "currency");
                    if text(&identity, "type").as_deref() != Some("EQUITY")
                        || text(&identity, "kind").as_deref() != Some("INVESTMENT")
                    {
                        issue(
                            &mut issues,
                            "INSTRUMENT_UNSUPPORTED",
                            Some(&id),
                            Some(&asset),
                            None,
                        );
                    }
                    if !matches!(currency.as_deref(), Some("USD" | "INR")) {
                        issue(
                            &mut issues,
                            "UNSUPPORTED_CURRENCY",
                            Some(&id),
                            Some(&asset),
                            None,
                        );
                    }
                    if quote
                        .as_ref()
                        .is_some_and(|q| Some(q.currency.as_str()) != currency.as_deref())
                    {
                        issue(
                            &mut issues,
                            "QUOTE_CURRENCY_CONFLICT",
                            Some(&id),
                            Some(&asset),
                            None,
                        );
                    }
                    // Existing asset metadata does not establish checked issuer/sector/share-class identity.
                    issue(
                        &mut issues,
                        "FINANCIAL_IDENTITY_UNVERIFIED",
                        Some(&id),
                        Some(&asset),
                        None,
                    );
                    positions.push(ResearchPosition {
                        account_id: id.clone(),
                        asset_id: asset.clone(),
                        quantity,
                        average_cost: decimal(
                            value.get("averageCost"),
                            &mut issues,
                            Some(&id),
                            Some(&asset),
                            "averageCost",
                        ),
                        contract_multiplier: decimal(
                            value.get("contractMultiplier"),
                            &mut issues,
                            Some(&id),
                            Some(&asset),
                            "contractMultiplier",
                        ),
                        currency,
                        instrument_type: text(&identity, "type"),
                        symbol: text(&identity, "symbol"),
                        exchange_mic: text(&identity, "mic"),
                        issuer_id: None,
                        share_class: None,
                        sector: None,
                        quote,
                    });
                }
            }
        } else {
            issue(&mut issues, "SNAPSHOT_MISSING", Some(&id), None, None);
        }
        output_accounts.push(ResearchAccount {
            id: id.clone(),
            name: text(account, "name").unwrap_or_default(),
            currency: text(account, "currency").unwrap_or_default(),
            archived: account["archived"] == 1,
            snapshot_date: snapshot.and_then(|s| text(s, "date")),
            snapshot_source: snapshot.and_then(|s| text(s, "source")),
            cash,
        });
    }
    for marker in &markers {
        let scope = text(marker, "scope").unwrap_or_default();
        let parsed = MarkerScope::parse(&scope);
        let relevant = match &parsed {
            MarkerScope::Account(id) => output_accounts.iter().any(|a| &a.id == id),
            _ => true,
        };
        if relevant && !marker["dirtyFrom"].is_null() {
            issue(&mut issues, "PROJECTION_DIRTY", None, None, Some(&scope));
        }
        if relevant
            && text(marker, "activityIssues").is_none_or(|raw| {
                serde_json::from_str::<Vec<Value>>(&raw).map_or(true, |v| !v.is_empty())
            })
        {
            issue(&mut issues, "ACTIVITY_ISSUES", None, None, Some(&scope));
        }
    }
    // Explicitly bounded FX rows; no inference from ticker names or quote currency.
    let fx_assets = rows(conn, "SELECT json_object('id',id,'key',instrument_key,'updatedAt',updated_at) AS data FROM assets WHERE instrument_key IN ('FX:USD/INR','FX:INR/USD') ORDER BY id LIMIT 201")?;
    if fx_assets.len() > 200 {
        return Err(Error::ConstraintViolation("RESEARCH_FX_LIMIT".into()));
    }
    let mut fx_quotes = Vec::new();
    for asset in &fx_assets {
        let id = text(asset, "id").unwrap_or_default();
        let raw = latest_quote(conn, &id)?;
        fingerprint_quotes.insert(id.clone(), raw.clone());
        fx_quotes.push(ResearchFxQuote {
            asset_id: id.clone(),
            pair: text(asset, "key").unwrap_or_default(),
            quote: raw.as_ref().map(|q| quote(q, day, &mut issues, &id)),
        });
    }
    if output_accounts.iter().any(|a| a.currency != base_currency)
        || positions
            .iter()
            .any(|p| p.currency.as_deref() != Some(base_currency))
    {
        issue(&mut issues, "FX_COMPATIBILITY_UNVERIFIED", None, None, None);
    }
    let plan = plan::load_plan_on_connection(conn, mode)?;
    if plan.is_none() {
        issue(
            &mut issues,
            "INVESTOR_PLAN_NOT_CONFIGURED",
            None,
            None,
            None,
        );
    }
    if let Some(saved) = &plan {
        let p = &saved.plan.profile;
        if p.base_currency.as_str() != base_currency {
            issue(
                &mut issues,
                "INVESTOR_PLAN_CURRENCY_CONFLICT",
                None,
                None,
                None,
            );
        }
        if !p.cash_complete
            || !p.payments_complete
            || !p.debts_complete
            || !p.holdings_complete
            || p.can_afford_to_lose_base.is_none()
            || p.willing_to_lose_base.is_none()
            || p.max_drawdown_pct.is_none()
            || p.goal_date.is_none()
        {
            issue(&mut issues, "INVESTOR_PLAN_INCOMPLETE", None, None, None);
        }
    }
    let plan_revision = plan.as_ref().map_or(0, |p| p.revision);
    let fingerprint = json!({"schemaVersion":1,"day":day.to_string(),"baseCurrency":base_currency,"policy":policy,"accounts":accounts,"snapshots":snapshots,"markers":markers,"assets":fingerprint_assets,"fxAssets":fx_assets,"quotes":fingerprint_quotes,"mode":mode,"plan":plan});
    let bytes = serde_json::to_vec(&fingerprint)
        .map_err(|_| Error::ConstraintViolation("RESEARCH_CONTEXT_INVALID".into()))?;
    let context_hash = hex::encode(Sha256::digest(&bytes));
    Ok(ResearchContext {
        schema_version: 1,
        captured_at: Utc::now().to_rfc3339(),
        valuation_date: day.to_string(),
        base_currency: base_currency.into(),
        context_hash,
        plan_revision,
        plan,
        mode,
        accounts: output_accounts,
        excluded_account_ids: excluded,
        positions,
        fx_quotes,
        coverage_complete: issues.is_empty(),
        issues,
    })
}

impl ResearchContextRepository for ResearchRepository {
    fn load_context(&self, day: NaiveDate, base_currency: &str) -> Result<ResearchContext> {
        let mut conn = get_connection(&self.pool)?;
        conn.transaction::<_, StorageError, _>(|conn| {
            load_context_on_connection(conn, day, base_currency).map_err(StorageError::from)
        })
        .map_err(Error::from)
    }

    fn load_context_for_mode(
        &self,
        day: NaiveDate,
        base_currency: &str,
        mode: wealthfolio_core::research::plan::ResearchMode,
    ) -> Result<ResearchContext> {
        let mut conn = get_connection(&self.pool)?;
        conn.transaction::<_, StorageError, _>(|conn| {
            load_context_for_mode_on_connection(conn, day, base_currency, mode)
                .map_err(StorageError::from)
        })
        .map_err(Error::from)
    }
}
