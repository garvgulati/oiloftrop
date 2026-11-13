use wealthfolio_core::research::calculator::{
    calculate_example, ExampleAssessment, ExampleRequest,
};
use wealthfolio_core::research::company::{retrieve_company, CompanyData, CompanyRequest};
use wealthfolio_core::research::plan::{SavePlanRequest, StoredPlan};
use wealthfolio_core::research::ResearchContext;

use crate::profiles::ProfileAccess;
use wealthfolio_core::research::india::{
    self, AnalyseRequest, ConnectionRequest, ConnectionStatus, FinancialData, SearchRequest,
    SearchResults,
};

// Native ProfileAccess captures DatabaseRuntime::for_profile, which is built
// with registry.secret_store(&profile), not the shared unscoped store.
fn india_store(
    state: &ProfileAccess,
) -> Result<std::sync::Arc<dyn wealthfolio_core::secrets::SecretStore>, String> {
    let _context = state.context()?;
    Ok(state.secret_store.clone())
}
#[tauri::command]
pub async fn get_india_research_connection(
    state: ProfileAccess,
) -> Result<ConnectionStatus, String> {
    india::connection_status(india_store(&state)?.as_ref()).map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn set_india_research_connection(
    state: ProfileAccess,
    request: ConnectionRequest,
) -> Result<ConnectionStatus, String> {
    india::save_connection(india_store(&state)?.as_ref(), request).map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn remove_india_research_connection(
    state: ProfileAccess,
) -> Result<ConnectionStatus, String> {
    india::remove_connection(india_store(&state)?.as_ref()).map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn search_india_research(
    state: ProfileAccess,
    request: SearchRequest,
) -> Result<SearchResults, String> {
    let store = india_store(&state)?;
    india::search(store.as_ref(), request)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn analyse_india_research(
    state: ProfileAccess,
    request: AnalyseRequest,
) -> Result<FinancialData, String> {
    let store = india_store(&state)?;
    india::analyse(store.as_ref(), request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn load_research_company(
    request: CompanyRequest,
    state: ProfileAccess,
) -> Result<CompanyData, String> {
    let _context = state.context()?;
    retrieve_company(request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn load_india_research_chart(
    state: ProfileAccess,
    request: india::ChartRequest,
) -> Result<india::ChartData, String> {
    let store = india_store(&state)?;
    india::chart(store.as_ref(), request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn load_india_research_ratios(
    state: ProfileAccess,
    request: india::RatiosRequest,
) -> Result<india::RatiosData, String> {
    let store = india_store(&state)?;
    india::ratios(store.as_ref(), request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_research_plan(state: ProfileAccess) -> Result<Option<StoredPlan>, String> {
    state
        .context()?
        .research_service
        .current_plan()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_research_plan(
    request: SavePlanRequest,
    state: ProfileAccess,
) -> Result<StoredPlan, String> {
    state
        .context()?
        .research_service
        .save_plan(request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_research_context(state: ProfileAccess) -> Result<ResearchContext, String> {
    let context = state.context()?;
    context
        .research_service
        .current_context(context.get_base_currency())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn run_research_example(
    request: ExampleRequest,
    state: ProfileAccess,
) -> Result<ExampleAssessment, String> {
    let _context = state.context()?;
    calculate_example(request.case_id)
        .await
        .map_err(|error| error.to_string())
}
