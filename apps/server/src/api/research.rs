use std::sync::Arc;

use axum::{
    routing::{get, post},
    Extension, Json, Router,
};
use wealthfolio_core::research::{
    calculator::{calculate_example, ExampleAssessment, ExampleRequest},
    company::{retrieve_company, CompanyRequest},
    india::{self, AnalyseRequest, ConnectionRequest, SearchRequest},
    plan::{SavePlanRequest, StoredPlan},
    ResearchContext,
};

use crate::{error::ApiResult, main_lib::AppState};

async fn get_research_context(
    Extension(state): Extension<Arc<AppState>>,
) -> ApiResult<Json<ResearchContext>> {
    Ok(Json(
        state
            .research_service
            .current_context(state.base_currency()?)
            .await?,
    ))
}

pub fn router() -> Router {
    Router::new()
        .route("/research/context", get(get_research_context))
        .route("/research/example", post(run_research_example))
        .route("/research/company", post(load_company))
        .route("/research/india/search", post(search_india))
        .route("/research/india/analyse", post(analyse_india))
        .route("/research/india/chart", post(chart_india))
        .route("/research/india/ratios", post(ratios_india))
        .route(
            "/research/india/connection",
            get(india_status)
                .put(set_india_connection)
                .delete(remove_india_connection),
        )
        .route(
            "/research/plan",
            get(get_research_plan).put(save_research_plan),
        )
}

// Unlike ordinary data reads, these operations explicitly require the captured
// owner/profile grant and obtain its store from the registry. They never use
// AppState.secret_store. WebProfiles::research_secret_store re-admits the grant;
// registry.secret_store returns the tested ScopedSecretStore for that profile.
async fn india_status(
    Extension(root): Extension<Arc<crate::profiles::WebProfiles>>,
    Extension(access): Extension<crate::profiles::ProfileAccess>,
) -> ApiResult<axum::response::Response> {
    let store = root.research_secret_store(&access)?;
    Ok(uncached(india::connection_status(store.as_ref())?))
}
async fn set_india_connection(
    Extension(root): Extension<Arc<crate::profiles::WebProfiles>>,
    Extension(access): Extension<crate::profiles::ProfileAccess>,
    Json(request): Json<ConnectionRequest>,
) -> ApiResult<axum::response::Response> {
    let store = root.research_secret_store(&access)?;
    Ok(uncached(india::save_connection(store.as_ref(), request)?))
}
async fn remove_india_connection(
    Extension(root): Extension<Arc<crate::profiles::WebProfiles>>,
    Extension(access): Extension<crate::profiles::ProfileAccess>,
) -> ApiResult<axum::response::Response> {
    let store = root.research_secret_store(&access)?;
    Ok(uncached(india::remove_connection(store.as_ref())?))
}
fn uncached<T: serde::Serialize>(value: T) -> axum::response::Response {
    use axum::response::IntoResponse;
    let mut response = Json(value).into_response();
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}
async fn search_india(
    Extension(root): Extension<Arc<crate::profiles::WebProfiles>>,
    Extension(access): Extension<crate::profiles::ProfileAccess>,
    Json(request): Json<SearchRequest>,
) -> ApiResult<axum::response::Response> {
    let store = root.research_secret_store(&access)?;
    Ok(uncached(india::search(store.as_ref(), request).await?))
}
async fn analyse_india(
    Extension(root): Extension<Arc<crate::profiles::WebProfiles>>,
    Extension(access): Extension<crate::profiles::ProfileAccess>,
    Json(request): Json<AnalyseRequest>,
) -> ApiResult<axum::response::Response> {
    let store = root.research_secret_store(&access)?;
    Ok(uncached(india::analyse(store.as_ref(), request).await?))
}
async fn chart_india(
    Extension(root): Extension<Arc<crate::profiles::WebProfiles>>,
    Extension(access): Extension<crate::profiles::ProfileAccess>,
    Json(request): Json<india::ChartRequest>,
) -> ApiResult<axum::response::Response> {
    let store = root.research_secret_store(&access)?;
    Ok(uncached(india::chart(store.as_ref(), request).await?))
}
async fn ratios_india(
    Extension(root): Extension<Arc<crate::profiles::WebProfiles>>,
    Extension(access): Extension<crate::profiles::ProfileAccess>,
    Json(request): Json<india::RatiosRequest>,
) -> ApiResult<axum::response::Response> {
    let store = root.research_secret_store(&access)?;
    Ok(uncached(india::ratios(store.as_ref(), request).await?))
}

async fn load_company(
    Extension(_state): Extension<Arc<AppState>>,
    Json(request): Json<CompanyRequest>,
) -> ApiResult<axum::response::Response> {
    use axum::response::IntoResponse;
    let mut response = Json(retrieve_company(request).await?).into_response();
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}

async fn get_research_plan(
    Extension(state): Extension<Arc<AppState>>,
) -> ApiResult<Json<Option<StoredPlan>>> {
    Ok(Json(state.research_service.current_plan().await?))
}

async fn save_research_plan(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<SavePlanRequest>,
) -> ApiResult<Json<StoredPlan>> {
    Ok(Json(state.research_service.save_plan(request).await?))
}

async fn run_research_example(
    Extension(_state): Extension<Arc<AppState>>,
    Json(request): Json<ExampleRequest>,
) -> ApiResult<Json<ExampleAssessment>> {
    Ok(Json(calculate_example(request.case_id).await?))
}
