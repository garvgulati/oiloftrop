#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "isolated synthetic policy tests"
)]

use axum::{
    body::{to_bytes, Body},
    http::{Method, Request, StatusCode},
    routing::any,
    Router,
};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tower::ServiceExt;
use wealthfolio_core::runtime_boundary::RuntimePurpose;
use wealthfolio_server::{
    auth::{AuthConfig, CookieSecurePolicy},
    config::Config,
    oidc::OidcConfig,
    runtime_policy::{
        personal_csp, protect_api, request_allowed, trace_path, validate_configuration,
    },
};

fn config() -> Config {
    Config {
        listen_addr: "127.0.0.1:0".parse().unwrap(),
        db_path: "unused.db".into(),
        cors_allow: vec!["https://private.example.test".into()],
        request_timeout: Duration::from_secs(30),
        static_dir: "unused".into(),
        addons_root: "unused".into(),
        raw_secret_key: vec![7; 32],
        secrets_encryption_key: [7; 32],
        database_key: [9; 32],
        db_encryption_required: true,
        auth: Some(AuthConfig {
            password_hash: Some("synthetic-unused-hash".into()),
            jwt_secret: vec![8; 32],
            access_token_ttl: Duration::from_secs(60),
            cookie_secure: CookieSecurePolicy::Always,
        }),
        oidc: None,
        mcp_enabled: false,
        mcp_audit_enabled: true,
        mcp_allowed_hosts: None,
    }
}

// This fixture never starts the portfolio host, touches a profile or creates an
// HTTP client. A counter represents the downstream operation that must not run.
fn spy_router(paths: &[&str], calls: Arc<AtomicUsize>) -> Router {
    let mut router = Router::new();
    for path in paths {
        let calls = calls.clone();
        router = router.route(
            path,
            any(move || {
                let calls = calls.clone();
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    StatusCode::OK
                }
            }),
        );
    }
    protect_api(router, RuntimePurpose::Personal)
}

#[tokio::test]
async fn disabled_and_unknown_routes_never_execute_or_parse_private_payloads() {
    let cases = [
        ("POST", "/api/v1/ai/chat/stream"),
        ("GET", "/api/v1/ai/threads"),
        ("POST", "/api/v1/connect/sync"),
        ("POST", "/api/v1/addons/synthetic/network/request"),
        ("GET", "/api/v1/secrets"),
        ("PUT", "/api/v1/settings"),
        ("PUT", "/api/v1/providers/settings"),
        ("POST", "/api/v1/market-data/sync"),
        ("GET", "/api/v1/market-data/search"),
        ("POST", "/api/v1/profiles/profile_auth_storage"),
        ("POST", "/api/v1/profiles/create_profile"),
        ("POST", "/api/v1/profiles/delete_profile"),
        ("POST", "/api/v1/profiles/get_profile_sync_identity"),
        ("POST", "/api/v1/profiles/update_profile_sync_identity"),
        ("GET", "/api/v1/research/context"),
        ("POST", "/api/v1/research/example"),
        ("POST", "/api/v1/snapshots"),
        ("POST", "/api/v1/snapshots/import"),
        ("GET", "/api/v1/holdings"),
        ("GET", "/api/v1/app/check-update"),
        ("POST", "/api/v1/utilities/database/backup"),
        ("POST", "/mcp"),
        ("POST", "/api/v1/new-upstream-endpoint"),
    ];
    let paths: Vec<_> = cases.iter().map(|(_, path)| *path).collect();
    let calls = Arc::new(AtomicUsize::new(0));
    let router = spy_router(&paths, calls.clone());
    for (method, path) in cases {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from("synthetic-private-invalid-json"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}");
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body = String::from_utf8(to_bytes(response.into_body(), 4096).await.unwrap().to_vec())
            .unwrap();
        assert!(!body.contains("synthetic-private"));
        assert!(!body.contains(path));
        assert!(body.contains("PERSONAL_CAPABILITY_DISABLED"));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn read_only_and_explicit_public_operations_pass_without_widening_methods() {
    let paths = [
        "/api/v1/accounts",
        "/api/v1/research/plan",
        "/api/v1/research/company",
        "/api/v1/research/india/chart",
        "/api/v1/research/india/connection",
    ];
    let calls = Arc::new(AtomicUsize::new(0));
    let router = spy_router(&paths, calls.clone());
    for (method, path) in [
        ("GET", paths[0]),
        ("PUT", paths[1]),
        ("POST", paths[2]),
        ("POST", paths[3]),
        ("PUT", paths[4]),
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(
            response.headers()["content-security-policy"],
            personal_csp(RuntimePurpose::Personal).unwrap()
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 5);
    for (method, path) in [
        ("POST", paths[0]),
        ("DELETE", paths[1]),
        ("GET", paths[2]),
        ("PUT", paths[3]),
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 5);
}

#[tokio::test]
async fn ordinary_and_fictional_hosts_keep_existing_handler_and_header_behavior() {
    for purpose in [RuntimePurpose::Standard, RuntimePurpose::Fictional] {
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let router = protect_api(
            Router::new().route(
                "/api/v1/upstream-operation",
                any(move || {
                    counted.fetch_add(1, Ordering::SeqCst);
                    async { StatusCode::OK }
                }),
            ),
            purpose,
        );
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/upstream-operation")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(response.headers().get("content-security-policy").is_none());
        assert!(response.headers().get("cache-control").is_none());
    }
}

#[test]
fn no_prefix_encoding_or_profile_command_bypass_and_other_modes_preserve_access() {
    for path in [
        "/api/v1/research/company/extra",
        "/api/v1/research/company/",
        "/api/v1/%72esearch/company",
        "/api/v10/research/company",
        "/api/v1/research/../connect/sync",
        "/api/v1/profiles/new_command",
    ] {
        assert!(!request_allowed(
            RuntimePurpose::Personal,
            &Method::POST,
            path
        ));
        assert!(request_allowed(
            RuntimePurpose::Standard,
            &Method::POST,
            path
        ));
        assert!(request_allowed(
            RuntimePurpose::Fictional,
            &Method::POST,
            path
        ));
    }
    assert!(request_allowed(
        RuntimePurpose::Personal,
        &Method::POST,
        "/api/v1/profiles/unlock_profile"
    ));
    assert!(!request_allowed(
        RuntimePurpose::Personal,
        &Method::GET,
        "/api/v1/auth/oidc/login"
    ));
}

#[test]
fn personal_configuration_rejects_network_auth_insecure_access_and_agent_endpoints() {
    let valid = config();
    validate_configuration(RuntimePurpose::Personal, &valid).unwrap();
    for mutate in [
        |c: &mut Config| c.mcp_enabled = true,
        |c: &mut Config| c.db_encryption_required = false,
        |c: &mut Config| c.listen_addr = "0.0.0.0:8088".parse().unwrap(),
        |c: &mut Config| c.auth = None,
        |c: &mut Config| c.auth.as_mut().unwrap().cookie_secure = CookieSecurePolicy::Auto,
        |c: &mut Config| c.auth.as_mut().unwrap().password_hash = Some(String::new()),
        |c: &mut Config| c.cors_allow = vec!["*".into()],
        |c: &mut Config| c.cors_allow = vec!["http://private.example.test".into()],
        |c: &mut Config| c.cors_allow = vec!["https://private.example.test/extra".into()],
        |c: &mut Config| c.cors_allow = vec!["https://private.example.test/".into()],
        |c: &mut Config| {
            c.oidc = Some(OidcConfig {
                issuer_url: "https://unused.example.test".into(),
                client_id: "unused".into(),
                client_secret: None,
                redirect_url: "https://private.example.test/callback".into(),
                scopes: vec![],
                allowed_emails: vec![],
                allowed_subs: vec![],
                post_logout_redirect_url: None,
                rp_logout: false,
            })
        },
    ] {
        let mut candidate = valid.clone();
        mutate(&mut candidate);
        assert!(validate_configuration(RuntimePurpose::Personal, &candidate).is_err());
        assert!(validate_configuration(RuntimePurpose::Standard, &candidate).is_ok());
    }
}

#[test]
fn browser_policy_and_tracing_do_not_admit_cloud_origins_or_private_paths() {
    let policy = personal_csp(RuntimePurpose::Personal).unwrap();
    let directive = |name: &str| {
        policy
            .split(';')
            .map(str::trim)
            .find(|part| part.starts_with(name))
            .unwrap()
    };
    assert_eq!(directive("connect-src"), "connect-src 'self'");
    assert!(!directive("img-src").contains("https:"));
    assert_eq!(directive("child-src"), "child-src 'none'");
    assert!(directive("frame-src")
        .ends_with("https://www.tradingview-widget.com/embed-widget/advanced-chart/"));
    assert!(personal_csp(RuntimePurpose::Standard).is_none());
    let private_path = "/api/v1/accounts/synthetic-private-account/holdings";
    assert_eq!(
        trace_path(RuntimePurpose::Personal, private_path),
        "<private-api>"
    );
    assert_eq!(
        trace_path(RuntimePurpose::Standard, private_path),
        private_path
    );
}

#[tokio::test]
async fn personal_cold_start_is_denied_before_files_or_auth_network() {
    const CHILD_FLAG: &str = "OILOFTROP_PERSONAL_POLICY_CHILD";
    if std::env::var_os(CHILD_FLAG).is_none() {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child.env_clear();
        for name in [
            "SystemRoot",
            "WINDIR",
            "PATH",
            "PATHEXT",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
            "TEMP",
            "TMP",
        ] {
            if let Some(value) = std::env::var_os(name) {
                child.env(name, value);
            }
        }
        child
            .env(CHILD_FLAG, "1")
            .env("WF_RUNTIME_PURPOSE", "personal")
            .env("WF_RUNTIME_BOUNDARY_VERSION", "1")
            .arg("--exact")
            .arg("personal_cold_start_is_denied_before_files_or_auth_network");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            child.creation_flags(0x08000000);
        }
        let result = child.output().unwrap();
        assert!(
            result.status.success(),
            "isolated child failed: {} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("must-not-be-created");
    let mut personal = config();
    personal.db_path = root.join("app.db").to_str().unwrap().into();
    personal.addons_root = root.to_str().unwrap().into();
    let trap = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    trap.set_nonblocking(true).unwrap();
    for oidc in [false, true] {
        if oidc {
            personal.oidc = Some(OidcConfig {
                issuer_url: format!("http://{}", trap.local_addr().unwrap()),
                client_id: "synthetic".into(),
                client_secret: None,
                redirect_url: "https://private.example.test/callback".into(),
                scopes: vec![],
                allowed_emails: vec![],
                allowed_subs: vec![],
                post_logout_redirect_url: None,
                rp_logout: false,
            });
        }
        let reason = if oidc {
            "external sign-in discovery"
        } else {
            "Personal portfolio services are not ready"
        };
        let direct = tokio::time::timeout(
            Duration::from_secs(2),
            wealthfolio_server::build_state(&personal),
        )
        .await
        .unwrap();
        assert!(direct.err().unwrap().to_string().contains(reason));
        let profiles = tokio::time::timeout(
            Duration::from_secs(2),
            wealthfolio_server::profiles::WebProfiles::open(&personal),
        )
        .await
        .unwrap();
        let web_reason = if oidc {
            reason
        } else {
            "Personal provisioning is required"
        };
        assert!(profiles.err().unwrap().to_string().contains(web_reason));
        let router = tokio::time::timeout(
            Duration::from_secs(2),
            wealthfolio_server::api::app_router_from_config(&personal),
        )
        .await
        .unwrap();
        assert!(router.err().unwrap().to_string().contains(web_reason));
        assert!(!root.exists());
        assert!(matches!(trap.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
    }
}
