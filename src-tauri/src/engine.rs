//! Thin re-exports of the shared execution cores in `safai-engine`.
//!
//! Kept so existing `crate::engine::…` call sites in commands and the
//! scheduler stay stable after the extraction.

pub use safai_engine::{
    build_scan_config, delete_blocking, drive_mount, normalize_slashes, preview_delete,
    scan_blocking,
};
