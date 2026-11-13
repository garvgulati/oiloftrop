#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "synthetic local provisioning fixtures"
)]
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};
use wealthfolio_storage_sqlite::db::portable;

const PASSWORD: &str = "Synthetic personal 日本語 password";
const BACKUP_PASSWORD: &str = "Different synthetic backup password";
fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_wealthfolio-server"));
    command
        .current_dir(root)
        .env("WF_RUNTIME_PURPOSE", "personal")
        .env("WF_RUNTIME_BOUNDARY_VERSION", "1")
        .env_remove("WEALTHFOLIO_E2E")
        .env_remove("WEALTHFOLIO_FIXTURE_DIR")
        .env_remove("WEALTHFOLIO_FIXTURE_AS_OF")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}
fn run(mut command: Command, password: &str) -> Output {
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(password.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
fn hash(path: &Path) -> Vec<u8> {
    Sha256::digest(fs::read(path).unwrap()).to_vec()
}
fn init(root: &Path, state: &Path) -> serde_json::Value {
    let mut cmd = command(root);
    cmd.args(["personal", "init"])
        .arg(state)
        .arg("--password-stdin");
    let result = run(cmd, PASSWORD);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

#[test]
fn fresh_setup_is_encrypted_relocatable_and_exports_a_protected_empty_portfolio() {
    let root = tempfile::tempdir().unwrap();
    let staging = root.path().join("state.setup");
    let result = init(root.path(), &staging);
    let id = result["profileId"].as_str().unwrap();
    let recovery = result["recoveryCode"].as_str().unwrap();
    assert!(!recovery.is_empty());
    let profile_db = staging.join("data/profiles").join(id).join("app.db");
    assert!(!fs::read(&profile_db)
        .unwrap()
        .starts_with(b"SQLite format 3\0"));
    for path in [
        staging.join("master-key.txt"),
        staging.join("password-hash.txt"),
        staging.join("data/secrets.json"),
        staging.join("data/profiles.json"),
    ] {
        let bytes = fs::read(path).unwrap();
        assert!(!bytes
            .windows(PASSWORD.len())
            .any(|w| w == PASSWORD.as_bytes()));
        assert!(!bytes
            .windows(recovery.len())
            .any(|w| w == recovery.as_bytes()));
    }
    let state = root.path().join("state");
    assert!(staging.starts_with(root.path()) && state.starts_with(root.path()));
    fs::rename(&staging, &state).unwrap();
    let db = state.join("data/profiles").join(id).join("app.db");
    let before = hash(&db);
    let verify = command(root.path())
        .args(["personal", "verify"])
        .arg(&state)
        .output()
        .unwrap();
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );
    assert_eq!(hash(&db), before);
    let backup = root.path().join("personal.backup");
    let mut cmd = command(root.path());
    cmd.args(["personal", "backup"])
        .arg(&state)
        .arg(&backup)
        .arg("--password-stdin");
    let output = run(cmd, BACKUP_PASSWORD);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let backup_before = hash(&backup);
    let prepared =
        portable::prepare_import(&backup, root.path(), Some(BACKUP_PASSWORD), None).unwrap();
    let conn = prepared.access.connect_rusqlite().unwrap();
    for table in [
        "accounts",
        "holdings_snapshots",
        "quotes",
        "activities",
        "personal_snapshot_imports",
    ] {
        let count: i64 = conn
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "fresh setup populated {table}");
    }
    let mut cmd = command(root.path());
    cmd.args(["personal", "backup"])
        .arg(&state)
        .arg(&backup)
        .arg("--password-stdin");
    assert!(!run(cmd, BACKUP_PASSWORD).status.success());
    assert_eq!(hash(&backup), backup_before);
    assert_eq!(hash(&db), before);
}

#[test]
fn provisioning_rejects_wrong_mode_fixtures_existing_files_and_short_passwords() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("fresh-state");
    for (purpose, version, fixture) in [
        ("standard", "1", false),
        ("fictional", "1", false),
        ("personal", "2", false),
        ("personal", "1", true),
    ] {
        let mut cmd = command(root.path());
        cmd.env("WF_RUNTIME_PURPOSE", purpose)
            .env("WF_RUNTIME_BOUNDARY_VERSION", version);
        if fixture {
            cmd.env("WEALTHFOLIO_E2E", "0");
        }
        cmd.args(["personal", "init"])
            .arg(&target)
            .arg("--password-stdin");
        assert!(!run(cmd, PASSWORD).status.success());
        assert!(!target.exists());
    }
    let mut cmd = command(root.path());
    cmd.args(["personal", "init"])
        .arg(&target)
        .arg("--password-stdin");
    assert!(!run(cmd, "short").status.success());
    assert!(!target.exists());
    fs::create_dir(&target).unwrap();
    fs::write(target.join("preserve.txt"), "synthetic existing data").unwrap();
    let before = hash(&target.join("preserve.txt"));
    let mut cmd = command(root.path());
    cmd.args(["personal", "init"])
        .arg(&target)
        .arg("--password-stdin");
    assert!(!run(cmd, PASSWORD).status.success());
    assert_eq!(hash(&target.join("preserve.txt")), before);
    assert!(!target.join("master-key.txt").exists());
}

#[test]
fn verification_does_not_recreate_missing_registry_or_accept_invalid_auth_protection() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let info = init(root.path(), &state);
    let db = state
        .join("data/profiles")
        .join(info["profileId"].as_str().unwrap())
        .join("app.db");
    let before = hash(&db);
    let original_hash = fs::read(state.join("password-hash.txt")).unwrap();
    fs::write(state.join("password-hash.txt"), "invalid").unwrap();
    let result = command(root.path())
        .args(["personal", "verify"])
        .arg(&state)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(hash(&db), before);
    fs::write(state.join("password-hash.txt"), original_hash).unwrap();
    fs::remove_file(state.join("data/profiles.json")).unwrap();
    let result = command(root.path())
        .args(["personal", "verify"])
        .arg(&state)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!state.join("data/profiles.json").exists());
    assert_eq!(hash(&db), before);
}
