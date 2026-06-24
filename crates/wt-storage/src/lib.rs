//! Arrow IPC / Feather v2 storage crate.
//!
//! Phase 0 only defines the crate boundary. Polars IPC schemas and readers will
//! be implemented in Phase 1.

pub use wt_core::{Kline, Tick};

pub const STORAGE_FORMAT: &str = "ipc_feather_v2";
