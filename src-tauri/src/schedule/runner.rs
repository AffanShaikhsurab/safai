//! The background scheduler: one long-lived task that decides when to run
//! maintenance, and runs it.
//!
//! # Why a coarse wall-clock tick instead of one long sleep
//!
//! The obvious implementation is `sleep(next_due - now)`. It is also wrong on a
//! laptop. `tokio::time::sleep` is built on `Instant`, which on Windows is
//! backed by `QueryPerformanceCounter` and **does not advance while the machine
//! is suspended**. Sleep the lid for six hours and a 24-hour timer fires six
//! hours late, silently, forever drifting.
//!
//! So the loop wakes on a coarse [`TICK`] and compares *wall-clock*
//! `SystemTime` against a persisted `nextDueAt`. That gets three things for
//! free:
//!
//! * **Suspend/resume immunity** — no `RegisterSuspendResumeNotification`
//!   plumbing needed; the next tick after wake sees the real time.
//! * **Missed-run catch-up** — if the machine was off through its window, the
//!   first tick after boot finds the run overdue and does it (the behaviour
//!   Task Scheduler calls `StartWhenAvailable`).
//! * **Config changes take effect immediately** — [`ScheduleRuntime::wake`]
//!   interrupts the tick, so saving settings re-evaluates at once.
//!
//! A 60-second tick costs a timer wakeup and, at most, one
//! `GetDiskFreeSpaceExW` — cheaper than the WMI disk-space event subscription
//! it replaces, which polls internally in a separate service process.
//!
//! # Where the policy lives
//!
//! This module observes and executes; it decides nothing. [`observe`] reads the
//! world into a [`Conditions`] value and [`super::decision::decide`] turns that
//! into a [`Decision`]. Keeping the branching in a pure function is what makes
//! the trigger/constraint matrix unit-testable.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use safai_rules::{CleanupItem, ScanEvent, ScanReport};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_store::StoreExt;

use crate::dto::{DeleteEvent, DriveInfo};
use crate::engine;
use crate::state::SafaiState;
use crate::winsys::{self, BackgroundMode};

use super::config::{
    describe_cadence, next_due_at, now_secs, RunPhase, RunRecord, ScheduleConfig, ScheduleState,
    TriggerKind,
};
use super::decision::{decide, Conditions, Decision};
use super::policy::{self, AutoCleanPlan};

// ---------------------------------------------------------------------------
// Tunables
// ---------------------------------------------------------------------------

/// How often the scheduler re-evaluates. Coarse on purpose — see module docs.
const TICK: Duration = Duration::from_secs(60);

/// Grace period after app start before the first evaluation, so a launch-at-
/// logon start doesn't compete with the rest of the user's startup programs.
const STARTUP_GRACE: Duration = Duration::from_secs(90);

/// Minimum interval between live progress pushes to the webview.
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(250);

/// Store file + keys. Owned by Rust; the frontend goes through commands.
const STORE_FILE: &str = "safai-schedule.json";
const KEY_CONFIG: &str = "config";
const KEY_STATE: &str = "state";

/// Event names pushed to the webview.
pub const EVENT_STATUS: &str = "automation://status";
pub const EVENT_PROGRESS: &str = "automation://progress";
pub const EVENT_REPORT: &str = "automation://report";

// ---------------------------------------------------------------------------
// Live progress
// ---------------------------------------------------------------------------

/// Lightweight, high-frequency view of the run in flight.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationProgress {
    pub phase: RunPhase,
    pub current_path: String,
    pub found_bytes: u64,
    pub item_count: u32,
    pub deleted: u32,
    pub reclaimed_bytes: u64,
    pub skipped: u32,
}

// ---------------------------------------------------------------------------
// Status snapshot
// ---------------------------------------------------------------------------

/// Everything the Automation screen and the tray need, in one payload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationStatus {
    pub config: ScheduleConfig,
    /// Unix seconds of the last completed run.
    pub last_run_at: Option<u64>,
    /// Unix seconds the next cadence run is due, or `None` for manual-only.
    pub next_due_at: Option<u64>,
    /// Is a run executing right now?
    pub running: bool,
    pub phase: RunPhase,
    pub current_trigger: Option<TriggerKind>,
    /// Why the scheduler is holding back, if it is (idle/battery/app busy).
    pub deferred_reason: Option<String>,
    /// Newest-first audit trail.
    pub history: Vec<RunRecord>,
    /// The watched drive, for the threshold readout.
    pub disk: Option<DriveInfo>,
    /// Used percentage of the watched drive.
    pub disk_used_percent: Option<f64>,
    /// Is the app actually registered to launch at logon?
    pub autostart_registered: bool,
    /// Seconds since the last user input.
    pub idle_secs: u64,
    pub on_battery: bool,
    /// Human summary of the cadence, e.g. "Weekly at 02:00".
    pub cadence_label: String,
    pub progress: AutomationProgress,
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

/// Shared scheduler state, managed as `Arc<ScheduleRuntime>` so background
/// tasks can hold an owned handle without borrowing from `AppHandle`.
pub struct ScheduleRuntime {
    config: Mutex<ScheduleConfig>,
    state: Mutex<ScheduleState>,
    /// Interrupts the tick so config changes and "Run now" apply immediately.
    wake: tokio::sync::Notify,
    /// A "Run now" request waiting to be picked up by the next evaluation.
    pending_manual: AtomicBool,
    running: AtomicBool,
    phase: Mutex<RunPhase>,
    trigger: Mutex<Option<TriggerKind>>,
    deferred: Mutex<Option<String>>,
    progress: Mutex<AutomationProgress>,
    last_progress_emit: Mutex<Instant>,
    /// Cancellation for the automatic run (separate from the interactive one so
    /// "Stop automation" and "Cancel scan" can't clobber each other).
    cancel: Arc<AtomicBool>,
}

impl ScheduleRuntime {
    fn new(config: ScheduleConfig, state: ScheduleState) -> Self {
        ScheduleRuntime {
            config: Mutex::new(config),
            state: Mutex::new(state),
            wake: tokio::sync::Notify::new(),
            pending_manual: AtomicBool::new(false),
            running: AtomicBool::new(false),
            phase: Mutex::new(RunPhase::Idle),
            trigger: Mutex::new(None),
            deferred: Mutex::new(None),
            progress: Mutex::new(AutomationProgress::default()),
            last_progress_emit: Mutex::new(Instant::now()),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn config(&self) -> ScheduleConfig {
        self.config.lock().unwrap().clone()
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Replace the config (already sanitized) and re-evaluate immediately.
    pub fn set_config(&self, cfg: ScheduleConfig) {
        *self.config.lock().unwrap() = cfg;
        self.wake.notify_one();
    }

    /// Queue an immediate run, bypassing cadence and constraints.
    pub fn request_run(&self) {
        self.pending_manual.store(true, Ordering::SeqCst);
        self.wake.notify_one();
    }

    /// Ask the in-flight automatic run to wind down.
    pub fn cancel_run(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    fn set_phase(&self, phase: RunPhase) {
        *self.phase.lock().unwrap() = phase;
        self.progress.lock().unwrap().phase = phase;
    }

    fn set_deferred(&self, reason: Option<String>) -> bool {
        let mut slot = self.deferred.lock().unwrap();
        let changed = *slot != reason;
        *slot = reason;
        changed
    }
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

/// Load config + state from the store, falling back to defaults. Best-effort:
/// a missing or corrupt file just means "defaults", never a startup failure.
fn load_persisted(app: &AppHandle) -> (ScheduleConfig, ScheduleState) {
    let Ok(store) = app.store(STORE_FILE) else {
        return (ScheduleConfig::default(), ScheduleState::default());
    };
    let config = store
        .get(KEY_CONFIG)
        .and_then(|v| serde_json::from_value::<ScheduleConfig>(v).ok())
        .unwrap_or_default()
        .sanitized();
    let state = store
        .get(KEY_STATE)
        .and_then(|v| serde_json::from_value::<ScheduleState>(v).ok())
        .unwrap_or_default();
    (config, state)
}

fn persist_config(app: &AppHandle, cfg: &ScheduleConfig) {
    if let Ok(store) = app.store(STORE_FILE) {
        if let Ok(value) = serde_json::to_value(cfg) {
            store.set(KEY_CONFIG, value);
            let _ = store.save();
        }
    }
}

fn persist_state(app: &AppHandle, state: &ScheduleState) {
    if let Ok(store) = app.store(STORE_FILE) {
        if let Ok(value) = serde_json::to_value(state) {
            store.set(KEY_STATE, value);
            let _ = store.save();
        }
    }
}

// ---------------------------------------------------------------------------
// Status assembly + emission
// ---------------------------------------------------------------------------

/// Path whose volume the threshold watches.
fn watched_path(cfg: &ScheduleConfig) -> std::path::PathBuf {
    if !cfg.threshold_path.trim().is_empty() {
        return std::path::PathBuf::from(&cfg.threshold_path);
    }
    safai_rules::default_roots()
        .into_iter()
        .next()
        .unwrap_or_else(|| std::path::PathBuf::from("C:/"))
}

/// Build the full status snapshot.
pub fn status(app: &AppHandle) -> AutomationStatus {
    let rt = runtime(app);
    let cfg = rt.config();

    let path = watched_path(&cfg);
    let disk = winsys::disk_free_total(&path).map(|(free_bytes, total_bytes)| DriveInfo {
        mount: engine::drive_mount(&path),
        free_bytes,
        total_bytes,
    });
    let disk_used_percent = disk.as_ref().and_then(|d| {
        if d.total_bytes == 0 {
            None
        } else {
            Some((d.total_bytes.saturating_sub(d.free_bytes) as f64 / d.total_bytes as f64) * 100.0)
        }
    });

    let (last_run_at, history) = {
        let state = rt.state.lock().unwrap();
        (state.last_run_at, state.history.clone())
    };

    // Read every guarded field into a local first: as the tail expression, a
    // struct literal keeps its temporaries (the `MutexGuard`s) alive past the
    // point where the local `Arc` is dropped.
    let running = rt.is_running();
    let phase = *rt.phase.lock().unwrap();
    let current_trigger = *rt.trigger.lock().unwrap();
    let deferred_reason = rt.deferred.lock().unwrap().clone();
    let progress = rt.progress.lock().unwrap().clone();

    AutomationStatus {
        next_due_at: next_due_at(&cfg, last_run_at),
        cadence_label: describe_cadence(&cfg),
        autostart_registered: autostart_registered(app),
        running,
        phase,
        current_trigger,
        deferred_reason,
        progress,
        idle_secs: winsys::idle_secs(),
        on_battery: winsys::on_battery(),
        config: cfg,
        last_run_at,
        history,
        disk,
        disk_used_percent,
    }
}

/// Push a full status snapshot to the webview and refresh the tray.
pub fn push_status(app: &AppHandle) {
    let snapshot = status(app);
    let _ = app.emit(EVENT_STATUS, &snapshot);
    crate::tray::refresh(app, &snapshot);
}

/// Push the lightweight progress payload, throttled.
fn push_progress(app: &AppHandle, rt: &ScheduleRuntime, force: bool) {
    {
        let mut last = rt.last_progress_emit.lock().unwrap();
        if !force && last.elapsed() < PROGRESS_EMIT_INTERVAL {
            return;
        }
        *last = Instant::now();
    }
    let payload = rt.progress.lock().unwrap().clone();
    let _ = app.emit(EVENT_PROGRESS, payload);
}

/// Is the app currently registered to launch at logon?
#[cfg(desktop)]
fn autostart_registered(app: &AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[cfg(not(desktop))]
fn autostart_registered(_app: &AppHandle) -> bool {
    false
}

/// Apply the config's autostart preference to the OS. Returns the resulting
/// registration state, which is what gets persisted — so a refused
/// registration shows up in the UI rather than being silently assumed.
#[cfg(desktop)]
pub fn sync_autostart(app: &AppHandle, wanted: bool) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    let _ = if wanted {
        manager.enable()
    } else {
        manager.disable()
    };
    manager.is_enabled().unwrap_or(false)
}

#[cfg(not(desktop))]
pub fn sync_autostart(_app: &AppHandle, _wanted: bool) -> bool {
    false
}

/// Fetch the managed runtime handle. Panics if called before [`init`], which
/// can only happen from a code path that runs before `setup` finishes.
pub fn runtime(app: &AppHandle) -> Arc<ScheduleRuntime> {
    app.state::<Arc<ScheduleRuntime>>().inner().clone()
}

/// Non-panicking variant, for callbacks that can in principle fire before
/// `setup` has registered the runtime (window events being the practical case).
pub fn try_runtime(app: &AppHandle) -> Option<Arc<ScheduleRuntime>> {
    app.try_state::<Arc<ScheduleRuntime>>()
        .map(|state| state.inner().clone())
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

/// Load persisted settings, register the runtime, reconcile autostart, and
/// start the tick loop. Call once from `setup`.
pub fn init(app: &AppHandle) {
    let (config, state) = load_persisted(app);
    // Keep the OS registration honest with the saved preference — the user may
    // have removed the startup entry from Task Manager behind our back.
    if config.enabled {
        sync_autostart(app, config.autostart);
    }
    app.manage(Arc::new(ScheduleRuntime::new(config, state)));
    spawn_tick(app.clone());
}

/// Spawn the evaluation loop.
fn spawn_tick(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let rt = runtime(&app);
        // The loop waits *before* evaluating, and the first wait is the longer
        // startup grace. Crucially, `wake` cuts any wait short — so pressing
        // "Run now" during the grace period acts immediately instead of
        // appearing to do nothing for a minute and a half.
        let mut delay = STARTUP_GRACE;
        loop {
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = rt.wake.notified() => {}
            }
            delay = TICK;
            evaluate(&app, &rt).await;
        }
    });
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

async fn evaluate(app: &AppHandle, rt: &Arc<ScheduleRuntime>) {
    let cfg = rt.config();
    let conditions = observe(app, rt, &cfg);

    match decide(&cfg, &conditions) {
        Decision::Run(trigger) => {
            rt.set_deferred(None);
            execute(app, rt, trigger).await;
        }
        Decision::Defer {
            reason,
            requeue_manual,
        } => {
            if requeue_manual {
                rt.pending_manual.store(true, Ordering::SeqCst);
            }
            // Only wake the UI when the reason actually changed, so a machine
            // sitting on "waiting until you're idle" doesn't emit every minute.
            if rt.set_deferred(Some(reason)) {
                push_status(app);
            }
        }
        Decision::Idle => {
            if rt.set_deferred(None) {
                push_status(app);
            }
        }
    }
}

/// Gather the state of the world for [`decide`].
///
/// This function holds no policy — it only reads. All the branching lives in
/// [`super::decision`], where it can be tested without an app or a real disk.
fn observe(app: &AppHandle, rt: &Arc<ScheduleRuntime>, cfg: &ScheduleConfig) -> Conditions {
    // Read before consuming the manual request: when a run is already in
    // flight, a "Run now" pressed mid-run is left queued for the tick after it
    // finishes rather than being swallowed here.
    let run_in_flight = rt.is_running();
    let manual_requested = if run_in_flight {
        false
    } else {
        rt.pending_manual.swap(false, Ordering::SeqCst)
    };

    let gate = app.state::<SafaiState>();
    let (last_run_at, last_threshold_run_at) = {
        let state = rt.state.lock().unwrap();
        (state.last_run_at, state.last_threshold_run_at)
    };

    Conditions {
        now: now_secs(),
        manual_requested,
        run_in_flight,
        activity_busy: gate.is_busy(),
        ui_engaged: gate.is_ui_engaged(),
        disk_used_percent: if cfg.threshold_enabled {
            winsys::disk_used_percent(&watched_path(cfg))
        } else {
            None
        },
        idle_secs: winsys::idle_secs(),
        on_battery: winsys::on_battery(),
        last_run_at,
        last_threshold_run_at,
    }
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

async fn execute(app: &AppHandle, rt: &Arc<ScheduleRuntime>, trigger: TriggerKind) {
    let cfg = rt.config();
    let started_at = now_secs();
    let clock = Instant::now();

    // Claim the interactive gate too, so a manual scan can't start mid-run.
    let gate = app.state::<SafaiState>();
    let Some(_hold) = gate.try_acquire() else {
        // Lost a race with the UI. Re-queue an explicit request so it isn't
        // lost; never *clear* the flag here, or a losing scheduled run would
        // discard a manual request queued in the meantime.
        if trigger == TriggerKind::Manual {
            rt.pending_manual.store(true, Ordering::SeqCst);
        }
        return;
    };

    rt.running.store(true, Ordering::SeqCst);
    rt.cancel.store(false, Ordering::SeqCst);
    *rt.trigger.lock().unwrap() = Some(trigger);
    *rt.progress.lock().unwrap() = AutomationProgress::default();
    rt.set_phase(RunPhase::Scanning);
    push_status(app);

    // Background CPU + I/O priority and EcoQoS for unattended runs only. When
    // the user pressed "Run now" they're watching a progress bar and want it
    // fast, not polite.
    let _background = if trigger == TriggerKind::Manual {
        None
    } else {
        Some(BackgroundMode::enter())
    };

    // ---- Scan ----------------------------------------------------------
    let scan_cfg = engine::build_scan_config(&[], true);
    let cancel = rt.cancel.clone();
    let sink_rt = rt.clone();
    let sink_app = app.clone();

    let scanned = tauri::async_runtime::spawn_blocking(move || {
        let sink = move |event: ScanEvent| {
            match &event {
                ScanEvent::Progress {
                    current_path,
                    found_bytes,
                    ..
                } => {
                    let mut p = sink_rt.progress.lock().unwrap();
                    p.current_path = current_path.clone();
                    p.found_bytes = *found_bytes;
                }
                ScanEvent::Found { item } => {
                    let mut p = sink_rt.progress.lock().unwrap();
                    p.item_count += 1;
                    p.found_bytes = p.found_bytes.saturating_add(item.size_bytes);
                }
                ScanEvent::Finished {
                    total_reclaimable_bytes,
                    item_count,
                } => {
                    let mut p = sink_rt.progress.lock().unwrap();
                    p.found_bytes = *total_reclaimable_bytes;
                    p.item_count = *item_count;
                }
                ScanEvent::Started { .. } => {}
            }
            push_progress(
                &sink_app,
                &sink_rt,
                matches!(event, ScanEvent::Finished { .. }),
            );
        };
        engine::scan_blocking(&scan_cfg, &cancel, &sink)
    })
    .await;

    let mut report = match scanned {
        Ok(report) => report,
        Err(err) => {
            finish(
                app,
                rt,
                trigger,
                started_at,
                clock,
                RunRecord {
                    at: started_at,
                    trigger,
                    scanned_items: 0,
                    reclaimable_bytes: 0,
                    cleaned_items: 0,
                    reclaimed_bytes: 0,
                    auto_cleaned: false,
                    skipped_items: 0,
                    duration_ms: clock.elapsed().as_millis() as u64,
                    error: Some(format!("scan failed: {err}")),
                },
                None,
            );
            return;
        }
    };

    let scanned_items: u32 = report.groups.iter().map(|g| g.items.len() as u32).sum();
    let reclaimable_bytes = report.total_reclaimable_bytes;

    // Make the findings actionable from the UI (ids resolve server-side).
    remember_items(app, &report);

    // ---- Optional autopilot cleanup ------------------------------------
    let mut cleaned_items = 0u32;
    let mut reclaimed_bytes = 0u64;
    let mut skipped_items = 0u32;
    let mut plan_was_capped = false;

    if cfg.auto_clean && !rt.cancel.load(Ordering::SeqCst) {
        let plan: AutoCleanPlan = policy::select_auto_clean(&report, &cfg);
        plan_was_capped = plan.excluded_by_cap > 0;

        if !plan.is_empty() {
            rt.set_phase(RunPhase::Cleaning);
            push_status(app);

            let roots = app
                .state::<SafaiState>()
                .allowed_roots
                .lock()
                .unwrap()
                .clone();
            let resolved: Vec<(String, Option<CleanupItem>)> = plan
                .items
                .iter()
                .map(|item| (item.id.clone(), Some(item.clone())))
                .collect();

            // Collected from the event stream rather than inferred from the
            // report: `DeleteReport.skipped` holds paths, and "already gone"
            // items count as deleted, so the sink is the only accurate source.
            let removed_ids: Arc<Mutex<std::collections::HashSet<String>>> =
                Arc::new(Mutex::new(std::collections::HashSet::new()));

            let cancel = rt.cancel.clone();
            let sink_rt = rt.clone();
            let sink_app = app.clone();
            let sink_removed = removed_ids.clone();
            let to_recycle_bin = cfg.to_recycle_bin;

            let delete_report = tauri::async_runtime::spawn_blocking(move || {
                let sink = move |event: DeleteEvent| {
                    match &event {
                        DeleteEvent::Deleted {
                            id,
                            path,
                            size_bytes,
                        } => {
                            sink_removed.lock().unwrap().insert(id.clone());
                            let mut p = sink_rt.progress.lock().unwrap();
                            p.deleted += 1;
                            p.reclaimed_bytes = p.reclaimed_bytes.saturating_add(*size_bytes);
                            p.current_path = path.clone();
                        }
                        DeleteEvent::Skipped { path, .. } => {
                            let mut p = sink_rt.progress.lock().unwrap();
                            p.skipped += 1;
                            p.current_path = path.clone();
                        }
                        DeleteEvent::Started { .. } | DeleteEvent::Finished { .. } => {}
                    }
                    push_progress(
                        &sink_app,
                        &sink_rt,
                        matches!(event, DeleteEvent::Finished { .. }),
                    );
                };
                engine::delete_blocking(resolved, &roots, to_recycle_bin, &cancel, &sink)
            })
            .await;

            if let Ok(delete_report) = delete_report {
                cleaned_items = delete_report.deleted;
                reclaimed_bytes = delete_report.reclaimed_bytes;
                skipped_items = delete_report.skipped.len() as u32;

                // The report handed back to the UI must not advertise space
                // that no longer exists, so drop what was actually removed.
                let removed = removed_ids.lock().unwrap().clone();
                report = prune_report(report, &removed);
                remember_items(app, &report);
            }
        }
    }

    let record = RunRecord {
        at: started_at,
        trigger,
        scanned_items,
        reclaimable_bytes,
        cleaned_items,
        reclaimed_bytes,
        auto_cleaned: cfg.auto_clean,
        skipped_items,
        duration_ms: clock.elapsed().as_millis() as u64,
        error: if rt.cancel.load(Ordering::SeqCst) {
            Some("stopped early".to_string())
        } else if plan_was_capped {
            Some("hit the per-run size cap; some items were left".to_string())
        } else {
            None
        },
    };

    finish(app, rt, trigger, started_at, clock, record, Some(report));
}

/// Wrap up: persist bookkeeping, notify, emit, reset live state.
fn finish(
    app: &AppHandle,
    rt: &Arc<ScheduleRuntime>,
    trigger: TriggerKind,
    started_at: u64,
    _clock: Instant,
    record: RunRecord,
    report: Option<ScanReport>,
) {
    // Persist bookkeeping first so a crash right after a run can't cause it to
    // repeat immediately.
    {
        let mut state = rt.state.lock().unwrap();
        state.last_run_at = Some(started_at);
        if trigger == TriggerKind::Threshold {
            state.last_threshold_run_at = Some(started_at);
        }
        state.record(record.clone());
    }
    persist_state(app, &rt.state.lock().unwrap().clone());

    rt.set_phase(RunPhase::Idle);
    *rt.trigger.lock().unwrap() = None;
    rt.running.store(false, Ordering::SeqCst);
    rt.cancel.store(false, Ordering::SeqCst);

    if let Some(report) = report {
        let _ = app.emit(EVENT_REPORT, &report);
    }
    push_status(app);
    notify_result(app, &rt.config(), &record);
}

/// Replace the backend's id→item map so UI actions resolve against the newest
/// findings.
fn remember_items(app: &AppHandle, report: &ScanReport) {
    let state = app.state::<SafaiState>();
    let mut items = state.last_items.lock().unwrap();
    items.clear();
    for group in &report.groups {
        for item in &group.items {
            items.insert(item.id.clone(), item.clone());
        }
    }
}

/// Drop `removed` ids from a report and re-total it.
fn prune_report(mut report: ScanReport, removed: &std::collections::HashSet<String>) -> ScanReport {
    if removed.is_empty() {
        return report;
    }
    for group in &mut report.groups {
        group.items.retain(|item| !removed.contains(&item.id));
        group.total_bytes = group.items.iter().map(|i| i.size_bytes).sum();
    }
    report.groups.retain(|group| !group.items.is_empty());
    report.total_reclaimable_bytes = report.groups.iter().map(|g| g.total_bytes).sum();
    report
}

/// Tell the user what happened, if they asked to be told and there's something
/// worth saying.
fn notify_result(app: &AppHandle, cfg: &ScheduleConfig, record: &RunRecord) {
    if !cfg.notify {
        return;
    }
    let (title, body) = if let Some(err) = &record.error {
        if record.cleaned_items == 0 && record.scanned_items == 0 {
            ("Safai automation stopped", err.clone())
        } else {
            (
                "Safai automation finished",
                format!("{} · {err}", human_summary(record)),
            )
        }
    } else if record.cleaned_items > 0 {
        (
            "Safai freed up space",
            format!(
                "Reclaimed {} across {} item{}.",
                human_bytes(record.reclaimed_bytes),
                record.cleaned_items,
                if record.cleaned_items == 1 { "" } else { "s" }
            ),
        )
    } else if record.reclaimable_bytes > 0 {
        (
            "Safai found space to reclaim",
            format!(
                "{} across {} item{} — open Safai to review.",
                human_bytes(record.reclaimable_bytes),
                record.scanned_items,
                if record.scanned_items == 1 { "" } else { "s" }
            ),
        )
    } else {
        // Nothing found and nothing cleaned: stay quiet.
        return;
    };

    let _ = app.notification().builder().title(title).body(body).show();
}

fn human_summary(record: &RunRecord) -> String {
    if record.cleaned_items > 0 {
        format!("Reclaimed {}", human_bytes(record.reclaimed_bytes))
    } else {
        format!("Found {}", human_bytes(record.reclaimable_bytes))
    }
}

/// Compact byte formatting for notifications and the tray tooltip.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

// ---------------------------------------------------------------------------
// Config mutation entry point (used by commands + tray)
// ---------------------------------------------------------------------------

/// Apply, sanitize, persist and act on a new config.
pub fn apply_config(app: &AppHandle, incoming: ScheduleConfig) -> AutomationStatus {
    let cfg = incoming.sanitized();

    // Reconcile the OS logon entry. Turning automation off also removes it —
    // an app that can't do anything shouldn't be starting itself.
    let wanted_autostart = cfg.enabled && cfg.autostart;
    let registered = sync_autostart(app, wanted_autostart);

    let mut cfg = cfg;
    cfg.autostart = registered;

    persist_config(app, &cfg);
    runtime(app).set_config(cfg);

    let snapshot = status(app);
    let _ = app.emit(EVENT_STATUS, &snapshot);
    crate::tray::refresh(app, &snapshot);
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;
    use safai_rules::{Category, CategoryGroup, CleanupItem, SafetyTier};
    use std::collections::HashSet;

    fn item(id: &str, size_bytes: u64) -> CleanupItem {
        CleanupItem {
            id: id.to_string(),
            rule_id: "npm-cache".to_string(),
            label: id.to_string(),
            category: Category::PackageCache,
            tier: SafetyTier::Safe,
            path: format!("C:/test/{id}"),
            size_bytes,
            regenerates: true,
            last_modified_secs: None,
            note: String::new(),
            selected_by_default: true,
        }
    }

    fn report_of(items: Vec<CleanupItem>) -> ScanReport {
        let total: u64 = items.iter().map(|i| i.size_bytes).sum();
        ScanReport {
            total_reclaimable_bytes: total,
            groups: vec![CategoryGroup {
                category: Category::PackageCache,
                label: "Package caches".to_string(),
                total_bytes: total,
                items,
            }],
            scanned_roots: Vec::new(),
            warnings: Vec::new(),
        }
    }

    // -- prune_report ----------------------------------------------------
    //
    // After autopilot deletes something, the report handed back to the UI must
    // not still advertise that space, or the user sees a "Free up 8 GB" button
    // for bytes that no longer exist.

    #[test]
    fn pruning_removes_deleted_items_and_retotals() {
        let report = report_of(vec![item("a", 100), item("b", 250), item("c", 50)]);
        let removed: HashSet<String> = ["a", "c"].iter().map(|s| s.to_string()).collect();

        let pruned = prune_report(report, &removed);

        assert_eq!(pruned.groups.len(), 1);
        assert_eq!(pruned.groups[0].items.len(), 1);
        assert_eq!(pruned.groups[0].items[0].id, "b");
        assert_eq!(pruned.groups[0].total_bytes, 250);
        assert_eq!(pruned.total_reclaimable_bytes, 250);
    }

    #[test]
    fn pruning_drops_groups_that_end_up_empty() {
        let report = report_of(vec![item("a", 100)]);
        let removed: HashSet<String> = std::iter::once("a".to_string()).collect();

        let pruned = prune_report(report, &removed);

        assert!(
            pruned.groups.is_empty(),
            "an emptied group should not linger"
        );
        assert_eq!(pruned.total_reclaimable_bytes, 0);
    }

    #[test]
    fn pruning_nothing_is_a_no_op() {
        let report = report_of(vec![item("a", 100), item("b", 250)]);
        let pruned = prune_report(report, &HashSet::new());

        assert_eq!(pruned.groups[0].items.len(), 2);
        assert_eq!(pruned.total_reclaimable_bytes, 350);
    }

    #[test]
    fn pruning_ignores_ids_that_were_never_in_the_report() {
        let report = report_of(vec![item("a", 100)]);
        let removed: HashSet<String> = std::iter::once("ghost".to_string()).collect();

        let pruned = prune_report(report, &removed);
        assert_eq!(pruned.total_reclaimable_bytes, 100);
    }

    // -- human_bytes -----------------------------------------------------

    #[test]
    fn byte_formatting_covers_the_notification_range() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        // Two decimals would be noise in a toast; one is enough under 10.
        assert_eq!(human_bytes(1024 * 1024 * 3 / 2), "1.5 MB");
        // At/above 10 the fraction is dropped entirely.
        assert_eq!(human_bytes(1024 * 1024 * 42), "42 MB");
        assert_eq!(human_bytes(1024 * 1024 * 1024 * 7), "7.0 GB");
    }

    #[test]
    fn byte_formatting_saturates_at_terabytes_instead_of_overflowing() {
        // The unit table stops at TB; an absurd value must still format.
        let formatted = human_bytes(u64::MAX);
        assert!(formatted.ends_with(" TB"), "got {formatted:?}");
    }
}
