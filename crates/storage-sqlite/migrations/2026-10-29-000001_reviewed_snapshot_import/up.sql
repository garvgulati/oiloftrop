-- Local-only provenance is inseparable from its holdings snapshot. A legacy
-- deletion must fail rather than silently erase the unknown/known distinction.
CREATE TABLE personal_snapshot_imports (
    snapshot_id TEXT PRIMARY KEY NOT NULL REFERENCES holdings_snapshots(id) ON DELETE RESTRICT,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    profile_id TEXT NOT NULL,
    review_id TEXT NOT NULL UNIQUE,
    input_hash TEXT NOT NULL CHECK(length(input_hash)=64),
    schema_version INTEGER NOT NULL CHECK(schema_version=1),
    canonical_input TEXT NOT NULL CHECK(json_valid(canonical_input) AND length(CAST(canonical_input AS BLOB))<=2097152),
    asset_ids TEXT NOT NULL CHECK(json_valid(asset_ids)),
    as_of TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    history_available INTEGER NOT NULL DEFAULT 0 CHECK(history_available=0),
    UNIQUE(account_id,as_of)
);
