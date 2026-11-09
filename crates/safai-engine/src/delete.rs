//! Deletion DTOs shared by Tauri IPC and the CLI.
//!
//! Field names and serde casing mirror `src/lib/types.ts`.

use safai_rules::SafetyTier;
use serde::{Deserialize, Serialize};

/// One entry in a dry-run deletion plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletePlanItem {
    pub id: String,
    pub path: String,
    pub size_bytes: u64,
    pub tier: SafetyTier,
    pub allowed: bool,
    pub reason: Option<String>,
}

/// The full dry-run plan returned by preview.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletePlan {
    pub items: Vec<DeletePlanItem>,
    pub total_bytes: u64,
    pub blocked_count: u32,
}

/// Progress events streamed during delete.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Summary returned once all items are processed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteReport {
    pub deleted: u32,
    pub reclaimed_bytes: u64,
    pub skipped: Vec<String>,
}
