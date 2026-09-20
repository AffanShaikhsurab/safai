//! Deletion & info DTOs (implementation-plan.md §3.4).
//!
//! Scan/delete DTOs live in `safai-engine` so the CLI shares them. This module
//! re-exports those and adds Tauri-only info types. Field names and serde casing
//! are **normative** — they mirror `src/lib/types.ts` exactly.

use serde::Serialize;

pub use safai_engine::{DeleteEvent, DeletePlan, DeleteReport, DriveInfo};
pub use safai_rules::SafetyTier;

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
