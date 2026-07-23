//! # safai-rules
//!
//! The Safai cleanup detection engine (implementation-plan.md §5 WS2).
//!
//! This crate is deliberately free of any Tauri dependency: it exposes the
//! domain data contracts (§3), a declarative rule table, dev-tool detection and
//! env-path expansion, and a scan driver ([`run_scan`]) that streams progress
//! through a plain callback. WS3 adapts that callback to a Tauri `Channel`.

pub mod detect;
pub mod model;
pub mod rules;
pub mod scan;

// Re-export the model data contracts (§3.1–§3.3).
pub use model::{
    Category, CategoryGroup, CleanupItem, SafetyTier, ScanEvent, ScanReport,
};

// Re-export the primary entry points used by WS3.
pub use detect::{default_roots, detect_tools};
pub use scan::{label_for, run_scan, ScanConfig};
