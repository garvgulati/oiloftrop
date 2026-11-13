#![allow(clippy::unwrap_used, reason = "disposable encrypted boundary fixtures")]

use std::sync::Arc;
use wealthfolio_core::runtime_boundary::RuntimePurpose;
use wealthfolio_storage_sqlite::db::{snapshots, DbAccess, DbEncryptionKey};

fn migrate_marker(access: &DbAccess) {
    access
        .connect_rusqlite()
        .unwrap()
        .execute_batch(include_str!(
            "../migrations/2026-10-28-000001_runtime_boundary/up.sql"
        ))
        .unwrap();
}

#[test]
fn encrypted_personal_marker_survives_reopen_and_rejects_trial_or_standard_runtime() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp
        .path()
        .join("personal.db")
        .to_str()
        .unwrap()
        .to_string();
    let key = Arc::new(DbEncryptionKey::generate());
    let access = DbAccess::encrypted(&path, key.clone());
    let admission = access
        .check_runtime_boundary(RuntimePurpose::Personal)
        .unwrap();
    migrate_marker(&access);
    access.seal_runtime_boundary(admission).unwrap();
    access
        .connect_rusqlite()
        .unwrap()
        .execute_batch(
            "CREATE TABLE owner_record(value TEXT); INSERT INTO owner_record VALUES('synthetic');",
        )
        .unwrap();
    drop(access);
    let reopened = DbAccess::encrypted(path, key);
    assert!(reopened
        .check_runtime_boundary(RuntimePurpose::Personal)
        .is_ok());
    assert!(reopened
        .check_runtime_boundary(RuntimePurpose::Fictional)
        .is_err());
    assert!(reopened
        .check_runtime_boundary(RuntimePurpose::Standard)
        .is_err());
    let retained: String = reopened
        .connect_rusqlite()
        .unwrap()
        .query_row("SELECT value FROM owner_record", [], |row| row.get(0))
        .unwrap();
    assert_eq!(retained, "synthetic");
}

#[test]
fn personal_rejects_plaintext_and_unclassified_existing_data() {
    let temp = tempfile::tempdir().unwrap();
    let plain = DbAccess::plaintext(temp.path().join("plain.db").to_str().unwrap());
    assert!(plain
        .check_runtime_boundary(RuntimePurpose::Personal)
        .is_err());
    assert!(!temp.path().join("plain.db").exists());
    let encrypted = DbAccess::encrypted(
        temp.path().join("existing.db").to_str().unwrap(),
        Arc::new(DbEncryptionKey::generate()),
    );
    encrypted
        .connect_rusqlite()
        .unwrap()
        .execute_batch("CREATE TABLE existing_owner_data(id TEXT);")
        .unwrap();
    assert!(encrypted
        .check_runtime_boundary(RuntimePurpose::Personal)
        .is_err());
    let admission = encrypted
        .check_runtime_boundary(RuntimePurpose::Fictional)
        .unwrap();
    migrate_marker(&encrypted);
    encrypted.seal_runtime_boundary(admission).unwrap();
    assert!(encrypted
        .check_runtime_boundary(RuntimePurpose::Personal)
        .is_err());
}

#[test]
fn future_version_and_conflicting_seal_are_rejected_without_relabelling() {
    let temp = tempfile::tempdir().unwrap();
    let access = DbAccess::plaintext(temp.path().join("trial.db").to_str().unwrap());
    let fictional = access
        .check_runtime_boundary(RuntimePurpose::Fictional)
        .unwrap();
    let standard = access
        .check_runtime_boundary(RuntimePurpose::Standard)
        .unwrap();
    migrate_marker(&access);
    access.seal_runtime_boundary(fictional).unwrap();
    assert!(access.seal_runtime_boundary(standard).is_err());
    assert!(access
        .check_runtime_boundary(RuntimePurpose::Fictional)
        .is_ok());
    access
        .connect_rusqlite()
        .unwrap()
        .execute("UPDATE runtime_boundary SET boundary_version=2", [])
        .unwrap();
    assert!(access
        .check_runtime_boundary(RuntimePurpose::Fictional)
        .is_err());
}

#[test]
fn admission_cannot_be_used_to_seal_a_different_profile() {
    let temp = tempfile::tempdir().unwrap();
    let first = DbAccess::plaintext(temp.path().join("first.db").to_str().unwrap());
    let second = DbAccess::plaintext(temp.path().join("second.db").to_str().unwrap());
    let admission = first
        .check_runtime_boundary(RuntimePurpose::Fictional)
        .unwrap();
    assert!(second.seal_runtime_boundary(admission).is_err());
    assert!(!temp.path().join("second.db").exists());
}

#[test]
fn encrypted_backup_preserves_purpose_and_rejects_wrong_key() {
    let temp = tempfile::tempdir().unwrap();
    let key = Arc::new(DbEncryptionKey::generate());
    let access = DbAccess::encrypted(
        temp.path().join("personal.db").to_str().unwrap(),
        key.clone(),
    );
    let admission = access
        .check_runtime_boundary(RuntimePurpose::Personal)
        .unwrap();
    migrate_marker(&access);
    access.seal_runtime_boundary(admission).unwrap();
    let backup_path = snapshots::create(
        &access,
        temp.path().to_str().unwrap(),
        snapshots::SnapshotReason::Manual,
    )
    .unwrap();
    let backup = DbAccess::encrypted(backup_path.to_str().unwrap(), key);
    assert!(backup
        .check_runtime_boundary(RuntimePurpose::Personal)
        .is_ok());
    assert!(backup
        .check_runtime_boundary(RuntimePurpose::Fictional)
        .is_err());
    let wrong_key = DbAccess::encrypted(
        backup_path.to_str().unwrap(),
        Arc::new(DbEncryptionKey::generate()),
    );
    assert!(wrong_key
        .check_runtime_boundary(RuntimePurpose::Personal)
        .is_err());
}
