#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "disposable encrypted personal-host verification"
)]
use reqwest::{Client, Method, StatusCode};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

const PASSWORD: &str = "Synthetic personal host password 42";
const ORIGIN: &str = "https://private.example.test";

fn command(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_wealthfolio-server"));
    cmd.env_clear();
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
            cmd.env(name, value);
        }
    }
    cmd.current_dir(root)
        .env("WF_RUNTIME_PURPOSE", "personal")
        .env("WF_RUNTIME_BOUNDARY_VERSION", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd
}

struct Host(Child);
impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn start(root: &Path, state: &Path, client: &Client) -> (Host, String) {
    let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reservation.local_addr().unwrap();
    let log = fs::File::create(root.join("synthetic-host.log")).unwrap();
    let mut cmd = command(root);
    cmd.env("WF_LISTEN_ADDR", address.to_string())
        .env("WF_DB_PATH", state.join("data/app.db"))
        .env("WF_DATA_DIR", state.join("data"))
        .env("WF_SECRET_KEY_FILE", state.join("master-key.txt"))
        .env("WF_SECRET_FILE", state.join("data/secrets.json"))
        .env("WF_DB_REQUIRE_ENCRYPTION", "1")
        .env(
            "WF_AUTH_PASSWORD_HASH",
            fs::read_to_string(state.join("password-hash.txt"))
                .unwrap()
                .trim(),
        )
        .env("WF_COOKIE_SECURE", "true")
        .env("WF_CORS_ALLOW_ORIGINS", ORIGIN)
        .env("WF_MCP_ENABLED", "false")
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log);
    drop(reservation);
    let mut host = Host(cmd.spawn().unwrap());
    let base = format!("http://{address}/api/v1");
    for _ in 0..200 {
        assert!(
            host.0.try_wait().unwrap().is_none(),
            "Disposable personal host exited during startup"
        );
        if client
            .get(format!("{base}/healthz"))
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
        {
            return (host, base);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("Disposable personal host did not become ready");
}

async fn call(
    client: &Client,
    base: &str,
    method: Method,
    path: &str,
    body: Value,
    cookie: Option<&str>,
    scope: Option<&str>,
) -> (StatusCode, Value) {
    let mut request = client
        .request(method, format!("{base}{path}"))
        .header("origin", ORIGIN);
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    if let Some(scope) = scope {
        request = request.header("x-wf-profile-scope", scope);
    }
    let response = request.json(&body).send().await.unwrap();
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
    let status = response.status();
    let value = response.json().await.unwrap_or(Value::Null);
    (status, value)
}

async fn sign_in(client: &Client, base: &str, profile: &str) -> (String, String) {
    let response = client
        .post(format!("{base}/auth/login"))
        .header("origin", ORIGIN)
        .json(&json!({"password":PASSWORD}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let header = response
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(header.contains("Secure") && header.contains("HttpOnly"));
    let cookie = header.split(';').next().unwrap().to_owned();
    let (status, session) = call(
        client,
        base,
        Method::POST,
        "/profiles/unlock_profile",
        json!({"profileId":profile,"proof":PASSWORD}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "Protected personal runtime did not open"
    );
    (cookie, session["scopeId"].as_str().unwrap().to_owned())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn provisioned_host_reviews_saves_reloads_and_rejects_unapproved_services() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let mut init = command(root.path())
        .args(["personal", "init"])
        .arg(&state)
        .arg("--password-stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    init.stdin
        .take()
        .unwrap()
        .write_all(PASSWORD.as_bytes())
        .unwrap();
    let output = init.wait_with_output().unwrap();
    assert!(output.status.success(), "Synthetic provisioning failed");
    let setup: Value = serde_json::from_slice(&output.stdout).unwrap();
    let profile = setup["profileId"].as_str().unwrap();
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap();
    let (host, base) = start(root.path(), &state, &client).await;
    let (status, _) = call(
        &client,
        &base,
        Method::GET,
        "/personal/holdings/status",
        Value::Null,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (cookie, scope) = sign_in(&client, &base, profile).await;
    let (status, metadata) = call(
        &client,
        &base,
        Method::GET,
        "/personal/holdings/status",
        Value::Null,
        Some(&cookie),
        Some(&scope),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(metadata, json!({"supported":true,"accounts":[]}));
    let account = uuid::Uuid::new_v4();
    let (status, _) = call(
        &client,
        &base,
        Method::POST,
        "/personal/holdings/accounts",
        json!({"id":account,"name":"Synthetic broker account","currency":"INR"}),
        Some(&cookie),
        Some(&scope),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let input = json!({"schemaVersion":1,"accountId":account,"currency":"INR",
        "source":{"label":"Synthetic broker","account":"Synthetic source"},"asOf":"2026-10-29",
        "settledScope":"partial","otherExposure":"unknown","cash":null,
        "positions":[{"isin":"INE213A01029","symbol":"ONGC","name":"Synthetic equity",
            "currency":"INR","sourceVenue":"XNSE","valuationVenue":"XNSE",
            "ordinaryEquityConfirmed":true,"quantity":"1.23456789","averageCost":null,"price":null}]});
    let (status, prepared) = call(
        &client,
        &base,
        Method::POST,
        "/personal/holdings/prepare",
        input,
        Some(&cookie),
        Some(&scope),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let confirm = json!({"reviewId":prepared["review"]["reviewId"],"inputHash":prepared["review"]["inputHash"]});
    let (status, receipt) = call(
        &client,
        &base,
        Method::POST,
        "/personal/holdings/confirm",
        confirm.clone(),
        Some(&cookie),
        Some(&scope),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, retry) = call(
        &client,
        &base,
        Method::POST,
        "/personal/holdings/confirm",
        confirm.clone(),
        Some(&cookie),
        Some(&scope),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(receipt, retry);
    for (method, path, body) in [
        (
            Method::POST,
            "/profiles/create_profile",
            json!({"name":"Other"}),
        ),
        (
            Method::POST,
            "/profiles/delete_profile",
            json!({"id":profile}),
        ),
        (
            Method::POST,
            "/profiles/set_profile_password",
            json!({"proof":PASSWORD,"password":null}),
        ),
        (Method::GET, "/holdings", Value::Null),
        (Method::GET, "/research/context", Value::Null),
        (Method::POST, "/ai/chat/stream", json!({})),
        (Method::POST, "/market-data/sync", json!({})),
        (Method::GET, "/secrets", Value::Null),
    ] {
        let (status, _) = call(
            &client,
            &base,
            method,
            path,
            body,
            Some(&cookie),
            Some(&scope),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    let (status, _) = call(
        &client,
        &base,
        Method::POST,
        "/profiles/lock_profile",
        json!({}),
        Some(&cookie),
        Some(&scope),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(
        &client,
        &base,
        Method::POST,
        "/personal/holdings/confirm",
        confirm,
        Some(&cookie),
        Some(&scope),
    )
    .await;
    assert!(!status.is_success());
    drop(host);
    let (host, base) = start(root.path(), &state, &client).await;
    let (cookie, scope) = sign_in(&client, &base, profile).await;
    let (status, saved) = call(
        &client,
        &base,
        Method::POST,
        "/personal/holdings/latest",
        json!({"accountId":account}),
        Some(&cookie),
        Some(&scope),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["receipt"], receipt);
    assert_eq!(saved["snapshot"]["positions"][0]["quantity"], "1.23456789");
    assert!(saved["snapshot"]["positions"][0]["averageCost"].is_null());
    assert!(saved["snapshot"]["positions"][0]["price"].is_null());
    assert!(saved["snapshot"]["cash"].is_null());
    assert_eq!(saved["historyAvailable"], false);
    drop(host);
    let database = state.join("data/profiles").join(profile).join("app.db");
    assert!(!fs::read(database)
        .unwrap()
        .starts_with(b"SQLite format 3\0"));
}
