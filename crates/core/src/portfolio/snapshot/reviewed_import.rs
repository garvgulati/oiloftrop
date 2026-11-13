//! Reviewed, local-only settled-equity snapshots. None means unknown; it must
//! never be turned into a financial zero or a market quote by a reader.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{Error, Result};

pub const MAX_REVIEW_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_REVIEW_POSITIONS: usize = 200;
pub const REVIEWED_ACCOUNT_META_KEY: &str = "personalReviewedHoldings";
const MAX_PENDING: usize = 5;

pub fn import_error(code: &str) -> Error {
    Error::ConstraintViolation(code.into())
}
pub fn snapshot_day(now: DateTime<Utc>) -> NaiveDate {
    now.with_timezone(&chrono_tz::Asia::Kolkata).date_naive()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SettledScope {
    Complete,
    Partial,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OtherExposure {
    NoneConfirmed,
    Unknown,
    Unresolved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotSourceReference {
    pub label: String,
    pub account: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedPrice {
    pub amount: String,
    pub currency: String,
    pub observed_on: NaiveDate,
    pub source: String,
    pub venue: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedEquity {
    pub isin: String,
    pub symbol: String,
    pub name: String,
    pub currency: String,
    pub source_venue: String,
    pub valuation_venue: String,
    pub ordinary_equity_confirmed: bool,
    pub quantity: String,
    pub average_cost: Option<String>,
    pub price: Option<ReviewedPrice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedSnapshotInput {
    pub schema_version: u32,
    pub account_id: Uuid,
    pub currency: String,
    pub source: SnapshotSourceReference,
    pub as_of: NaiveDate,
    pub settled_scope: SettledScope,
    pub other_exposure: OtherExposure,
    /// None is unknown. Known cash, including zero, requires an explicit entry.
    pub cash: Option<BTreeMap<String, String>>,
    pub positions: Vec<ReviewedEquity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfirmReviewedImportRequest {
    pub review_id: Uuid,
    pub input_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscardReviewedImportRequest {
    pub review_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LatestReviewedImportRequest {
    pub account_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewedImportAccount {
    pub id: Uuid,
    pub name: String,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewReviewedImportAccount {
    pub id: Uuid,
    pub name: String,
    pub currency: String,
}
impl NewReviewedImportAccount {
    pub fn validate(mut self) -> Result<Self> {
        if self.id.is_nil() || !matches!(self.currency.as_str(), "INR" | "USD") {
            return Err(import_error("PERSONAL_IMPORT_ACCOUNT_INVALID"));
        }
        self.name = label(&self.name)?;
        Ok(self)
    }
}

pub struct CommitReviewedAccount {
    pub input: NewReviewedImportAccount,
    pub authorization: ImportWriteAuthorization,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewedImportStatus {
    pub supported: bool,
    pub accounts: Vec<ReviewedImportAccount>,
}

impl ReviewedImportStatus {
    pub fn unsupported() -> Self {
        Self {
            supported: false,
            accounts: Vec::new(),
        }
    }
}

/// Only validation can construct this token; repositories never accept raw DTOs.
#[derive(Debug, Clone)]
pub struct ValidatedSnapshot(ReviewedSnapshotInput);
impl ValidatedSnapshot {
    pub fn input(&self) -> &ReviewedSnapshotInput {
        &self.0
    }
    pub fn input_hash(&self) -> Result<String> {
        canonical_hash(&self.0)
    }
}

pub fn canonical_hash(value: &impl Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(|_| import_error("PERSONAL_IMPORT_ENCODING"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn label(raw: &str) -> Result<String> {
    let value = raw.trim();
    if value.is_empty() || value.len() > 160 || value.chars().any(char::is_control) {
        return Err(import_error("PERSONAL_IMPORT_LABEL_INVALID"));
    }
    Ok(value.into())
}

pub fn import_decimal(raw: &str, positive: bool) -> Result<Decimal> {
    let text = raw.trim();
    if text.len() > 40 || text.is_empty() || !text.bytes().all(|c| c.is_ascii_digit() || c == b'.')
    {
        return Err(import_error("PERSONAL_IMPORT_DECIMAL_INVALID"));
    }
    let value = Decimal::from_str_exact(text)
        .map_err(|_| import_error("PERSONAL_IMPORT_DECIMAL_INVALID"))?
        .normalize();
    if value.scale() > 8
        || value < Decimal::ZERO
        || (positive && value.is_zero())
        || value > Decimal::from(1_000_000_000_000u64)
    {
        return Err(import_error("PERSONAL_IMPORT_DECIMAL_UNSUPPORTED"));
    }
    Ok(value)
}

/// checked_mul can reduce scale to fit Decimal's coefficient. Reject arithmetic
/// that cannot be represented exactly instead of silently rounding cost basis.
pub fn exact_product(left: Decimal, right: Decimal) -> Result<Decimal> {
    let mut coefficient = left
        .mantissa()
        .checked_mul(right.mantissa())
        .ok_or_else(|| import_error("PERSONAL_IMPORT_ARITHMETIC_UNSUPPORTED"))?;
    let mut scale = left.scale() + right.scale();
    while scale > 0 && coefficient % 10 == 0 {
        coefficient /= 10;
        scale -= 1;
    }
    Decimal::try_from_i128_with_scale(coefficient, scale)
        .map_err(|_| import_error("PERSONAL_IMPORT_ARITHMETIC_UNSUPPORTED"))
}

pub fn exact_sum(values: &[Decimal]) -> Result<Decimal> {
    let mut total = Decimal::ZERO;
    for value in values {
        let scale = total.scale().max(value.scale());
        let align = |number: Decimal| {
            number
                .mantissa()
                .checked_mul(10_i128.checked_pow(scale - number.scale())?)
        };
        let mut coefficient = align(total)
            .and_then(|left| align(*value).and_then(|right| left.checked_add(right)))
            .ok_or_else(|| import_error("PERSONAL_IMPORT_ARITHMETIC_UNSUPPORTED"))?;
        let mut normalized_scale = scale;
        while normalized_scale > 0 && coefficient % 10 == 0 {
            coefficient /= 10;
            normalized_scale -= 1;
        }
        total = Decimal::try_from_i128_with_scale(coefficient, normalized_scale)
            .map_err(|_| import_error("PERSONAL_IMPORT_ARITHMETIC_UNSUPPORTED"))?;
    }
    Ok(total)
}

pub fn validate_snapshot(
    mut input: ReviewedSnapshotInput,
    today: NaiveDate,
) -> Result<ValidatedSnapshot> {
    if serde_json::to_vec(&input)
        .map_err(|_| import_error("PERSONAL_IMPORT_ENCODING"))?
        .len()
        > MAX_REVIEW_BYTES
        || input.schema_version != 1
        || input.positions.len() > MAX_REVIEW_POSITIONS
        || input.account_id.is_nil()
        || input.as_of > today
        || input.as_of < NaiveDate::from_ymd_opt(1900, 1, 1).expect("valid supported minimum date")
        || !matches!(input.currency.as_str(), "INR" | "USD")
    {
        return Err(import_error("PERSONAL_IMPORT_SCOPE_UNSUPPORTED"));
    }
    input.source.label = label(&input.source.label)?;
    input.source.account = label(&input.source.account)?;
    if let Some(cash) = &mut input.cash {
        if cash.len() != 1 || !cash.contains_key(&input.currency) {
            return Err(import_error("PERSONAL_IMPORT_CASH_SCOPE_UNSUPPORTED"));
        }
        for amount in cash.values_mut() {
            *amount = import_decimal(amount, false)?.to_string();
        }
    }
    let mut identities = BTreeSet::new();
    for position in &mut input.positions {
        position.isin = position.isin.trim().to_uppercase();
        let parsed = crate::utils::isin::parse_isin(&position.isin)
            .map_err(|_| import_error("PERSONAL_IMPORT_ISIN_INVALID"))?;
        if !identities.insert(position.isin.clone()) {
            return Err(import_error("PERSONAL_IMPORT_DUPLICATE_ISIN"));
        }
        let valid_venue = |mic: &str| match input.currency.as_str() {
            "INR" => parsed.country_code == "IN" && matches!(mic, "XNSE" | "XBOM"),
            "USD" => {
                parsed.country_code == "US"
                    && matches!(mic, "XNAS" | "XNYS" | "XASE" | "ARCX" | "BATS")
            }
            _ => false,
        };
        if !position.ordinary_equity_confirmed
            || position.currency != input.currency
            || !valid_venue(&position.source_venue)
            || !valid_venue(&position.valuation_venue)
        {
            return Err(import_error("PERSONAL_IMPORT_EQUITY_IDENTITY_UNSUPPORTED"));
        }
        position.symbol = position.symbol.trim().to_uppercase();
        if position.symbol.is_empty()
            || position.symbol.len() > 32
            || !position
                .symbol
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-'))
        {
            return Err(import_error("PERSONAL_IMPORT_SYMBOL_INVALID"));
        }
        position.name = label(&position.name)?;
        let quantity = import_decimal(&position.quantity, true)?;
        position.quantity = quantity.to_string();
        if let Some(cost) = &mut position.average_cost {
            let parsed = import_decimal(cost, false)?;
            exact_product(quantity, parsed)?;
            *cost = parsed.to_string();
        }
        if let Some(price) = &mut position.price {
            if price.currency != position.currency
                || price.venue != position.valuation_venue
                || price.observed_on > input.as_of
                || price.observed_on
                    < NaiveDate::from_ymd_opt(1900, 1, 1).expect("valid supported minimum date")
            {
                return Err(import_error("PERSONAL_IMPORT_PRICE_PROVENANCE_INVALID"));
            }
            price.amount = import_decimal(&price.amount, true)?.to_string();
            price.source = label(&price.source)?;
        }
    }
    input.positions.sort_by(|a, b| a.isin.cmp(&b.isin));
    if input.positions.iter().all(|p| p.average_cost.is_some()) {
        let costs = input
            .positions
            .iter()
            .map(|p| {
                exact_product(
                    import_decimal(&p.quantity, true)?,
                    import_decimal(
                        p.average_cost
                            .as_deref()
                            .ok_or_else(|| import_error("PERSONAL_IMPORT_DECIMAL_INVALID"))?,
                        false,
                    )?,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        exact_sum(&costs)?;
    }
    Ok(ValidatedSnapshot(input))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportReceipt {
    pub schema_version: u32,
    pub review_id: Uuid,
    pub input_hash: String,
    pub account_id: Uuid,
    pub snapshot_id: String,
    pub as_of: NaiveDate,
    pub imported_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavedReviewedSnapshot {
    pub receipt: ImportReceipt,
    pub snapshot: ReviewedSnapshotInput,
    pub asset_ids: BTreeMap<String, String>,
    /// Snapshot imports do not establish trades, acquisition dates or tax lots.
    pub history_available: bool,
}

pub struct ImportTarget {
    pub fingerprint: String,
    pub account_name: String,
    pub currency: String,
    pub previous: Option<ReviewedSnapshotInput>,
    pub existing: Option<ImportReceipt>,
}

pub struct CommitReviewedImport {
    pub review_id: Uuid,
    pub expected_fingerprint: String,
    pub input: ValidatedSnapshot,
    pub authorization: ImportWriteAuthorization,
}

/// Host-owned authority. Implementations must consult the current session state.
pub trait ImportGrantAuthority: Send + Sync {
    fn require_current(&self, profile: Uuid, grant: Uuid, owner: &str) -> Result<()>;
}

pub struct RegistryImportGrantAuthority(pub Arc<crate::profiles::ProfileRegistry>);

impl ImportGrantAuthority for RegistryImportGrantAuthority {
    fn require_current(&self, profile: Uuid, grant: Uuid, owner: &str) -> Result<()> {
        let current = self
            .0
            .sessions
            .admit(owner, grant)
            .map_err(|_| import_error("PERSONAL_IMPORT_GRANT_INVALID"))?;
        if current.profile_id != profile || self.0.profile(profile).is_err() {
            return Err(import_error("PERSONAL_IMPORT_GRANT_INVALID"));
        }
        Ok(())
    }
}

pub struct ImportWriteAuthorization {
    authority: Arc<dyn ImportGrantAuthority>,
    profile: Uuid,
    grant: Uuid,
    owner: String,
    expires_at: DateTime<Utc>,
}

impl ImportWriteAuthorization {
    pub fn require_profile(&self, profile: Uuid) -> Result<()> {
        if profile != self.profile {
            return Err(import_error("PERSONAL_IMPORT_GRANT_INVALID"));
        }
        self.require_current()
    }

    pub fn require_current(&self) -> Result<()> {
        self.authority
            .require_current(self.profile, self.grant, &self.owner)
    }

    pub fn require_unexpired(&self, now: DateTime<Utc>) -> Result<()> {
        self.require_current()?;
        if now >= self.expires_at {
            return Err(import_error("PERSONAL_IMPORT_REVIEW_EXPIRED"));
        }
        Ok(())
    }
}

#[async_trait]
pub trait ReviewedImportRepository: Send + Sync {
    fn profile_id(&self) -> Uuid;
    fn target(&self, input: &ValidatedSnapshot) -> Result<ImportTarget>;
    fn receipt(&self, review_id: Uuid) -> Result<Option<ImportReceipt>>;
    fn latest(&self, account_id: Uuid) -> Result<Option<SavedReviewedSnapshot>>;
    async fn commit(&self, request: CommitReviewedImport) -> Result<ImportReceipt>;
    async fn create_account(&self, request: CommitReviewedAccount)
        -> Result<ReviewedImportAccount>;
}

/// Authenticated host metadata, never deserialized from a request body.
pub struct ImportReviewBinding {
    profile_id: Uuid,
    digest: String,
    grant_id: Uuid,
    owner: String,
    authority: Arc<dyn ImportGrantAuthority>,
}
impl ImportReviewBinding {
    fn write_authorization(&self, expires_at: DateTime<Utc>) -> ImportWriteAuthorization {
        ImportWriteAuthorization {
            authority: self.authority.clone(),
            profile: self.profile_id,
            grant: self.grant_id,
            owner: self.owner.clone(),
            expires_at,
        }
    }
    pub fn new(
        profile_id: Uuid,
        grant_id: Uuid,
        browser_owner: &str,
        authority: Arc<dyn ImportGrantAuthority>,
    ) -> Result<Self> {
        if profile_id.is_nil()
            || grant_id.is_nil()
            || browser_owner.is_empty()
            || browser_owner.len() > 256
        {
            return Err(import_error("PERSONAL_IMPORT_GRANT_INVALID"));
        }
        authority.require_current(profile_id, grant_id, browser_owner)?;
        Ok(Self {
            profile_id,
            digest: canonical_hash(&(profile_id, grant_id, browser_owner))?,
            grant_id,
            owner: browser_owner.to_owned(),
            authority,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportChangeKind {
    Added,
    Changed,
    RemovedFromCompleteSnapshot,
    NotCoveredByPartialSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportChange {
    pub isin: String,
    pub kind: ImportChangeKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReview {
    pub review_id: Uuid,
    pub input_hash: String,
    pub expires_at: DateTime<Utc>,
    pub account_name: String,
    pub snapshot: ReviewedSnapshotInput,
    pub changes: Vec<ImportChange>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PreparedReviewedImport {
    Review { review: ImportReview },
    AlreadyImported { receipt: ImportReceipt },
}

struct PendingImport {
    review: ImportReview,
    binding: String,
    fingerprint: String,
    input: ValidatedSnapshot,
}

pub struct ReviewedImportService {
    repository: Arc<dyn ReviewedImportRepository>,
    pending: Mutex<BTreeMap<Uuid, PendingImport>>,
}

impl ReviewedImportService {
    pub fn new(repository: Arc<dyn ReviewedImportRepository>) -> Self {
        Self {
            repository,
            pending: Mutex::new(BTreeMap::new()),
        }
    }

    pub async fn create_account(
        &self,
        binding: &ImportReviewBinding,
        input: NewReviewedImportAccount,
    ) -> Result<ReviewedImportAccount> {
        self.require_binding(binding)?;
        let input = input.validate()?;
        self.repository
            .create_account(CommitReviewedAccount {
                input,
                authorization: binding
                    .write_authorization(Utc::now() + chrono::Duration::minutes(30)),
            })
            .await
    }

    fn require_binding(&self, binding: &ImportReviewBinding) -> Result<()> {
        if binding.profile_id != self.repository.profile_id() {
            return Err(import_error("PERSONAL_IMPORT_GRANT_INVALID"));
        }
        binding
            .authority
            .require_current(binding.profile_id, binding.grant_id, &binding.owner)
    }

    pub async fn prepare(
        &self,
        binding: &ImportReviewBinding,
        input: ReviewedSnapshotInput,
    ) -> Result<PreparedReviewedImport> {
        self.prepare_at(binding, input, Utc::now()).await
    }

    async fn prepare_at(
        &self,
        binding: &ImportReviewBinding,
        input: ReviewedSnapshotInput,
        now: DateTime<Utc>,
    ) -> Result<PreparedReviewedImport> {
        self.require_binding(binding)?;
        let day = snapshot_day(now);
        let input = validate_snapshot(input, day)?;
        let hash = input.input_hash()?;
        let repository = self.repository.clone();
        let captured = input.clone();
        let target =
            crate::portfolio::coordinator::blocking(move || repository.target(&captured)).await?;
        self.require_binding(binding)?;
        if let Some(receipt) = target.existing {
            if receipt.input_hash != hash {
                return Err(import_error("PERSONAL_IMPORT_DATE_EXISTS"));
            }
            return Ok(PreparedReviewedImport::AlreadyImported { receipt });
        }
        if target.currency != input.input().currency {
            return Err(import_error("PERSONAL_IMPORT_CURRENCY_CONFLICT"));
        }
        let review = ImportReview {
            review_id: Uuid::new_v4(),
            input_hash: hash,
            expires_at: now + chrono::Duration::minutes(30),
            account_name: target.account_name,
            snapshot: input.input().clone(),
            changes: changes(target.previous.as_ref(), input.input()),
            warnings: warnings(input.input(), day),
        };
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| import_error("PERSONAL_IMPORT_REVIEW_UNAVAILABLE"))?;
        pending.retain(|_, value| value.review.expires_at > now);
        if pending.len() >= MAX_PENDING {
            return Err(import_error("PERSONAL_IMPORT_REVIEW_LIMIT"));
        }
        pending.insert(
            review.review_id,
            PendingImport {
                review: review.clone(),
                binding: binding.digest.clone(),
                fingerprint: target.fingerprint,
                input,
            },
        );
        Ok(PreparedReviewedImport::Review { review })
    }

    pub async fn confirm(
        &self,
        binding: &ImportReviewBinding,
        review_id: Uuid,
        expected_hash: String,
    ) -> Result<ImportReceipt> {
        self.confirm_at(binding, review_id, expected_hash, Utc::now())
            .await
    }

    async fn confirm_at(
        &self,
        binding: &ImportReviewBinding,
        review_id: Uuid,
        expected_hash: String,
        now: DateTime<Utc>,
    ) -> Result<ImportReceipt> {
        self.require_binding(binding)?;
        let repository = self.repository.clone();
        let saved =
            crate::portfolio::coordinator::blocking(move || repository.receipt(review_id)).await?;
        self.require_binding(binding)?;
        if let Some(receipt) = saved {
            if receipt.input_hash != expected_hash {
                return Err(import_error("PERSONAL_IMPORT_HASH_CONFLICT"));
            }
            return Ok(receipt);
        }
        let request = {
            let pending = self
                .pending
                .lock()
                .map_err(|_| import_error("PERSONAL_IMPORT_REVIEW_UNAVAILABLE"))?;
            let value = pending
                .get(&review_id)
                .ok_or_else(|| import_error("PERSONAL_IMPORT_REVIEW_REQUIRED"))?;
            if value.binding != binding.digest {
                return Err(import_error("PERSONAL_IMPORT_GRANT_INVALID"));
            }
            if value.review.expires_at <= now {
                return Err(import_error("PERSONAL_IMPORT_REVIEW_EXPIRED"));
            }
            if value.review.input_hash != expected_hash {
                return Err(import_error("PERSONAL_IMPORT_HASH_CONFLICT"));
            }
            CommitReviewedImport {
                review_id,
                expected_fingerprint: value.fingerprint.clone(),
                input: value.input.clone(),
                authorization: binding.write_authorization(value.review.expires_at),
            }
        };
        let receipt = self.repository.commit(request).await?;
        // A cache-cleanup failure must not turn a committed import into failure.
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&review_id);
        }
        Ok(receipt)
    }

    pub fn discard(&self, binding: &ImportReviewBinding, review_id: Uuid) -> Result<()> {
        self.require_binding(binding)?;
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| import_error("PERSONAL_IMPORT_REVIEW_UNAVAILABLE"))?;
        if pending
            .get(&review_id)
            .is_some_and(|value| value.binding != binding.digest)
        {
            return Err(import_error("PERSONAL_IMPORT_GRANT_INVALID"));
        }
        pending.remove(&review_id);
        Ok(())
    }

    pub async fn latest(
        &self,
        binding: &ImportReviewBinding,
        account_id: Uuid,
    ) -> Result<Option<SavedReviewedSnapshot>> {
        self.require_binding(binding)?;
        let repository = self.repository.clone();
        let saved =
            crate::portfolio::coordinator::blocking(move || repository.latest(account_id)).await?;
        self.require_binding(binding)?;
        Ok(saved)
    }
}

fn changes(
    previous: Option<&ReviewedSnapshotInput>,
    current: &ReviewedSnapshotInput,
) -> Vec<ImportChange> {
    let before: BTreeMap<_, _> = previous
        .into_iter()
        .flat_map(|s| s.positions.iter())
        .map(|p| (p.isin.clone(), p))
        .collect();
    let after: BTreeMap<_, _> = current
        .positions
        .iter()
        .map(|p| (p.isin.clone(), p))
        .collect();
    let mut changes = Vec::new();
    for (isin, row) in &after {
        let kind = match before.get(isin) {
            None => Some(ImportChangeKind::Added),
            Some(old) if *old != *row => Some(ImportChangeKind::Changed),
            _ => None,
        };
        if let Some(kind) = kind {
            changes.push(ImportChange {
                isin: isin.clone(),
                kind,
            });
        }
    }
    for isin in before.keys().filter(|isin| !after.contains_key(*isin)) {
        changes.push(ImportChange {
            isin: isin.clone(),
            kind: if current.settled_scope == SettledScope::Complete
                && current.other_exposure == OtherExposure::NoneConfirmed
            {
                ImportChangeKind::RemovedFromCompleteSnapshot
            } else {
                ImportChangeKind::NotCoveredByPartialSnapshot
            },
        });
    }
    changes
}

fn warnings(input: &ReviewedSnapshotInput, today: NaiveDate) -> Vec<String> {
    let mut result = vec!["SNAPSHOT_ONLY_NO_TRADE_OR_TAX_HISTORY".into()];
    if input.as_of != today {
        result.push("SOURCE_DATE_IS_HISTORICAL".into());
    }
    if input.cash.is_none() {
        result.push("CASH_UNKNOWN".into());
    }
    if input.settled_scope == SettledScope::Partial {
        result.push("SETTLED_SCOPE_PARTIAL".into());
    }
    if input.other_exposure != OtherExposure::NoneConfirmed {
        result.push("OTHER_EXPOSURE_UNRESOLVED".into());
    }
    if input.positions.iter().any(|p| p.average_cost.is_none()) {
        result.push("PURCHASE_COST_UNKNOWN".into());
    }
    if input.positions.iter().any(|p| p.price.is_none()) {
        result.push("MARKET_PRICE_UNKNOWN".into());
    }
    if input.positions.iter().any(|p| {
        p.price
            .as_ref()
            .is_some_and(|q| q.observed_on != input.as_of)
    }) {
        result.push("PRICE_DATE_DIFFERS_FROM_SNAPSHOT".into());
    }
    result
}

#[cfg(test)]
#[path = "reviewed_import_tests.rs"]
mod tests;
