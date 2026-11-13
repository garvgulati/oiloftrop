//! Offline personal provisioning. No listener, providers, workers or owner data import.
use anyhow::{ensure, Context};
use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2, PasswordHash,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::Arc,
};
use uuid::Uuid;
use wealthfolio_core::{profiles::ProfileRegistry, runtime_boundary::RuntimePurpose};
use wealthfolio_storage_sqlite::db::{self, DbAccess, DbEncryptionKey};
use zeroize::Zeroizing;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupResult {
    pub profile_id: Uuid,
    // Delivered to the local setup UI only; never written to diagnostics or a plaintext file.
    pub recovery_code: String,
}

fn require_personal() -> anyhow::Result<()> {
    ensure!(RuntimePurpose::from_env()? == RuntimePurpose::Personal,
        "Personal setup requires explicit personal purpose and compatible boundary version, without fixture settings.");
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// The installer creates an owner-protected parent directory before invoking this.
/// A partial setup remains in its unpublished staging directory; it is never overwritten.
pub fn initialize(state: &Path, password: &str) -> anyhow::Result<SetupResult> {
    require_personal()?;
    ensure!(
        (12..=128).contains(&password.chars().count())
            && password.len() <= 512
            && !password.chars().any(char::is_control),
        "Use a sign-in password of 12–128 characters without control characters."
    );
    ensure!(
        !state.exists(),
        "Personal setup requires a fresh state directory; existing files were preserved."
    );
    let mut master = Zeroizing::new([0u8; 32]);
    OsRng.fill_bytes(&mut master[..]);
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_| anyhow::anyhow!("Could not prepare sign-in protection"))?
        .to_string();
    fs::create_dir(state).context("Cannot create the fresh personal staging directory")?;
    let encoded = Zeroizing::new(BASE64.encode(&master[..]));
    write_new(&state.join("master-key.txt"), encoded.as_bytes())?;
    write_new(&state.join("password-hash.txt"), hash.as_bytes())?;
    let data = state.join("data");
    fs::create_dir(&data)?;
    let (_, vault_key) = crate::auth::derive_keys(&master[..]);
    let store = crate::secrets::build_secret_store(
        data.join("secrets.json"),
        Some(vault_key),
        Some(&master[..]),
    )?;
    let registry = ProfileRegistry::open(data.clone(), data.join("app.db"), Arc::new(store))?;
    let id = registry.default_id()?;
    let recovery_code = registry
        .set_password(id, None, Some(password))?
        .context("Profile protection did not return its local recovery code")?;
    let profile = registry.profile(id)?;
    ensure!(
        profile.legacy_database.is_none(),
        "Personal setup cannot adopt a legacy database."
    );
    let path = registry.paths(&profile).database;
    fs::create_dir_all(path.parent().context("Invalid profile directory")?)?;
    let key = Arc::new(DbEncryptionKey::from_bytes(
        &crate::auth::derive_profile_database_key(&master[..], id),
    ));
    let access = DbAccess::encrypted(path.to_str().context("Invalid profile path")?, key);
    let owner = db::DatabaseOwner::acquire(access.path())?;
    access.prepare()?;
    let admission = access.check_runtime_boundary(RuntimePurpose::Personal)?;
    access.run_migrations()?;
    access.seal_runtime_boundary(admission)?;
    let conn = access.connect_rusqlite()?;
    for (name, value) in [
        ("base_currency", "INR"),
        ("timezone", "Asia/Kolkata"),
        ("sync_enabled", "false"),
        ("auto_update_check_enabled", "false"),
        ("onboarding_completed", "true"),
    ] {
        conn.execute("INSERT INTO app_settings(setting_key,setting_value) VALUES(?,?) ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value", [name, value])?;
    }
    drop(conn);
    drop(owner);
    drop(registry);
    verify(state)?;
    Ok(SetupResult {
        profile_id: id,
        recovery_code,
    })
}

/// Offline validation only. It does not run migrations, replace a DB or fetch data.
pub fn verify(state: &Path) -> anyhow::Result<()> {
    let _verified = open_verified(state)?;
    Ok(())
}

/// Admit only the provisioned local layout, before opening auth or profile services.
pub(crate) fn verify_web_configuration(config: &crate::config::Config) -> anyhow::Result<()> {
    require_personal()?;
    let database = std::path::absolute(&config.db_path)?;
    let data = database
        .parent()
        .context("Personal provisioning is required before web startup")?;
    let state = data
        .parent()
        .context("Personal provisioning is required before web startup")?;
    ensure!(
        data.file_name().is_some_and(|name| name == "data")
            && database.file_name().is_some_and(|name| name == "app.db"),
        "Personal provisioning is required before web startup"
    );
    let hash = fs::read_to_string(state.join("password-hash.txt"))
        .context("Personal provisioning is required before web startup")?;
    ensure!(
        config
            .auth
            .as_ref()
            .and_then(|auth| auth.password_hash.as_deref())
            .is_some_and(|configured| configured.trim() == hash.trim()),
        "Personal authentication must match its provisioned state"
    );
    let master = Zeroizing::new(crate::config::load_secret_key(
        None,
        Some(state.join("master-key.txt").into_os_string()),
    )?);
    ensure!(
        *master == config.raw_secret_key,
        "Personal encryption must match its provisioned state"
    );
    verify(state)
}

pub(crate) fn verify_profile_configuration(
    config: &crate::config::Config,
    registry: &ProfileRegistry,
    id: Uuid,
) -> anyhow::Result<()> {
    require_personal()?;
    let profiles = registry.list()?;
    ensure!(
        profiles.len() == 1 && profiles[0].id == id,
        "Personal hosting requires its one provisioned owner profile"
    );
    let profile = registry.profile(id)?;
    ensure!(
        profile.legacy_database.is_none()
            && profile.connect.is_none()
            && registry.is_protected(id)?,
        "Personal hosting requires a protected local profile"
    );
    let path = registry.paths(&profile).database;
    ensure!(
        path.is_file()
            && std::path::absolute(&config.db_path)? == std::path::absolute(&path)?
            && config.database_key
                == crate::auth::derive_profile_database_key(&config.raw_secret_key, id),
        "Personal hosting cannot create or adopt a different profile database"
    );
    let access = DbAccess::encrypted(
        path.to_str().context("Invalid profile path")?,
        Arc::new(DbEncryptionKey::from_bytes(&config.database_key)),
    );
    access.check_runtime_boundary(RuntimePurpose::Personal)?;
    crate::database_restore::validate_reviewed_profile(&access, Some(id))
}

struct VerifiedState {
    access: DbAccess,
    _owner: db::DatabaseOwner,
    _registry: ProfileRegistry,
}

/// A password-protected copy of the selected personal database, never its master key.
pub fn backup(state: &Path, output: &Path, password: &str) -> anyhow::Result<()> {
    db::portable::validate_password(password)?;
    ensure!(
        !output.exists(),
        "Backup output already exists; no file was overwritten."
    );
    let verified = open_verified(state)?;
    let exported = db::portable::export(&verified.access, state, Some(password))?;
    let mut destination = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output)?;
    std::io::copy(&mut fs::File::open(&exported.path)?, &mut destination)?;
    destination.sync_all()?;
    Ok(())
}

fn open_verified(state: &Path) -> anyhow::Result<VerifiedState> {
    require_personal()?;
    ensure!(state.is_dir(), "Personal state directory is missing.");
    let hash = fs::read_to_string(state.join("password-hash.txt"))?;
    let parsed = PasswordHash::new(hash.trim())
        .map_err(|_| anyhow::anyhow!("Personal sign-in protection is invalid"))?;
    ensure!(
        parsed.algorithm.as_str() == "argon2id",
        "Personal sign-in requires Argon2id protection."
    );
    let master = Zeroizing::new(crate::config::load_secret_key(
        None,
        Some(state.join("master-key.txt").into_os_string()),
    )?);
    let data = state.join("data");
    ensure!(
        data.join("profiles.json").is_file(),
        "Personal profile registry is missing; existing data was preserved."
    );
    let (_, vault_key) = crate::auth::derive_keys(&master);
    let store = crate::secrets::build_secret_store(
        data.join("secrets.json"),
        Some(vault_key),
        Some(&master),
    )?;
    let registry = ProfileRegistry::open(data.clone(), data.join("app.db"), Arc::new(store))?;
    let profiles = registry.list()?;
    ensure!(profiles.len() == 1, "This personal release requires its one protected owner profile; use accounts for separate brokers.");
    let summary = profiles
        .into_iter()
        .next()
        .context("Personal state has no protected profile")?;
    let profile = registry.profile(summary.id)?;
    ensure!(profile.legacy_database.is_none() && profile.connect.is_none() && registry.is_protected(profile.id)?,
            "Personal state requires fresh, password-protected local profiles without inherited connections.");
    let path = registry.paths(&profile).database;
    ensure!(
        path.is_file(),
        "Personal profile database is missing; restore its compatible encrypted backup."
    );
    let key = Arc::new(DbEncryptionKey::from_bytes(
        &crate::auth::derive_profile_database_key(&master, profile.id),
    ));
    let access = DbAccess::encrypted(path.to_str().context("Invalid profile path")?, key);
    let owner = db::DatabaseOwner::acquire(access.path())
        .context("Stop the personal app before verifying or recovering its state")?;
    access.check_runtime_boundary(RuntimePurpose::Personal)?;
    db::maintenance::verify_for_reopen(&access)?;
    crate::database_restore::validate_reviewed_profile(&access, Some(profile.id))?;
    Ok(VerifiedState {
        access,
        _owner: owner,
        _registry: registry,
    })
}
