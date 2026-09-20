//! Shared Safai scan/delete execution cores.
//!
//! Used by the Tauri desktop app, the background scheduler, and `safai-cli`.
//! All callers share the same guardrails and deletion path.

pub mod delete;
pub mod delete_engine;
pub mod disk;
pub mod engine;

pub use delete::{DeleteEvent, DeletePlan, DeletePlanItem, DeleteReport};
pub use disk::DriveInfo;
pub use engine::{
    build_scan_config, delete_blocking, drive_mount, is_within_allowed, normalize_slashes,
    preview_delete, scan_blocking,
};
