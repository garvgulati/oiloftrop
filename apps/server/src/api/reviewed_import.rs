//! Thin, local-only reviewed-import endpoints. Host metadata is never input JSON.
use std::sync::Arc;

use axum::{
    body::Body,
    extract::DefaultBodyLimit,
    http::{header, Request},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Extension, Json, Router,
};
use wealthfolio_core::{
    accounts::{AccountServiceTrait, TrackingMode},
    portfolio::snapshot::reviewed_import::*,
    profiles::ProfileRegistry,
    runtime_boundary::RuntimePurpose,
    Error as CoreError,
};

use crate::{
    error::{ApiError, ApiResult},
    main_lib::AppState,
    profiles::{ProfileAccess, WebProfiles},
};

#[derive(Clone)]
struct ReviewedEndpointContext {
    service: Arc<ReviewedImportService>,
    binding: Arc<ImportReviewBinding>,
}

fn safe_error(error: CoreError) -> ApiError {
    match &error {
        CoreError::ConstraintViolation(code)
            if code.starts_with("PERSONAL_IMPORT_")
                && code.len() <= 80
                && code.bytes().all(|c| c.is_ascii_uppercase() || c == b'_') =>
        {
            ApiError::Core(error)
        }
        _ => ApiError::Internal("PERSONAL_IMPORT_UNAVAILABLE".into()),
    }
}

fn bind_scope(
    purpose: RuntimePurpose,
    bound: Option<&(Arc<ProfileRegistry>, uuid::Uuid)>,
    registry: &Arc<ProfileRegistry>,
    access: &ProfileAccess,
) -> ApiResult<ImportReviewBinding> {
    if purpose != RuntimePurpose::Personal {
        return Err(ApiError::Forbidden("PERSONAL_IMPORT_UNSUPPORTED".into()));
    }
    let (bound_registry, profile) =
        bound.ok_or_else(|| safe_error(import_error("PERSONAL_IMPORT_GRANT_INVALID")))?;
    if !Arc::ptr_eq(bound_registry, registry) || *profile != access.session.profile_id {
        return Err(safe_error(import_error("PERSONAL_IMPORT_GRANT_INVALID")));
    }
    ImportReviewBinding::new(
        *profile,
        access.session.scope_id,
        &access.owner,
        Arc::new(RegistryImportGrantAuthority(registry.clone())),
    )
    .map_err(safe_error)
}

async fn admit(
    Extension(state): Extension<Arc<AppState>>,
    Extension(root): Extension<Arc<WebProfiles>>,
    Extension(access): Extension<ProfileAccess>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let context = async {
        let binding = bind_scope(state.runtime_purpose, state.profile_binding.get(), &root.registry, &access)?;
        let pool = state.db_pool.clone();
        let writer = state.writer.clone();
        let db_access = state.db_access.clone();
        let profile = access.session.profile_id;
        let service = state.reviewed_import_service.get_or_try_init(|| async move {
            tokio::task::spawn_blocking(move || {
                let repo = wealthfolio_storage_sqlite::portfolio::snapshot::reviewed_import::ReviewedSnapshotRepository::new(
                    pool, writer, profile, &db_access)?;
                Ok(Arc::new(ReviewedImportService::new(Arc::new(repo))))
            }).await.map_err(|_| import_error("PERSONAL_IMPORT_UNAVAILABLE"))?
        }).await.map_err(safe_error)?.clone();
        // Initialization may await database access. Re-admit before handling input.
        let _ = bind_scope(state.runtime_purpose, state.profile_binding.get(), &root.registry, &access)?;
        Ok::<_, ApiError>(ReviewedEndpointContext { service, binding: Arc::new(binding) })
    }.await;
    match context {
        Ok(context) => {
            request.extensions_mut().insert(context);
            next.run(request).await
        }
        Err(error) => error.into_response(),
    }
}

fn operations() -> Router {
    Router::new()
        .route("/personal/holdings/accounts", post(create_account))
        .route("/personal/holdings/confirm", post(confirm))
        .route("/personal/holdings/discard", post(discard))
        .route("/personal/holdings/latest", post(latest))
        .layer(DefaultBodyLimit::max(4096))
        .merge(
            Router::new()
                .route("/personal/holdings/prepare", post(prepare))
                .layer(DefaultBodyLimit::max(MAX_REVIEW_BYTES)),
        )
}

pub fn router() -> Router {
    operations()
        .layer(middleware::from_fn(admit))
        .route("/personal/holdings/status", get(status))
        .layer(middleware::from_fn(no_store))
}

async fn no_store(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

async fn status(
    Extension(state): Extension<Arc<AppState>>,
    Extension(root): Extension<Arc<WebProfiles>>,
    Extension(access): Extension<ProfileAccess>,
) -> ApiResult<Json<ReviewedImportStatus>> {
    if state.runtime_purpose != RuntimePurpose::Personal {
        return Ok(Json(ReviewedImportStatus::unsupported()));
    }
    let _ = bind_scope(
        state.runtime_purpose,
        state.profile_binding.get(),
        &root.registry,
        &access,
    )?;
    let service = state.account_service.clone();
    let accounts = tokio::task::spawn_blocking(move || service.get_non_archived_accounts())
        .await
        .map_err(|_| safe_error(import_error("PERSONAL_IMPORT_UNAVAILABLE")))?
        .map_err(safe_error)?;
    let _ = bind_scope(
        state.runtime_purpose,
        state.profile_binding.get(),
        &root.registry,
        &access,
    )?;
    let accounts = accounts
        .into_iter()
        .filter(|a| {
            a.is_active
                && a.tracking_mode == TrackingMode::Holdings
                && matches!(a.currency.as_str(), "INR" | "USD")
        })
        .filter_map(|a| {
            uuid::Uuid::parse_str(&a.id)
                .ok()
                .map(|id| ReviewedImportAccount {
                    id,
                    name: a.name,
                    currency: a.currency,
                })
        })
        .collect();
    Ok(Json(ReviewedImportStatus {
        supported: true,
        accounts,
    }))
}

async fn create_account(
    Extension(context): Extension<ReviewedEndpointContext>,
    Json(input): Json<NewReviewedImportAccount>,
) -> ApiResult<Json<ReviewedImportAccount>> {
    Ok(Json(
        context
            .service
            .create_account(&context.binding, input)
            .await
            .map_err(safe_error)?,
    ))
}

async fn prepare(
    Extension(context): Extension<ReviewedEndpointContext>,
    Json(input): Json<ReviewedSnapshotInput>,
) -> ApiResult<Json<PreparedReviewedImport>> {
    Ok(Json(
        context
            .service
            .prepare(&context.binding, input)
            .await
            .map_err(safe_error)?,
    ))
}
async fn confirm(
    Extension(context): Extension<ReviewedEndpointContext>,
    Json(input): Json<ConfirmReviewedImportRequest>,
) -> ApiResult<Json<ImportReceipt>> {
    Ok(Json(
        context
            .service
            .confirm(&context.binding, input.review_id, input.input_hash)
            .await
            .map_err(safe_error)?,
    ))
}
async fn discard(
    Extension(context): Extension<ReviewedEndpointContext>,
    Json(input): Json<DiscardReviewedImportRequest>,
) -> ApiResult<Json<()>> {
    context
        .service
        .discard(&context.binding, input.review_id)
        .map_err(safe_error)?;
    Ok(Json(()))
}
async fn latest(
    Extension(context): Extension<ReviewedEndpointContext>,
    Json(input): Json<LatestReviewedImportRequest>,
) -> ApiResult<Json<Option<SavedReviewedSnapshot>>> {
    Ok(Json(
        context
            .service
            .latest(&context.binding, input.account_id)
            .await
            .map_err(safe_error)?,
    ))
}

#[cfg(test)]
#[path = "reviewed_import_tests.rs"]
mod tests;
