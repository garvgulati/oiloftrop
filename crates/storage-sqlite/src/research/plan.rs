use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use diesel::{
    prelude::*,
    sql_types::{BigInt, Integer, Text},
};
use wealthfolio_core::{
    research::plan::{
        ResearchMode, ResearchPlan, ResearchPlanRepository, SavePlanRequest, StoredPlan,
    },
    Error, Result,
};

use crate::{
    db::{get_connection, WriteHandle},
    errors::StorageError,
    DbPool,
};

pub struct PlanRepository {
    pool: Arc<DbPool>,
    writer: WriteHandle,
}
impl PlanRepository {
    pub fn new(pool: Arc<DbPool>, writer: WriteHandle) -> Self {
        Self { pool, writer }
    }
}

#[derive(QueryableByName)]
struct PlanRow {
    #[diesel(sql_type = Integer)]
    schema_version: i32,
    #[diesel(sql_type = BigInt)]
    revision: i64,
    #[diesel(sql_type = Text)]
    payload: String,
    #[diesel(sql_type = Text)]
    created_at: String,
    #[diesel(sql_type = Text)]
    updated_at: String,
}

pub fn load_plan_on_connection(
    conn: &mut SqliteConnection,
    mode: ResearchMode,
) -> Result<Option<StoredPlan>> {
    let row = diesel::sql_query("SELECT schema_version,revision,CASE WHEN length(CAST(payload AS BLOB)) <= 262144 THEN payload ELSE '' END AS payload,created_at,updated_at FROM research_plan WHERE mode=?")
        .bind::<Text,_>(mode.as_str()).get_result::<PlanRow>(conn).optional().map_err(StorageError::from)?;
    row.map(|row| {
        if row.schema_version != 1 || row.revision < 1 || row.revision > 9_007_199_254_740_991 {
            return Err(Error::ConstraintViolation(
                "RESEARCH_PLAN_VERSION_UNSUPPORTED".into(),
            ));
        }
        let plan: ResearchPlan = serde_json::from_str(&row.payload)
            .map_err(|_| Error::ConstraintViolation("RESEARCH_PLAN_INVALID".into()))?;
        plan.validate()?;
        Ok(StoredPlan {
            schema_version: 1,
            mode,
            revision: row.revision as u64,
            plan,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    })
    .transpose()
}

#[async_trait]
impl ResearchPlanRepository for PlanRepository {
    fn load_plan(&self, mode: ResearchMode) -> Result<Option<StoredPlan>> {
        let mut conn = get_connection(&self.pool)?;
        load_plan_on_connection(&mut conn, mode)
    }

    async fn save_plan(&self, mode: ResearchMode, request: SavePlanRequest) -> Result<StoredPlan> {
        request.plan.validate()?;
        if request.expected_revision >= 9_007_199_254_740_991 {
            return Err(Error::ConstraintViolation(
                "RESEARCH_PLAN_REVISION_INVALID".into(),
            ));
        }
        self.writer.exec_tx(move |tx| {
            let current = load_plan_on_connection(tx.conn(), mode)?;
            if current.as_ref().map_or(0, |p| p.revision) != request.expected_revision {
                return Err(Error::ConstraintViolation("RESEARCH_PLAN_EDIT_CONFLICT".into()));
            }
            // Resolve links under this same immediate write transaction. A manual
            // cash answer cannot silently duplicate unknown or different broker cash.
            if !request.plan.cash_links.is_empty() {
                let day = Utc::now().with_timezone(&chrono::FixedOffset::east_opt(19_800).expect("valid India offset")).date_naive();
                let context = super::load_context_for_mode_on_connection(tx.conn(), day, request.plan.profile.base_currency.as_str(), mode)?;
                if context.issues.iter().any(|issue| matches!(issue.code.as_str(), "PROJECTION_DIRTY" | "ACTIVITY_ISSUES" | "CORRUPT_SNAPSHOT" | "SNAPSHOT_MISSING")) {
                    return Err(Error::ConstraintViolation("RESEARCH_CASH_LINK_RECORDS_INCOMPLETE".into()));
                }
                for link in &request.plan.cash_links {
                    let cash = request.plan.profile.cash.iter().find(|v| v.id == link.cash_id)
                        .ok_or_else(|| Error::ConstraintViolation("RESEARCH_CASH_LINK_INVALID".into()))?;
                    let account = context.accounts.iter().find(|a| a.id == link.account_id)
                        .ok_or_else(|| Error::ConstraintViolation("RESEARCH_CASH_LINK_INVALID".into()))?;
                    let linked = account.cash.get(cash.currency.as_str()).and_then(Option::as_deref);
                    if linked.is_none_or(|raw| raw.parse::<rust_decimal::Decimal>().ok() != cash.amount.parse::<rust_decimal::Decimal>().ok())
                        || account.snapshot_date.as_deref() != Some(cash.date.as_str())
                    { return Err(Error::ConstraintViolation("RESEARCH_CASH_LINK_CONFLICT".into())); }
                }
            }
            let now = Utc::now().to_rfc3339();
            let revision = request.expected_revision + 1;
            let payload = serde_json::to_string(&request.plan).map_err(|_| Error::ConstraintViolation("RESEARCH_PLAN_INVALID".into()))?;
            if request.expected_revision == 0 {
                diesel::sql_query("INSERT INTO research_plan(mode,schema_version,revision,payload,created_at,updated_at) VALUES(?,1,1,?,?,?)")
                    .bind::<Text,_>(mode.as_str()).bind::<Text,_>(&payload).bind::<Text,_>(&now).bind::<Text,_>(&now)
                    .execute(tx.conn()).map_err(StorageError::from)?;
            } else {
                let count = diesel::sql_query("UPDATE research_plan SET revision=?,payload=?,updated_at=? WHERE mode=? AND revision=?")
                    .bind::<BigInt,_>(revision as i64).bind::<Text,_>(&payload).bind::<Text,_>(&now)
                    .bind::<Text,_>(mode.as_str()).bind::<BigInt,_>(request.expected_revision as i64)
                    .execute(tx.conn()).map_err(StorageError::from)?;
                if count != 1 { return Err(Error::ConstraintViolation("RESEARCH_PLAN_EDIT_CONFLICT".into())); }
            }
            Ok(StoredPlan { schema_version: 1, mode, revision, plan: request.plan,
                created_at: current.map_or_else(|| now.clone(), |p| p.created_at), updated_at: now })
        }).await
    }
}
