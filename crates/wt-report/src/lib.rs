//! Backtest reporting crate.
//!
//! Phase 0 defines the crate boundary for performance metrics and report output.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MetricsSummary {
    pub total_return: f64,
    pub sharpe_ratio: Option<f64>,
    pub max_drawdown: f64,
    pub win_rate: Option<f64>,
    pub profit_factor: Option<f64>,
}
