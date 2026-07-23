//! Shared domain data contracts (implementation-plan.md §3.1–§3.3).
//!
//! Field names and serde casing are **normative** — they are mirrored exactly
//! by the TypeScript definitions in `src/lib/types.ts` and are what crosses the
//! Tauri IPC boundary. Do not rename fields or change casing.

use serde::{Deserialize, Serialize};

// -------------------------------------------------------------------------
// §3.1 Domain enums
// -------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    PackageCache,  // uv, npm, gradle, bun, pub, cargo registry
    EditorStorage, // Cursor/VS Code workspaceStorage, globalStorage, bloated state.vscdb
    BuildArtifact, // node_modules, target, build, .next, dist, .dart_tool
    Temp,          // %LOCALAPPDATA%\Temp, etc.
    Model,         // LM Studio / downloaded model files
    Browser,       // playwright browsers, etc.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SafetyTier {
    Safe,    // regenerates automatically; pre-selected by default (green)
    Review,  // regenerable but user should confirm (amber)
    Caution, // user data / not pure cache; never pre-selected (red)
}

// -------------------------------------------------------------------------
// §3.2 Findings
// -------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupItem {
    pub id: String,      // stable, unique (e.g. hash of path)
    pub rule_id: String, // which rule produced it (e.g. "uv-cache")
    pub label: String,   // human label ("Python uv cache")
    pub category: Category,
    pub tier: SafetyTier,
    pub path: String, // normalized, forward-slash display path
    pub size_bytes: u64,
    pub regenerates: bool,
    pub last_modified_secs: Option<u64>, // unix secs; for staleness sorting
    pub note: String,                    // why it's safe / what happens if removed
    pub selected_by_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryGroup {
    pub category: Category,
    pub label: String,
    pub total_bytes: u64,
    pub items: Vec<CleanupItem>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub total_reclaimable_bytes: u64,
    pub groups: Vec<CategoryGroup>,
    pub scanned_roots: Vec<String>,
    pub warnings: Vec<String>,
}

// -------------------------------------------------------------------------
// §3.3 Progress event (streamed via `Channel<ScanEvent>`)
// -------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum ScanEvent {
    Started {
        roots: Vec<String>,
    },
    Progress {
        current_path: String,
        found_bytes: u64,
        rules_checked: u32,
        rules_total: u32,
    },
    Found {
        item: CleanupItem,
    },
    Finished {
        total_reclaimable_bytes: u64,
        item_count: u32,
    },
}
