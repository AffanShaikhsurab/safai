//! Deletion & info DTOs (implementation-plan.md §3.4).
//!
//! These are the types the backend sends to the frontend that are *not*
//! already defined in `safai-rules::model`. Field names and serde casing are
//! **normative** — they mirror `src/lib/types.ts` exactly. All structs use
//! `#[serde(rename_all = "camelCase")]`; the streamed `DeleteEvent` enum uses
//! the tagged `{ event, data }` shape (like `ScanEvent`).

use serde::Serialize;

// Re-export `SafetyTier` from the rules crate so the DTOs below (and any
// downstream consumer of `dto`) share the one canonical definition.
pub use safai_rules::SafetyTier;

/// One entry in a dry-run deletion plan.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletePlanItem {
    pub id: String,
    pub path: String,
    pub size_bytes: u64,
    pub tier: SafetyTier,
    pub allowed: bool,
    pub reason: Option<String>,
}

/// The full dry-run plan returned by `preview_delete`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletePlan {
    pub items: Vec<DeletePlanItem>,
    pub total_bytes: u64,
    pub blocked_count: u32,
}

/// Progress events streamed via `Channel<DeleteEvent>` during `delete`.
#[derive(Debug, Clone, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum DeleteEvent {
    Started {
        total: u32,
    },
    Deleted {
        id: String,
        path: String,
        size_bytes: u64,
    },
    Skipped {
        id: String,
        path: String,
        reason: String,
    },
    Finished {
        deleted: u32,
        reclaimed_bytes: u64,
        skipped: u32,
    },
}

/// Summary returned by `delete` once all items are processed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteReport {
    pub deleted: u32,
    pub reclaimed_bytes: u64,
    pub skipped: Vec<String>,
}

/// Free/total space for the drive containing a path (header gauge).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveInfo {
    pub mount: String,
    pub free_bytes: u64,
    pub total_bytes: u64,
}

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
