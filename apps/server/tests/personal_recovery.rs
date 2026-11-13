#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "synthetic offline recovery fixtures"
)]
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
    sync::{Arc, Mutex},
};
use uuid::Uuid;
use wealthfolio_core::{
    portfolio::snapshot::reviewed_import::*, profiles::ProfileRegistry,
    runtime_boundary::RuntimePurpose,
};
use wealthfolio_storage_sqlite::{
    db::{self, portable, DbAccess, DbEncryptionKey},
    portfolio::snapshot::reviewed_import::ReviewedSnapshotRepository,
};

const SECRET: &str = "--------------------------------";
const PASSWORD: &str = "synthetic offline recovery password";
#[derive(Default)]
struct Secrets(Mutex<HashMap<String, String>>);
impl wealthfolio_core::secrets::SecretStore for Secrets {
    fn get_secret(&self, key: &str) -> wealthfolio_core::Result<Option<String>> {
        Ok(self.0.lock().unwrap().get(key).cloned())
    }
    fn set_secret(&self, key: &str, value: &str) -> wealthfolio_core::Result<()> {
        self.0.lock().unwrap().insert(key.into(), value.into());
        Ok(())
    }
    fn delete_secret(&self, key: &str) -> wealthfolio_core::Result<()> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}
struct Grant;
impl ImportGrantAuthority for Grant {
    fn require_current(&self, _: Uuid, _: Uuid, _: &str) -> wealthfolio_core::Result<()> {
        Ok(())
    }
}
fn hash(path: &Path) -> Vec<u8> {
    Sha256::digest(fs::read(path).unwrap()).to_vec()
}
fn theme(access: &DbAccess) -> String {
    access
        .connect_rusqlite()
        .unwrap()
        .query_row(
            "SELECT setting_value FROM app_settings WHERE setting_key='theme'",
            [],
            |row| row.get(0),
        )
        .unwrap()
}
fn set_theme(access: &DbAccess, value: &str) {
    access
        .connect_rusqlite()
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO app_settings(setting_key,setting_value) VALUES('theme',?)",
            [value],
        )
        .unwrap();
}
fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_wealthfolio-server"));
    command
        .current_dir(root)
        .env("WF_DB_PATH", root.join("app.db"))
        .env("WF_DATA_DIR", "")
        .env("WF_SECRET_KEY", SECRET)
        .env("WF_SECRET_KEY_FILE", "")
        .env("WF_SECRET_FILE", root.join("secrets.json"))
        .env("WF_DB_REQUIRE_ENCRYPTION", "1")
        .env("WF_RUNTIME_PURPOSE", "personal")
        .env("WF_RUNTIME_BOUNDARY_VERSION", "1")
        .env_remove("WEALTHFOLIO_E2E")
        .env_remove("WEALTHFOLIO_FIXTURE_DIR")
        .env_remove("WEALTHFOLIO_FIXTURE_AS_OF");
    command
}
fn restore(root: &Path, backup: &Path, profile: Uuid) -> Output {
    let mut child = command(root)
        .args(["db", "restore"])
        .arg(backup)
        .args([
            "--profile",
            &profile.to_string(),
            "--password-stdin",
            "--yes",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(PASSWORD.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
async fn record(access: &DbAccess, profile: Uuid, account: Option<Uuid>) -> SavedReviewedSnapshot {
    let owner = Arc::new(db::DatabaseOwner::acquire(access.path()).unwrap());
    let pool = access.create_pool_with_owner(owner.clone()).unwrap();
    let (writer, task) =
        db::write_actor::spawn_writer_with_outbox_observer((*pool).clone(), Arc::new(|| {}))
            .unwrap();
    let repo = Arc::new(
        ReviewedSnapshotRepository::new(pool.clone(), writer.clone(), profile, access).unwrap(),
    );
    let service = ReviewedImportService::new(repo.clone());
    let binding =
        ImportReviewBinding::new(profile, Uuid::new_v4(), "synthetic-owner", Arc::new(Grant))
            .unwrap();
    let account = if let Some(id) = account {
        id
    } else {
        let account = service
            .create_account(
                &binding,
                NewReviewedImportAccount {
                    id: Uuid::new_v4(),
                    name: "Synthetic personal account".into(),
                    currency: "INR".into(),
                },
            )
            .await
            .unwrap();
        let input = ReviewedSnapshotInput {
            schema_version: 1,
            account_id: account.id,
            currency: "INR".into(),
            source: SnapshotSourceReference {
                label: "Synthetic broker".into(),
                account: "Synthetic A".into(),
            },
            as_of: snapshot_day(chrono::Utc::now()),
            settled_scope: SettledScope::Complete,
            other_exposure: OtherExposure::NoneConfirmed,
            cash: Some(BTreeMap::from([("INR".into(), "0".into())])),
            positions: vec![ReviewedEquity {
                isin: "INE213A01029".into(),
                symbol: "ONGC".into(),
                name: "Synthetic share".into(),
                currency: "INR".into(),
                source_venue: "XNSE".into(),
                valuation_venue: "XNSE".into(),
                ordinary_equity_confirmed: true,
                quantity: "1.23456789".into(),
                average_cost: None,
                price: None,
            }],
        };
        let PreparedReviewedImport::Review { review } =
            service.prepare(&binding, input).await.unwrap()
        else {
            panic!("expected review")
        };
        service
            .confirm(&binding, review.review_id, review.input_hash)
            .await
            .unwrap();
        account.id
    };
    let result = service.latest(&binding, account).await.unwrap().unwrap();
    drop(service);
    writer.shutdown().await;
    task.join().await;
    drop(repo);
    drop(pool);
    drop(writer);
    for _ in 0..200 {
        if Arc::strong_count(&owner) == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert_eq!(Arc::strong_count(&owner), 1);
    drop(owner);
    result
}

#[tokio::test]
async fn portable_personal_recovery_preserves_records_and_rejects_foreign_scope_plaintext_and_conversion(
) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("app.db");
    let key = Arc::new(DbEncryptionKey::from_bytes(
        &wealthfolio_server::auth::derive_database_key(SECRET.as_bytes()),
    ));
    let access = DbAccess::encrypted(path.to_str().unwrap(), key.clone());
    access.prepare().unwrap();
    let admission = access
        .check_runtime_boundary(RuntimePurpose::Personal)
        .unwrap();
    access.run_migrations().unwrap();
    access.seal_runtime_boundary(admission).unwrap();
    // Registered synthetic legacy layout exercises the same offline resolver.
    // The existing database_restore suite covers fresh-profile key derivation.
    let registry = ProfileRegistry::open(
        root.path().to_path_buf(),
        path.clone(),
        Arc::new(Secrets::default()),
    )
    .unwrap();
    let profile = registry.default_id().unwrap();
    drop(registry);
    let expected = record(&access, profile, None).await;
    set_theme(&access, "dark");
    let compatible = portable::export(&access, root.path(), Some(PASSWORD)).unwrap();
    let unencrypted = portable::export(&access, root.path(), None).unwrap();
    let foreign_path = root.path().join("foreign.db");
    fs::copy(&path, &foreign_path).unwrap();
    let foreign = DbAccess::encrypted(foreign_path.to_str().unwrap(), key);
    foreign
        .connect_rusqlite()
        .unwrap()
        .execute(
            "UPDATE personal_snapshot_imports SET profile_id=?",
            [Uuid::new_v4().to_string()],
        )
        .unwrap();
    let foreign_backup = portable::export(&foreign, root.path(), Some(PASSWORD)).unwrap();
    set_theme(&access, "light");
    let before = hash(&path);
    for backup in [&unencrypted.path, &foreign_backup.path] {
        let output = restore(root.path(), backup, profile);
        assert!(!output.status.success());
        assert_eq!(hash(&path), before);
        assert_eq!(theme(&access), "light");
    }
    for purpose in ["personal", "standard"] {
        let output = command(root.path())
            .env("WF_RUNTIME_PURPOSE", purpose)
            .args(["db", "decrypt", "--profile", &profile.to_string()])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(hash(&path), before);
    }
    let output = restore(root.path(), &compatible.path, profile);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(theme(&access), "dark");
    access
        .check_runtime_boundary(RuntimePurpose::Personal)
        .unwrap();
    let actual = record(&access, profile, Some(expected.receipt.account_id)).await;
    assert_eq!(actual.receipt, expected.receipt);
    assert_eq!(actual.snapshot, expected.snapshot);
    assert!(!actual.history_available);
}
