//! Backtest reporting crate.
//!
//! Phase 6 computes core performance metrics from backtest equity and trade
//! outputs. The API is intentionally data-oriented so it can later write JSON,
//! Markdown, or HTML reports without changing the metric engine.

use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde::{Deserialize, Serialize};
use wt_backtest::EquityPoint;
use wt_core::Trade;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub total_return: f64,
    pub annualized_return: Option<f64>,
    pub annualized_volatility: Option<f64>,
    pub sharpe_ratio: Option<f64>,
    pub max_drawdown: f64,
    pub calmar_ratio: Option<f64>,
    pub win_rate: Option<f64>,
    pub profit_factor: Option<f64>,
    pub payoff_ratio: Option<f64>,
    pub avg_trade_pnl: Option<f64>,
    pub trade_count: usize,
    pub fee_total: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReportConfig {
    pub periods_per_year: f64,
    pub risk_free_rate: f64,
}

impl Default for ReportConfig {
    fn default() -> Self {
        // Crypto trades 24/7. For daily equity use 365; for minute/bar equity
        // callers should pass the appropriate annualization factor.
        Self {
            periods_per_year: 365.0,
            risk_free_rate: 0.0,
        }
    }
}

pub fn compute_metrics(
    equity: &[EquityPoint],
    trades: &[Trade],
    config: &ReportConfig,
) -> MetricsSummary {
    if equity.is_empty() {
        return MetricsSummary::default();
    }

    let first_equity = decimal_to_f64(equity.first().expect("checked non-empty").equity);
    let last_equity = decimal_to_f64(equity.last().expect("checked non-empty").equity);
    let total_return = if first_equity == 0.0 {
        0.0
    } else {
        last_equity / first_equity - 1.0
    };

    let returns = period_returns(equity);
    let annualized_return = if equity.len() > 1 {
        let years = (equity.len() - 1) as f64 / config.periods_per_year;
        if years > 0.0 && first_equity > 0.0 && last_equity > 0.0 {
            Some((last_equity / first_equity).powf(1.0 / years) - 1.0)
        } else {
            None
        }
    } else {
        None
    };
    let annualized_volatility = stddev(&returns).map(|vol| vol * config.periods_per_year.sqrt());
    let sharpe_ratio = match (annualized_return, annualized_volatility) {
        (Some(ret), Some(vol)) if vol > 0.0 => Some((ret - config.risk_free_rate) / vol),
        _ => None,
    };

    let max_drawdown = max_drawdown(equity);
    let calmar_ratio = annualized_return.and_then(|ret| {
        if max_drawdown.abs() > f64::EPSILON {
            Some(ret / max_drawdown.abs())
        } else {
            None
        }
    });

    let trade_pnls = trades
        .iter()
        .map(|trade| decimal_to_f64(trade.realized_pnl))
        .collect::<Vec<_>>();
    let winners = trade_pnls
        .iter()
        .copied()
        .filter(|pnl| *pnl > 0.0)
        .collect::<Vec<_>>();
    let losers = trade_pnls
        .iter()
        .copied()
        .filter(|pnl| *pnl < 0.0)
        .collect::<Vec<_>>();
    let win_rate = if trade_pnls.is_empty() {
        None
    } else {
        Some(winners.len() as f64 / trade_pnls.len() as f64)
    };
    let gross_profit = winners.iter().sum::<f64>();
    let gross_loss_abs = losers.iter().map(|pnl| pnl.abs()).sum::<f64>();
    let profit_factor = if gross_loss_abs > 0.0 {
        Some(gross_profit / gross_loss_abs)
    } else {
        None
    };
    let avg_win = average(&winners);
    let avg_loss_abs = average_abs(&losers);
    let payoff_ratio = match (avg_win, avg_loss_abs) {
        (Some(win), Some(loss)) if loss > 0.0 => Some(win / loss),
        _ => None,
    };
    let avg_trade_pnl = average(&trade_pnls);
    let fee_total = trades
        .iter()
        .map(|trade| decimal_to_f64(trade.fee))
        .sum::<f64>();

    MetricsSummary {
        total_return,
        annualized_return,
        annualized_volatility,
        sharpe_ratio,
        max_drawdown,
        calmar_ratio,
        win_rate,
        profit_factor,
        payoff_ratio,
        avg_trade_pnl,
        trade_count: trades.len(),
        fee_total,
    }
}

fn period_returns(equity: &[EquityPoint]) -> Vec<f64> {
    equity
        .windows(2)
        .filter_map(|window| {
            let prev = decimal_to_f64(window[0].equity);
            let next = decimal_to_f64(window[1].equity);
            if prev == 0.0 {
                None
            } else {
                Some(next / prev - 1.0)
            }
        })
        .collect()
}

fn max_drawdown(equity: &[EquityPoint]) -> f64 {
    let mut peak = f64::NEG_INFINITY;
    let mut max_dd = 0.0;
    for point in equity {
        let value = decimal_to_f64(point.equity);
        peak = peak.max(value);
        if peak > 0.0 {
            let dd = value / peak - 1.0;
            if dd < max_dd {
                max_dd = dd;
            }
        }
    }
    max_dd
}

fn average(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

fn average_abs(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().map(|value| value.abs()).sum::<f64>() / values.len() as f64)
    }
}

fn stddev(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let mean = average(values).expect("checked len");
    let variance = values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / (values.len() - 1) as f64;
    Some(variance.sqrt())
}

fn decimal_to_f64(value: Decimal) -> f64 {
    value.to_f64().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use wt_core::{AccountId, OrderId, PositionSide, Side, Symbol};

    use super::*;

    #[test]
    fn computes_equity_and_trade_metrics() {
        let equity = vec![
            equity_point(0, "10000"),
            equity_point(1, "11000"),
            equity_point(2, "10500"),
            equity_point(3, "12000"),
        ];
        let trades = vec![trade("100", "1"), trade("-50", "1"), trade("25", "1")];

        let metrics = compute_metrics(
            &equity,
            &trades,
            &ReportConfig {
                periods_per_year: 365.0,
                risk_free_rate: 0.0,
            },
        );

        assert!((metrics.total_return - 0.2).abs() < 1e-12);
        assert_eq!(metrics.trade_count, 3);
        assert_eq!(metrics.win_rate, Some(2.0 / 3.0));
        assert_eq!(metrics.profit_factor, Some(2.5));
        assert_eq!(metrics.payoff_ratio, Some(1.25));
        assert_eq!(metrics.avg_trade_pnl, Some(25.0));
        assert_eq!(metrics.fee_total, 3.0);
        assert!(metrics.max_drawdown < 0.0);
        assert!(metrics.annualized_return.is_some());
    }

    #[test]
    fn handles_empty_inputs() {
        assert_eq!(
            compute_metrics(&[], &[], &ReportConfig::default()),
            MetricsSummary::default()
        );
    }

    fn equity_point(ts: i64, equity: &str) -> EquityPoint {
        let value = Decimal::from_str(equity).unwrap();
        EquityPoint {
            ts,
            balance: value,
            equity: value,
            realized_pnl: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
            fee_total: Decimal::ZERO,
        }
    }

    fn trade(pnl: &str, fee: &str) -> Trade {
        Trade {
            ts_trade: 0,
            strategy_id: Some("unit".to_owned()),
            account_id: AccountId("backtest".to_owned()),
            symbol: Symbol::from("BTCUSDT"),
            side: Side::Buy,
            position_side: PositionSide::Both,
            price: Decimal::ONE,
            qty: Decimal::ONE,
            fee: Decimal::from_str(fee).unwrap(),
            fee_asset: "USDT".to_owned(),
            realized_pnl: Decimal::from_str(pnl).unwrap(),
            order_id: Some(OrderId("1".to_owned())),
        }
    }
}
