//! Read-only research inputs. No provider calls or financial inference occur here.

pub mod calculator;
pub mod company;
pub mod india;
pub mod plan;
pub mod runs;
#[cfg(windows)]
mod windows_job;

use std::sync::Arc;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::Result;
use plan::{ResearchMode, ResearchPlanRepository, SavePlanRequest, StoredPlan};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResearchIssue {
    pub code: String,
    pub account_id: Option<String>,
    pub asset_id: Option<String>,
    pub field: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResearchAccount {
    pub id: String,
    pub name: String,
    pub currency: String,
    pub archived: bool,
    pub snapshot_date: Option<String>,
    pub snapshot_source: Option<String>,
    pub cash: std::collections::BTreeMap<String, Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResearchQuote {
    pub price: Option<String>,
    pub adjusted_price: Option<String>,
    pub currency: String,
    pub observation_date: String,
    pub observed_at: String,
    pub source: String,
    /// Raw close and adjusted close are retained separately; compatibility is unverified.
    pub price_basis: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResearchPosition {
    pub account_id: String,
    pub asset_id: String,
    pub quantity: Option<String>,
    pub average_cost: Option<String>,
    pub currency: Option<String>,
    pub contract_multiplier: Option<String>,
    pub instrument_type: Option<String>,
    pub symbol: Option<String>,
    pub exchange_mic: Option<String>,
    pub issuer_id: Option<String>,
    pub share_class: Option<String>,
    pub sector: Option<String>,
    pub quote: Option<ResearchQuote>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResearchFxQuote {
    pub asset_id: String,
    pub pair: String,
    pub quote: Option<ResearchQuote>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResearchContext {
    pub schema_version: u32,
    pub captured_at: String,
    pub valuation_date: String,
    pub base_currency: String,
    pub context_hash: String,
    /// Zero means absent, not answered.
    pub plan_revision: u64,
    pub mode: ResearchMode,
    pub plan: Option<StoredPlan>,
    pub accounts: Vec<ResearchAccount>,
    pub excluded_account_ids: Vec<String>,
    pub positions: Vec<ResearchPosition>,
    pub fx_quotes: Vec<ResearchFxQuote>,
    pub issues: Vec<ResearchIssue>,
    pub coverage_complete: bool,
}

pub trait ResearchContextRepository: Send + Sync {
    fn load_context(&self, day: NaiveDate, base_currency: &str) -> Result<ResearchContext>;
    fn load_context_for_mode(
        &self,
        day: NaiveDate,
        base_currency: &str,
        mode: ResearchMode,
    ) -> Result<ResearchContext>;
}

pub struct ResearchService {
    repository: Arc<dyn ResearchContextRepository>,
    plans: Arc<dyn ResearchPlanRepository>,
    mode: ResearchMode,
}

impl ResearchService {
    pub fn new(
        repository: Arc<dyn ResearchContextRepository>,
        plans: Arc<dyn ResearchPlanRepository>,
        mode: ResearchMode,
    ) -> Self {
        Self {
            repository,
            plans,
            mode,
        }
    }

    pub async fn current_plan(&self) -> Result<Option<StoredPlan>> {
        let repository = self.plans.clone();
        let mode = self.mode;
        crate::portfolio::coordinator::blocking(move || repository.load_plan(mode)).await
    }

    pub async fn save_plan(&self, request: SavePlanRequest) -> Result<StoredPlan> {
        request.plan.validate()?;
        self.plans.save_plan(self.mode, request).await
    }

    pub async fn current_context(&self, base_currency: String) -> Result<ResearchContext> {
        let day = chrono::Utc::now()
            .with_timezone(&chrono_tz::Asia::Kolkata)
            .date_naive();
        self.context(day, base_currency).await
    }

    pub async fn context(&self, day: NaiveDate, base_currency: String) -> Result<ResearchContext> {
        let repository = self.repository.clone();
        let mode = self.mode;
        crate::portfolio::coordinator::blocking(move || {
            repository.load_context_for_mode(day, &base_currency, mode)
        })
        .await
    }
}
