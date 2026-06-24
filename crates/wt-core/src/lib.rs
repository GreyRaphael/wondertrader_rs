//! Core domain types shared by the WonderTrader Rust workspace.
//!
//! This crate intentionally avoids exchange-specific behavior. Binance-specific
//! normalization, storage, engines, execution, and backtesting live in sibling
//! crates and depend on these stable types.

pub mod config;
pub mod error;
pub mod event;
pub mod logging;
pub mod market;
pub mod order;
pub mod time;
pub mod types;

pub use config::{AppConfig, EngineMode, MarketConfig, StorageConfig};
pub use error::{Result, WtCoreError};
pub use event::{MarketEvent, ScheduleEvent, SessionEvent, SessionEventKind};
pub use market::{BookTicker, Kline, KlineInterval, Tick, TickSource};
pub use order::{
    AccountId, ClientOrderId, Order, OrderId, OrderStatus, OrderType, Position, PositionSide, Side,
    TargetPosition, TimeInForce, Trade,
};
pub use time::{TsNs, system_time_to_ts_ns, unix_ts_ns_now};
pub use types::{ExchangeSymbol, Price, Qty, StrategyId, Symbol};
