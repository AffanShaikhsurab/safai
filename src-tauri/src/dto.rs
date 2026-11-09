//! Deletion & info DTOs (implementation-plan.md §3.4).
//!
//! Scan/delete DTOs live in `safai-engine` so the CLI shares them. This module
//! re-exports those and adds Tauri-only info types. Field names and serde casing
//! are **normative** — they mirror `src/lib/types.ts` exactly.

use serde::Serialize;

pub use safai_engine::{DeleteEvent, DeletePlan, DeleteReport, DriveInfo};
pub use safai_rules::SafetyTier;

/// One fixed volume, as shown on the setup screen and in the drive filter.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveEntry {
    /// Display mount, e.g. `"C:"`.
    pub mount: String,
    /// Root path safe to pass to scan commands, e.g. `"C:/"`.
    pub root: String,
    pub label: String,
    pub file_system: String,
    pub free_bytes: u64,
    pub total_bytes: u64,
    pub used_percent: Option<f64>,
    pub system: bool,
    pub readable: bool,
    pub kind: String,
}

impl From<crate::winsys::VolumeDetail> for DriveEntry {
    fn from(volume: crate::winsys::VolumeDetail) -> Self {
        let used_percent = if volume.total_bytes == 0 {
            None
        } else {
            Some(
                (volume.total_bytes.saturating_sub(volume.free_bytes) as f64
                    / volume.total_bytes as f64)
                    * 100.0,
            )
        };
        DriveEntry {
            mount: volume.mount,
            root: volume.root,
            label: volume.label,
            file_system: volume.file_system,
            free_bytes: volume.free_bytes,
            total_bytes: volume.total_bytes,
            used_percent,
            system: volume.system,
            readable: volume.readable,
            kind: volume.kind,
        }
    }
}

/// Per-drive usage in the automation status payload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveUsage {
    pub mount: String,
    pub free_bytes: u64,
    pub total_bytes: u64,
    pub used_percent: Option<f64>,
}

/// Whether a given dev tool is installed (UI chips + rule gating).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub id: String,
    pub label: String,
    pub detected: bool,
}

/// One entry of the cleanup rule table, exposed so the Automation screen can
/// offer a per-rule autopilot opt-in rather than a category blanket.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleInfo {
    pub id: String,
    pub label: String,
    pub category: safai_rules::Category,
    pub tier: SafetyTier,
    pub regenerates: bool,
    pub note: String,
    /// Discovered by directory name (`node_modules`, `target`, …) rather than a
    /// fixed known path.
    pub pattern_based: bool,
}
