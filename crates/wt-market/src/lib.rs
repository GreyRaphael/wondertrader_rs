//! Market data gateway crate.
//!
//! Phase 0 only defines the crate boundary. Binance REST/WebSocket market data
//! adapters will be implemented in later phases.

pub use wt_core::{BookTicker, Kline, KlineInterval, MarketEvent, Tick, TickSource};

pub const CRATE_PHASE: &str = "phase-0-skeleton";
