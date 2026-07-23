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

/// Whether a given dev tool is installed (UI chips + rule gating).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub id: String,
    pub label: String,
    pub detected: bool,
}
