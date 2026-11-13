#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "synthetic import endpoint fixtures"
)]
use super::*;
use axum::{body::to_bytes, http::StatusCode};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tower::ServiceExt;
use uuid::Uuid;

struct Grant(AtomicBool);
impl ImportGrantAuthority for Grant {
    fn require_current(&self, _: Uuid, _: Uuid, _: &str) -> wealthfolio_core::Result<()> {
        if self.0.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(import_error("PERSONAL_IMPORT_GRANT_INVALID"))
        }
    }
}
struct Repo {
    profile: Uuid,
    saved: Mutex<Option<SavedReviewedSnapshot>>,
}
#[async_trait::async_trait]
impl ReviewedImportRepository for Repo {
    async fn create_account(
        &self,
        request: CommitReviewedAccount,
    ) -> wealthfolio_core::Result<ReviewedImportAccount> {
        request.authorization.require_profile(self.profile)?;
        request
            .authorization
            .require_unexpired(chrono::Utc::now())?;
        let input = request.input.validate()?;
        Ok(ReviewedImportAccount {
            id: input.id,
            name: input.name,
            currency: input.currency,
        })
    }
    fn profile_id(&self) -> Uuid {
        self.profile
    }
    fn target(&self, input: &ValidatedSnapshot) -> wealthfolio_core::Result<ImportTarget> {
        let saved = self.saved.lock().unwrap();
        Ok(ImportTarget {
            fingerprint: "synthetic-target".into(),
            account_name: "Synthetic account".into(),
            currency: input.input().currency.clone(),
            previous: saved.as_ref().map(|s| s.snapshot.clone()),
            existing: saved.as_ref().map(|s| s.receipt.clone()),
        })
    }
    fn receipt(&self, id: Uuid) -> wealthfolio_core::Result<Option<ImportReceipt>> {
        Ok(self
            .saved
            .lock()
            .unwrap()
            .as_ref()
            .filter(|s| s.receipt.review_id == id)
            .map(|s| s.receipt.clone()))
    }
    fn latest(&self, account: Uuid) -> wealthfolio_core::Result<Option<SavedReviewedSnapshot>> {
        Ok(self
            .saved
            .lock()
            .unwrap()
            .as_ref()
            .filter(|s| s.snapshot.account_id == account)
            .cloned())
    }
    async fn commit(
        &self,
        request: CommitReviewedImport,
    ) -> wealthfolio_core::Result<ImportReceipt> {
        request.authorization.require_profile(self.profile)?;
        let now = chrono::Utc::now();
        request.authorization.require_unexpired(now)?;
        let snapshot = request.input.input().clone();
        let receipt = ImportReceipt {
            schema_version: 1,
            review_id: request.review_id,
            input_hash: request.input.input_hash()?,
            account_id: snapshot.account_id,
            snapshot_id: format!("{}_{}", snapshot.account_id, snapshot.as_of),
            as_of: snapshot.as_of,
            imported_at: now,
        };
        *self.saved.lock().unwrap() = Some(SavedReviewedSnapshot {
            receipt: receipt.clone(),
            snapshot,
            asset_ids: BTreeMap::new(),
            history_available: false,
        });
        Ok(receipt)
    }
}
fn fixture() -> (Router, Arc<Repo>, Arc<Grant>, Uuid) {
    let repo = Arc::new(Repo {
        profile: Uuid::new_v4(),
        saved: Mutex::new(None),
    });
    let grant = Arc::new(Grant(AtomicBool::new(true)));
    let binding = ImportReviewBinding::new(
        repo.profile,
        Uuid::new_v4(),
        "synthetic-owner",
        grant.clone(),
    )
    .unwrap();
    let router = operations()
        .layer(Extension(ReviewedEndpointContext {
            service: Arc::new(ReviewedImportService::new(repo.clone())),
            binding: Arc::new(binding),
        }))
        .layer(middleware::from_fn(no_store));
    (router, repo, grant, Uuid::new_v4())
}
fn input(account: Uuid) -> serde_json::Value {
    serde_json::json!({"schemaVersion":1,"accountId":account,"currency":"INR",
        "source":{"label":"Synthetic broker","account":"Synthetic source"},
        "asOf":snapshot_day(chrono::Utc::now()),"settledScope":"partial","otherExposure":"unknown","cash":null,
        "positions":[{"isin":"INE213A01029","symbol":"ONGC","name":"Synthetic equity","currency":"INR",
            "sourceVenue":"XNSE","valuationVenue":"XNSE","ordinaryEquityConfirmed":true,"quantity":"1.23456789","averageCost":null,"price":null}]})
}
async fn call(
    router: &Router,
    path: &str,
    value: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(value.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), MAX_REVIEW_BYTES + 4096)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

#[tokio::test]
async fn reviewed_http_flow_confirms_and_reloads_exact_unknowns() {
    let (router, repo, _, account) = fixture();
    let (status, prepared) = call(&router, "/personal/holdings/prepare", input(account)).await;
    assert_eq!(status, StatusCode::OK);
    let review = &prepared["review"];
    assert_eq!(review["snapshot"]["positions"][0]["quantity"], "1.23456789");
    assert!(review["snapshot"]["positions"][0]["averageCost"].is_null());
    assert!(repo.saved.lock().unwrap().is_none());
    let request =
        serde_json::json!({"reviewId":review["reviewId"],"inputHash":review["inputHash"]});
    let (status, receipt) = call(&router, "/personal/holdings/confirm", request.clone()).await;
    assert_eq!(status, StatusCode::OK);
    let (status, again) = call(&router, "/personal/holdings/confirm", request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(receipt, again);
    let (status, saved) = call(
        &router,
        "/personal/holdings/latest",
        serde_json::json!({"accountId":account}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["historyAvailable"], false);
    assert!(saved["snapshot"]["cash"].is_null());
    assert_eq!(saved["receipt"], receipt);
}

#[tokio::test]
async fn unknown_authority_fields_and_body_limits_cannot_reach_save() {
    let (router, repo, _, account) = fixture();
    for field in ["profileId", "grantId", "owner", "authorization"] {
        let mut value = input(account);
        value[field] = serde_json::json!("synthetic-forged");
        assert_eq!(
            call(&router, "/personal/holdings/prepare", value).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let value = serde_json::json!({"reviewId":Uuid::new_v4(),"inputHash":"x",(field):"synthetic-forged"});
        assert_eq!(
            call(&router, "/personal/holdings/confirm", value).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    let mut value = input(account);
    value["source"]["label"] = serde_json::json!("x".repeat(MAX_REVIEW_BYTES));
    assert_eq!(
        call(&router, "/personal/holdings/prepare", value).await.0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        call(
            &router,
            "/personal/holdings/confirm",
            serde_json::json!({"reviewId":Uuid::new_v4(),"inputHash":"x".repeat(5000)})
        )
        .await
        .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert!(repo.saved.lock().unwrap().is_none());
}

#[tokio::test]
async fn revoked_and_discarded_reviews_cannot_confirm() {
    let (router, repo, grant, account) = fixture();
    let (_, value) = call(&router, "/personal/holdings/prepare", input(account)).await;
    let review = &value["review"];
    let confirm =
        serde_json::json!({"reviewId":review["reviewId"],"inputHash":review["inputHash"]});
    grant.0.store(false, Ordering::SeqCst);
    assert_eq!(
        call(&router, "/personal/holdings/confirm", confirm.clone())
            .await
            .0,
        StatusCode::CONFLICT
    );
    grant.0.store(true, Ordering::SeqCst);
    assert_eq!(
        call(
            &router,
            "/personal/holdings/discard",
            serde_json::json!({"reviewId":review["reviewId"]})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&router, "/personal/holdings/confirm", confirm).await.0,
        StatusCode::CONFLICT
    );
    assert!(repo.saved.lock().unwrap().is_none());
}

#[tokio::test]
async fn account_intake_rejects_connection_fields_and_revoked_authority() {
    let (router, _, grant, _) = fixture();
    let value =
        serde_json::json!({"id":Uuid::new_v4(),"name":"Synthetic account","currency":"USD"});
    let (status, account) = call(&router, "/personal/holdings/accounts", value.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(account["currency"], "USD");
    let mut bad = value.clone();
    bad["provider"] = serde_json::json!("synthetic");
    assert_eq!(
        call(&router, "/personal/holdings/accounts", bad).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    grant.0.store(false, Ordering::SeqCst);
    assert_eq!(
        call(&router, "/personal/holdings/accounts", value).await.0,
        StatusCode::CONFLICT
    );
}

#[derive(Default)]
struct Secrets;
impl wealthfolio_core::secrets::SecretStore for Secrets {
    fn get_secret(&self, _: &str) -> wealthfolio_core::Result<Option<String>> {
        Ok(None)
    }
    fn set_secret(&self, _: &str, _: &str) -> wealthfolio_core::Result<()> {
        Ok(())
    }
    fn delete_secret(&self, _: &str) -> wealthfolio_core::Result<()> {
        Ok(())
    }
}
#[test]
fn scope_factory_requires_personal_mode_matching_registry_profile_and_live_session() {
    let root = tempfile::tempdir().unwrap();
    let registry = Arc::new(
        ProfileRegistry::open(
            root.path().join("profiles"),
            root.path().join("unused.db"),
            Arc::new(Secrets),
        )
        .unwrap(),
    );
    let profile = registry.create("Synthetic profile", "default").unwrap().id;
    let access = ProfileAccess {
        owner: "synthetic-owner".into(),
        session: registry
            .sessions
            .issue("synthetic-owner", profile, false, None)
            .unwrap(),
    };
    let bound = (registry.clone(), profile);
    assert!(bind_scope(RuntimePurpose::Personal, Some(&bound), &registry, &access).is_ok());
    for purpose in [RuntimePurpose::Standard, RuntimePurpose::Fictional] {
        assert!(bind_scope(purpose, Some(&bound), &registry, &access).is_err());
    }
    assert!(bind_scope(RuntimePurpose::Personal, None, &registry, &access).is_err());
    assert!(bind_scope(
        RuntimePurpose::Personal,
        Some(&(registry.clone(), Uuid::new_v4())),
        &registry,
        &access
    )
    .is_err());
    let other_root = tempfile::tempdir().unwrap();
    let other = Arc::new(
        ProfileRegistry::open(
            other_root.path().join("profiles"),
            other_root.path().join("unused.db"),
            Arc::new(Secrets),
        )
        .unwrap(),
    );
    assert!(bind_scope(RuntimePurpose::Personal, Some(&bound), &other, &access).is_err());
    registry.sessions.revoke(&access.owner).unwrap();
    assert!(bind_scope(RuntimePurpose::Personal, Some(&bound), &registry, &access).is_err());
}

#[test]
fn storage_errors_do_not_expose_private_values() {
    let error = safe_error(CoreError::ConstraintViolation(
        "synthetic secret SQL value".into(),
    ));
    assert_eq!(error.to_string(), "PERSONAL_IMPORT_UNAVAILABLE");
    assert!(safe_error(import_error("PERSONAL_IMPORT_TARGET_CHANGED"))
        .to_string()
        .contains("PERSONAL_IMPORT_TARGET_CHANGED"));
}
