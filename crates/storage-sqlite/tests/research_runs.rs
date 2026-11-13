#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "fictional history fixtures"
)]
use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use wealthfolio_core::research::{
    plan::{ResearchMode, ResearchPlan, ResearchPlanRepository, SavePlanRequest},
    runs::{ConfirmReview, ResearchRun, ResearchRunRepository, ReviewDraft, RunStatus},
    ResearchContextRepository,
};
use wealthfolio_storage_sqlite::{
    db::{self, DbAccess},
    research::{plan::PlanRepository, runs::RunRepository, ResearchRepository},
};

const MODE: ResearchMode = ResearchMode::Fictional;
fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-25T04:30:00Z")
        .unwrap()
        .with_timezone(&Utc)
}
fn reference(run: &ResearchRun) -> ConfirmReview {
    ConfirmReview {
        review_id: run.id.clone(),
        expected_input_hash: run.input_hash.clone(),
    }
}
fn input() -> ResearchPlan {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../packages/research-engine/fixtures/conditional-example-v1.json"
    ))
    .unwrap();
    serde_json::from_value(json!({"profile":fixture["portfolio"]["planning"],"cashLinks":[]}))
        .unwrap()
}
async fn fixture() -> (
    tempfile::TempDir,
    DbAccess,
    PlanRepository,
    ResearchRepository,
    RunRepository,
    db::WriteHandle,
) {
    let dir = tempfile::tempdir().unwrap();
    let access = DbAccess::plaintext(dir.path().join("fictional.db").to_str().unwrap());
    access.run_migrations().unwrap();
    let pool = access.create_pool().unwrap();
    let writer = db::write_actor::spawn_writer((*pool).clone()).unwrap();
    let plans = PlanRepository::new(pool.clone(), writer.clone());
    plans
        .save_plan(
            MODE,
            SavePlanRequest {
                expected_revision: 0,
                plan: input(),
            },
        )
        .await
        .unwrap();
    let context = ResearchRepository::new(pool.clone());
    let runs = RunRepository::new(pool, writer.clone());
    (dir, access, plans, context, runs, writer)
}
fn draft(context: &ResearchRepository) -> ReviewDraft {
    ReviewDraft {
        context: context
            .load_context_for_mode(now().date_naive(), "USD", MODE)
            .unwrap(),
        evidence_hash: "a".repeat(64),
        engine_build_id: "b".repeat(64),
        methodology_version: "conditional-decision-2.0".into(),
        assessment_as_of: now().to_rfc3339(),
        evidence_expires_at: now() + Duration::hours(1),
        payload: json!({"fixtureOnly":true,"result":{"score":null,"quantity":"0.99000000"},"proposal":{"budget":"100"},"warnings":["synthetic storage fixture, not a financial assessment"]}),
    }
}

#[tokio::test]
async fn saved_retry_preserves_original_snapshot_after_later_plan_changes() {
    let (_dir, _access, plans, context, runs, writer) = fixture().await;
    let review = runs.prepare_review(draft(&context), now()).await.unwrap();
    let saved = runs
        .confirm_review(MODE, reference(&review), now(), "USD".into())
        .await
        .unwrap();
    assert_eq!(saved.status, RunStatus::Saved);
    let mut newer = input();
    newer.profile.goal_label = "Later plan".into();
    plans
        .save_plan(
            MODE,
            SavePlanRequest {
                expected_revision: 1,
                plan: newer,
            },
        )
        .await
        .unwrap();
    let retry = runs
        .confirm_review(
            MODE,
            reference(&review),
            now() + Duration::days(3),
            "USD".into(),
        )
        .await
        .unwrap();
    assert_eq!(retry, saved);
    assert_eq!(retry.plan_revision, 1);
    assert_eq!(runs.read_run(MODE, &saved.id).unwrap(), saved);
    assert_eq!(runs.list_runs(MODE).unwrap().len(), 1);
    writer.shutdown().await;
}

#[tokio::test]
async fn membership_and_plan_changes_invalidate_prepare_and_pending_confirmation() {
    let (_dir, access, plans, context, runs, writer) = fixture().await;
    let in_flight = draft(&context);
    access.connect_rusqlite().unwrap().execute_batch("INSERT INTO accounts(id,name,currency) VALUES('fictional','Fictional membership change','USD');").unwrap();
    assert!(runs
        .prepare_review(in_flight, now())
        .await
        .unwrap_err()
        .to_string()
        .contains("CONTEXT_CONFLICT"));
    assert!(runs.list_runs(MODE).unwrap().is_empty());
    let review = runs.prepare_review(draft(&context), now()).await.unwrap();
    plans
        .save_plan(
            MODE,
            SavePlanRequest {
                expected_revision: 1,
                plan: input(),
            },
        )
        .await
        .unwrap();
    assert!(runs
        .confirm_review(MODE, reference(&review), now(), "USD".into())
        .await
        .unwrap_err()
        .to_string()
        .contains("CONTEXT_CONFLICT"));
    assert_eq!(
        runs.read_run(MODE, &review.id).unwrap().status,
        RunStatus::Review
    );
    writer.shutdown().await;
}

#[tokio::test]
async fn review_expiry_includes_evidence_freshness_and_rejects_wrong_identity_or_hash() {
    let (_dir, _access, _plans, context, runs, writer) = fixture().await;
    let mut input = draft(&context);
    input.evidence_expires_at = now() + Duration::minutes(2);
    let review = runs.prepare_review(input, now()).await.unwrap();
    assert!(runs
        .confirm_review(
            MODE,
            reference(&review),
            now() + Duration::minutes(2),
            "USD".into()
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("EXPIRED"));
    assert!(runs
        .confirm_review(
            ResearchMode::Personal,
            reference(&review),
            now(),
            "USD".into()
        )
        .await
        .is_err());
    assert!(runs
        .confirm_review(
            MODE,
            ConfirmReview {
                review_id: review.id.clone(),
                expected_input_hash: "c".repeat(64)
            },
            now(),
            "USD".into()
        )
        .await
        .is_err());
    assert!(runs
        .confirm_review(
            MODE,
            ConfirmReview {
                review_id: uuid::Uuid::new_v4().to_string(),
                expected_input_hash: review.input_hash.clone()
            },
            now(),
            "USD".into()
        )
        .await
        .is_err());
    assert!(serde_json::from_value::<ConfirmReview>(
        json!({"reviewId":review.id,"expectedInputHash":review.input_hash,"result":{"score":10}})
    )
    .is_err());
    let mut stale = draft(&context);
    stale.evidence_expires_at = now();
    assert!(runs.prepare_review(stale, now()).await.is_err());
    writer.shutdown().await;
}

#[tokio::test]
async fn capacity_bounds_never_automatically_prune_saved_history() {
    let (_dir, _access, _plans, context, runs, writer) = fixture().await;
    for _ in 0..5 {
        runs.prepare_review(draft(&context), now()).await.unwrap();
    }
    assert!(runs
        .prepare_review(draft(&context), now())
        .await
        .unwrap_err()
        .to_string()
        .contains("REVIEW_LIMIT"));
    let later = now() + Duration::minutes(31);
    let clean = runs.prepare_review(draft(&context), later).await.unwrap();
    assert_eq!(runs.list_runs(MODE).unwrap().len(), 1);
    // The first draft's evidence expires at +60 minutes; every save is before it.
    runs.confirm_review(MODE, reference(&clean), later, "USD".into())
        .await
        .unwrap();
    for _ in 1..20 {
        let review = runs.prepare_review(draft(&context), later).await.unwrap();
        runs.confirm_review(MODE, reference(&review), later, "USD".into())
            .await
            .unwrap();
    }
    let blocked = runs.prepare_review(draft(&context), later).await.unwrap();
    assert!(runs
        .confirm_review(MODE, reference(&blocked), later, "USD".into())
        .await
        .unwrap_err()
        .to_string()
        .contains("SAVED_RUN_LIMIT"));
    assert_eq!(
        runs.list_runs(MODE)
            .unwrap()
            .iter()
            .filter(|r| r.status == RunStatus::Saved)
            .count(),
        20
    );
    assert!(runs.read_run(MODE, &clean.id).is_ok());
    writer.shutdown().await;
}

#[tokio::test]
async fn hash_and_profile_bound_deletion_does_not_remove_other_records() {
    let (_dir, _access, _plans, context, runs, writer) = fixture().await;
    let review = runs.prepare_review(draft(&context), now()).await.unwrap();
    let (_other_dir, _other_access, _other_plans, _other_context, other, other_writer) =
        fixture().await;
    assert!(other.read_run(MODE, &review.id).is_err());
    assert!(other.delete_run(MODE, reference(&review)).await.is_err());
    assert!(runs
        .delete_run(ResearchMode::Personal, reference(&review))
        .await
        .is_err());
    assert!(runs
        .delete_run(
            MODE,
            ConfirmReview {
                review_id: review.id.clone(),
                expected_input_hash: "d".repeat(64)
            }
        )
        .await
        .is_err());
    assert!(runs.read_run(MODE, &review.id).is_ok());
    runs.delete_run(MODE, reference(&review)).await.unwrap();
    assert!(runs.read_run(MODE, &review.id).is_err());
    writer.shutdown().await;
    other_writer.shutdown().await;
}

#[tokio::test]
async fn failed_confirmation_rolls_back_and_snapshot_tampering_is_detected() {
    let (_dir, access, _plans, context, runs, writer) = fixture().await;
    let review = runs.prepare_review(draft(&context), now()).await.unwrap();
    access.connect_rusqlite().unwrap().execute_batch("CREATE TRIGGER reject_run_save BEFORE UPDATE ON research_runs BEGIN SELECT RAISE(ABORT,'fictional failure'); END;").unwrap();
    assert!(runs
        .confirm_review(MODE, reference(&review), now(), "USD".into())
        .await
        .is_err());
    assert_eq!(runs.read_run(MODE, &review.id).unwrap(), review);
    access
        .connect_rusqlite()
        .unwrap()
        .execute_batch(
            "DROP TRIGGER reject_run_save; UPDATE research_runs SET payload='{\"altered\":true}';",
        )
        .unwrap();
    assert!(runs
        .read_run(MODE, &review.id)
        .unwrap_err()
        .to_string()
        .contains("INTEGRITY_FAILED"));
    writer.shutdown().await;
}

#[tokio::test]
async fn oversized_snapshot_rejected_before_insertion_and_migration_is_repeatable() {
    let (_dir, access, _plans, context, runs, writer) = fixture().await;
    let mut oversized = draft(&context);
    oversized.payload = json!({"oversized":"£".repeat(1024*1024)});
    assert!(runs
        .prepare_review(oversized, now())
        .await
        .unwrap_err()
        .to_string()
        .contains("SIZE_LIMIT"));
    assert!(runs.list_runs(MODE).unwrap().is_empty());
    access.run_migrations().unwrap();
    let review = runs.prepare_review(draft(&context), now()).await.unwrap();
    assert_eq!(runs.read_run(MODE, &review.id).unwrap(), review);
    writer.shutdown().await;
}
