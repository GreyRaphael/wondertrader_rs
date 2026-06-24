use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{KlineInterval, Symbol};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    pub mode: EngineMode,
    pub market: MarketConfig,
    pub storage: StorageConfig,
    #[serde(default)]
    pub execution: ExecutionConfig,
    #[serde(default)]
    pub strategies: StrategyConfigs,
    #[serde(default)]
    pub backtest: Option<BacktestRunConfig>,
    #[serde(default)]
    pub report: ReportRunConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineMode {
    Backtest,
    Live,
    DryRun,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketConfig {
    pub exchange: String,
    pub symbols: Vec<Symbol>,
    pub tick_stream: String,
    pub kline_intervals: Vec<KlineInterval>,
    #[serde(default = "default_kline_source")]
    pub kline_source: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageConfig {
    pub root: String,
    pub format: String,
    pub flush_rows: usize,
    pub flush_interval_secs: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecutionConfig {
    #[serde(default = "default_true")]
    pub dry_run: bool,
    #[serde(default = "default_account_id")]
    pub account_id: String,
    #[serde(default)]
    pub maker_fee_bps: f64,
    #[serde(default = "default_taker_fee_bps")]
    pub taker_fee_bps: f64,
    #[serde(default)]
    pub slippage_bps: f64,
    #[serde(default)]
    pub max_notional: Option<Decimal>,
    #[serde(default)]
    pub allowed_symbols: Vec<Symbol>,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            dry_run: true,
            account_id: default_account_id(),
            maker_fee_bps: 0.0,
            taker_fee_bps: default_taker_fee_bps(),
            slippage_bps: 0.0,
            max_notional: None,
            allowed_symbols: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StrategyConfigs {
    #[serde(default)]
    pub cta_ma_cross: Option<CtaMaCrossConfig>,
    #[serde(default)]
    pub sel_momentum: Option<SelMomentumConfig>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CtaMaCrossConfig {
    pub symbol: Symbol,
    pub interval: KlineInterval,
    pub fast: usize,
    pub slow: usize,
    pub target_qty: Decimal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SelMomentumConfig {
    pub symbols: Vec<Symbol>,
    pub interval: KlineInterval,
    pub schedule: String,
    pub lookback_bars: usize,
    pub long_count: usize,
    pub short_count: usize,
    pub notional_per_leg: Decimal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BacktestRunConfig {
    #[serde(default)]
    pub start_ts: Option<i64>,
    #[serde(default)]
    pub end_ts: Option<i64>,
    #[serde(default = "default_initial_balance")]
    pub initial_balance: Decimal,
    #[serde(default = "default_backtest_output_dir")]
    pub output_dir: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReportRunConfig {
    #[serde(default = "default_periods_per_year")]
    pub periods_per_year: f64,
    #[serde(default)]
    pub risk_free_rate: f64,
}

impl Default for ReportRunConfig {
    fn default() -> Self {
        Self {
            periods_per_year: default_periods_per_year(),
            risk_free_rate: 0.0,
        }
    }
}

impl AppConfig {
    pub fn summary(&self) -> String {
        format!(
            "mode={:?}, exchange={}, symbols={}, intervals={}, storage_root={}, dry_run={}",
            self.mode,
            self.market.exchange,
            self.market.symbols.len(),
            self.market.kline_intervals.len(),
            self.storage.root,
            self.execution.dry_run
        )
    }
}

fn default_true() -> bool {
    true
}

fn default_account_id() -> String {
    "default".to_owned()
}

fn default_taker_fee_bps() -> f64 {
    4.0
}

fn default_initial_balance() -> Decimal {
    Decimal::from(10_000)
}

fn default_backtest_output_dir() -> String {
    "data/backtests".to_owned()
}

fn default_periods_per_year() -> f64 {
    365.0
}

fn default_kline_source() -> String {
    "exchange".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_toml_config_with_intervals() {
        let config: AppConfig = toml::from_str(
            r#"
            mode = "backtest"

            [market]
            exchange = "binance_usdm"
            symbols = ["BTCUSDT", "ETHUSDT"]
            tick_stream = "agg_trade"
            kline_intervals = ["1m", "15m"]

            [storage]
            root = "data"
            format = "ipc_feather_v2"
            flush_rows = 10000
            flush_interval_secs = 5
            "#,
        )
        .unwrap();

        assert_eq!(config.mode, EngineMode::Backtest);
        assert_eq!(config.market.symbols[0].as_str(), "BTCUSDT");
        assert_eq!(
            config.market.kline_intervals,
            vec![KlineInterval::M1, KlineInterval::M15]
        );
        assert!(config.execution.dry_run);
        assert_eq!(config.market.kline_source, "exchange");
    }

    #[test]
    fn parses_strategy_sections() {
        let config: AppConfig = toml::from_str(
            r#"
            mode = "dry_run"

            [market]
            exchange = "binance_usdm"
            symbols = ["BTCUSDT"]
            tick_stream = "agg_trade"
            kline_intervals = ["15m"]

            [storage]
            root = "data"
            format = "ipc_feather_v2"
            flush_rows = 10000
            flush_interval_secs = 5

            [strategies.cta_ma_cross]
            symbol = "BTCUSDT"
            interval = "15m"
            fast = 5
            slow = 20
            target_qty = "0.001"
            "#,
        )
        .unwrap();

        let cta = config.strategies.cta_ma_cross.unwrap();
        assert_eq!(cta.symbol.as_str(), "BTCUSDT");
        assert_eq!(cta.interval, KlineInterval::M15);
        assert_eq!(cta.target_qty, Decimal::new(1, 3));
    }
}
