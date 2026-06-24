//! Execution crate.
//!
//! Phase 0 defines the crate boundary for target-position execution and future
//! direct-order HFT/UFT execution.

pub use wt_core::{Order, TargetPosition, Trade};

pub const DEFAULT_EXECUTOR_ID: &str = "default";
