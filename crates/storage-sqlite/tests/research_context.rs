#![allow(clippy::unwrap_used, clippy::panic, reason = "fictional test fixtures")]

use chrono::NaiveDate;
use diesel::connection::SimpleConnection;
use serde_json::json;
use wealthfolio_core::research::ResearchContextRepository;
use wealthfolio_storage_sqlite::{
    db,
    research::{load_context_on_connection, ResearchRepository},
};

fn day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 24).unwrap()
}

fn fixture() -> (tempfile::TempDir, db::DbAccess, ResearchRepository) {
    let directory = tempfile::tempdir().unwrap();
    let access = db::DbAccess::plaintext(directory.path().join("fictional.db").to_str().unwrap());
    access.run_migrations().unwrap();
    let conn = access.connect_rusqlite().unwrap();
    conn.execute_batch("INSERT INTO accounts(id,name,currency,is_archived,tracking_mode) VALUES('fictional','Fictional hidden account','INR',1,'HOLDINGS');
        INSERT INTO app_settings(setting_key,setting_value) VALUES('base_currency','INR') ON CONFLICT(setting_key) DO UPDATE SET setting_value='INR';
        INSERT INTO assets(id,kind,name,quote_mode,quote_ccy,instrument_type,instrument_symbol,instrument_exchange_mic) VALUES('fictional-share','INVESTMENT','Fictional share','MANUAL','INR','EQUITY','TRIALIN','XNSE');
        INSERT INTO quotes(id,asset_id,day,source,close,currency,timestamp,created_at) VALUES('fictional-close','fictional-share','2026-10-24','MANUAL','224.64000000','INR','2026-10-24T10:00:00Z','2026-10-24T10:00:00Z');
        INSERT INTO holdings_snapshots(id,account_id,snapshot_date,currency) VALUES('fictional-snapshot','fictional','2026-10-24','INR');").unwrap();
    let positions = json!({"fictional-share":{"quantity":"999999999.12345678","averageCost":"123.45000000","currency":"INR","contractMultiplier":"1"}});
    conn.execute(
        "UPDATE holdings_snapshots SET positions=?1,cash_balances=?2",
        rusqlite::params![positions.to_string(), r#"{"INR":"123.45678901"}"#],
    )
    .unwrap();
    conn.execute_batch("UPDATE projection_state SET dirty_from=NULL,activity_issues='[]';")
        .unwrap();
    drop(conn);
    let repository = ResearchRepository::new(access.create_pool().unwrap());
    (directory, access, repository)
}

#[test]
fn exact_decimal_and_hidden_account_survive_the_research_boundary() {
    let (_dir, _access, repository) = fixture();
    let context = repository.load_context(day(), "INR").unwrap();
    assert_eq!(context.accounts.len(), 1);
    assert!(context.accounts[0].archived);
    assert_eq!(
        context.positions[0].quantity.as_deref(),
        Some("999999999.12345678")
    );
    assert_eq!(context.positions[0].average_cost.as_deref(), Some("123.45"));
    assert_eq!(
        context.accounts[0].cash["INR"].as_deref(),
        Some("123.45678901")
    );
    let quote = context.positions[0].quote.as_ref().unwrap();
    assert_eq!(quote.observed_at, "2026-10-24T10:00:00Z");
    assert_eq!(quote.price.as_deref(), Some("224.64"));
    assert_eq!(context.positions[0].issuer_id, None);
    assert!(!context.coverage_complete);
    assert_eq!(
        context.context_hash,
        repository.load_context(day(), "INR").unwrap().context_hash
    );
}

#[test]
fn corrupt_and_missing_snapshots_are_distinct_from_an_empty_completed_account() {
    let (_dir, access, repository) = fixture();
    let conn = access.connect_rusqlite().unwrap();
    conn.execute_batch("UPDATE holdings_snapshots SET positions='broken';")
        .unwrap();
    let corrupt = repository.load_context(day(), "INR").unwrap();
    assert!(corrupt.issues.iter().any(|v| v.code == "CORRUPT_SNAPSHOT"));
    assert!(corrupt.positions.is_empty());
    conn.execute_batch("DELETE FROM holdings_snapshots;")
        .unwrap();
    let missing = repository.load_context(day(), "INR").unwrap();
    assert!(missing.issues.iter().any(|v| v.code == "SNAPSHOT_MISSING"));
    assert_ne!(corrupt.context_hash, missing.context_hash);
    conn.execute_batch("INSERT INTO holdings_snapshots(id,account_id,snapshot_date,currency) VALUES('empty','fictional','2026-10-24','INR'); UPDATE projection_state SET dirty_from=NULL;").unwrap();
    let empty = repository.load_context(day(), "INR").unwrap();
    assert!(!empty
        .issues
        .iter()
        .any(|v| v.code == "SNAPSHOT_MISSING" || v.code == "CORRUPT_SNAPSHOT"));
    assert!(empty.positions.is_empty());
    assert!(empty
        .issues
        .iter()
        .any(|v| v.code == "INVESTOR_PLAN_NOT_CONFIGURED"));
}

#[test]
fn legacy_numeric_amount_is_unverified_instead_of_silently_rounded() {
    let (_dir, access, repository) = fixture();
    access.connect_rusqlite().unwrap().execute("UPDATE holdings_snapshots SET positions=?1",[r#"{"fictional-share":{"quantity":999999999.12345678,"averageCost":"1","currency":"INR","contractMultiplier":"1"}}"#]).unwrap();
    let context = repository.load_context(day(), "INR").unwrap();
    assert_eq!(context.positions[0].quantity, None);
    assert!(context
        .issues
        .iter()
        .any(|v| v.code == "LEGACY_NUMBER_PRECISION_UNVERIFIED"));
}

#[test]
fn date_identity_membership_and_projection_changes_invalidate_the_context() {
    let (_dir, access, repository) = fixture();
    let conn = access.connect_rusqlite().unwrap();
    let mut hash = repository.load_context(day(), "INR").unwrap().context_hash;
    for sql in [
        "UPDATE quotes SET timestamp='2026-10-24T11:00:00Z'",
        "UPDATE assets SET instrument_exchange_mic='XBOM' WHERE id='fictional-share'",
        "INSERT INTO accounts(id,name,currency,is_active) VALUES('new','New fictional account','INR',1)",
        "UPDATE projection_state SET version=version+1,dirty_from='2026-10-24'",
    ] {
        conn.execute_batch(sql).unwrap();
        let next = repository.load_context(day(),"INR").unwrap();
        assert_ne!(next.context_hash,hash);
        hash=next.context_hash;
    }
    assert!(repository
        .load_context(day(), "INR")
        .unwrap()
        .issues
        .iter()
        .any(|v| v.code == "PROJECTION_DIRTY"));
}

#[test]
fn actual_repository_read_stays_coherent_during_another_connections_commit() {
    let (_dir, access, repository) = fixture();
    let mut reader = access.connect().unwrap();
    let writer = access.connect_rusqlite().unwrap();
    reader.batch_execute("BEGIN DEFERRED;").unwrap();
    let before = load_context_on_connection(&mut reader, day(), "INR").unwrap();
    writer.execute_batch("UPDATE quotes SET close='999'; UPDATE app_settings SET setting_value='USD' WHERE setting_key='base_currency'; INSERT INTO accounts(id,name,currency) VALUES('concurrent','Fictional concurrent account','USD');").unwrap();
    let during = load_context_on_connection(&mut reader, day(), "INR").unwrap();
    assert_eq!(before.context_hash, during.context_hash);
    assert_eq!(during.accounts.len(), 1);
    assert_eq!(during.base_currency, "INR");
    reader.batch_execute("COMMIT;").unwrap();
    let after = repository.load_context(day(), "INR").unwrap();
    assert_ne!(before.context_hash, after.context_hash);
    assert_eq!(after.accounts.len(), 2);
    assert_eq!(after.base_currency, "USD");
    assert!(after
        .issues
        .iter()
        .any(|issue| issue.code == "SETTINGS_CHANGED_RELOAD"));
    assert_eq!(
        after.positions[0].quote.as_ref().unwrap().price.as_deref(),
        Some("999")
    );
}

#[test]
fn unsupported_short_and_foreign_currency_remain_visible() {
    let (_dir, access, repository) = fixture();
    access.connect_rusqlite().unwrap().execute("UPDATE holdings_snapshots SET positions=?1",[r#"{"fictional-share":{"quantity":"-2","averageCost":"1","currency":"EUR","contractMultiplier":"1"}}"#]).unwrap();
    let context = repository.load_context(day(), "INR").unwrap();
    assert_eq!(context.positions[0].quantity.as_deref(), Some("-2"));
    for code in [
        "SHORT_UNSUPPORTED",
        "UNSUPPORTED_CURRENCY",
        "QUOTE_CURRENCY_CONFLICT",
    ] {
        assert!(context.issues.iter().any(|v| v.code == code));
    }
}

#[test]
fn future_quotes_negative_cash_and_unsupported_instruments_remain_incomplete() {
    let (_dir, access, repository) = fixture();
    access.connect_rusqlite().unwrap().execute_batch("UPDATE quotes SET timestamp='2099-01-01T00:00:00Z'; UPDATE holdings_snapshots SET cash_balances='{\"INR\":\"-2\"}'; UPDATE assets SET instrument_type='OPTION' WHERE id='fictional-share';").unwrap();
    let context = repository.load_context(day(), "INR").unwrap();
    for code in [
        "QUOTE_PROVENANCE_INVALID",
        "NEGATIVE_CASH_UNSUPPORTED",
        "INSTRUMENT_UNSUPPORTED",
    ] {
        assert!(context.issues.iter().any(|issue| issue.code == code));
    }
    assert_eq!(context.accounts[0].cash["INR"].as_deref(), Some("-2"));
    assert_eq!(context.positions.len(), 1);
    assert!(!context.coverage_complete);
}

#[test]
fn excess_positions_are_rejected_instead_of_truncated() {
    let (_dir, access, repository) = fixture();
    let values: serde_json::Map<String, serde_json::Value> = (0..201)
        .map(|index| (format!("fictional-{index}"), json!({"quantity":"1"})))
        .collect();
    access
        .connect_rusqlite()
        .unwrap()
        .execute(
            "UPDATE holdings_snapshots SET positions=?1",
            [serde_json::to_string(&values).unwrap()],
        )
        .unwrap();
    assert!(repository
        .load_context(day(), "INR")
        .unwrap_err()
        .to_string()
        .contains("RESEARCH_POSITION_LIMIT"));
}

#[test]
fn oversized_snapshot_is_rejected_before_parsing_or_returning_partial_data() {
    let (_dir, access, repository) = fixture();
    let large = json!({"fictional": "x".repeat(2 * 1024 * 1024)}).to_string();
    access
        .connect_rusqlite()
        .unwrap()
        .execute("UPDATE holdings_snapshots SET positions=?1", [large])
        .unwrap();
    assert!(repository
        .load_context(day(), "INR")
        .unwrap_err()
        .to_string()
        .contains("RESEARCH_CONTEXT_SIZE_LIMIT"));
}
