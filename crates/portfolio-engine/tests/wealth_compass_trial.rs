//! Isolated Wealth Compass compatibility probe. All amounts, assets and FX are fictional.
//! No production code, provider calls or persistence is changed.
#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]

mod support;

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use support::{Pipeline, Scenario};
use wealthfolio_portfolio_engine::model::{AccountId, AssetId, ValueStatus};

const FACTS: &str = r#"
id: WEALTH-COMPASS-INR-USD-TRIAL
policy: { base_currency: INR, timezone: Asia/Kolkata, as_of: 2025-01-03 }
accounts:
  - { id: india, currency: INR }
  - { id: us, currency: USD }
assets:
  - { id: fictional-in, quote_ccy: INR }
  - { id: fictional-us, quote_ccy: USD }
activities:
  - { id: in-deposit, account: india, type: DEPOSIT, date: '2025-01-02T10:00:00Z', amount: 10000, currency: INR }
  - { id: us-deposit, account: us, type: DEPOSIT, date: '2025-01-02T10:00:00Z', amount: 100, currency: USD }
  - { id: in-buy, account: india, type: BUY, date: '2025-01-02T11:00:00Z', asset: fictional-in, quantity: 2, unit_price: 100, amount: 202, fee: 2, currency: INR }
  - { id: us-buy, account: us, type: BUY, date: '2025-01-02T11:00:00Z', asset: fictional-us, quantity: 0.5, unit_price: 100, amount: 51, fee: 1, currency: USD }
quotes:
  - { asset: fictional-in, day: 2025-01-02, close: 100 }
  - { asset: fictional-in, day: 2025-01-03, close: 110 }
  - { asset: fictional-us, day: 2025-01-02, close: 100 }
  - { asset: fictional-us, day: 2025-01-03, close: 120 }
fx_rates:
  - { from: USD, to: INR, day: 2025-01-02, rate: 80 }
  - { from: USD, to: INR, day: 2025-01-03, rate: 82 }
"#;

#[test]
fn fictional_inr_usd_cash_fractional_shares_costs_and_dated_fx_reconcile() {
    let scenario: Scenario = serde_yaml::from_str(FACTS).unwrap();
    let pipeline = Pipeline::from_scenario(&scenario);
    let india = &pipeline.bundle.final_state.accounts[&AccountId::new("india")];
    let us = &pipeline.bundle.final_state.accounts[&AccountId::new("us")];
    assert_eq!(
        india.positions[&AssetId::new("fictional-in")].total_cost_basis,
        dec!(202)
    );
    assert_eq!(
        us.positions[&AssetId::new("fictional-us")].quantity,
        dec!(0.5)
    );
    assert_eq!(
        us.positions[&AssetId::new("fictional-us")].total_cost_basis,
        dec!(51)
    );
    let in_day = pipeline.series[&AccountId::new("india")]
        .days
        .last()
        .unwrap();
    let us_day = pipeline.series[&AccountId::new("us")].days.last().unwrap();
    assert_eq!(in_day.value_status, ValueStatus::Complete);
    assert_eq!(us_day.value_status, ValueStatus::Complete);
    assert_eq!(in_day.cash_balance, dec!(9798));
    assert_eq!(in_day.investment_market_value, dec!(220));
    assert_eq!(us_day.cash_balance, dec!(49));
    assert_eq!(us_day.investment_market_value, dec!(60));
    // Hand-worked: INR 10018 + USD 109 * 82 = INR 18956.
    assert_eq!(
        in_day.total_value_base + us_day.total_value_base,
        dec!(18956)
    );
    // Historical contributions: INR 10000 + USD 100 * 80 = INR 18000.
    assert_eq!(
        in_day.net_contribution_base + us_day.net_contribution_base,
        dec!(18000)
    );
    assert_eq!(
        in_day.total_value_base + us_day.total_value_base
            - in_day.net_contribution_base
            - us_day.net_contribution_base,
        dec!(956)
    );
}

#[test]
fn missing_usd_inr_rates_do_not_become_complete_base_values() {
    let mut scenario: Scenario = serde_yaml::from_str(FACTS).unwrap();
    scenario.fx_rates.clear();
    let pipeline = Pipeline::from_scenario(&scenario);
    let us_day = pipeline.series[&AccountId::new("us")].days.last().unwrap();
    assert_ne!(us_day.value_status, ValueStatus::Complete);
    assert_eq!(us_day.total_value, Decimal::from(109));
}
