//! Personal data is served by the private local web host, not native hosting.
use crate::profiles::ProfileAccess;
use wealthfolio_core::portfolio::snapshot::reviewed_import::*;

fn unsupported(state: &ProfileAccess) -> Result<(), String> {
    let _ = state.context()?;
    Err("PERSONAL_IMPORT_UNSUPPORTED".into())
}

#[tauri::command]
pub async fn get_reviewed_import_status(
    state: ProfileAccess,
) -> Result<ReviewedImportStatus, String> {
    let _ = state.context()?;
    Ok(ReviewedImportStatus::unsupported())
}

#[tauri::command]
pub async fn create_reviewed_import_account(
    state: ProfileAccess,
    request: NewReviewedImportAccount,
) -> Result<ReviewedImportAccount, String> {
    let _ = request;
    unsupported(&state)?;
    Err("PERSONAL_IMPORT_UNSUPPORTED".into())
}

#[tauri::command]
pub async fn prepare_reviewed_import(
    state: ProfileAccess,
    request: ReviewedSnapshotInput,
) -> Result<PreparedReviewedImport, String> {
    let _ = request;
    unsupported(&state)?;
    Err("PERSONAL_IMPORT_UNSUPPORTED".into())
}

#[tauri::command]
pub async fn confirm_reviewed_import(
    state: ProfileAccess,
    request: ConfirmReviewedImportRequest,
) -> Result<ImportReceipt, String> {
    let _ = request;
    unsupported(&state)?;
    Err("PERSONAL_IMPORT_UNSUPPORTED".into())
}

#[tauri::command]
pub async fn discard_reviewed_import(
    state: ProfileAccess,
    request: DiscardReviewedImportRequest,
) -> Result<(), String> {
    let _ = request;
    unsupported(&state)
}

#[tauri::command]
pub async fn latest_reviewed_import(
    state: ProfileAccess,
    request: LatestReviewedImportRequest,
) -> Result<Option<SavedReviewedSnapshot>, String> {
    let _ = request;
    unsupported(&state)?;
    Err("PERSONAL_IMPORT_UNSUPPORTED".into())
}
