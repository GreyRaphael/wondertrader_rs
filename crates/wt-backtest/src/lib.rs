//! Backtesting crate.
//!
//! Phase 0 defines the crate boundary for historical event replay, matching,
//! and portfolio accounting.

pub use wt_core::{MarketEvent, Order, Position, Trade};

pub const BACKTEST_CLOCK: &str = "event_time";
