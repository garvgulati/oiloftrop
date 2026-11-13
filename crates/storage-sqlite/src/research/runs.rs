//! Bounded immutable history using the existing immediate writer transaction.
use crate::{
    db::{get_connection, WriteHandle},
    errors::StorageError,
    DbPool,
};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use diesel::{
    prelude::*,
    sql_types::{BigInt, Nullable, Text},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use wealthfolio_core::{
    research::{
        plan::ResearchMode,
        runs::{
            ConfirmReview, ResearchRun, ResearchRunRepository, ReviewDraft, RunStatus, RunSummary,
        },
    },
    Error, Result,
};

const LIMIT: usize = 2 * 1024 * 1024;
pub struct RunRepository {
    pool: Arc<DbPool>,
    writer: WriteHandle,
}
impl RunRepository {
    pub fn new(pool: Arc<DbPool>, writer: WriteHandle) -> Self {
        Self { pool, writer }
    }
}
fn fail(code: &str) -> Error {
    Error::ConstraintViolation(code.into())
}
fn digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn day(now: DateTime<Utc>) -> chrono::NaiveDate {
    now.with_timezone(&chrono::FixedOffset::east_opt(19_800).expect("valid India offset"))
        .date_naive()
}
fn hash(run: &ResearchRun) -> Result<String> {
    // Excludes mutable review/saved status and wall-clock save metadata.
    let bytes=serde_json::to_vec(&json!({"schemaVersion":run.schema_version,"mode":run.mode,"contextHash":run.context_hash,"planRevision":run.plan_revision,"evidenceHash":run.evidence_hash,"methodologyVersion":run.methodology_version,"engineBuildId":run.engine_build_id,"assessmentAsOf":run.assessment_as_of,"payload":run.payload})).map_err(|_|fail("RESEARCH_RUN_INVALID"))?;
    if bytes.len() > LIMIT {
        return Err(fail("RESEARCH_RUN_SIZE_LIMIT"));
    }
    Ok(hex::encode(Sha256::digest(bytes)))
}

#[derive(QueryableByName)]
struct RunRow {
    #[diesel(sql_type=Text)]
    id: String,
    #[diesel(sql_type=Text)]
    mode: String,
    #[diesel(sql_type=BigInt)]
    schema_version: i64,
    #[diesel(sql_type=Text)]
    status: String,
    #[diesel(sql_type=BigInt)]
    plan_revision: i64,
    #[diesel(sql_type=Text)]
    context_hash: String,
    #[diesel(sql_type=Text)]
    evidence_hash: String,
    #[diesel(sql_type=Text)]
    input_hash: String,
    #[diesel(sql_type=Text)]
    methodology_version: String,
    #[diesel(sql_type=Text)]
    engine_build_id: String,
    #[diesel(sql_type=Text)]
    assessment_as_of: String,
    #[diesel(sql_type=Text)]
    payload: String,
    #[diesel(sql_type=Text)]
    created_at: String,
    #[diesel(sql_type=Text)]
    expires_at: String,
    #[diesel(sql_type=Nullable<Text>)]
    saved_at: Option<String>,
}

fn read(conn: &mut SqliteConnection, mode: ResearchMode, id: &str) -> Result<ResearchRun> {
    if uuid::Uuid::parse_str(id).is_err() {
        return Err(fail("RESEARCH_RUN_NOT_FOUND"));
    }
    let row=diesel::sql_query("SELECT id,mode,CAST(schema_version AS INTEGER) AS schema_version,status,plan_revision,context_hash,evidence_hash,input_hash,methodology_version,engine_build_id,assessment_as_of,CASE WHEN length(CAST(payload AS BLOB))<=2097152 THEN payload ELSE '' END AS payload,created_at,expires_at,saved_at FROM research_runs WHERE id=? AND mode=?")
        .bind::<Text,_>(id).bind::<Text,_>(mode.as_str()).get_result::<RunRow>(conn).optional().map_err(StorageError::from)?
        .ok_or_else(||fail("RESEARCH_RUN_NOT_FOUND"))?;
    if row.schema_version != 1
        || row.plan_revision < 1
        || row.plan_revision > 9_007_199_254_740_991
        || row.mode != mode.as_str()
    {
        return Err(fail("RESEARCH_RUN_VERSION_UNSUPPORTED"));
    }
    let status = match row.status.as_str() {
        "review" => RunStatus::Review,
        "saved" => RunStatus::Saved,
        _ => return Err(fail("RESEARCH_RUN_INVALID")),
    };
    let run = ResearchRun {
        schema_version: 1,
        id: row.id,
        mode,
        status,
        plan_revision: row.plan_revision as u64,
        context_hash: row.context_hash,
        evidence_hash: row.evidence_hash,
        input_hash: row.input_hash,
        methodology_version: row.methodology_version,
        engine_build_id: row.engine_build_id,
        assessment_as_of: row.assessment_as_of,
        created_at: row.created_at,
        expires_at: row.expires_at,
        saved_at: row.saved_at,
        payload: serde_json::from_str(&row.payload).map_err(|_| fail("RESEARCH_RUN_INVALID"))?,
    };
    if hash(&run)? != run.input_hash {
        return Err(fail("RESEARCH_RUN_INTEGRITY_FAILED"));
    }
    Ok(run)
}

#[derive(QueryableByName)]
struct Count {
    #[diesel(sql_type=BigInt)]
    count: i64,
}
fn count(conn: &mut SqliteConnection, mode: ResearchMode, status: &str) -> Result<i64> {
    Ok(
        diesel::sql_query("SELECT count(*) AS count FROM research_runs WHERE mode=? AND status=?")
            .bind::<Text, _>(mode.as_str())
            .bind::<Text, _>(status)
            .get_result::<Count>(conn)
            .map_err(StorageError::from)?
            .count,
    )
}

#[async_trait]
impl ResearchRunRepository for RunRepository {
    async fn prepare_review(&self, draft: ReviewDraft, now: DateTime<Utc>) -> Result<ResearchRun> {
        if draft.context.schema_version != 1
            || draft.context.plan_revision == 0
            || !digest(&draft.context.context_hash)
            || !digest(&draft.evidence_hash)
            || !digest(&draft.engine_build_id)
            || draft.methodology_version != "conditional-decision-2.0"
            || !draft.payload.is_object()
            || DateTime::parse_from_rfc3339(&draft.assessment_as_of).is_err()
            || draft.evidence_expires_at <= now
        {
            return Err(fail("RESEARCH_RUN_INVALID"));
        }
        let mode = draft.context.mode;
        let payload = json!({"context":draft.context,"assessment":draft.payload,"evidenceExpiresAt":draft.evidence_expires_at.to_rfc3339()});
        let mut run = ResearchRun {
            schema_version: 1,
            id: uuid::Uuid::new_v4().to_string(),
            mode,
            status: RunStatus::Review,
            plan_revision: draft.context.plan_revision,
            context_hash: draft.context.context_hash.clone(),
            evidence_hash: draft.evidence_hash,
            engine_build_id: draft.engine_build_id,
            methodology_version: draft.methodology_version,
            assessment_as_of: draft.assessment_as_of,
            payload,
            created_at: now.to_rfc3339(),
            expires_at: (now + Duration::minutes(30))
                .min(draft.evidence_expires_at)
                .to_rfc3339(),
            saved_at: None,
            input_hash: String::new(),
        };
        run.input_hash = hash(&run)?;
        self.writer.exec_tx(move |tx| {
            let mut current=super::load_context_for_mode_on_connection(tx.conn(),day(now),&draft.context.base_currency,mode)?;
            // The hash does not authorize a substituted decoded snapshot.
            // Capture time alone may differ between coherent reads.
            current.captured_at=draft.context.captured_at.clone();
            if current!=draft.context {return Err(fail("RESEARCH_REVIEW_CONTEXT_CONFLICT"));}
            diesel::sql_query("DELETE FROM research_runs WHERE mode=? AND status='review' AND julianday(expires_at)<=julianday(?)")
                .bind::<Text,_>(mode.as_str()).bind::<Text,_>(&run.created_at).execute(tx.conn()).map_err(StorageError::from)?;
            if count(tx.conn(),mode,"review")?>=5 {return Err(fail("RESEARCH_REVIEW_LIMIT"));}
            let payload=serde_json::to_string(&run.payload).map_err(|_|fail("RESEARCH_RUN_INVALID"))?;
            diesel::sql_query("INSERT INTO research_runs(id,mode,schema_version,status,plan_revision,context_hash,evidence_hash,input_hash,methodology_version,engine_build_id,assessment_as_of,payload,created_at,expires_at,saved_at) VALUES(?,?,1,'review',?,?,?,?,?,?,?,?,?,?,NULL)")
                .bind::<Text,_>(&run.id).bind::<Text,_>(mode.as_str()).bind::<BigInt,_>(run.plan_revision as i64)
                .bind::<Text,_>(&run.context_hash).bind::<Text,_>(&run.evidence_hash).bind::<Text,_>(&run.input_hash)
                .bind::<Text,_>(&run.methodology_version).bind::<Text,_>(&run.engine_build_id).bind::<Text,_>(&run.assessment_as_of)
                .bind::<Text,_>(&payload).bind::<Text,_>(&run.created_at).bind::<Text,_>(&run.expires_at)
                .execute(tx.conn()).map_err(StorageError::from)?;
            Ok(run)
        }).await
    }

    async fn confirm_review(
        &self,
        mode: ResearchMode,
        request: ConfirmReview,
        now: DateTime<Utc>,
        base_currency: String,
    ) -> Result<ResearchRun> {
        self.writer.exec_tx(move |tx| {
            let mut run=read(tx.conn(),mode,&request.review_id)?;
            if run.input_hash!=request.expected_input_hash {return Err(fail("RESEARCH_REVIEW_HASH_CONFLICT"));}
            if run.status==RunStatus::Saved {return Ok(run);}
            let expires=DateTime::parse_from_rfc3339(&run.expires_at).map_err(|_|fail("RESEARCH_RUN_INVALID"))?;
            if now>=expires {return Err(fail("RESEARCH_REVIEW_EXPIRED"));}
            let current=super::load_context_for_mode_on_connection(tx.conn(),day(now),&base_currency,mode)?;
            if current.context_hash!=run.context_hash || current.plan_revision!=run.plan_revision {return Err(fail("RESEARCH_REVIEW_CONTEXT_CONFLICT"));}
            if count(tx.conn(),mode,"saved")?>=20 {return Err(fail("RESEARCH_SAVED_RUN_LIMIT"));}
            let saved=now.to_rfc3339();
            let changed=diesel::sql_query("UPDATE research_runs SET status='saved',saved_at=? WHERE id=? AND mode=? AND status='review' AND input_hash=?")
                .bind::<Text,_>(&saved).bind::<Text,_>(&run.id).bind::<Text,_>(mode.as_str()).bind::<Text,_>(&run.input_hash)
                .execute(tx.conn()).map_err(StorageError::from)?;
            if changed!=1 {return Err(fail("RESEARCH_REVIEW_CONTEXT_CONFLICT"));}
            run.status=RunStatus::Saved;run.saved_at=Some(saved);Ok(run)
        }).await
    }

    fn list_runs(&self, mode: ResearchMode) -> Result<Vec<RunSummary>> {
        #[derive(QueryableByName)]
        struct Row {
            #[diesel(sql_type=Text)]
            id: String,
            #[diesel(sql_type=Text)]
            status: String,
            #[diesel(sql_type=Text)]
            input_hash: String,
            #[diesel(sql_type=Text)]
            assessment_as_of: String,
            #[diesel(sql_type=Text)]
            created_at: String,
            #[diesel(sql_type=Text)]
            expires_at: String,
            #[diesel(sql_type=Nullable<Text>)]
            saved_at: Option<String>,
        }
        let mut conn = get_connection(&self.pool)?;
        // No payloads loaded for listing. Retain expired reviews until prepare.
        let rows=diesel::sql_query("SELECT id,status,input_hash,assessment_as_of,created_at,expires_at,saved_at FROM research_runs WHERE mode=? ORDER BY created_at DESC,id DESC LIMIT 26")
            .bind::<Text,_>(mode.as_str()).load::<Row>(&mut conn).map_err(StorageError::from)?;
        if rows.len() > 25 {
            return Err(fail("RESEARCH_RUN_LIMIT_INTEGRITY_FAILED"));
        }
        rows.into_iter()
            .map(|r| {
                Ok(RunSummary {
                    schema_version: 1,
                    id: r.id,
                    mode,
                    status: match r.status.as_str() {
                        "review" => RunStatus::Review,
                        "saved" => RunStatus::Saved,
                        _ => return Err(fail("RESEARCH_RUN_INVALID")),
                    },
                    input_hash: r.input_hash,
                    assessment_as_of: r.assessment_as_of,
                    created_at: r.created_at,
                    expires_at: r.expires_at,
                    saved_at: r.saved_at,
                })
            })
            .collect()
    }

    fn read_run(&self, mode: ResearchMode, id: &str) -> Result<ResearchRun> {
        let mut conn = get_connection(&self.pool)?;
        read(&mut conn, mode, id)
    }

    async fn delete_run(&self, mode: ResearchMode, request: ConfirmReview) -> Result<()> {
        self.writer
            .exec_tx(move |tx| {
                let run = read(tx.conn(), mode, &request.review_id)?;
                if run.input_hash != request.expected_input_hash {
                    return Err(fail("RESEARCH_REVIEW_HASH_CONFLICT"));
                }
                diesel::sql_query(
                    "DELETE FROM research_runs WHERE id=? AND mode=? AND input_hash=?",
                )
                .bind::<Text, _>(&run.id)
                .bind::<Text, _>(mode.as_str())
                .bind::<Text, _>(&run.input_hash)
                .execute(tx.conn())
                .map_err(StorageError::from)?;
                Ok(())
            })
            .await
    }
}
