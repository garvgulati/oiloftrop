//! Personal hosting uses an explicit capability list. Unknown routes fail
//! closed, including new upstream APIs. This does not replace profile admission.
use axum::{
    body::Body,
    extract::State,
    http::{header, Method, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Json, Router,
};
use wealthfolio_core::runtime_boundary::{RuntimeCapability, RuntimePurpose};

use crate::config::Config;

const PERSONAL_CSP: &str = "default-src 'self'; script-src 'self' 'sha256-OUUXM+aKkYdqwM38Z84FhgHpIYOk/e5Dz9UaAnwYXk8=' 'sha256-s/UhdlprnzFxx+iXOtDj2n/Jk+MSRz1g/1lyBtFatVw=' 'wasm-unsafe-eval' blob:; style-src 'self' 'unsafe-inline' blob:; img-src 'self' data: blob:; font-src 'self' data: blob:; media-src 'self' data: blob:; connect-src 'self'; frame-src https://www.tradingview-widget.com/embed-widget/advanced-chart/; child-src 'none'; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'; worker-src 'self' blob:";

pub fn personal_csp(purpose: RuntimePurpose) -> Option<&'static str> {
    (purpose == RuntimePurpose::Personal).then_some(PERSONAL_CSP)
}

/// Enforce before OIDC discovery or profile/provider initialization. The
/// personal setup uses local password auth and the approved loopback proxy.
pub fn validate_configuration(purpose: RuntimePurpose, config: &Config) -> anyhow::Result<()> {
    if purpose != RuntimePurpose::Personal {
        return Ok(());
    }
    anyhow::ensure!(
        config.oidc.is_none(),
        "Personal hosting does not enable external sign-in discovery."
    );
    anyhow::ensure!(!config.mcp_enabled, "Personal hosting does not enable MCP.");
    anyhow::ensure!(
        config.db_encryption_required,
        "Personal hosting requires database encryption."
    );
    anyhow::ensure!(
        config.listen_addr.ip().is_loopback(),
        "Personal hosting requires the approved loopback proxy."
    );
    anyhow::ensure!(
        config.auth.as_ref().is_some_and(|auth| auth
            .password_hash
            .as_ref()
            .is_some_and(|hash| !hash.trim().is_empty())
            && matches!(&auth.cookie_secure, crate::auth::CookieSecurePolicy::Always)),
        "Personal hosting requires password authentication and secure cookies."
    );
    anyhow::ensure!(
        !config.cors_allow.is_empty()
            && config.cors_allow.iter().all(|origin| {
                reqwest::Url::parse(origin).is_ok_and(|url| {
                    url.scheme() == "https"
                        && url.username().is_empty()
                        && url.password().is_none()
                        && url.path() == "/"
                        && url.query().is_none()
                        && url.fragment().is_none()
                        && *origin == url.origin().ascii_serialization()
                })
            }),
        "Personal hosting requires exact HTTPS browser origins."
    );
    Ok(())
}

fn request_capability(method: &Method, path: &str) -> Option<RuntimeCapability> {
    let path = path.strip_prefix("/api/v1").unwrap_or(path);
    match (method.as_str(), path) {
        ("GET", "/healthz" | "/readyz" | "/auth/status" | "/auth/me")
        | ("POST", "/auth/login" | "/auth/logout") => Some(RuntimeCapability::LocalMetadata),
        ("GET", "/settings" | "/settings/auto-update-enabled" | "/accounts" | "/app/info") => {
            Some(RuntimeCapability::LocalMetadata)
        }
        (
            "POST",
            "/profiles/get_profile_state"
            | "/profiles/unlock_profile"
            | "/profiles/lock_profile"
            | "/profiles/profile_activity"
            | "/profiles/update_profile"
            | "/profiles/set_profile_password"
            | "/profiles/recover_profile_password",
        ) => Some(RuntimeCapability::ProfileAccess),
        ("GET" | "PUT", "/research/plan") => Some(RuntimeCapability::InvestorPlan),
        ("GET", "/personal/holdings/status")
        | (
            "POST",
            "/personal/holdings/prepare"
            | "/personal/holdings/accounts"
            | "/personal/holdings/confirm"
            | "/personal/holdings/discard"
            | "/personal/holdings/latest",
        ) => Some(RuntimeCapability::ReviewedHoldings),
        (
            "POST",
            "/research/company"
            | "/research/india/search"
            | "/research/india/analyse"
            | "/research/india/chart"
            | "/research/india/ratios",
        ) => Some(RuntimeCapability::PublicResearch),
        ("GET" | "PUT" | "DELETE", "/research/india/connection") => {
            Some(RuntimeCapability::PublicResearchCredentials)
        }
        _ => None,
    }
}

pub fn request_allowed(purpose: RuntimePurpose, method: &Method, path: &str) -> bool {
    purpose != RuntimePurpose::Personal
        || request_capability(method, path).is_some_and(|capability| purpose.allows(capability))
}

/// URLs may contain account or asset identifiers. Personal request traces keep
/// the HTTP method/status but never record the original path or query.
pub fn trace_path(purpose: RuntimePurpose, path: &str) -> &str {
    if purpose == RuntimePurpose::Personal {
        "<private-api>"
    } else {
        path
    }
}

/// Add after all API and profile routes have been composed, before attaching
/// the static fallback. Disabled requests never reach a handler or body parser.
pub fn protect_api(router: Router, purpose: RuntimePurpose) -> Router {
    router.layer(middleware::from_fn_with_state(purpose, enforce))
}

async fn enforce(
    State(purpose): State<RuntimePurpose>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let mut response = if request_allowed(purpose, request.method(), request.uri().path()) {
        next.run(request).await
    } else {
        (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "code": "PERSONAL_CAPABILITY_DISABLED",
                "message": "This operation is unavailable in the private personal setup."
            })),
        )
            .into_response()
    };
    if let Some(csp) = personal_csp(purpose) {
        response.headers_mut().insert(
            header::CONTENT_SECURITY_POLICY,
            axum::http::HeaderValue::from_static(csp),
        );
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-store"),
        );
    }
    response
}
