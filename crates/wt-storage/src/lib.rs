//! Arrow IPC / Feather v2 storage crate.
//!
//! The first storage milestone focuses on stable schemas and small batch
//! read/write helpers. Later phases will add partitioned live writers and more
//! scan optimizations.

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

use polars::prelude::*;
use rust_decimal::Decimal;
use thiserror::Error;
use wt_core::{Kline, KlineInterval, Tick, TickSource};

pub const STORAGE_FORMAT: &str = "ipc_feather_v2";

pub type StorageResult<T> = Result<T, StorageError>;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("polars error: {0}")]
    Polars(#[from] PolarsError),

    #[error("invalid path: {0}")]
    InvalidPath(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataKind {
    Ticks,
    Klines,
}

impl DataKind {
    pub fn as_dir(self) -> &'static str {
        match self {
            Self::Ticks => "ticks",
            Self::Klines => "klines",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartitionPath {
    pub root: PathBuf,
    pub data_kind: DataKind,
    pub date: String,
    pub symbol: String,
    pub interval: Option<KlineInterval>,
    pub part: String,
}

impl PartitionPath {
    pub fn new(
        root: impl Into<PathBuf>,
        data_kind: DataKind,
        date: impl Into<String>,
        symbol: impl Into<String>,
        interval: Option<KlineInterval>,
        part: impl Into<String>,
    ) -> Self {
        Self {
            root: root.into(),
            data_kind,
            date: date.into(),
            symbol: symbol.into(),
            interval,
            part: part.into(),
        }
    }

    pub fn to_path(&self) -> PathBuf {
        let mut path = self.root.join(self.data_kind.as_dir());
        if let Some(interval) = self.interval {
            path = path.join(format!("interval={interval}"));
        }
        path.join(format!("date={}", self.date))
            .join(format!("symbol={}", self.symbol))
            .join(format!("{}.feather", self.part))
    }
}

pub mod schema {
    pub mod tick {
        pub const TS_EVENT: &str = "ts_event";
        pub const TS_RECV: &str = "ts_recv";
        pub const SYMBOL: &str = "symbol";
        pub const SOURCE: &str = "source";
        pub const TRADE_ID: &str = "trade_id";
        pub const PRICE: &str = "price";
        pub const QTY: &str = "qty";
        pub const SIDE: &str = "side";
        pub const BID_PRICE: &str = "bid_price";
        pub const BID_QTY: &str = "bid_qty";
        pub const ASK_PRICE: &str = "ask_price";
        pub const ASK_QTY: &str = "ask_qty";
        pub const IS_RECOVERED: &str = "is_recovered";
        pub const RAW_SEQ: &str = "raw_seq";

        pub const COLUMNS: &[&str] = &[
            TS_EVENT,
            TS_RECV,
            SYMBOL,
            SOURCE,
            TRADE_ID,
            PRICE,
            QTY,
            SIDE,
            BID_PRICE,
            BID_QTY,
            ASK_PRICE,
            ASK_QTY,
            IS_RECOVERED,
            RAW_SEQ,
        ];
    }

    pub mod kline {
        pub const OPEN_TIME: &str = "open_time";
        pub const CLOSE_TIME: &str = "close_time";
        pub const SYMBOL: &str = "symbol";
        pub const INTERVAL: &str = "interval";
        pub const OPEN: &str = "open";
        pub const HIGH: &str = "high";
        pub const LOW: &str = "low";
        pub const CLOSE: &str = "close";
        pub const VOLUME: &str = "volume";
        pub const QUOTE_VOLUME: &str = "quote_volume";
        pub const TRADE_COUNT: &str = "trade_count";
        pub const TAKER_BUY_VOLUME: &str = "taker_buy_volume";
        pub const TAKER_BUY_QUOTE_VOLUME: &str = "taker_buy_quote_volume";
        pub const IS_FINAL: &str = "is_final";
        pub const SOURCE: &str = "source";

        pub const COLUMNS: &[&str] = &[
            OPEN_TIME,
            CLOSE_TIME,
            SYMBOL,
            INTERVAL,
            OPEN,
            HIGH,
            LOW,
            CLOSE,
            VOLUME,
            QUOTE_VOLUME,
            TRADE_COUNT,
            TAKER_BUY_VOLUME,
            TAKER_BUY_QUOTE_VOLUME,
            IS_FINAL,
            SOURCE,
        ];
    }
}

pub trait FeatherBatch: Sized {
    fn to_dataframe(items: &[Self]) -> PolarsResult<DataFrame>;
    fn write_feather(path: impl AsRef<Path>, items: &[Self]) -> StorageResult<()> {
        write_ipc(path, &mut Self::to_dataframe(items)?)
    }
}

impl FeatherBatch for Tick {
    fn to_dataframe(items: &[Self]) -> PolarsResult<DataFrame> {
        ticks_to_dataframe(items)
    }
}

impl FeatherBatch for Kline {
    fn to_dataframe(items: &[Self]) -> PolarsResult<DataFrame> {
        klines_to_dataframe(items)
    }
}

pub fn ticks_to_dataframe(ticks: &[Tick]) -> PolarsResult<DataFrame> {
    df!(
        schema::tick::TS_EVENT => ticks.iter().map(|tick| tick.ts_event).collect::<Vec<_>>(),
        schema::tick::TS_RECV => ticks.iter().map(|tick| tick.ts_recv).collect::<Vec<_>>(),
        schema::tick::SYMBOL => ticks.iter().map(|tick| tick.symbol.as_str()).collect::<Vec<_>>(),
        schema::tick::SOURCE => ticks.iter().map(|tick| tick_source_to_str(tick.source)).collect::<Vec<_>>(),
        schema::tick::TRADE_ID => ticks.iter().map(|tick| tick.trade_id).collect::<Vec<_>>(),
        schema::tick::PRICE => ticks.iter().map(|tick| decimal_to_f64(tick.price)).collect::<Vec<_>>(),
        schema::tick::QTY => ticks.iter().map(|tick| decimal_to_f64(tick.qty)).collect::<Vec<_>>(),
        schema::tick::SIDE => ticks.iter().map(|tick| tick.side.as_deref()).collect::<Vec<_>>(),
        schema::tick::BID_PRICE => ticks.iter().map(|tick| tick.bid_price.map(decimal_to_f64)).collect::<Vec<_>>(),
        schema::tick::BID_QTY => ticks.iter().map(|tick| tick.bid_qty.map(decimal_to_f64)).collect::<Vec<_>>(),
        schema::tick::ASK_PRICE => ticks.iter().map(|tick| tick.ask_price.map(decimal_to_f64)).collect::<Vec<_>>(),
        schema::tick::ASK_QTY => ticks.iter().map(|tick| tick.ask_qty.map(decimal_to_f64)).collect::<Vec<_>>(),
        schema::tick::IS_RECOVERED => ticks.iter().map(|tick| tick.is_recovered).collect::<Vec<_>>(),
        schema::tick::RAW_SEQ => ticks.iter().map(|tick| tick.raw_seq).collect::<Vec<_>>(),
    )
}

pub fn klines_to_dataframe(klines: &[Kline]) -> PolarsResult<DataFrame> {
    df!(
        schema::kline::OPEN_TIME => klines.iter().map(|kline| kline.open_time).collect::<Vec<_>>(),
        schema::kline::CLOSE_TIME => klines.iter().map(|kline| kline.close_time).collect::<Vec<_>>(),
        schema::kline::SYMBOL => klines.iter().map(|kline| kline.symbol.as_str()).collect::<Vec<_>>(),
        schema::kline::INTERVAL => klines.iter().map(|kline| kline.interval.as_str()).collect::<Vec<_>>(),
        schema::kline::OPEN => klines.iter().map(|kline| decimal_to_f64(kline.open)).collect::<Vec<_>>(),
        schema::kline::HIGH => klines.iter().map(|kline| decimal_to_f64(kline.high)).collect::<Vec<_>>(),
        schema::kline::LOW => klines.iter().map(|kline| decimal_to_f64(kline.low)).collect::<Vec<_>>(),
        schema::kline::CLOSE => klines.iter().map(|kline| decimal_to_f64(kline.close)).collect::<Vec<_>>(),
        schema::kline::VOLUME => klines.iter().map(|kline| decimal_to_f64(kline.volume)).collect::<Vec<_>>(),
        schema::kline::QUOTE_VOLUME => klines.iter().map(|kline| decimal_to_f64(kline.quote_volume)).collect::<Vec<_>>(),
        schema::kline::TRADE_COUNT => klines.iter().map(|kline| kline.trade_count).collect::<Vec<_>>(),
        schema::kline::TAKER_BUY_VOLUME => klines.iter().map(|kline| decimal_to_f64(kline.taker_buy_volume)).collect::<Vec<_>>(),
        schema::kline::TAKER_BUY_QUOTE_VOLUME => klines.iter().map(|kline| decimal_to_f64(kline.taker_buy_quote_volume)).collect::<Vec<_>>(),
        schema::kline::IS_FINAL => klines.iter().map(|kline| kline.is_final).collect::<Vec<_>>(),
        schema::kline::SOURCE => klines.iter().map(|kline| kline.source.as_str()).collect::<Vec<_>>(),
    )
}

pub fn write_ipc(path: impl AsRef<Path>, dataframe: &mut DataFrame) -> StorageResult<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(path)?;
    IpcWriter::new(file).finish(dataframe)?;
    Ok(())
}

pub fn read_ipc(path: impl AsRef<Path>) -> StorageResult<DataFrame> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(StorageError::InvalidPath(path.to_path_buf()));
    }
    let file = File::open(path)?;
    Ok(IpcReader::new(file).finish()?)
}

pub fn scan_ipc(path: impl AsRef<Path>) -> StorageResult<LazyFrame> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(StorageError::InvalidPath(path.to_path_buf()));
    }
    let pl_path = PlRefPath::try_from_path(path)?;
    Ok(LazyFrame::scan_ipc(
        pl_path,
        IpcScanOptions::default(),
        UnifiedScanArgs::default(),
    )?)
}

pub fn scan_klines(
    path: impl AsRef<Path>,
    symbols: &[&str],
    interval: Option<KlineInterval>,
    start_open_time: Option<i64>,
    end_open_time: Option<i64>,
) -> StorageResult<LazyFrame> {
    let mut lf = scan_ipc(path)?;

    if !symbols.is_empty() {
        let allowed = Series::new("symbols".into(), symbols.to_vec());
        lf = lf.filter(col(schema::kline::SYMBOL).is_in(lit(allowed), false));
    }

    if let Some(interval) = interval {
        lf = lf.filter(col(schema::kline::INTERVAL).eq(lit(interval.as_str())));
    }

    if let Some(start) = start_open_time {
        lf = lf.filter(col(schema::kline::OPEN_TIME).gt_eq(lit(start)));
    }

    if let Some(end) = end_open_time {
        lf = lf.filter(col(schema::kline::OPEN_TIME).lt(lit(end)));
    }

    Ok(lf)
}

pub fn scan_ticks(
    path: impl AsRef<Path>,
    symbols: &[&str],
    start_ts_event: Option<i64>,
    end_ts_event: Option<i64>,
) -> StorageResult<LazyFrame> {
    let mut lf = scan_ipc(path)?;

    if !symbols.is_empty() {
        let allowed = Series::new("symbols".into(), symbols.to_vec());
        lf = lf.filter(col(schema::tick::SYMBOL).is_in(lit(allowed), false));
    }

    if let Some(start) = start_ts_event {
        lf = lf.filter(col(schema::tick::TS_EVENT).gt_eq(lit(start)));
    }

    if let Some(end) = end_ts_event {
        lf = lf.filter(col(schema::tick::TS_EVENT).lt(lit(end)));
    }

    Ok(lf)
}

fn tick_source_to_str(source: TickSource) -> &'static str {
    match source {
        TickSource::Trade => "trade",
        TickSource::AggTrade => "agg_trade",
        TickSource::BookTicker => "book_ticker",
        TickSource::RestRecovered => "rest_recovered",
    }
}

fn decimal_to_f64(value: Decimal) -> f64 {
    // Decimal to f64 conversion is acceptable for Phase 1 storage/analysis
    // columns. Order/execution code keeps Decimal in domain objects.
    value.try_into().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rust_decimal::Decimal;
    use wt_core::{Symbol, Tick};

    use super::*;

    #[test]
    fn builds_kline_partition_path() {
        let path = PartitionPath::new(
            "data/raw",
            DataKind::Klines,
            "2026-06-24",
            "BTCUSDT",
            Some(KlineInterval::M15),
            "part-000",
        )
        .to_path();

        assert_eq!(
            path,
            PathBuf::from(
                "data/raw/klines/interval=15m/date=2026-06-24/symbol=BTCUSDT/part-000.feather"
            )
        );
    }

    #[test]
    fn tick_dataframe_has_stable_schema() {
        let ticks = vec![Tick {
            ts_event: 1,
            ts_recv: 2,
            symbol: Symbol::from("BTCUSDT"),
            source: TickSource::AggTrade,
            trade_id: Some(42),
            price: Decimal::from_str("65000.5").unwrap(),
            qty: Decimal::from_str("0.01").unwrap(),
            side: Some("buy".to_owned()),
            bid_price: None,
            bid_qty: None,
            ask_price: None,
            ask_qty: None,
            is_recovered: false,
            raw_seq: Some(42),
        }];

        let df = ticks_to_dataframe(&ticks).unwrap();
        assert_eq!(column_names(&df), schema::tick::COLUMNS);
        assert_eq!(df.height(), 1);
    }

    #[test]
    fn kline_dataframe_has_stable_schema() {
        let klines = vec![Kline {
            open_time: 1,
            close_time: 60_000_000_000,
            symbol: Symbol::from("ETHUSDT"),
            interval: KlineInterval::M1,
            open: Decimal::from_str("3000").unwrap(),
            high: Decimal::from_str("3010").unwrap(),
            low: Decimal::from_str("2990").unwrap(),
            close: Decimal::from_str("3005").unwrap(),
            volume: Decimal::from_str("12.5").unwrap(),
            quote_volume: Decimal::from_str("37500").unwrap(),
            trade_count: 100,
            taker_buy_volume: Decimal::from_str("6.2").unwrap(),
            taker_buy_quote_volume: Decimal::from_str("18600").unwrap(),
            is_final: true,
            source: "rest_kline".to_owned(),
        }];

        let df = klines_to_dataframe(&klines).unwrap();
        assert_eq!(column_names(&df), schema::kline::COLUMNS);
        assert_eq!(df.height(), 1);
    }

    fn column_names(df: &DataFrame) -> Vec<&str> {
        df.get_column_names()
            .into_iter()
            .map(|name| name.as_str())
            .collect()
    }

    #[test]
    fn writes_and_reads_ipc() {
        let path =
            std::env::temp_dir().join(format!("wt-storage-test-{}.feather", std::process::id()));
        let klines = vec![Kline {
            open_time: 1,
            close_time: 60_000_000_000,
            symbol: Symbol::from("BTCUSDT"),
            interval: KlineInterval::M1,
            open: Decimal::ONE,
            high: Decimal::ONE,
            low: Decimal::ONE,
            close: Decimal::ONE,
            volume: Decimal::ONE,
            quote_volume: Decimal::ONE,
            trade_count: 1,
            taker_buy_volume: Decimal::ONE,
            taker_buy_quote_volume: Decimal::ONE,
            is_final: true,
            source: "unit_test".to_owned(),
        }];

        Kline::write_feather(&path, &klines).unwrap();
        let df = read_ipc(&path).unwrap();
        assert_eq!(df.height(), 1);
        let _ = std::fs::remove_file(path);
    }
}
