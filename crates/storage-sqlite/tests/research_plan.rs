#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "fictional research fixtures"
)]

use chrono::NaiveDate;
use serde_json::json;
use std::sync::Arc;
use wealthfolio_core::research::{
    plan::{ResearchMode, ResearchPlan, ResearchPlanRepository, SavePlanRequest},
    ResearchContextRepository,
};
use wealthfolio_storage_sqlite::{
    db::{self, DbAccess, DbEncryptionKey},
    research::{plan::PlanRepository, ResearchRepository},
};

fn plan() -> ResearchPlan {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../packages/research-engine/fixtures/conditional-example-v1.json"
    ))
    .unwrap();
    serde_json::from_value(json!({"profile":fixture["portfolio"]["planning"],"cashLinks":[]}))
        .unwrap()
}

fn request(revision: u64, name: &str) -> SavePlanRequest {
    let mut plan = plan();
    plan.profile.goal_label = name.into();
    SavePlanRequest {
        expected_revision: revision,
        plan,
    }
}

fn fixture() -> (tempfile::TempDir, DbAccess, PlanRepository, db::WriteHandle) {
    let dir = tempfile::tempdir().unwrap();
    let access = DbAccess::plaintext(dir.path().join("fictional.db").to_str().unwrap());
    access.run_migrations().unwrap();
    let pool = access.create_pool().unwrap();
    let writer = db::write_actor::spawn_writer((*pool).clone()).unwrap();
    let repository = PlanRepository::new(pool, writer.clone());
    (dir, access, repository, writer)
}

#[tokio::test]
async fn creation_and_stale_edit_cannot_overwrite_newer_answers() {
    let (_dir, _access, repo, writer) = fixture();
    assert!(repo.load_plan(ResearchMode::Fictional).unwrap().is_none());
    let saved = repo
        .save_plan(ResearchMode::Fictional, request(0, "First device"))
        .await
        .unwrap();
    assert_eq!(saved.revision, 1);
    let error = repo
        .save_plan(ResearchMode::Fictional, request(0, "Stale create"))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("RESEARCH_PLAN_EDIT_CONFLICT"));
    let next = repo
        .save_plan(
            ResearchMode::Fictional,
            request(1, "Reviewed second device"),
        )
        .await
        .unwrap();
    assert_eq!(next.revision, 2);
    assert_eq!(saved.created_at, next.created_at);
    assert!(repo
        .save_plan(ResearchMode::Fictional, request(1, "Stale edit"))
        .await
        .is_err());
    assert_eq!(
        repo.load_plan(ResearchMode::Fictional).unwrap().unwrap(),
        next
    );
    assert!(serde_json::from_value::<SavePlanRequest>(json!({"plan":plan()})).is_err());
    assert!(serde_json::from_value::<SavePlanRequest>(
        json!({"expectedRevision":0,"plan":plan(),"mode":"personal"})
    )
    .is_err());
    writer.shutdown().await;
}

#[tokio::test]
async fn independent_writers_accept_exactly_one_draft_from_a_revision() {
    let (_dir, access, repo, writer) = fixture();
    repo.save_plan(ResearchMode::Fictional, request(0, "Opened"))
        .await
        .unwrap();
    let pool = access.create_pool().unwrap();
    let second_writer = db::write_actor::spawn_writer((*pool).clone()).unwrap();
    let second = PlanRepository::new(pool, second_writer.clone());
    let (a, b) = tokio::join!(
        repo.save_plan(ResearchMode::Fictional, request(1, "A")),
        second.save_plan(ResearchMode::Fictional, request(1, "B"))
    );
    assert_ne!(a.is_ok(), b.is_ok());
    assert_eq!(
        repo.load_plan(ResearchMode::Fictional)
            .unwrap()
            .unwrap()
            .revision,
        2
    );
    writer.shutdown().await;
    second_writer.shutdown().await;
}

#[tokio::test]
async fn modes_and_profile_databases_are_separate_and_plan_invalidates_context() {
    let (_dir, access, repo, writer) = fixture();
    let context = ResearchRepository::new(access.create_pool().unwrap());
    let day = NaiveDate::from_ymd_opt(2026, 10, 25).unwrap();
    let before = context
        .load_context_for_mode(day, "INR", ResearchMode::Fictional)
        .unwrap();
    repo.save_plan(ResearchMode::Fictional, request(0, "Example only"))
        .await
        .unwrap();
    let after = context
        .load_context_for_mode(day, "INR", ResearchMode::Fictional)
        .unwrap();
    assert_ne!(before.context_hash, after.context_hash);
    assert_eq!(after.plan_revision, 1);
    assert!(context.load_context(day, "INR").unwrap().plan.is_none());
    assert!(repo.load_plan(ResearchMode::Personal).unwrap().is_none());
    let (_other_dir, _other_access, other, other_writer) = fixture();
    assert!(other.load_plan(ResearchMode::Fictional).unwrap().is_none());
    writer.shutdown().await;
    other_writer.shutdown().await;
}

#[tokio::test]
async fn transaction_failure_preserves_previous_plan_and_revision() {
    let (_dir, access, repo, writer) = fixture();
    let before = repo
        .save_plan(ResearchMode::Fictional, request(0, "Before"))
        .await
        .unwrap();
    access.connect_rusqlite().unwrap().execute_batch("CREATE TRIGGER reject_research_update BEFORE UPDATE ON research_plan BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    assert!(repo
        .save_plan(ResearchMode::Fictional, request(1, "After"))
        .await
        .is_err());
    assert_eq!(
        repo.load_plan(ResearchMode::Fictional).unwrap().unwrap(),
        before
    );
    writer.shutdown().await;
}

#[test]
fn unknown_values_remain_unknown_and_invalid_or_unbounded_input_is_rejected() {
    let original = plan();
    original.validate().unwrap();
    let mut unknown = original.clone();
    unknown.profile.can_afford_to_lose_base = None;
    unknown.profile.cash_complete = false;
    unknown.validate().unwrap();
    let roundtrip: ResearchPlan =
        serde_json::from_str(&serde_json::to_string(&unknown).unwrap()).unwrap();
    assert!(roundtrip.profile.can_afford_to_lose_base.is_none());
    assert!(!roundtrip.profile.cash_complete);
    for mutate in [
        |p: &mut ResearchPlan| p.profile.goal_amount = Some("1e3".into()),
        |p: &mut ResearchPlan| p.profile.can_afford_to_lose_base = Some("0.123456789".into()),
        |p: &mut ResearchPlan| p.profile.max_drawdown_pct = Some("100.00000001".into()),
        |p: &mut ResearchPlan| p.profile.confirmed_date = "2026-02-30".into(),
        |p: &mut ResearchPlan| p.profile.real_hurdle_pct = Some("-100".into()),
        |p: &mut ResearchPlan| p.profile.goal_date = None,
        |p: &mut ResearchPlan| p.profile.restrictions = "x".repeat(1501),
    ] {
        let mut invalid = original.clone();
        mutate(&mut invalid);
        assert!(invalid.validate().is_err());
    }
    let mut invalid = serde_json::to_value(original).unwrap();
    invalid["profile"]["key"] = "must not store".into();
    assert!(serde_json::from_value::<ResearchPlan>(invalid).is_err());
}

#[tokio::test]
async fn linked_cash_must_match_existing_account_currency_date_and_exact_balance() {
    let (_dir, access, repo, writer) = fixture();
    access.connect_rusqlite().unwrap().execute_batch("INSERT INTO accounts(id,name,currency,tracking_mode) VALUES('fictional','Fictional','INR','HOLDINGS'); INSERT INTO holdings_snapshots(id,account_id,snapshot_date,currency,cash_balances) VALUES('cash','fictional','2026-10-12','INR','{\"INR\":\"123.45678901\"}'); UPDATE projection_state SET dirty_from=NULL,activity_issues='[]';").unwrap();
    let mut value = plan();
    let cash_id = "11111111-1111-4111-8111-111111111111";
    value.profile.cash=serde_json::from_value(json!([{"id":cash_id,"name":"Fictional cash","currency":"INR","amount":"123.45678901","reserve":"0","date":"2026-10-12","reconciled":true}])).unwrap();
    value.cash_links =
        serde_json::from_value(json!([{"cashId":cash_id,"accountId":"fictional"}])).unwrap();
    repo.save_plan(
        ResearchMode::Fictional,
        SavePlanRequest {
            expected_revision: 0,
            plan: value.clone(),
        },
    )
    .await
    .unwrap();
    value.profile.cash[0].amount = "123.45678902".into();
    assert!(repo
        .save_plan(
            ResearchMode::Fictional,
            SavePlanRequest {
                expected_revision: 1,
                plan: value.clone()
            }
        )
        .await
        .is_err());
    value.profile.cash[0].amount = "123.45678901".into();
    value.cash_links[0].account_id = "missing".into();
    assert!(repo
        .save_plan(
            ResearchMode::Fictional,
            SavePlanRequest {
                expected_revision: 1,
                plan: value
            }
        )
        .await
        .is_err());
    writer.shutdown().await;
}

#[test]
fn cash_reserves_and_source_links_do_not_lose_small_amounts_or_duplicate_balances() {
    let mut value = plan();
    value.profile.cash = serde_json::from_value(json!([{
        "id":"90000000-0000-4000-8000-000000000001","name":"Fictional cash","currency":"USD",
        "amount":"900000000000.00000001","reserve":"900000000000.00000002",
        "date":"2026-10-12","reconciled":true
    }]))
    .unwrap();
    assert!(
        value.validate().is_err(),
        "A one-unit excess at eight decimal places cannot round away"
    );
    value.profile.cash[0].reserve = "0".into();
    value.cash_links = serde_json::from_value(json!([
        {"cashId":"90000000-0000-4000-8000-000000000001","accountId":"fictional"},
        {"cashId":"90000000-0000-4000-8000-000000000001","accountId":"fictional"}
    ]))
    .unwrap();
    assert!(value.validate().is_err());
    value.cash_links.clear();
    value.profile.cash.push(value.profile.cash[0].clone());
    assert!(value.validate().is_err());
}

#[test]
fn debt_components_reconcile_exactly_and_an_overpayment_cannot_be_clipped() {
    let mut value = plan();
    value.profile.debts = serde_json::from_value(json!([{
        "id":"90000000-0000-4000-8000-000000000002","label":"Fictional loan","currency":"USD",
        "balance":"10.00000001","annualInterestPct":null,"balanceDate":"2026-10-12",
        "repayments":[{"id":"90000000-0000-4000-8000-000000000003","label":"Fictional repayment","currency":"USD","amount":"2","date":"2026-11-12"}],
        "liabilityPath":{"openingDate":"2026-10-12","principal":"9","interestDue":"1.00000001","feesDue":"0","confirmedThrough":"2026-11-12",
            "rows":[{"date":"2026-11-12","interestAccrued":"0","feesAccrued":"0","capitalizedInterest":"1.00000001","capitalizedFees":"0","principalPaid":"2","interestPaid":"0","feesPaid":"0"}]}
    }])).unwrap();
    value.validate().unwrap();
    value.profile.debts[0].liability_path.as_mut().unwrap().rows[0].fees_paid = "0.00000001".into();
    assert!(
        value.validate().is_err(),
        "The payment components must match the actual cash payment exactly"
    );
    value.profile.debts[0].liability_path.as_mut().unwrap().rows[0].fees_paid = "0".into();
    value.profile.debts[0].liability_path.as_mut().unwrap().rows[0].principal_paid = "21".into();
    value.profile.debts[0].repayments[0].amount = "21".into();
    assert!(value.validate().is_err());
}

#[tokio::test]
async fn retained_key_snapshot_restores_plan_and_wrong_key_cannot_read_it() {
    use wealthfolio_core::research::runs::{ConfirmReview, ResearchRunRepository, ReviewDraft};
    use wealthfolio_storage_sqlite::research::runs::RunRepository;
    let dir = tempfile::tempdir().unwrap();
    let key = Arc::new(DbEncryptionKey::generate());
    let access = DbAccess::encrypted(
        dir.path().join("encrypted.db").to_str().unwrap(),
        key.clone(),
    );
    access.run_migrations().unwrap();
    let pool = access.create_pool().unwrap();
    let writer = db::write_actor::spawn_writer((*pool).clone()).unwrap();
    let context_repo = ResearchRepository::new(pool.clone());
    let run_repo = RunRepository::new(pool.clone(), writer.clone());
    let repo = PlanRepository::new(pool, writer.clone());
    let saved = repo
        .save_plan(
            ResearchMode::Fictional,
            request(0, "Fictional encrypted answer"),
        )
        .await
        .unwrap();
    let time = chrono::DateTime::parse_from_rfc3339("2026-10-25T04:30:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let context = context_repo
        .load_context_for_mode(time.date_naive(), "USD", ResearchMode::Fictional)
        .unwrap();
    let review = run_repo
        .prepare_review(
            ReviewDraft {
                context,
                evidence_hash: "a".repeat(64),
                engine_build_id: "b".repeat(64),
                methodology_version: "conditional-decision-2.0".into(),
                assessment_as_of: time.to_rfc3339(),
                evidence_expires_at: time + chrono::Duration::hours(1),
                payload: json!({"fictionalStorageFixture":true,"result":{"score":null}}),
            },
            time,
        )
        .await
        .unwrap();
    let saved_run = run_repo
        .confirm_review(
            ResearchMode::Fictional,
            ConfirmReview {
                review_id: review.id.clone(),
                expected_input_hash: review.input_hash.clone(),
            },
            time,
            "USD".into(),
        )
        .await
        .unwrap();
    let snapshot = dir.path().join("snapshot.db");
    db::backup_database_to_file(&access, snapshot.to_str().unwrap()).unwrap();
    assert_ne!(
        &std::fs::read(&snapshot).unwrap()[..16],
        b"SQLite format 3\0"
    );
    let restored = DbAccess::encrypted(snapshot.to_str().unwrap(), key);
    restored.run_migrations().unwrap();
    let restored_pool = restored.create_pool().unwrap();
    let restored_writer = db::write_actor::spawn_writer((*restored_pool).clone()).unwrap();
    let restored_run_repo = RunRepository::new(restored_pool.clone(), restored_writer.clone());
    let restored_repo = PlanRepository::new(restored_pool, restored_writer.clone());
    assert_eq!(
        restored_run_repo
            .read_run(ResearchMode::Fictional, &saved_run.id)
            .unwrap(),
        saved_run
    );
    assert_eq!(
        restored_repo
            .load_plan(ResearchMode::Fictional)
            .unwrap()
            .unwrap(),
        saved
    );
    let wrong = DbAccess::encrypted(
        snapshot.to_str().unwrap(),
        Arc::new(DbEncryptionKey::generate()),
    );
    // SQLite opens lazily: force a page read to test the wrong key.
    let wrong_connection = wrong.connect_rusqlite().unwrap();
    assert!(wrong_connection
        .query_row("SELECT count(*) FROM sqlite_master", [], |row| row
            .get::<_, i64>(0))
        .is_err());
    writer.shutdown().await;
    restored_writer.shutdown().await;
}
