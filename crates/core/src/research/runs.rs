//! Host-owned reviewed records. There is deliberately no Deserialize on a draft:
//! a future calculation service must construct it from validated host inputs,
//! never accept a browser-supplied result. This persistence contract alone does
//! not enable preparing or saving assessments through an API.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{plan::ResearchMode, ResearchContext};
use crate::Result;

#[derive(Debug, Clone)]
pub struct ReviewDraft {
    pub context: ResearchContext,
    pub evidence_hash: String,
    pub methodology_version: String,
    pub engine_build_id: String,
    pub assessment_as_of: String,
    /// Provider/fixture policy supplied by the host producer, not retrieval time.
    pub evidence_expires_at: DateTime<Utc>,
    /// Full immutable input/result snapshot, already validated by the producer.
    /// Storage enforces size, ownership, versions and digest, not financial math.
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Review,
    Saved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResearchRun {
    pub schema_version: u32,
    pub id: String,
    pub mode: ResearchMode,
    pub status: RunStatus,
    pub plan_revision: u64,
    pub context_hash: String,
    pub evidence_hash: String,
    pub input_hash: String,
    pub methodology_version: String,
    pub engine_build_id: String,
    pub assessment_as_of: String,
    pub created_at: String,
    pub expires_at: String,
    pub saved_at: Option<String>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunSummary {
    pub schema_version: u32,
    pub id: String,
    pub mode: ResearchMode,
    pub status: RunStatus,
    pub input_hash: String,
    pub assessment_as_of: String,
    pub created_at: String,
    pub expires_at: String,
    pub saved_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfirmReview {
    pub review_id: String,
    pub expected_input_hash: String,
}

#[async_trait::async_trait]
pub trait ResearchRunRepository: Send + Sync {
    async fn prepare_review(&self, draft: ReviewDraft, now: DateTime<Utc>) -> Result<ResearchRun>;
    async fn confirm_review(
        &self,
        mode: ResearchMode,
        request: ConfirmReview,
        now: DateTime<Utc>,
        base_currency: String,
    ) -> Result<ResearchRun>;
    fn list_runs(&self, mode: ResearchMode) -> Result<Vec<RunSummary>>;
    fn read_run(&self, mode: ResearchMode, id: &str) -> Result<ResearchRun>;
    async fn delete_run(&self, mode: ResearchMode, request: ConfirmReview) -> Result<()>;
}
