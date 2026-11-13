#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "synthetic encrypted import fixtures"
)]
use chrono::{NaiveDate, Utc};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use uuid::Uuid;
use wealthfolio_core::{portfolio::snapshot::reviewed_import::*, runtime_boundary::RuntimePurpose};
use wealthfolio_storage_sqlite::{
    db::{self, DatabaseOwner, DbAccess, DbEncryptionKey, WriteHandle, WriterTask},
    portfolio::snapshot::reviewed_import::ReviewedSnapshotRepository,
    DbPool,
};

struct CurrentGrant;
impl ImportGrantAuthority for CurrentGrant {
    fn require_current(&self, _: Uuid, _: Uuid, _: &str) -> wealthfolio_core::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn dedicated_usd_account_retries_without_fx_sync_or_duplicate_creation() {
    let h = Harness::new().await;
    let service = ReviewedImportService::new(h.repo.clone());
    let binding = h.binding();
    let input = NewReviewedImportAccount {
        id: Uuid::new_v4(),
        name: " Synthetic USD account ".into(),
        currency: "USD".into(),
    };
    let count = h.count("accounts");
    let assets = h.count("assets");
    let quotes = h.count("quotes");
    let outbox = h.count("sync_outbox");
    let first = service
        .create_account(&binding, input.clone())
        .await
        .unwrap();
    assert_eq!(first.name, "Synthetic USD account");
    let again = service
        .create_account(&h.binding(), input.clone())
        .await
        .unwrap();
    assert_eq!(first.id, again.id);
    assert_eq!(h.count("accounts"), count + 1);
    assert_eq!(h.count("assets"), assets);
    assert_eq!(h.count("quotes"), quotes);
    assert_eq!(h.count("sync_outbox"), outbox);
    let conn = h.access.connect_rusqlite().unwrap();
    let (mode, active, meta, provider): (String, bool, String, Option<String>) = conn
        .query_row(
            "SELECT tracking_mode,is_active,meta,provider FROM accounts WHERE id=?",
            [first.id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(mode, "HOLDINGS");
    assert!(active);
    assert!(provider.is_none());
    assert_eq!(
        serde_json::from_str::<Value>(&meta).unwrap()[REVIEWED_ACCOUNT_META_KEY],
        true
    );
    drop(conn);
    let mut conflict = input.clone();
    conflict.currency = "INR".into();
    assert!(service
        .create_account(&binding, conflict)
        .await
        .unwrap_err()
        .to_string()
        .contains("ACCOUNT_CONFLICT"));
    let mut conflict = input;
    conflict.name = "Changed after first save".into();
    assert!(service.create_account(&binding, conflict).await.is_err());
    let mut snapshot = super_us_input(first.id);
    snapshot.positions[0].average_cost = None;
    save(&service, &binding, snapshot).await;
    assert!(service.latest(&binding, first.id).await.unwrap().is_some());
    drop(service);
    h.finish().await;
}

fn super_us_input(account: Uuid) -> ReviewedSnapshotInput {
    let mut value = input(account, snapshot_day(Utc::now()));
    value.currency = "USD".into();
    let row = &mut value.positions[0];
    row.isin = "US0378331005".into();
    row.symbol = "AAPL".into();
    row.currency = "USD".into();
    row.source_venue = "XNAS".into();
    row.valuation_venue = "XNAS".into();
    value
}

#[tokio::test]
async fn account_creation_rolls_back_if_grant_revokes_at_any_write_checkpoint() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    for boundary in [1, 2, 3, 4] {
        let h = Harness::new().await;
        let service = ReviewedImportService::new(h.repo.clone());
        let authority = Arc::new(RevokingGrant(AtomicUsize::new(0)));
        let binding = ImportReviewBinding::new(
            h.profile,
            Uuid::new_v4(),
            "synthetic-owner",
            authority.clone(),
        )
        .unwrap();
        let before = h.count("accounts");
        authority.0.store(boundary, Ordering::SeqCst);
        let result = service
            .create_account(
                &binding,
                NewReviewedImportAccount {
                    id: Uuid::new_v4(),
                    name: "Synthetic new account".into(),
                    currency: "INR".into(),
                },
            )
            .await;
        assert!(result.unwrap_err().to_string().contains("GRANT_INVALID"));
        assert_eq!(h.count("accounts"), before);
        drop(service);
        h.finish().await;
    }
}

struct QueuedGrant {
    valid: std::sync::atomic::AtomicBool,
    armed: std::sync::atomic::AtomicBool,
    checks: std::sync::atomic::AtomicUsize,
    read_done: tokio::sync::Notify,
}
impl ImportGrantAuthority for QueuedGrant {
    fn require_current(&self, _: Uuid, _: Uuid, _: &str) -> wealthfolio_core::Result<()> {
        use std::sync::atomic::Ordering;
        if !self.valid.load(Ordering::SeqCst) {
            return Err(import_error("PERSONAL_IMPORT_GRANT_INVALID"));
        }
        if self.armed.load(Ordering::SeqCst) && self.checks.fetch_add(1, Ordering::SeqCst) == 1 {
            self.read_done.notify_one();
        }
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queued_confirmation_rechecks_access_and_uses_writer_timestamp() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    for revoke in [true, false] {
        let h = Harness::new().await;
        let authority = Arc::new(QueuedGrant {
            valid: AtomicBool::new(true),
            armed: AtomicBool::new(false),
            checks: AtomicUsize::new(0),
            read_done: tokio::sync::Notify::new(),
        });
        let binding = ImportReviewBinding::new(
            h.profile,
            Uuid::new_v4(),
            "synthetic-owner",
            authority.clone(),
        )
        .unwrap();
        let service = ReviewedImportService::new(h.repo.clone());
        let prepared = review(
            service
                .prepare(&binding, input(h.account, snapshot_day(Utc::now())))
                .await
                .unwrap(),
        );
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let writer = h.writer.clone();
        let blocker = tokio::spawn(async move {
            writer
                .exec(move |_| {
                    entered_tx.send(()).unwrap();
                    release_rx
                        .recv_timeout(std::time::Duration::from_secs(10))
                        .unwrap();
                    Ok(())
                })
                .await
                .unwrap();
        });
        entered_rx.await.unwrap();
        authority.armed.store(true, Ordering::SeqCst);
        {
            let confirmation = service.confirm(&binding, prepared.review_id, prepared.input_hash);
            tokio::pin!(confirmation);
            tokio::select! {
                biased;
                result = &mut confirmation => panic!("writer should still be blocked: {result:?}"),
                _ = authority.read_done.notified() => {}
            }
            authority.valid.store(!revoke, Ordering::SeqCst);
            let released_at = Utc::now();
            release_tx.send(()).unwrap();
            let result = confirmation.await;
            if revoke {
                assert!(result.unwrap_err().to_string().contains("GRANT_INVALID"));
            } else {
                assert!(result.unwrap().imported_at >= released_at);
            }
        }
        blocker.await.unwrap();
        let expected = if revoke { 0 } else { 1 };
        assert_eq!(h.count("holdings_snapshots"), expected);
        assert_eq!(h.count("personal_snapshot_imports"), expected);
        drop(service);
        h.finish().await;
    }
}

// Armed only after preparing the review, so each confirmation boundary is tested.
struct RevokingGrant(std::sync::atomic::AtomicUsize);
impl ImportGrantAuthority for RevokingGrant {
    fn require_current(&self, _: Uuid, _: Uuid, _: &str) -> wealthfolio_core::Result<()> {
        use std::sync::atomic::Ordering;
        let remaining = self.0.load(Ordering::SeqCst);
        if remaining == 0 {
            return Ok(());
        }
        if self.0.fetch_sub(1, Ordering::SeqCst) == 1 {
            return Err(import_error("PERSONAL_IMPORT_GRANT_INVALID"));
        }
        Ok(())
    }
}

#[tokio::test]
async fn revoked_grant_rolls_back_even_at_final_transaction_check() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    for boundary in [1, 2, 3, 4, 5] {
        let h = Harness::new().await;
        let authority = Arc::new(RevokingGrant(AtomicUsize::new(0)));
        let binding = ImportReviewBinding::new(
            h.profile,
            Uuid::new_v4(),
            "synthetic-owner",
            authority.clone(),
        )
        .unwrap();
        let service = ReviewedImportService::new(h.repo.clone());
        let prepared = review(
            service
                .prepare(&binding, input(h.account, snapshot_day(Utc::now())))
                .await
                .unwrap(),
        );
        let assets_before = h.count("assets");
        authority.0.store(boundary, Ordering::SeqCst);
        assert!(service
            .confirm(&binding, prepared.review_id, prepared.input_hash)
            .await
            .unwrap_err()
            .to_string()
            .contains("GRANT_INVALID"));
        assert_eq!(authority.0.load(Ordering::SeqCst), 0);
        assert_eq!(h.count("assets"), assets_before);
        assert_eq!(h.count("holdings_snapshots"), 0);
        assert_eq!(h.count("snapshot_positions"), 0);
        assert_eq!(h.count("personal_snapshot_imports"), 0);
        drop(service);
        h.finish().await;
    }
}

struct Restore {
    source: PathBuf,
    key: Arc<DbEncryptionKey>,
    profile: Uuid,
    account: Uuid,
}
struct Harness {
    root: tempfile::TempDir,
    access: DbAccess,
    pool: Arc<DbPool>,
    writer: WriteHandle,
    task: WriterTask,
    repo: Arc<ReviewedSnapshotRepository>,
    owner: Arc<DatabaseOwner>,
    profile: Uuid,
    account: Uuid,
}
impl Harness {
    async fn new() -> Self {
        Self::setup(None).await
    }
    async fn setup(restore: Option<Restore>) -> Self {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("personal.db");
        let (key, profile, account, fresh) = match restore {
            Some(saved) => {
                std::fs::copy(saved.source, &path).unwrap();
                (saved.key, saved.profile, saved.account, false)
            }
            None => (
                Arc::new(DbEncryptionKey::generate()),
                Uuid::new_v4(),
                Uuid::new_v4(),
                true,
            ),
        };
        let access = DbAccess::encrypted(path.to_str().unwrap(), key);
        let owner = Arc::new(DatabaseOwner::acquire(access.path()).unwrap());
        access.prepare().unwrap();
        let admitted = access
            .check_runtime_boundary(RuntimePurpose::Personal)
            .unwrap();
        access.run_migrations().unwrap();
        access.seal_runtime_boundary(admitted).unwrap();
        if fresh {
            access.connect_rusqlite().unwrap().execute("INSERT INTO accounts(id,name,currency,is_active,tracking_mode) VALUES(?,'Synthetic account A','INR',1,'HOLDINGS')",[account.to_string()]).unwrap();
        }
        let pool = access.create_pool_with_owner(owner.clone()).unwrap();
        let (writer, task) =
            db::write_actor::spawn_writer_with_outbox_observer((*pool).clone(), Arc::new(|| {}))
                .unwrap();
        let repo = Arc::new(
            ReviewedSnapshotRepository::new(pool.clone(), writer.clone(), profile, &access)
                .unwrap(),
        );
        Self {
            root,
            access,
            pool,
            writer,
            task,
            repo,
            owner,
            profile,
            account,
        }
    }
    fn binding(&self) -> ImportReviewBinding {
        ImportReviewBinding::new(
            self.profile,
            Uuid::new_v4(),
            "synthetic-owner-session",
            Arc::new(CurrentGrant),
        )
        .unwrap()
    }
    fn count(&self, table: &str) -> i64 {
        self.access
            .connect_rusqlite()
            .unwrap()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    }
    fn add_account(&self) -> Uuid {
        let account = Uuid::new_v4();
        self.access.connect_rusqlite().unwrap().execute("INSERT INTO accounts(id,name,currency,is_active,tracking_mode) VALUES(?,'Synthetic account B','INR',1,'HOLDINGS')",[account.to_string()]).unwrap();
        account
    }
    async fn finish(self) {
        let Self {
            root,
            access,
            pool,
            writer,
            task,
            repo,
            owner,
            ..
        } = self;
        writer.shutdown().await;
        task.join().await;
        drop(repo);
        drop(pool);
        drop(writer);
        drop(access);
        let end = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        while Arc::strong_count(&owner) > 1 && tokio::time::Instant::now() < end {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        assert_eq!(Arc::strong_count(&owner), 1);
        drop(owner);
        root.close().unwrap();
    }
}
fn input(account: Uuid, day: NaiveDate) -> ReviewedSnapshotInput {
    ReviewedSnapshotInput {
        schema_version: 1,
        account_id: account,
        currency: "INR".into(),
        source: SnapshotSourceReference {
            label: "Synthetic broker".into(),
            account: "Synthetic source A".into(),
        },
        as_of: day,
        settled_scope: SettledScope::Complete,
        other_exposure: OtherExposure::Unknown,
        cash: None,
        positions: vec![ReviewedEquity {
            isin: "INE213A01029".into(),
            symbol: "ONGC".into(),
            name: "Synthetic ordinary-equity input".into(),
            currency: "INR".into(),
            source_venue: "XNSE".into(),
            valuation_venue: "XNSE".into(),
            ordinary_equity_confirmed: true,
            quantity: "1.23456789".into(),
            average_cost: Some("123.4567".into()),
            price: None,
        }],
    }
}
fn review(prepared: PreparedReviewedImport) -> ImportReview {
    match prepared {
        PreparedReviewedImport::Review { review } => review,
        _ => panic!("expected pending review"),
    }
}
async fn save(
    service: &ReviewedImportService,
    binding: &ImportReviewBinding,
    input: ReviewedSnapshotInput,
) -> ImportReceipt {
    let review = review(service.prepare(binding, input).await.unwrap());
    service
        .confirm(binding, review.review_id, review.input_hash)
        .await
        .unwrap()
}

#[tokio::test]
async fn exact_snapshot_and_nulls_reload_without_quotes_trades_or_lots() {
    let h = Harness::new().await;
    let service = ReviewedImportService::new(h.repo.clone());
    let binding = h.binding();
    let day = snapshot_day(Utc::now()).pred_opt().unwrap();
    let quotes = h.count("quotes");
    let activities = h.count("activities");
    let lots = h.count("lots");
    let receipt = save(&service, &binding, input(h.account, day)).await;
    let saved = service.latest(&binding, h.account).await.unwrap().unwrap();
    assert_eq!(saved.receipt, receipt);
    assert_eq!(saved.snapshot.as_of, day);
    assert!(snapshot_day(receipt.imported_at) > saved.snapshot.as_of);
    assert_eq!(saved.snapshot.positions[0].quantity, "1.23456789");
    assert_eq!(
        saved.snapshot.positions[0].average_cost.as_deref(),
        Some("123.4567")
    );
    assert!(saved.snapshot.positions[0].price.is_none());
    assert!(saved.snapshot.cash.is_none());
    assert!(!saved.history_available);
    let conn = h.access.connect_rusqlite().unwrap();
    let raw: String = conn
        .query_row(
            "SELECT positions FROM holdings_snapshots WHERE id=?",
            [&receipt.snapshot_id],
            |row| row.get(0),
        )
        .unwrap();
    let value: Value = serde_json::from_str(&raw).unwrap();
    let position = value.as_object().unwrap().values().next().unwrap();
    assert_eq!(position["quantity"], "1.23456789");
    assert_eq!(position["averageCost"], "123.4567");
    let exact: (String, String) = conn
        .query_row(
            "SELECT quantity,average_cost FROM snapshot_positions WHERE snapshot_id=?",
            [&receipt.snapshot_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(exact, ("1.23456789".into(), "123.4567".into()));
    drop(conn);
    assert_eq!(h.count("quotes"), quotes);
    assert_eq!(h.count("activities"), activities);
    assert_eq!(h.count("lots"), lots);
    drop(service);
    h.finish().await;
}

#[tokio::test]
async fn unknown_and_known_zero_remain_distinct_and_dated_prices_stay_date_only() {
    let h = Harness::new().await;
    let service = ReviewedImportService::new(h.repo.clone());
    let binding = h.binding();
    let day = snapshot_day(Utc::now());
    let mut unknown = input(h.account, day.pred_opt().unwrap());
    unknown.positions[0].average_cost = None;
    save(&service, &binding, unknown).await;
    let first = service.latest(&binding, h.account).await.unwrap().unwrap();
    assert!(first.snapshot.positions[0].average_cost.is_none());
    assert!(first.snapshot.cash.is_none());
    let mut known = input(h.account, day);
    known.positions[0].average_cost = Some("0.0000".into());
    known.cash = Some(BTreeMap::from([("INR".into(), "0.00".into())]));
    known.positions[0].price = Some(ReviewedPrice {
        amount: "150.125".into(),
        currency: "INR".into(),
        observed_on: day.pred_opt().unwrap(),
        source: "Synthetic reviewed close".into(),
        venue: "XNSE".into(),
    });
    save(&service, &binding, known).await;
    let saved = service.latest(&binding, h.account).await.unwrap().unwrap();
    assert_eq!(
        saved.snapshot.positions[0].average_cost.as_deref(),
        Some("0")
    );
    assert_eq!(saved.snapshot.cash.as_ref().unwrap()["INR"], "0");
    assert_eq!(
        saved.snapshot.positions[0]
            .price
            .as_ref()
            .unwrap()
            .observed_on,
        day.pred_opt().unwrap()
    );
    assert_eq!(h.count("quotes"), 0);
    drop(service);
    h.finish().await;
}

#[tokio::test]
async fn same_isin_across_brokers_retains_one_asset_and_separate_positions() {
    let h = Harness::new().await;
    let service = ReviewedImportService::new(h.repo.clone());
    let binding = h.binding();
    let day = snapshot_day(Utc::now());
    save(&service, &binding, input(h.account, day)).await;
    let second = h.add_account();
    let mut other = input(second, day);
    other.source.account = "Synthetic source B".into();
    other.positions[0].source_venue = "XBOM".into();
    other.positions[0].symbol = "500312".into();
    other.positions[0].quantity = "2.5678".into();
    save(&service, &binding, other).await;
    let a = service.latest(&binding, h.account).await.unwrap().unwrap();
    let b = service.latest(&binding, second).await.unwrap().unwrap();
    assert_eq!(a.asset_ids, b.asset_ids);
    assert_ne!(a.receipt.account_id, b.receipt.account_id);
    assert_eq!(b.snapshot.positions[0].source_venue, "XBOM");
    assert_eq!(b.snapshot.positions[0].valuation_venue, "XNSE");
    assert_eq!(a.snapshot.positions[0].quantity, "1.23456789");
    assert_eq!(b.snapshot.positions[0].quantity, "2.5678");
    assert_eq!(h.count("personal_snapshot_imports"), 2);
    drop(service);
    h.finish().await;
}

#[tokio::test]
async fn target_changes_and_existing_date_replacements_are_rejected() {
    let h = Harness::new().await;
    let service = ReviewedImportService::new(h.repo.clone());
    let binding = h.binding();
    let day = snapshot_day(Utc::now());
    let draft = input(h.account, day);
    let pending = review(service.prepare(&binding, draft.clone()).await.unwrap());
    h.access
        .connect_rusqlite()
        .unwrap()
        .execute(
            "UPDATE accounts SET name='Changed synthetic target' WHERE id=?",
            [h.account.to_string()],
        )
        .unwrap();
    assert!(service
        .confirm(&binding, pending.review_id, pending.input_hash)
        .await
        .unwrap_err()
        .to_string()
        .contains("TARGET_CHANGED"));
    assert_eq!(h.count("holdings_snapshots"), 0);
    let saved = save(&service, &binding, draft.clone()).await;
    let mut replacement = draft.clone();
    replacement.positions[0].quantity = "4".into();
    assert!(service
        .prepare(&binding, replacement)
        .await
        .unwrap_err()
        .to_string()
        .contains("DATE_EXISTS"));
    assert!(
        matches!(service.prepare(&binding,draft).await.unwrap(),PreparedReviewedImport::AlreadyImported{receipt} if receipt==saved)
    );
    assert_eq!(h.count("holdings_snapshots"), 1);
    drop(service);
    h.finish().await;
}

#[tokio::test]
async fn interrupted_atomic_write_rolls_back_assets_snapshot_positions_and_receipt() {
    let h = Harness::new().await;
    let service = ReviewedImportService::new(h.repo.clone());
    let binding = h.binding();
    let day = snapshot_day(Utc::now());
    let before = h.count("assets");
    let pending = review(
        service
            .prepare(&binding, input(h.account, day))
            .await
            .unwrap(),
    );
    h.access.connect_rusqlite().unwrap().execute_batch("CREATE TRIGGER synthetic_import_failure BEFORE INSERT ON personal_snapshot_imports BEGIN SELECT RAISE(ABORT,'synthetic interruption'); END;").unwrap();
    assert!(service
        .confirm(&binding, pending.review_id, pending.input_hash.clone())
        .await
        .is_err());
    assert_eq!(h.count("assets"), before);
    assert_eq!(h.count("holdings_snapshots"), 0);
    assert_eq!(h.count("snapshot_positions"), 0);
    assert_eq!(h.count("personal_snapshot_imports"), 0);
    h.access
        .connect_rusqlite()
        .unwrap()
        .execute_batch("DROP TRIGGER synthetic_import_failure;")
        .unwrap();
    assert!(service
        .confirm(&binding, pending.review_id, pending.input_hash)
        .await
        .is_ok());
    drop(service);
    h.finish().await;
}

#[tokio::test]
async fn durable_receipt_and_nulls_survive_encrypted_backup_restore_and_new_grant() {
    let h = Harness::new().await;
    let service = ReviewedImportService::new(h.repo.clone());
    let binding = h.binding();
    let day = snapshot_day(Utc::now());
    let mut draft = input(h.account, day);
    draft.positions[0].average_cost = None;
    let receipt = save(&service, &binding, draft).await;
    let backup = db::snapshots::create(
        &h.access,
        h.root.path().to_str().unwrap(),
        db::snapshots::SnapshotReason::Manual,
    )
    .unwrap();
    let restored = Harness::setup(Some(Restore {
        source: backup,
        key: h.access.key().unwrap().clone(),
        profile: h.profile,
        account: h.account,
    }))
    .await;
    let restarted = ReviewedImportService::new(restored.repo.clone());
    let new_grant = restored.binding();
    assert_eq!(
        restarted
            .confirm(&new_grant, receipt.review_id, receipt.input_hash.clone())
            .await
            .unwrap(),
        receipt
    );
    let saved = restarted
        .latest(&new_grant, restored.account)
        .await
        .unwrap()
        .unwrap();
    assert!(saved.snapshot.positions[0].average_cost.is_none());
    assert!(saved.snapshot.cash.is_none());
    assert!(!saved.history_available);
    assert_eq!(restored.count("personal_snapshot_imports"), 1);
    assert_eq!(restored.count("holdings_snapshots"), 1);
    drop(restarted);
    restored.finish().await;
    drop(service);
    h.finish().await;
}

#[tokio::test]
async fn foreign_writer_scope_and_legacy_row_drift_fail_closed() {
    let h = Harness::new().await;
    let foreign = Harness::new().await;
    let wrong = Arc::new(
        ReviewedSnapshotRepository::new(
            h.pool.clone(),
            foreign.writer.clone(),
            h.profile,
            &h.access,
        )
        .unwrap(),
    );
    let service = ReviewedImportService::new(wrong);
    let binding = h.binding();
    let day = snapshot_day(Utc::now());
    let pending = review(
        service
            .prepare(&binding, input(h.account, day))
            .await
            .unwrap(),
    );
    assert!(service
        .confirm(&binding, pending.review_id, pending.input_hash)
        .await
        .unwrap_err()
        .to_string()
        .contains("DATABASE_SCOPE_INVALID"));
    assert_eq!(h.count("holdings_snapshots"), 0);
    assert_eq!(foreign.count("holdings_snapshots"), 0);
    drop(service);
    let service = ReviewedImportService::new(h.repo.clone());
    save(&service, &binding, input(h.account, day)).await;
    h.access
        .connect_rusqlite()
        .unwrap()
        .execute_batch("UPDATE holdings_snapshots SET cost_basis='999';")
        .unwrap();
    assert!(service
        .latest(&binding, h.account)
        .await
        .unwrap_err()
        .to_string()
        .contains("RECORD_CONFLICT"));
    drop(service);
    foreign.finish().await;
    h.finish().await;
}

#[tokio::test]
async fn legacy_deletion_is_restricted_and_parallel_reviews_do_not_duplicate() {
    let h = Harness::new().await;
    let service = ReviewedImportService::new(h.repo.clone());
    let binding = h.binding();
    let day = snapshot_day(Utc::now());
    let draft = input(h.account, day);
    let first = review(service.prepare(&binding, draft.clone()).await.unwrap());
    let second = review(service.prepare(&binding, draft).await.unwrap());
    let saved = service
        .confirm(&binding, first.review_id, first.input_hash.clone())
        .await
        .unwrap();
    assert!(service
        .confirm(&binding, second.review_id, second.input_hash)
        .await
        .unwrap_err()
        .to_string()
        .contains("TARGET_CHANGED"));
    let restart = ReviewedImportService::new(h.repo.clone());
    assert_eq!(
        restart
            .confirm(&h.binding(), first.review_id, first.input_hash)
            .await
            .unwrap(),
        saved
    );
    let conn = h.access.connect_rusqlite().unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    assert!(conn
        .execute(
            "DELETE FROM holdings_snapshots WHERE id=?",
            [&saved.snapshot_id]
        )
        .is_err());
    drop(conn);
    assert_eq!(h.count("holdings_snapshots"), 1);
    drop(restart);
    drop(service);
    h.finish().await;
}

#[tokio::test]
async fn multiple_positions_reload_independently_of_json_object_order() {
    let h = Harness::new().await;
    let service = ReviewedImportService::new(h.repo.clone());
    let binding = h.binding();
    let day = snapshot_day(Utc::now());
    let mut draft = input(h.account, day);
    let mut second = draft.positions[0].clone();
    second.isin = (0..=9)
        .map(|digit| format!("IN000000001{digit}"))
        .find(|candidate| wealthfolio_core::utils::isin::parse_isin(candidate).is_ok())
        .unwrap();
    second.symbol = "SYNTH2".into();
    second.name = "Synthetic second identity, not a market recommendation".into();
    second.quantity = "0.11111111".into();
    draft.positions.push(second);
    save(&service, &binding, draft).await;
    for _ in 0..16 {
        let saved = service.latest(&binding, h.account).await.unwrap().unwrap();
        assert_eq!(saved.snapshot.positions.len(), 2);
    }
    assert_eq!(h.count("snapshot_positions"), 2);
    drop(service);
    h.finish().await;
}

#[tokio::test]
async fn us_fractional_shares_keep_usd_without_fx_or_quote_generation() {
    let h = Harness::new().await;
    let account = h.add_account();
    let conn = h.access.connect_rusqlite().unwrap();
    conn.execute(
        "UPDATE accounts SET currency='USD' WHERE id=?",
        [account.to_string()],
    )
    .unwrap();
    let fx_before: i64 = conn
        .query_row("SELECT count(*) FROM assets WHERE kind='FX'", [], |row| {
            row.get(0)
        })
        .unwrap();
    drop(conn);
    let service = ReviewedImportService::new(h.repo.clone());
    let binding = h.binding();
    let day = snapshot_day(Utc::now());
    let mut draft = input(account, day);
    draft.currency = "USD".into();
    draft.positions[0].currency = "USD".into();
    draft.positions[0].isin = "US0378331005".into();
    draft.positions[0].symbol = "AAPL".into();
    draft.positions[0].name = "Synthetic US ordinary-equity input".into();
    draft.positions[0].source_venue = "XNAS".into();
    draft.positions[0].valuation_venue = "XNAS".into();
    draft.positions[0].quantity = "0.12345678".into();
    draft.positions[0].average_cost = Some("200.12345678".into());
    draft.cash = Some(BTreeMap::from([("USD".into(), "0".into())]));
    save(&service, &binding, draft).await;
    let saved = service.latest(&binding, account).await.unwrap().unwrap();
    assert_eq!(saved.snapshot.currency, "USD");
    assert_eq!(saved.snapshot.positions[0].quantity, "0.12345678");
    assert_eq!(
        saved.snapshot.positions[0].average_cost.as_deref(),
        Some("200.12345678")
    );
    assert!(saved.snapshot.positions[0].price.is_none());
    let fx_after: i64 = h
        .access
        .connect_rusqlite()
        .unwrap()
        .query_row("SELECT count(*) FROM assets WHERE kind='FX'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(fx_before, fx_after);
    assert_eq!(h.count("quotes"), 0);
    drop(service);
    h.finish().await;
}
