#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]

use wealthfolio_core::accounts::{Account, AccountRepositoryTrait, AccountUpdate, NewAccount};
use wealthfolio_storage_sqlite::{accounts::AccountRepository, db};

fn edit(account: &Account, name: &str) -> AccountUpdate {
    let mut value = serde_json::to_value(account).unwrap();
    value["expectedUpdatedAt"] = value["updatedAt"].clone();
    value["name"] = name.into();
    serde_json::from_value(value).unwrap()
}

async fn fixture() -> (
    tempfile::TempDir,
    AccountRepository,
    Account,
    db::WriteHandle,
) {
    let dir = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let access = db::DbAccess::plaintext(dir.path().join("app.db").to_str().unwrap());
    access.run_migrations().unwrap();
    let pool = access.create_pool().unwrap();
    let writer = db::write_actor::spawn_writer((*pool).clone()).unwrap();
    let repository = AccountRepository::new(pool, writer.clone());
    let account = repository
        .create(
            serde_json::from_value::<NewAccount>(serde_json::json!({
                "name": "Fictional account", "accountType": "SECURITIES", "currency": "INR",
                "isDefault": false, "isActive": true, "trackingMode": "HOLDINGS"
            }))
            .unwrap(),
        )
        .await
        .unwrap();
    (dir, repository, account, writer)
}

#[tokio::test]
async fn stale_edit_preserves_newer_save_and_reload_allows_retry() {
    let (_dir, repository, opened, writer) = fixture().await;
    let first = repository
        .update(edit(&opened, "First device"))
        .await
        .unwrap();
    let stale = repository
        .update(edit(&opened, "Stale second device"))
        .await;
    assert!(
        stale.is_err(),
        "A stale form must not overwrite the first device"
    );
    assert!(stale
        .unwrap_err()
        .to_string()
        .contains("ACCOUNT_EDIT_CONFLICT"));
    let saved = repository.get_by_id(&opened.id).unwrap();
    assert_eq!(saved.name, "First device");
    assert_eq!(saved.updated_at, first.updated_at);
    let reloaded = repository
        .update(edit(&saved, "Reviewed second device"))
        .await
        .unwrap();
    assert_eq!(reloaded.name, "Reviewed second device");
    assert!(reloaded.updated_at > first.updated_at);
    writer.shutdown().await;
    drop(repository);
}

#[tokio::test]
async fn concurrent_edits_of_one_version_admit_exactly_one() {
    let (_dir, repository, opened, writer) = fixture().await;
    let (first, second) = tokio::join!(
        repository.update(edit(&opened, "First device")),
        repository.update(edit(&opened, "Second device")),
    );
    assert_ne!(
        first.is_ok(),
        second.is_ok(),
        "Exactly one save may use a version"
    );
    let winner = first.or(second).unwrap();
    assert_eq!(repository.get_by_id(&opened.id).unwrap().name, winner.name);
    assert!(winner.updated_at > opened.updated_at);
    writer.shutdown().await;
    drop(repository);
}

#[test]
fn update_without_original_version_is_rejected() {
    let update = serde_json::from_value::<AccountUpdate>(serde_json::json!({
        "id": "fictional", "name": "Unversioned", "accountType": "SECURITIES",
        "isDefault": false, "isActive": true
    }));
    assert!(
        update.is_err(),
        "An omitted revision must not bypass conflict protection"
    );
}

#[tokio::test]
async fn version_advances_even_when_the_saved_clock_is_ahead() {
    let (dir, repository, opened, writer) = fixture().await;
    let future = chrono::Utc::now().naive_utc() + chrono::Duration::days(1);
    let access = db::DbAccess::plaintext(dir.path().join("app.db").to_str().unwrap());
    access
        .connect_rusqlite()
        .unwrap()
        .execute(
            "UPDATE accounts SET updated_at = ?1 WHERE id = ?2",
            rusqlite::params![future.to_string(), opened.id],
        )
        .unwrap();
    let original = repository.get_by_id(&opened.id).unwrap();
    let saved = repository
        .update(edit(&original, "Reviewed future version"))
        .await
        .unwrap();
    assert_eq!(
        saved.updated_at,
        original.updated_at + chrono::Duration::microseconds(1)
    );
    writer.shutdown().await;
    drop(repository);
}
