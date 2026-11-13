#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "synthetic reviewed snapshot tests"
)]
use super::*;

struct CurrentGrant;
impl ImportGrantAuthority for CurrentGrant {
    fn require_current(&self, _: Uuid, _: Uuid, _: &str) -> Result<()> {
        Ok(())
    }
}

#[test]
fn dedicated_account_inputs_require_supported_currency_uuid_and_name() {
    let valid = NewReviewedImportAccount {
        id: Uuid::new_v4(),
        name: " Synthetic A ".into(),
        currency: "INR".into(),
    };
    assert_eq!(valid.clone().validate().unwrap().name, "Synthetic A");
    for currency in ["", "EUR", "inr"] {
        let mut input = valid.clone();
        input.currency = currency.into();
        assert!(input.validate().is_err());
    }
    let mut input = valid.clone();
    input.id = Uuid::nil();
    assert!(input.validate().is_err());
    let mut input = valid;
    input.name = "\n".into();
    assert!(input.validate().is_err());
}

#[derive(Default)]
struct SyntheticSecrets(Mutex<BTreeMap<String, String>>);
impl crate::secrets::SecretStore for SyntheticSecrets {
    fn list_secrets(&self) -> Result<Vec<String>> {
        Ok(self.0.lock().unwrap().keys().cloned().collect())
    }
    fn get_secret(&self, key: &str) -> Result<Option<String>> {
        Ok(self.0.lock().unwrap().get(key).cloned())
    }
    fn set_secret(&self, key: &str, value: &str) -> Result<()> {
        self.0.lock().unwrap().insert(key.into(), value.into());
        Ok(())
    }
    fn delete_secret(&self, key: &str) -> Result<()> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

#[test]
fn registry_authority_rejects_foreign_rotated_revoked_and_deleting_profiles() {
    let root = tempfile::tempdir().unwrap();
    let registry = Arc::new(
        crate::profiles::ProfileRegistry::open(
            root.path().join("profiles"),
            root.path().join("unused.db"),
            Arc::new(SyntheticSecrets::default()),
        )
        .unwrap(),
    );
    let profile = registry.create("Synthetic profile", "default").unwrap().id;
    let session = registry
        .sessions
        .issue("synthetic-owner", profile, true, None)
        .unwrap();
    let authority = RegistryImportGrantAuthority(registry.clone());
    assert!(authority
        .require_current(profile, session.scope_id, "synthetic-owner")
        .is_ok());
    assert!(authority
        .require_current(Uuid::new_v4(), session.scope_id, "synthetic-owner")
        .is_err());
    assert!(authority
        .require_current(profile, session.scope_id, "another-owner")
        .is_err());
    let rotated = registry
        .sessions
        .rotate("synthetic-owner", session.scope_id, None)
        .unwrap();
    assert!(authority
        .require_current(profile, session.scope_id, "synthetic-owner")
        .is_err());
    assert!(authority
        .require_current(profile, rotated.scope_id, "synthetic-owner")
        .is_ok());
    registry.sessions.revoke("synthetic-owner").unwrap();
    assert!(authority
        .require_current(profile, rotated.scope_id, "synthetic-owner")
        .is_err());
    registry
        .begin_delete(profile, "Synthetic profile", None)
        .unwrap();
    // Even an accidentally reissued session cannot admit a deleting profile.
    let reissued = registry
        .sessions
        .issue("synthetic-owner", profile, true, None)
        .unwrap();
    assert!(authority
        .require_current(profile, reissued.scope_id, "synthetic-owner")
        .is_err());
}

#[test]
fn write_deadline_is_checked_again_at_commit_time() {
    let now = Utc::now();
    let authorization = ImportWriteAuthorization {
        authority: Arc::new(CurrentGrant),
        profile: Uuid::new_v4(),
        grant: Uuid::new_v4(),
        owner: "synthetic-owner".into(),
        expires_at: now,
    };
    assert!(authorization.require_profile(authorization.profile).is_ok());
    assert!(authorization.require_profile(Uuid::new_v4()).is_err());
    assert!(authorization
        .require_unexpired(now - chrono::Duration::seconds(1))
        .is_ok());
    assert!(authorization
        .require_unexpired(now)
        .unwrap_err()
        .to_string()
        .contains("REVIEW_EXPIRED"));
    assert!(authorization
        .require_unexpired(now + chrono::Duration::seconds(1))
        .is_err());
}

fn input(account: Uuid, day: NaiveDate) -> ReviewedSnapshotInput {
    ReviewedSnapshotInput {
        schema_version: 1,
        account_id: account,
        currency: "INR".into(),
        source: SnapshotSourceReference {
            label: "Synthetic broker".into(),
            account: "Synthetic A".into(),
        },
        as_of: day,
        settled_scope: SettledScope::Complete,
        other_exposure: OtherExposure::Unknown,
        cash: None,
        positions: vec![ReviewedEquity {
            isin: "INE213A01029".into(),
            symbol: "ONGC".into(),
            name: "Synthetic ordinary-equity holding".into(),
            currency: "INR".into(),
            source_venue: "XNSE".into(),
            valuation_venue: "XNSE".into(),
            ordinary_equity_confirmed: true,
            quantity: "1.23456789".into(),
            average_cost: None,
            price: None,
        }],
    }
}

#[test]
fn unknown_cash_and_cost_remain_different_from_known_zero() {
    let day = Utc::now().date_naive();
    let raw = input(Uuid::new_v4(), day);
    let unknown = validate_snapshot(raw.clone(), day).unwrap();
    assert!(unknown.input().cash.is_none());
    assert!(unknown.input().positions[0].average_cost.is_none());
    let mut zero = raw;
    zero.cash = Some(BTreeMap::from([("INR".into(), "0.00".into())]));
    zero.positions[0].average_cost = Some("0.000".into());
    let zero = validate_snapshot(zero, day).unwrap();
    assert_eq!(zero.input().cash.as_ref().unwrap()["INR"], "0");
    assert_eq!(zero.input().positions[0].average_cost.as_deref(), Some("0"));
    assert_ne!(unknown.input_hash().unwrap(), zero.input_hash().unwrap());
    assert!(zero.input().positions[0].price.is_none());
}

#[test]
fn precise_validation_never_rounds_or_conflates_venues() {
    let day = Utc::now().date_naive();
    let raw = input(Uuid::new_v4(), day);
    let mut duplicate = raw.clone();
    let mut alias = duplicate.positions[0].clone();
    alias.source_venue = "XBOM".into();
    duplicate.positions.push(alias);
    assert!(validate_snapshot(duplicate, day)
        .unwrap_err()
        .to_string()
        .contains("DUPLICATE_ISIN"));
    for bad in [
        "0",
        "-1",
        "NaN",
        "1.000000001",
        "1.00000000000000000000000000001",
    ] {
        let mut draft = raw.clone();
        draft.positions[0].quantity = bad.into();
        assert!(validate_snapshot(draft, day).is_err());
    }
    assert!(exact_product(
        import_decimal("999999999999.12345678", true).unwrap(),
        import_decimal("999999999999.12345678", true).unwrap()
    )
    .is_err());
    assert_eq!(
        exact_product(
            import_decimal("0.12345678", true).unwrap(),
            import_decimal("123.4567", true).unwrap()
        )
        .unwrap()
        .to_string(),
        "15.241566651426"
    );
}

#[test]
fn dated_price_and_classification_are_explicit_inputs() {
    let day = Utc::now().date_naive();
    let raw = input(Uuid::new_v4(), day);
    let mut unsupported = raw.clone();
    unsupported.positions[0].ordinary_equity_confirmed = false;
    assert!(validate_snapshot(unsupported, day).is_err());
    let mut draft = raw;
    draft.positions[0].average_cost = Some("123.45".into());
    assert!(validate_snapshot(draft.clone(), day)
        .unwrap()
        .input()
        .positions[0]
        .price
        .is_none());
    draft.positions[0].price = Some(ReviewedPrice {
        amount: "150".into(),
        currency: "INR".into(),
        observed_on: day.succ_opt().unwrap(),
        source: "Synthetic dated observation".into(),
        venue: "XNSE".into(),
    });
    assert!(validate_snapshot(draft.clone(), day).is_err());
    draft.positions[0].price.as_mut().unwrap().observed_on = day;
    assert!(validate_snapshot(draft, day).is_ok());
}

#[test]
fn omission_from_partial_or_unresolved_scope_is_not_a_sale() {
    let day = Utc::now().date_naive();
    let before = input(Uuid::new_v4(), day);
    let mut after = before.clone();
    after.positions.clear();
    assert_eq!(
        changes(Some(&before), &after)[0].kind,
        ImportChangeKind::NotCoveredByPartialSnapshot
    );
    after.other_exposure = OtherExposure::NoneConfirmed;
    assert_eq!(
        changes(Some(&before), &after)[0].kind,
        ImportChangeKind::RemovedFromCompleteSnapshot
    );
    after.settled_scope = SettledScope::Partial;
    assert_eq!(
        changes(Some(&before), &after)[0].kind,
        ImportChangeKind::NotCoveredByPartialSnapshot
    );
}

struct MockRepository {
    profile: Uuid,
    fingerprint: Mutex<String>,
    receipt: Mutex<Option<ImportReceipt>>,
}
#[async_trait]
impl ReviewedImportRepository for MockRepository {
    async fn create_account(&self, _: CommitReviewedAccount) -> Result<ReviewedImportAccount> {
        Err(import_error("PERSONAL_IMPORT_UNSUPPORTED"))
    }
    fn profile_id(&self) -> Uuid {
        self.profile
    }
    fn target(&self, input: &ValidatedSnapshot) -> Result<ImportTarget> {
        Ok(ImportTarget {
            fingerprint: self.fingerprint.lock().unwrap().clone(),
            account_name: "Synthetic account".into(),
            currency: input.input().currency.clone(),
            previous: None,
            existing: self.receipt.lock().unwrap().clone(),
        })
    }
    fn receipt(&self, id: Uuid) -> Result<Option<ImportReceipt>> {
        Ok(self
            .receipt
            .lock()
            .unwrap()
            .as_ref()
            .filter(|r| r.review_id == id)
            .cloned())
    }
    fn latest(&self, _: Uuid) -> Result<Option<SavedReviewedSnapshot>> {
        Ok(None)
    }
    async fn commit(&self, request: CommitReviewedImport) -> Result<ImportReceipt> {
        let imported_at = Utc::now();
        request.authorization.require_unexpired(imported_at)?;
        if *self.fingerprint.lock().unwrap() != request.expected_fingerprint {
            return Err(import_error("PERSONAL_IMPORT_TARGET_CHANGED"));
        }
        let input = request.input.input();
        let receipt = ImportReceipt {
            schema_version: 1,
            review_id: request.review_id,
            input_hash: request.input.input_hash()?,
            account_id: input.account_id,
            snapshot_id: format!("{}_{}", input.account_id, input.as_of),
            as_of: input.as_of,
            imported_at,
        };
        *self.receipt.lock().unwrap() = Some(receipt.clone());
        Ok(receipt)
    }
}
fn repository() -> Arc<MockRepository> {
    Arc::new(MockRepository {
        profile: Uuid::new_v4(),
        fingerprint: Mutex::new("target-1".into()),
        receipt: Mutex::new(None),
    })
}
fn binding(repo: &MockRepository) -> ImportReviewBinding {
    ImportReviewBinding::new(
        repo.profile,
        Uuid::new_v4(),
        "synthetic-browser",
        Arc::new(CurrentGrant),
    )
    .unwrap()
}
fn review(value: PreparedReviewedImport) -> ImportReview {
    match value {
        PreparedReviewedImport::Review { review } => review,
        _ => panic!("expected new review"),
    }
}

#[tokio::test]
async fn pending_reviews_bind_grants_and_expiry_and_detect_target_edits() {
    let repo = repository();
    let service = ReviewedImportService::new(repo.clone());
    let grant = binding(&repo);
    let now = Utc::now();
    let day = now.with_timezone(&chrono_tz::Asia::Kolkata).date_naive();
    let first = review(
        service
            .prepare_at(&grant, input(Uuid::new_v4(), day), now)
            .await
            .unwrap(),
    );
    assert!(service
        .confirm_at(
            &binding(&repo),
            first.review_id,
            first.input_hash.clone(),
            now
        )
        .await
        .is_err());
    assert!(service
        .confirm_at(
            &grant,
            first.review_id,
            first.input_hash.clone(),
            now + chrono::Duration::minutes(31)
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("EXPIRED"));
    *repo.fingerprint.lock().unwrap() = "target-2".into();
    assert!(service
        .confirm_at(&grant, first.review_id, first.input_hash, now)
        .await
        .unwrap_err()
        .to_string()
        .contains("TARGET_CHANGED"));
    assert!(repo.receipt.lock().unwrap().is_none());
}

#[tokio::test]
async fn lost_response_retry_survives_service_restart_without_a_second_review() {
    let repo = repository();
    let service = ReviewedImportService::new(repo.clone());
    let grant = binding(&repo);
    let day = Utc::now()
        .with_timezone(&chrono_tz::Asia::Kolkata)
        .date_naive();
    let raw = input(Uuid::new_v4(), day);
    let pending = review(service.prepare(&grant, raw.clone()).await.unwrap());
    let saved = service
        .confirm(&grant, pending.review_id, pending.input_hash.clone())
        .await
        .unwrap();
    let restarted = ReviewedImportService::new(repo.clone());
    assert_eq!(
        restarted
            .confirm(
                &binding(&repo),
                pending.review_id,
                pending.input_hash.clone()
            )
            .await
            .unwrap(),
        saved
    );
    assert!(restarted
        .confirm(&grant, pending.review_id, "changed".into())
        .await
        .is_err());
    assert!(matches!(
        restarted.prepare(&grant, raw).await.unwrap(),
        PreparedReviewedImport::AlreadyImported { .. }
    ));
}

#[tokio::test]
async fn pending_limit_is_bounded_and_only_its_grant_can_discard() {
    let repo = repository();
    let service = ReviewedImportService::new(repo.clone());
    let grant = binding(&repo);
    let day = Utc::now()
        .with_timezone(&chrono_tz::Asia::Kolkata)
        .date_naive();
    let mut ids = Vec::new();
    for _ in 0..MAX_PENDING {
        ids.push(
            review(
                service
                    .prepare(&grant, input(Uuid::new_v4(), day))
                    .await
                    .unwrap(),
            )
            .review_id,
        );
    }
    assert!(service
        .prepare(&grant, input(Uuid::new_v4(), day))
        .await
        .is_err());
    assert!(service.discard(&binding(&repo), ids[0]).is_err());
    service.discard(&grant, ids[0]).unwrap();
    assert!(service
        .prepare(&grant, input(Uuid::new_v4(), day))
        .await
        .is_ok());
}
