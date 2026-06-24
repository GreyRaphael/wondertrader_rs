//! Strategy engine crate.
//!
//! Phase 0 defines the crate boundary for CTA, SEL, HFT, and UFT engines.

pub use wt_core::{MarketEvent, TargetPosition};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineKind {
    Cta,
    Sel,
    Hft,
    Uft,
}
