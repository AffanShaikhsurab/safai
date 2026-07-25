//! App-managed state (implementation-plan.md §4).
//!
//! Holds the deletion allow-list, the shared cancellation flag, the last scan's
//! items keyed by `id` — so `preview_delete`/`delete` resolve `id → path`
//! server-side rather than trusting client-supplied paths — and the activity
//! gate that keeps interactive and scheduled work from colliding.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
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

    /// A scan or deletion is executing right now (from either the UI or the
    /// scheduler).
    ///
    /// Both share one flag because both mutate `last_items`: two concurrent
    /// scans would leave the id→item map describing neither of them.
    busy: AtomicBool,
    /// The user is mid-flow in the Clean screens (scanning/results/cleaning).
    ///
    /// Separate from `busy` because a user sitting on the Results screen with a
    /// selection built up isn't running anything — but an automatic scan would
    /// still invalidate every id they've ticked, so automation stays out of the
    /// way until they're done.
    ui_engaged: AtomicBool,
}

impl SafaiState {
    /// Build fresh state, seeding `allowed_roots` from the rules crate's
    /// suggested default roots (user profile + known cache parents).
    pub fn new() -> Self {
        SafaiState {
            allowed_roots: Mutex::new(safai_rules::default_roots()),
            cancel: Arc::new(AtomicBool::new(false)),
            last_items: Mutex::new(HashMap::new()),
            busy: AtomicBool::new(false),
            ui_engaged: AtomicBool::new(false),
        }
    }

    /// Is a scan or deletion running?
    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    /// Is the user mid-flow in the Clean screens?
    pub fn is_ui_engaged(&self) -> bool {
        self.ui_engaged.load(Ordering::SeqCst)
    }

    /// Mark (or clear) the user being mid-flow in the Clean screens.
    pub fn set_ui_engaged(&self, engaged: bool) {
        self.ui_engaged.store(engaged, Ordering::SeqCst);
    }

    /// Claim the activity slot, or `None` if something already holds it.
    ///
    /// The returned guard releases on drop, so an early return or a panic in a
    /// command handler can't leave the app permanently "busy".
    pub fn try_acquire(&self) -> Option<ActivityHold<'_>> {
        self.busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| ActivityHold { state: self })
    }
}

impl Default for SafaiState {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard for the activity slot. See [`SafaiState::try_acquire`].
pub struct ActivityHold<'a> {
    state: &'a SafaiState,
}

impl Drop for ActivityHold<'_> {
    fn drop(&mut self) {
        self.state.busy.store(false, Ordering::SeqCst);
    }
}
