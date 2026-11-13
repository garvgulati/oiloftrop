//! Persisted investor answers. Validation mirrors the accepted planning contract;
//! missing answers remain null/false and are never inferred from account values.
use std::{collections::BTreeSet, str::FromStr};

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchMode {
    Fictional,
    Personal,
}

impl ResearchMode {
    /// Host configuration, never a browser-supplied mode flag. Fixture hosting
    /// is reserved for the isolated trial; ordinary profiles default to personal.
    pub fn for_host() -> Self {
        if std::env::var("WEALTHFOLIO_E2E").as_deref() == Ok("1")
            && std::env::var_os("WEALTHFOLIO_FIXTURE_DIR").is_some()
        {
            Self::Fictional
        } else {
            Self::Personal
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fictional => "fictional",
            Self::Personal => "personal",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum PlanCurrency {
    INR,
    USD,
}

impl PlanCurrency {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::INR => "INR",
            Self::USD => "USD",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Payment {
    pub id: String,
    pub label: String,
    pub currency: PlanCurrency,
    pub amount: String,
    pub date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashAnswer {
    pub id: String,
    pub name: String,
    pub currency: PlanCurrency,
    pub amount: String,
    pub reserve: String,
    pub date: String,
    pub reconciled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiabilityRow {
    pub date: String,
    pub interest_accrued: String,
    pub fees_accrued: String,
    pub capitalized_interest: String,
    pub capitalized_fees: String,
    pub principal_paid: String,
    pub interest_paid: String,
    pub fees_paid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiabilityPath {
    pub opening_date: String,
    pub principal: String,
    pub interest_due: String,
    pub fees_due: String,
    pub confirmed_through: String,
    pub rows: Vec<LiabilityRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebtAnswer {
    pub id: String,
    pub label: String,
    pub currency: PlanCurrency,
    pub balance: String,
    pub annual_interest_pct: Option<String>,
    pub repayments: Vec<Payment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cash_schedule_through: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule_basis: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule_confirmed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liability_path: Option<LiabilityPath>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanningProfile {
    pub schema_version: u32,
    pub base_currency: PlanCurrency,
    pub goal_label: String,
    pub goal_date: Option<String>,
    pub goal_currency: PlanCurrency,
    pub goal_amount: Option<String>,
    pub goal_in_todays_money: bool,
    pub cash: Vec<CashAnswer>,
    pub payments: Vec<Payment>,
    pub debts: Vec<DebtAnswer>,
    pub cash_complete: bool,
    pub payments_complete: bool,
    pub debts_complete: bool,
    pub holdings_complete: bool,
    pub new_money_available_base: Option<String>,
    pub can_afford_to_lose_base: Option<String>,
    pub willing_to_lose_base: Option<String>,
    pub max_drawdown_pct: Option<String>,
    pub issuer_limit_pct: Option<String>,
    pub sector_limit_pct: Option<String>,
    pub currency_limit_pct: Option<String>,
    pub real_hurdle_pct: Option<String>,
    pub restrictions: String,
    pub confirmed_date: String,
}

/// Explicit source links prevent counting the same broker cash as additional manual cash.
/// The accepted engine profile remains unchanged; links are adapter provenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashLink {
    pub cash_id: String,
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResearchPlan {
    pub profile: PlanningProfile,
    pub cash_links: Vec<CashLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredPlan {
    pub schema_version: u32,
    pub mode: ResearchMode,
    pub revision: u64,
    pub plan: ResearchPlan,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavePlanRequest {
    pub expected_revision: u64,
    pub plan: ResearchPlan,
}

#[async_trait::async_trait]
pub trait ResearchPlanRepository: Send + Sync {
    fn load_plan(&self, mode: ResearchMode) -> Result<Option<StoredPlan>>;
    async fn save_plan(&self, mode: ResearchMode, request: SavePlanRequest) -> Result<StoredPlan>;
}

fn invalid() -> Error {
    Error::ConstraintViolation("RESEARCH_PLAN_INVALID".into())
}
fn check(test: bool) -> Result<()> {
    if test {
        Ok(())
    } else {
        Err(invalid())
    }
}
fn date(raw: &str) -> Result<()> {
    let parsed = NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_err(|_| invalid())?;
    check(parsed.format("%Y-%m-%d").to_string() == raw)
}
fn label(raw: &str, max: usize, required: bool) -> Result<()> {
    check(raw == raw.trim() && raw.encode_utf16().count() <= max && (!required || !raw.is_empty()))
}
fn money(raw: &str) -> Result<Decimal> {
    let parts: Vec<_> = raw.split('.').collect();
    check(
        parts.len() <= 2
            && !parts[0].is_empty()
            && parts[0].len() <= 12
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            && parts.get(1).is_none_or(|part| part.len() <= 8),
    )?;
    let value = Decimal::from_str(raw).map_err(|_| invalid())?;
    check(value <= Decimal::from(1_000_000_000_000u64))?;
    Ok(value)
}
fn optional_money(raw: &Option<String>) -> Result<()> {
    raw.as_deref().map(money).transpose().map(|_| ())
}
fn percentage(raw: &Option<String>) -> Result<()> {
    if let Some(raw) = raw {
        check(
            raw.split('.').next().is_some_and(|p| p.len() <= 3)
                && money(raw)? <= Decimal::from(100),
        )?;
    }
    Ok(())
}
fn unique_id(id: &str, ids: &mut BTreeSet<String>) -> Result<()> {
    check(
        uuid::Uuid::parse_str(id).is_ok_and(|value| value.to_string().eq_ignore_ascii_case(id))
            && ids.insert(id.to_ascii_lowercase()),
    )
}
fn payment(value: &Payment, ids: &mut BTreeSet<String>) -> Result<()> {
    unique_id(&value.id, ids)?;
    label(&value.label, 100, true)?;
    date(&value.date)?;
    check(money(&value.amount)? > Decimal::ZERO)
}

impl ResearchPlan {
    pub fn validate(&self) -> Result<()> {
        // Bound serialized input as well as row counts before persistence or hashing.
        check(serde_json::to_vec(self).map_err(|_| invalid())?.len() <= 256 * 1024)?;
        let p = &self.profile;
        check(
            p.schema_version == 1
                && p.cash.len() <= 30
                && p.payments.len() <= 50
                && p.debts.len() <= 20
                && self.cash_links.len() <= 30,
        )?;
        label(&p.goal_label, 1500, false)?;
        label(&p.restrictions, 1500, false)?;
        date(&p.confirmed_date)?;
        check(p.goal_date.is_some() == p.goal_amount.is_some())?;
        if let Some(raw) = &p.goal_date {
            date(raw)?;
        }
        for raw in [
            &p.goal_amount,
            &p.new_money_available_base,
            &p.can_afford_to_lose_base,
            &p.willing_to_lose_base,
        ] {
            optional_money(raw)?;
        }
        for raw in [
            &p.max_drawdown_pct,
            &p.issuer_limit_pct,
            &p.sector_limit_pct,
            &p.currency_limit_pct,
        ] {
            percentage(raw)?;
        }
        if let Some(raw) = &p.real_hurdle_pct {
            let value = money(raw.strip_prefix('-').unwrap_or(raw))?;
            check(if raw.starts_with('-') {
                value < Decimal::from(100)
            } else {
                value <= Decimal::from(200)
            })?;
        }
        let mut ids = BTreeSet::new();
        for cash in &p.cash {
            unique_id(&cash.id, &mut ids)?;
            label(&cash.name, 100, true)?;
            date(&cash.date)?;
            check(money(&cash.reserve)? <= money(&cash.amount)?)?;
        }
        for value in &p.payments {
            payment(value, &mut ids)?;
        }
        for debt in &p.debts {
            unique_id(&debt.id, &mut ids)?;
            label(&debt.label, 100, true)?;
            let balance = money(&debt.balance)?;
            percentage(&debt.annual_interest_pct)?;
            check(debt.repayments.len() <= 30)?;
            for value in &debt.repayments {
                payment(value, &mut ids)?;
                check(value.currency == debt.currency)?;
            }
            if let Some(raw) = &debt.balance_date {
                date(raw)?;
            }
            if let Some(raw) = &debt.cash_schedule_through {
                date(raw)?;
            }
            if let Some(raw) = &debt.schedule_basis {
                label(raw, 1000, false)?;
            }
            if let (Some(start), Some(end)) = (&debt.balance_date, &debt.cash_schedule_through) {
                check(end >= start)?;
            }
            if let Some(path) = &debt.liability_path {
                date(&path.opening_date)?;
                date(&path.confirmed_through)?;
                check(
                    !path.rows.is_empty()
                        && path.rows.len() <= 150
                        && debt.balance_date.as_ref() == Some(&path.opening_date)
                        && path
                            .rows
                            .last()
                            .is_some_and(|row| row.date == path.confirmed_through),
                )?;
                let mut principal = money(&path.principal)?;
                let mut interest = money(&path.interest_due)?;
                let mut fees = money(&path.fees_due)?;
                check(principal + interest + fees == balance)?;
                let mut previous = &path.opening_date;
                for (i, row) in path.rows.iter().enumerate() {
                    date(&row.date)?;
                    check(row.date >= *previous && (i == 0 || row.date > *previous))?;
                    let ai = money(&row.interest_accrued)?;
                    let af = money(&row.fees_accrued)?;
                    let ci = money(&row.capitalized_interest)?;
                    let cf = money(&row.capitalized_fees)?;
                    let pp = money(&row.principal_paid)?;
                    let pi = money(&row.interest_paid)?;
                    let pf = money(&row.fees_paid)?;
                    let cash: Decimal = debt
                        .repayments
                        .iter()
                        .filter(|p| p.date == row.date)
                        .map(|p| money(&p.amount))
                        .collect::<Result<Vec<_>>>()?
                        .into_iter()
                        .sum();
                    check(
                        ci <= interest + ai
                            && cf <= fees + af
                            && cash == pp + pi + pf
                            && !debt.repayments.iter().any(|p| {
                                p.date >= *previous
                                    && p.date < row.date
                                    && (i == 0 || p.date > *previous)
                            }),
                    )?;
                    principal += ci + cf - pp;
                    interest += ai - ci - pi;
                    fees += af - cf - pf;
                    check(
                        principal >= Decimal::ZERO
                            && interest >= Decimal::ZERO
                            && fees >= Decimal::ZERO,
                    )?;
                    previous = &row.date;
                }
            }
        }
        let mut linked_cash = BTreeSet::new();
        let mut sources = BTreeSet::new();
        for link in &self.cash_links {
            label(&link.account_id, 200, true)?;
            let cash = p
                .cash
                .iter()
                .find(|v| v.id == link.cash_id)
                .ok_or_else(invalid)?;
            check(
                cash.reconciled
                    && linked_cash.insert(&link.cash_id)
                    && sources.insert((&link.account_id, &cash.currency)),
            )?;
        }
        Ok(())
    }
}
