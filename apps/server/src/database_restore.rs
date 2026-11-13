//! Offline portable restore. No listener, authentication discovery or service workers.
use anyhow::{ensure, Context};
use std::{fs, io::Read, path::Path, sync::Arc};
use wealthfolio_core::runtime_boundary::RuntimePurpose;
use wealthfolio_storage_sqlite::db::{self, maintenance, portable, DbAccess, DbEncryptionKey};

pub fn run_database_restore(
    path: &Path,
    password: Option<&str>,
    confirmed: bool,
) -> anyhow::Result<()> {
    run_profile_database_restore(path, password, confirmed, None)
}
pub fn run_profile_database_restore(
    path: &Path,
    password: Option<&str>,
    confirmed: bool,
    profile: Option<uuid::Uuid>,
) -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let purpose = RuntimePurpose::from_env()?;
    let secret = crate::config::load_secret_key(
        std::env::var_os("WF_SECRET_KEY"),
        std::env::var_os("WF_SECRET_KEY_FILE"),
    )?;
    let database = crate::config::database_path_from_env()?;
    let (database, database_key, registry) =
        crate::profiles::offline_database(database, &secret, profile)?;
    let key = Arc::new(DbEncryptionKey::from_bytes(&database_key));
    let required = std::env::var("WF_DB_REQUIRE_ENCRYPTION")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "true" | "1" | "yes"
            )
        })
        .unwrap_or(false);
    ensure!(
        purpose != RuntimePurpose::Personal || required,
        "Personal restore requires encryption; the destination was not replaced."
    );
    let destination = Path::new(&database);
    ensure!(
        fs::symlink_metadata(destination).is_ok_and(|entry| entry.is_file() && entry.len() > 0),
        "Restore requires an existing, nonempty database at {database}. Check the mounted data directory; missing or unreadable database recovery is not supported by this command."
    );
    let owner = db::DatabaseOwner::acquire(&database)
        .context("Cannot restore while the server or another database operation is running")?;
    let mut header = [0; 16];
    fs::File::open(destination)?.read_exact(&mut header)?;
    ensure!(
        (header != *b"SQLite format 3\0") == required,
        "Existing database does not match WF_DB_REQUIRE_ENCRYPTION. Correct the configuration or use db encrypt/db decrypt first; restore does not change encryption policy."
    );
    let current = DbAccess::new(&database, required.then(|| key.clone()));
    current
        .check_runtime_boundary(purpose)
        .context("Destination runtime purpose is incompatible; nothing was restored")?;
    let selected_profile = if let Some(registry) = &registry {
        Some(profile.unwrap_or(registry.default_id()?))
    } else {
        None
    };
    if purpose == RuntimePurpose::Personal {
        validate_reviewed_profile(&current, selected_profile)?;
        let mut source_header = [0; 16];
        fs::File::open(path)?.read_exact(&mut source_header)?;
        ensure!(
            source_header != *b"SQLite format 3\0",
            "Personal restore requires an encrypted backup; the destination was not replaced."
        );
    }
    maintenance::verify_for_reopen(&current).context(
        "Destination database cannot be read with the configured key and policy. Preserve it and correct the configuration before restoring",
    )?;
    maintenance::check_sqlite_locks(&current)?;
    let root = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let root = root.to_str().context("Invalid database directory")?;
    let scratch = db::profile_scratch_dir(root)?;

    let decoded;
    let (backup_path, backup_password) = if db::cloud_backups::is_recovery_package(path)? {
        let store: Arc<dyn wealthfolio_core::secrets::SecretStore> =
            if let Some(registry) = &registry {
                let id = profile.unwrap_or(registry.default_id()?);
                registry.secret_store(&registry.profile(id)?)
            } else {
                let (_, vault_key) = crate::auth::derive_keys(&secret);
                Arc::new(crate::secrets::build_secret_store(
                    std::env::var("WF_SECRET_FILE")
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(|_| Path::new(root).join("secrets.json")),
                    Some(vault_key),
                    Some(&secret),
                )?)
            };
        decoded = db::cloud_backups::decoded_package(
            fs::File::open(path)?,
            store.as_ref(),
            password,
            &scratch,
        )?;
        (decoded.path(), None)
    } else {
        (path, password)
    };
    // Match the existing offline conversion path and the shared scratch helper.
    let prepared = portable::prepare_import(backup_path, &scratch, backup_password, Some(key))
        .context("Backup validation failed; the destination database was not replaced")?;
    prepared.access.check_runtime_boundary(purpose).context(
        "Backup runtime purpose or version is incompatible; the destination was not replaced",
    )?;
    if purpose == RuntimePurpose::Personal {
        validate_reviewed_profile(&prepared.access, selected_profile)?;
    }
    println!(
        "Validated backup: {} accounts, {} activities. Destination encryption: {}.",
        prepared.summary.account_count,
        prepared.summary.activity_count,
        if required { "enabled" } else { "disabled" },
    );
    ensure!(
        confirmed,
        "Validation only: the destination database was not replaced. Run again with --yes to replace the portfolio."
    );
    let outcome = maintenance::run(
        root,
        &current,
        maintenance::MaintenanceRequest::Restore {
            backup_path: prepared.access.path().into(),
            device_key: prepared.access.key().cloned(),
        },
        &owner,
    )?;
    if purpose == RuntimePurpose::Personal {
        println!("Restore completed. Start the compatible personal runtime and verify the saved records. Automatic providers and synchronization remain disabled.");
    } else {
        println!("Restore completed. Reconnect Wealthfolio Connect and device sync after starting the server.");
    }
    if let Some(backup) = outcome.pre_operation_backup {
        println!("Previous database snapshot: {backup}");
    }
    Ok(())
}

pub(crate) fn validate_reviewed_profile(
    access: &DbAccess,
    profile: Option<uuid::Uuid>,
) -> anyhow::Result<()> {
    let conn = access.connect_rusqlite()?;
    let mut query = conn
        .prepare("SELECT DISTINCT profile_id FROM personal_snapshot_imports LIMIT 2")
        .context("Personal recovery requires compatible reviewed-import metadata")?;
    let ids = query
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    ensure!(
        ids.len() <= 1,
        "Personal recovery found conflicting profile identities; the destination was not replaced."
    );
    if let Some(id) = ids.first() {
        ensure!(uuid::Uuid::parse_str(id).ok().zip(profile).is_some_and(|(stored, selected)| stored == selected),
            "Reviewed holdings belong to a different or unregistered profile. Select their original registered profile; the destination was not replaced.");
    }
    Ok(())
}
