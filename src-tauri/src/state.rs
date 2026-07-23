//! App-managed state (implementation-plan.md §4).
//!
//! Holds the deletion allow-list, the shared cancellation flag, and the last
//! scan's items keyed by `id` — so `preview_delete`/`delete` resolve
//! `id → path` server-side rather than trusting client-supplied paths.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use safai_rules::CleanupItem;

/// The single piece of state registered with `app.manage(...)`.
pub struct SafaiState {
    /// Known cleanup roots; deletions must resolve inside one of these.
    pub allowed_roots: Mutex<Vec<PathBuf>>,
    /// Shared cancellation flag checked by the running scan.
    pub cancel: Arc<AtomicBool>,
    /// Last scan's items, keyed by their stable `id`.
    pub last_items: Mutex<HashMap<String, CleanupItem>>,
}

impl SafaiState {
    /// Build fresh state, seeding `allowed_roots` from the rules crate's
    /// suggested default roots (user profile + known cache parents).
    pub fn new() -> Self {
        SafaiState {
            allowed_roots: Mutex::new(safai_rules::default_roots()),
            cancel: Arc::new(AtomicBool::new(false)),
            last_items: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for SafaiState {
    fn default() -> Self {
        Self::new()
    }
}
