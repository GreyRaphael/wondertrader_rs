use serde::{Deserialize, Serialize};

use crate::{KlineInterval, Symbol};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    pub mode: EngineMode,
    pub market: MarketConfig,
    pub storage: StorageConfig,
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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageConfig {
    pub root: String,
    pub format: String,
    pub flush_rows: usize,
    pub flush_interval_secs: u64,
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
    }
}
