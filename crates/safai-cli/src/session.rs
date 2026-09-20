//! File-backed scan session so preview/delete work across CLI invocations.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use safai_engine::DeletePlan;
use safai_rules::{CleanupItem, ScanReport};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SESSION_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSession {
    pub version: u32,
    pub created_at_secs: u64,
    pub allowed_roots: Vec<PathBuf>,
    pub report: ScanReport,
    /// id → item map for server-authoritative delete.
    pub items: HashMap<String, CleanupItem>,
    /// One-shot confirm token from the last successful preview.
    #[serde(default)]
    pub pending_token: Option<String>,
    /// Ids that the pending token covers (order-insensitive).
    #[serde(default)]
    pub pending_ids: Vec<String>,
}

impl ScanSession {
    pub fn from_report(report: ScanReport, roots: &[PathBuf]) -> Self {
        let mut items = HashMap::new();
        for group in &report.groups {
            for item in &group.items {
                items.insert(item.id.clone(), item.clone());
            }
        }
        Self {
            version: SESSION_VERSION,
            created_at_secs: now_secs(),
            allowed_roots: roots.to_vec(),
            report,
            items,
            pending_token: None,
            pending_ids: Vec::new(),
        }
    }
}

pub fn default_session_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("AppData/Local")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("safai").join("last-scan.json")
}

pub fn save_session(path: &Path, session: &ScanSession) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(session)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(path, json)
}

pub fn load_session(path: &Path) -> std::io::Result<ScanSession> {
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Build a confirm token bound to the session contents + requested ids + plan totals.
pub fn save_preview_token(session: &ScanSession, ids: &[String], plan: &DeletePlan) -> String {
    let mut sorted = ids.to_vec();
    sorted.sort();
    let mut hasher = Sha256::new();
    hasher.update(session.created_at_secs.to_le_bytes());
    hasher.update(plan.total_bytes.to_le_bytes());
    hasher.update(plan.blocked_count.to_le_bytes());
    for id in &sorted {
        hasher.update(id.as_bytes());
        hasher.update(b"\0");
    }
    // Bind to item paths so a re-scan invalidates old tokens even with same ids.
    for id in &sorted {
        if let Some(item) = session.items.get(id) {
            hasher.update(item.path.as_bytes());
            hasher.update(item.size_bytes.to_le_bytes());
        }
    }
    hex::encode(hasher.finalize())
}

/// Verify `--token` matches a prior preview of the same ids against this session.
pub fn verify_token(session: &ScanSession, ids: &[String], token: &str) -> bool {
    let Some(pending) = session.pending_token.as_deref() else {
        return false;
    };
    if pending != token {
        return false;
    }
    let mut a = session.pending_ids.clone();
    let mut b = ids.to_vec();
    a.sort();
    b.sort();
    a == b
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
