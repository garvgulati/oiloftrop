//! Atomic, profile-local reviewed imports. No service/resolver/provider calls.
use super::{AccountStateSnapshotDB, SnapshotRepository};
use crate::{
    accounts::AccountDB,
    assets::{AssetDB, InsertableAssetDB},
    db::{get_connection, DbAccess, WriteHandle},
    errors::StorageError,
    DbPool,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::{
    prelude::*,
    sql_types::{BigInt, Integer, Nullable, Text},
};
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
};
use uuid::Uuid;
use wealthfolio_core::{
    accounts::{account_types, NewAccount, TrackingMode},
    assets::{AssetKind, InstrumentType, NewAsset, QuoteMode},
    portfolio::snapshot::{reviewed_import::*, AccountStateSnapshot, Position, SnapshotSource},
    Error, Result,
};

const TARGET_BYTES: i64 = 4 * 1024 * 1024;
const TARGET_SNAPSHOTS: usize = 200;

pub struct ReviewedSnapshotRepository {
    pool: Arc<DbPool>,
    writer: WriteHandle,
    profile_id: Uuid,
    database_path: PathBuf,
}

impl ReviewedSnapshotRepository {
    pub fn new(
        pool: Arc<DbPool>,
        writer: WriteHandle,
        profile_id: Uuid,
        access: &DbAccess,
    ) -> Result<Self> {
        if profile_id.is_nil() || !access.is_encrypted() {
            return Err(import_error("PERSONAL_IMPORT_ENCRYPTED_SCOPE_REQUIRED"));
        }
        access
            .check_runtime_boundary(wealthfolio_core::runtime_boundary::RuntimePurpose::Personal)?;
        let database_path = std::fs::canonicalize(access.path())
            .map_err(|_| import_error("PERSONAL_IMPORT_DATABASE_SCOPE_INVALID"))?;
        {
            let mut conn = get_connection(&pool)?;
            ensure_database(&mut conn, &database_path)?;
        }
        Ok(Self {
            pool,
            writer,
            profile_id,
            database_path,
        })
    }
}

#[derive(QueryableByName)]
struct JsonRow {
    #[diesel(sql_type=Text)]
    data: String,
}
#[derive(QueryableByName)]
struct Scalar {
    #[diesel(sql_type=BigInt)]
    value: i64,
}
#[derive(QueryableByName)]
struct FileRow {
    #[diesel(sql_type=Text)]
    file: String,
}

#[derive(Debug, Clone, Serialize, QueryableByName)]
struct ImportRow {
    #[diesel(sql_type=Text)]
    snapshot_id: String,
    #[diesel(sql_type=Text)]
    account_id: String,
    #[diesel(sql_type=Text)]
    profile_id: String,
    #[diesel(sql_type=Text)]
    review_id: String,
    #[diesel(sql_type=Text)]
    input_hash: String,
    #[diesel(sql_type=Integer)]
    schema_version: i32,
    #[diesel(sql_type=Text)]
    canonical_input: String,
    #[diesel(sql_type=Text)]
    asset_ids: String,
    #[diesel(sql_type=Text)]
    as_of: String,
    #[diesel(sql_type=Text)]
    imported_at: String,
    #[diesel(sql_type=Integer)]
    history_available: i32,
}

fn ensure_database(conn: &mut SqliteConnection, path: &Path) -> Result<()> {
    let file = diesel::sql_query("SELECT file FROM pragma_database_list WHERE name='main'")
        .get_result::<FileRow>(conn)
        .map_err(StorageError::from)?;
    if std::fs::canonicalize(file.file)
        .map_err(|_| import_error("PERSONAL_IMPORT_DATABASE_SCOPE_INVALID"))?
        .as_path()
        != path
    {
        return Err(import_error("PERSONAL_IMPORT_DATABASE_SCOPE_INVALID"));
    }
    let personal=diesel::sql_query("SELECT count(*) AS value FROM runtime_boundary WHERE singleton=1 AND purpose=? AND boundary_version=?")
        .bind::<Text,_>(wealthfolio_core::runtime_boundary::RuntimePurpose::Personal.as_str()).bind::<BigInt,_>(i64::from(wealthfolio_core::runtime_boundary::RUNTIME_BOUNDARY_VERSION)).get_result::<Scalar>(conn).map_err(StorageError::from)?;
    if personal.value != 1 {
        return Err(import_error("PERSONAL_IMPORT_PERSONAL_RUNTIME_REQUIRED"));
    }
    Ok(())
}

fn today() -> chrono::NaiveDate {
    snapshot_day(Utc::now())
}

fn read_rows(
    conn: &mut SqliteConnection,
    account: Uuid,
) -> Result<(Vec<AccountStateSnapshotDB>, Vec<ImportRow>)> {
    let bytes=diesel::sql_query("SELECT (SELECT COALESCE(sum(length(CAST(positions AS BLOB))+length(CAST(cash_balances AS BLOB))),0) FROM holdings_snapshots WHERE account_id=?) + (SELECT COALESCE(sum(length(CAST(canonical_input AS BLOB))+length(CAST(asset_ids AS BLOB))),0) FROM personal_snapshot_imports WHERE account_id=?) AS value")
        .bind::<Text,_>(account.to_string()).bind::<Text,_>(account.to_string()).get_result::<Scalar>(conn).map_err(StorageError::from)?;
    if bytes.value > TARGET_BYTES {
        return Err(import_error("PERSONAL_IMPORT_TARGET_LIMIT"));
    }
    let snapshots = diesel::sql_query(
        "SELECT * FROM holdings_snapshots WHERE account_id=? ORDER BY snapshot_date,id LIMIT 201",
    )
    .bind::<Text, _>(account.to_string())
    .load::<AccountStateSnapshotDB>(conn)
    .map_err(StorageError::from)?;
    let imports=diesel::sql_query("SELECT * FROM personal_snapshot_imports WHERE account_id=? ORDER BY as_of,review_id LIMIT 201")
        .bind::<Text,_>(account.to_string()).load::<ImportRow>(conn).map_err(StorageError::from)?;
    if snapshots.len() > TARGET_SNAPSHOTS || imports.len() != snapshots.len() {
        return Err(import_error("PERSONAL_IMPORT_TARGET_NOT_DEDICATED"));
    }
    Ok((snapshots, imports))
}

fn account(conn: &mut SqliteConnection, id: Uuid) -> Result<Value> {
    let sql="SELECT json_object('id',id,'name',name,'currency',currency,'active',is_active,'archived',is_archived,'trackingMode',tracking_mode,'type',account_type,'meta',meta,'provider',provider,'providerAccountId',provider_account_id,'accountNumber',account_number,'created',created_at,'updated',updated_at) AS data FROM accounts WHERE id=?";
    let row = diesel::sql_query(sql)
        .bind::<Text, _>(id.to_string())
        .get_result::<JsonRow>(conn)
        .optional()
        .map_err(StorageError::from)?
        .ok_or_else(|| import_error("PERSONAL_IMPORT_ACCOUNT_REQUIRED"))?;
    if row.data.len() > 65536 {
        return Err(import_error("PERSONAL_IMPORT_TARGET_LIMIT"));
    }
    let account: Value = serde_json::from_str(&row.data)
        .map_err(|_| import_error("PERSONAL_IMPORT_RECORD_INVALID"))?;
    if account["active"] != 1 || account["archived"] != 0 || account["trackingMode"] != "HOLDINGS" {
        return Err(import_error("PERSONAL_IMPORT_HOLDINGS_ACCOUNT_REQUIRED"));
    }
    let activities =
        diesel::sql_query("SELECT count(*) AS value FROM activities WHERE account_id=?")
            .bind::<Text, _>(id.to_string())
            .get_result::<Scalar>(conn)
            .map_err(StorageError::from)?;
    if activities.value != 0 {
        return Err(import_error("PERSONAL_IMPORT_TARGET_NOT_DEDICATED"));
    }
    Ok(account)
}

fn asset_for(conn: &mut SqliteConnection, position: &ReviewedEquity) -> Result<Option<AssetDB>> {
    let id = format!("personal-stock-{}", position.isin);
    use crate::schema::assets::dsl as assets;
    let same_isin=diesel::dsl::sql::<Nullable<Text>>("CASE WHEN json_valid(metadata) THEN upper(json_extract(metadata,'$.identifiers.isin')) ELSE NULL END")
        .eq(Some(position.isin.as_str()));
    let same_listing = assets::kind
        .eq(AssetKind::Investment.as_db_str())
        .and(assets::instrument_symbol.eq(Some(position.symbol.as_str())))
        .and(assets::instrument_exchange_mic.eq(Some(position.source_venue.as_str())));
    let found = assets::assets
        .filter(assets::id.eq(&id).or(same_isin).or(same_listing))
        .select(AssetDB::as_select())
        .limit(3)
        .load::<AssetDB>(conn)
        .map_err(StorageError::from)?;
    if found.len() > 1 {
        return Err(import_error("PERSONAL_IMPORT_IDENTITY_AMBIGUOUS"));
    }
    if let Some(asset) = found.first() {
        let metadata: Value = serde_json::from_str(asset.metadata.as_deref().unwrap_or("null"))
            .map_err(|_| import_error("PERSONAL_IMPORT_IDENTITY_CONFLICT"))?;
        if metadata["identifiers"]["isin"] != position.isin
            || asset.kind != AssetKind::Investment.as_db_str()
            || asset.instrument_type.as_deref() != Some(InstrumentType::Equity.as_db_str())
            || asset.quote_ccy != position.currency
            || asset.quote_mode != QuoteMode::Manual.as_db_str()
            || asset.is_active != 1
        {
            return Err(import_error("PERSONAL_IMPORT_IDENTITY_CONFLICT"));
        }
    }
    Ok(found.into_iter().next())
}

fn snapshot_row(
    input: &ValidatedSnapshot,
    ids: &BTreeMap<String, String>,
    now: DateTime<Utc>,
) -> Result<AccountStateSnapshotDB> {
    let source = input.input();
    let account = source.account_id.to_string();
    let mut positions = HashMap::new();
    let mut known_costs = Vec::new();
    let all_costs = source.positions.iter().all(|p| p.average_cost.is_some());
    for equity in &source.positions {
        let asset = ids
            .get(&equity.isin)
            .ok_or_else(|| import_error("PERSONAL_IMPORT_IDENTITY_CONFLICT"))?
            .clone();
        let quantity = import_decimal(&equity.quantity, true)?;
        let cost = equity
            .average_cost
            .as_deref()
            .map(|cost| import_decimal(cost, false))
            .transpose()?;
        let basis = cost.map(|cost| exact_product(quantity, cost)).transpose()?;
        if let Some(basis) = basis {
            known_costs.push(basis);
        }
        positions.insert(
            asset.clone(),
            Position {
                id: format!("POS-{asset}-{account}"),
                account_id: account.clone(),
                asset_id: asset,
                quantity,
                average_cost: cost.unwrap_or(Decimal::ZERO),
                total_cost_basis: basis.unwrap_or(Decimal::ZERO),
                currency: equity.currency.clone(),
                // Compatibility-only unknown sentinel. This is NOT an acquisition
                // date; the immutable companion explicitly declares no history.
                inception_date: DateTime::UNIX_EPOCH,
                created_at: now,
                last_updated: now,
                cost_basis_account: basis,
                cost_basis_base: None,
                ..Position::default()
            },
        );
    }
    let cash = source
        .cash
        .as_ref()
        .map(|cash| {
            cash.iter()
                .map(|(currency, amount)| Ok((currency.clone(), import_decimal(amount, false)?)))
                .collect::<Result<HashMap<_, _>>>()
        })
        .transpose()?
        .unwrap_or_default();
    let cost_basis = if all_costs {
        exact_sum(&known_costs)?
    } else {
        Decimal::ZERO
    };
    let cash_total = cash.get(&source.currency).copied().unwrap_or(Decimal::ZERO);
    let snapshot = AccountStateSnapshot {
        id: format!("{account}_{}", source.as_of),
        account_id: account,
        snapshot_date: source.as_of,
        currency: source.currency.clone(),
        positions,
        cash_balances: cash,
        cost_basis,
        cash_total_account_currency: cash_total,
        calculated_at: now.naive_utc(),
        source: SnapshotSource::ManualEntry,
        ..AccountStateSnapshot::default()
    };
    let mut row = AccountStateSnapshotDB::from(snapshot);
    // The general conversion rounds aggregate columns. This reviewed route must
    // retain exact account-currency amounts and never invoke FX conversion.
    row.cost_basis = cost_basis.to_string();
    row.cash_total_account_currency = cash_total.to_string();
    Ok(row)
}

fn saved(
    row: &ImportRow,
    legacy: &AccountStateSnapshotDB,
    profile: Uuid,
) -> Result<SavedReviewedSnapshot> {
    let invalid = || import_error("PERSONAL_IMPORT_RECORD_INVALID");
    if row.schema_version != 1
        || row.history_available != 0
        || row.profile_id != profile.to_string()
        || row.canonical_input.len() > MAX_REVIEW_BYTES
        || row.asset_ids.len() > 65536
    {
        return Err(invalid());
    }
    let raw: ReviewedSnapshotInput =
        serde_json::from_str(&row.canonical_input).map_err(|_| invalid())?;
    let validated = validate_snapshot(raw, today())?;
    let ids: BTreeMap<String, String> =
        serde_json::from_str(&row.asset_ids).map_err(|_| invalid())?;
    if ids.len() != validated.input().positions.len()
        || ids
            .values()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != ids.len()
        || validated.input_hash()? != row.input_hash
    {
        return Err(invalid());
    }
    let imported = DateTime::parse_from_rfc3339(&row.imported_at)
        .map_err(|_| invalid())?
        .with_timezone(&Utc);
    let expected = snapshot_row(&validated, &ids, imported)?;
    if logical_snapshot_row(&expected)? != logical_snapshot_row(legacy)?
        || row.account_id != validated.input().account_id.to_string()
        || row.snapshot_id != legacy.id
        || row.as_of != validated.input().as_of.to_string()
    {
        return Err(import_error("PERSONAL_IMPORT_RECORD_CONFLICT"));
    }
    Ok(SavedReviewedSnapshot {
        receipt: ImportReceipt {
            schema_version: 1,
            review_id: Uuid::parse_str(&row.review_id).map_err(|_| invalid())?,
            input_hash: row.input_hash.clone(),
            account_id: validated.input().account_id,
            snapshot_id: row.snapshot_id.clone(),
            as_of: validated.input().as_of,
            imported_at: imported,
        },
        snapshot: validated.input().clone(),
        asset_ids: ids,
        history_available: false,
    })
}

fn logical_snapshot_row(row: &AccountStateSnapshotDB) -> Result<Value> {
    let mut value =
        serde_json::to_value(row).map_err(|_| import_error("PERSONAL_IMPORT_RECORD_INVALID"))?;
    // HashMap key order is not financial data. Parse the JSON columns without
    // converting any decimal strings to numbers; all other columns stay exact.
    value["positions"] = serde_json::from_str(&row.positions)
        .map_err(|_| import_error("PERSONAL_IMPORT_RECORD_INVALID"))?;
    value["cashBalances"] = serde_json::from_str(&row.cash_balances)
        .map_err(|_| import_error("PERSONAL_IMPORT_RECORD_INVALID"))?;
    Ok(value)
}

fn target(
    conn: &mut SqliteConnection,
    input: &ValidatedSnapshot,
    profile: Uuid,
) -> Result<(ImportTarget, BTreeMap<String, Option<AssetDB>>)> {
    let wanted = input.input();
    let account = account(conn, wanted.account_id)?;
    if account["currency"] != wanted.currency {
        return Err(import_error("PERSONAL_IMPORT_CURRENCY_CONFLICT"));
    }
    let (snapshots, imports) = read_rows(conn, wanted.account_id)?;
    let mut history = Vec::new();
    for legacy in &snapshots {
        let provenance = imports
            .iter()
            .find(|row| row.snapshot_id == legacy.id)
            .ok_or_else(|| import_error("PERSONAL_IMPORT_TARGET_NOT_DEDICATED"))?;
        history.push(saved(provenance, legacy, profile)?);
    }
    let previous = history.last();
    if previous.is_some_and(|previous| previous.snapshot.source != wanted.source) {
        return Err(import_error("PERSONAL_IMPORT_SOURCE_ACCOUNT_CONFLICT"));
    }
    let existing = history
        .iter()
        .find(|saved| saved.snapshot.as_of == wanted.as_of)
        .map(|saved| saved.receipt.clone());
    if existing.is_none()
        && previous.is_some_and(|previous| wanted.as_of <= previous.snapshot.as_of)
    {
        return Err(import_error("PERSONAL_IMPORT_DATE_NOT_APPEND_ONLY"));
    }
    if existing.is_none() && snapshots.len() >= TARGET_SNAPSHOTS {
        return Err(import_error("PERSONAL_IMPORT_TARGET_LIMIT"));
    }
    let mut assets = BTreeMap::new();
    for position in &wanted.positions {
        assets.insert(position.isin.clone(), asset_for(conn, position)?);
    }
    let fingerprint = canonical_hash(
        &json!({"profile":profile,"account":account,"snapshots":snapshots,"provenance":imports,"assets":assets}),
    )?;
    Ok((
        ImportTarget {
            fingerprint,
            account_name: account["name"]
                .as_str()
                .ok_or_else(|| import_error("PERSONAL_IMPORT_RECORD_INVALID"))?
                .into(),
            currency: wanted.currency.clone(),
            previous: previous.map(|p| p.snapshot.clone()),
            existing,
        },
        assets,
    ))
}

fn receipt_on_connection(
    conn: &mut SqliteConnection,
    id: Uuid,
    profile: Uuid,
) -> Result<Option<ImportReceipt>> {
    let row = diesel::sql_query("SELECT * FROM personal_snapshot_imports WHERE review_id=?")
        .bind::<Text, _>(id.to_string())
        .get_result::<ImportRow>(conn)
        .optional()
        .map_err(StorageError::from)?;
    row.map(|row| {
        let legacy = diesel::sql_query("SELECT * FROM holdings_snapshots WHERE id=?")
            .bind::<Text, _>(&row.snapshot_id)
            .get_result::<AccountStateSnapshotDB>(conn)
            .map_err(StorageError::from)?;
        Ok(saved(&row, &legacy, profile)?.receipt)
    })
    .transpose()
}

#[async_trait]
impl ReviewedImportRepository for ReviewedSnapshotRepository {
    async fn create_account(
        &self,
        request: CommitReviewedAccount,
    ) -> Result<ReviewedImportAccount> {
        let profile = self.profile_id;
        let path = self.database_path.clone();
        self.writer
            .exec_tx(move |tx| {
                let conn = tx.conn();
                ensure_database(conn, &path)?;
                request.authorization.require_profile(profile)?;
                request.authorization.require_unexpired(Utc::now())?;
                let input = request.input.validate()?;
                let existing = crate::schema::accounts::table
                    .find(input.id.to_string())
                    .select(AccountDB::as_select())
                    .first::<AccountDB>(conn)
                    .optional()
                    .map_err(StorageError::from)?;
                if let Some(existing) = existing {
                    let dedicated = existing
                        .meta
                        .as_deref()
                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                        .is_some_and(|meta| {
                            meta.get(REVIEWED_ACCOUNT_META_KEY) == Some(&Value::Bool(true))
                        });
                    if !dedicated
                        || existing.name != input.name
                        || existing.currency != input.currency
                        || existing.account_type != account_types::SECURITIES
                        || existing.tracking_mode != "HOLDINGS"
                        || !existing.is_active
                        || existing.is_archived
                        || existing.provider.is_some()
                        || existing.provider_account_id.is_some()
                        || existing.platform_id.is_some()
                        || existing.account_number.is_some()
                    {
                        return Err(import_error("PERSONAL_IMPORT_ACCOUNT_CONFLICT"));
                    }
                } else {
                    let new = NewAccount {
                        id: Some(input.id.to_string()),
                        name: input.name.clone(),
                        account_type: account_types::SECURITIES.into(),
                        group: None,
                        currency: input.currency.clone(),
                        is_default: false,
                        is_active: true,
                        platform_id: None,
                        account_number: None,
                        meta: Some(json!({(REVIEWED_ACCOUNT_META_KEY):true}).to_string()),
                        provider: None,
                        provider_account_id: None,
                        is_archived: false,
                        tracking_mode: TrackingMode::Holdings,
                    };
                    new.validate()?;
                    let mut row = AccountDB::from(new);
                    row.ensure_default_accounting_meta()?;
                    diesel::insert_into(crate::schema::accounts::table)
                        .values(&row)
                        .execute(conn)
                        .map_err(StorageError::from)?;
                }
                request.authorization.require_unexpired(Utc::now())?;
                Ok(ReviewedImportAccount {
                    id: input.id,
                    name: input.name,
                    currency: input.currency,
                })
            })
            .await
    }
    fn profile_id(&self) -> Uuid {
        self.profile_id
    }
    fn target(&self, input: &ValidatedSnapshot) -> Result<ImportTarget> {
        let mut conn = get_connection(&self.pool)?;
        conn.transaction::<_, StorageError, _>(|conn| {
            ensure_database(conn, &self.database_path)
                .and_then(|()| target(conn, input, self.profile_id).map(|target| target.0))
                .map_err(StorageError::from)
        })
        .map_err(Error::from)
    }
    fn receipt(&self, id: Uuid) -> Result<Option<ImportReceipt>> {
        let mut conn = get_connection(&self.pool)?;
        conn.transaction::<_, StorageError, _>(|conn| {
            ensure_database(conn, &self.database_path)
                .and_then(|()| receipt_on_connection(conn, id, self.profile_id))
                .map_err(StorageError::from)
        })
        .map_err(Error::from)
    }
    fn latest(&self, id: Uuid) -> Result<Option<SavedReviewedSnapshot>> {
        let mut conn = get_connection(&self.pool)?;
        conn.transaction::<_, StorageError, _>(|conn| {
            ensure_database(conn, &self.database_path)
                .and_then(|()| latest_on_connection(conn, id, self.profile_id))
                .map_err(StorageError::from)
        })
        .map_err(Error::from)
    }
    async fn commit(&self, request: CommitReviewedImport) -> Result<ImportReceipt> {
        let profile = self.profile_id;
        let path = self.database_path.clone();
        self.writer
            .exec_tx(move |tx| commit_on_connection(tx.conn(), request, profile, &path))
            .await
    }
}

fn latest_on_connection(
    conn: &mut SqliteConnection,
    id: Uuid,
    profile: Uuid,
) -> Result<Option<SavedReviewedSnapshot>> {
    account(conn, id)?;
    let (snapshots, imports) = read_rows(conn, id)?;
    let Some(legacy) = snapshots.last() else {
        return Ok(None);
    };
    let row = imports
        .iter()
        .find(|row| row.snapshot_id == legacy.id)
        .ok_or_else(|| import_error("PERSONAL_IMPORT_TARGET_NOT_DEDICATED"))?;
    let saved = saved(row, legacy, profile)?;
    for position in &saved.snapshot.positions {
        let asset = asset_for(conn, position)?
            .ok_or_else(|| import_error("PERSONAL_IMPORT_IDENTITY_CONFLICT"))?;
        if saved.asset_ids.get(&position.isin) != Some(&asset.id) {
            return Err(import_error("PERSONAL_IMPORT_IDENTITY_CONFLICT"));
        }
    }
    Ok(Some(saved))
}

fn commit_on_connection(
    conn: &mut SqliteConnection,
    request: CommitReviewedImport,
    profile: Uuid,
    path: &Path,
) -> Result<ImportReceipt> {
    ensure_database(conn, path)?;
    request.authorization.require_profile(profile)?;
    let hash = request.input.input_hash()?;
    if let Some(receipt) = receipt_on_connection(conn, request.review_id, profile)? {
        if receipt.input_hash != hash {
            return Err(import_error("PERSONAL_IMPORT_HASH_CONFLICT"));
        }
        request.authorization.require_current()?;
        return Ok(receipt);
    }
    request.authorization.require_unexpired(Utc::now())?;
    let imported_at = Utc::now();
    validate_snapshot(request.input.input().clone(), snapshot_day(imported_at))?;
    let (current, assets) = target(conn, &request.input, profile)?;
    if current.fingerprint != request.expected_fingerprint {
        return Err(import_error("PERSONAL_IMPORT_TARGET_CHANGED"));
    }
    if current.existing.is_some() {
        return Err(import_error("PERSONAL_IMPORT_DATE_EXISTS"));
    }
    let mut ids = BTreeMap::new();
    for position in &request.input.input().positions {
        let id = if let Some(asset) = assets.get(&position.isin).and_then(Option::as_ref) {
            asset.id.clone()
        } else {
            let id = format!("personal-stock-{}", position.isin);
            let asset = NewAsset {
                id: Some(id.clone()),
                kind: AssetKind::Investment,
                name: Some(position.name.clone()),
                display_code: Some(position.symbol.clone()),
                is_active: true,
                quote_mode: QuoteMode::Manual,
                quote_ccy: position.currency.clone(),
                instrument_type: Some(InstrumentType::Equity),
                instrument_symbol: Some(position.symbol.clone()),
                instrument_exchange_mic: Some(position.source_venue.clone()),
                metadata: Some(
                    json!({"identifiers":{"isin":position.isin},"personalSnapshotOnly":true}),
                ),
                ..NewAsset::default()
            };
            asset.validate()?;
            let row = InsertableAssetDB::from(asset);
            diesel::insert_into(crate::schema::assets::table)
                .values(&row)
                .execute(conn)
                .map_err(StorageError::from)?;
            id
        };
        ids.insert(position.isin.clone(), id);
    }
    let row = snapshot_row(&request.input, &ids, imported_at)?;
    diesel::insert_into(crate::schema::holdings_snapshots::table)
        .values(&row)
        .execute(conn)
        .map_err(StorageError::from)?;
    SnapshotRepository::write_snapshot_positions(conn, &row.id, &request_input_positions(&row)?)?;
    let input = request.input.input();
    let canonical_input =
        serde_json::to_string(input).map_err(|_| import_error("PERSONAL_IMPORT_ENCODING"))?;
    let asset_ids =
        serde_json::to_string(&ids).map_err(|_| import_error("PERSONAL_IMPORT_ENCODING"))?;
    diesel::sql_query("INSERT INTO personal_snapshot_imports(snapshot_id,account_id,profile_id,review_id,input_hash,schema_version,canonical_input,asset_ids,as_of,imported_at,history_available) VALUES(?,?,?,?,?,1,?,?,?,?,0)")
                .bind::<Text,_>(&row.id).bind::<Text,_>(input.account_id.to_string()).bind::<Text,_>(profile.to_string()).bind::<Text,_>(request.review_id.to_string()).bind::<Text,_>(&hash)
                .bind::<Text,_>(canonical_input).bind::<Text,_>(asset_ids).bind::<Text,_>(input.as_of.to_string()).bind::<Text,_>(imported_at.to_rfc3339()).execute(conn).map_err(StorageError::from)?;
    // Final authorization check is inside the existing immediate transaction.
    // Revocation or expiry before this point rolls back all import writes.
    request.authorization.require_unexpired(Utc::now())?;
    Ok(ImportReceipt {
        schema_version: 1,
        review_id: request.review_id,
        input_hash: hash,
        account_id: input.account_id,
        snapshot_id: row.id,
        as_of: input.as_of,
        imported_at,
    })
}

fn request_input_positions(row: &AccountStateSnapshotDB) -> Result<HashMap<String, Position>> {
    serde_json::from_str(&row.positions)
        .map_err(|_| Error::ConstraintViolation("PERSONAL_IMPORT_ENCODING".into()))
}
