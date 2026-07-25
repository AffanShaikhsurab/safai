//! Automation configuration, persisted state, and schedule arithmetic.
//!
//! Field names and serde casing are **normative** — they are mirrored by the
//! TypeScript definitions in `src/lib/types.ts`.
//!
//! `ScheduleConfig` carries `#[serde(default)]` so a config file written by an
//! older build (or a newer one) still loads: unknown keys are ignored and
//! missing keys fall back to the defaults below. That matters because this file
//! lives in the user's app-data directory and outlives any single release.

use chrono::{Duration as ChronoDuration, Local, TimeZone};
use safai_rules::{Category, SafetyTier};
use serde::{Deserialize, Serialize};

/// How often an automatic maintenance run should happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Cadence {
    /// No time-based runs (threshold triggers can still fire).
    Manual,
    Daily,
    EveryThreeDays,
    Weekly,
}

impl Cadence {
    /// Interval in days, or `None` for [`Cadence::Manual`].
    pub fn interval_days(self) -> Option<i64> {
        match self {
            Cadence::Manual => None,
            Cadence::Daily => Some(1),
            Cadence::EveryThreeDays => Some(3),
            Cadence::Weekly => Some(7),
        }
    }
}

/// What caused a run to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TriggerKind {
    /// The cadence timer came due.
    Cadence,
    /// Disk usage crossed the configured threshold.
    Threshold,
    /// The user pressed "Run now" (in-app or from the tray).
    Manual,
}

/// Where an automatic run currently is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunPhase {
    #[default]
    Idle,
    Scanning,
    Cleaning,
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Hard ceiling on `max_auto_clean_bytes`, regardless of what the config says.
/// An automatic run that wants to remove more than this is almost certainly a
/// misconfiguration, and the user should look at it themselves.
pub const AUTO_CLEAN_HARD_CAP_BYTES: u64 = 200 * 1024 * 1024 * 1024; // 200 GiB

/// Default cap on how much a single automatic cleanup may remove.
pub const DEFAULT_MAX_AUTO_CLEAN_BYTES: u64 = 50 * 1024 * 1024 * 1024; // 50 GiB

/// User-facing automation settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScheduleConfig {
    /// Master switch. When false the scheduler tick does nothing.
    pub enabled: bool,

    // -- Time trigger ----------------------------------------------------
    pub cadence: Cadence,
    /// Local hour (0–23) the cadence run prefers.
    pub preferred_hour: u8,

    // -- Capacity trigger ------------------------------------------------
    /// Also run when the drive gets full, independent of the cadence.
    pub threshold_enabled: bool,
    /// Fire when *used* space reaches this percentage (e.g. 85 → at 85% full).
    pub threshold_percent: u8,
    /// Which drive to watch, as a path. Empty = the drive holding the profile.
    pub threshold_path: String,

    // -- Constraints (Task-Scheduler-style deferral) ----------------------
    /// Only start when the session has been idle for `idle_minutes`.
    pub run_only_when_idle: bool,
    pub idle_minutes: u32,
    /// Defer while running on battery.
    pub skip_on_battery: bool,

    // -- What an automatic run is allowed to do --------------------------
    /// `false` = scan and notify only (the user still presses the button).
    /// `true`  = autopilot: also delete whatever matches the policy below.
    pub auto_clean: bool,
    /// Safety tiers autopilot may remove. `Caution` is rejected in code no
    /// matter what this says.
    pub auto_clean_tiers: Vec<SafetyTier>,
    /// Categories autopilot may remove.
    pub auto_clean_categories: Vec<Category>,
    /// Optional narrowing to specific rule ids. Empty = every rule allowed by
    /// the tier + category filters.
    pub auto_clean_rule_ids: Vec<String>,
    /// Byte ceiling for a single autopilot run.
    pub max_auto_clean_bytes: u64,
    /// Send autopilot deletions to the Recycle Bin (recoverable).
    pub to_recycle_bin: bool,

    // -- Presence --------------------------------------------------------
    /// Register the app to launch at logon (hidden, straight to the tray).
    pub autostart: bool,
    /// Closing the window hides it to the tray instead of quitting.
    pub minimize_to_tray: bool,
    /// Post a notification when an automatic run finds or frees something.
    pub notify: bool,
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        ScheduleConfig {
            enabled: false,
            cadence: Cadence::Weekly,
            // 02:00 local — late enough that the machine is usually idle, and
            // the run is cheap enough that it doesn't matter if it isn't.
            preferred_hour: 2,
            threshold_enabled: true,
            threshold_percent: 85,
            threshold_path: String::new(),
            run_only_when_idle: true,
            idle_minutes: 5,
            skip_on_battery: true,
            // Conservative by default: look, report, let the user decide.
            auto_clean: false,
            auto_clean_tiers: vec![SafetyTier::Safe],
            auto_clean_categories: vec![Category::PackageCache, Category::Temp, Category::Browser],
            auto_clean_rule_ids: Vec::new(),
            max_auto_clean_bytes: DEFAULT_MAX_AUTO_CLEAN_BYTES,
            to_recycle_bin: true,
            autostart: false,
            minimize_to_tray: true,
            notify: true,
        }
    }
}

impl ScheduleConfig {
    /// Clamp anything a hand-edited config file (or a future UI bug) could get
    /// wrong, and strip policy choices that are never allowed.
    pub fn sanitized(mut self) -> Self {
        self.preferred_hour = self.preferred_hour.min(23);
        // Below 50% "full" a cleanup trigger is noise; at 100% it can never fire.
        self.threshold_percent = self.threshold_percent.clamp(50, 99);
        self.idle_minutes = self.idle_minutes.clamp(1, 240);
        self.max_auto_clean_bytes = self
            .max_auto_clean_bytes
            .clamp(1024 * 1024, AUTO_CLEAN_HARD_CAP_BYTES);

        // `Caution` items are the user's own data (downloaded models, editor
        // state). Autopilot never touches them, and that is not configurable.
        self.auto_clean_tiers.retain(|t| *t != SafetyTier::Caution);
        if self.auto_clean_tiers.is_empty() {
            self.auto_clean_tiers.push(SafetyTier::Safe);
        }
        self
    }
}

// ---------------------------------------------------------------------------
// Persisted run state
// ---------------------------------------------------------------------------

/// One entry in the automation audit trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    /// Unix seconds when the run started.
    pub at: u64,
    pub trigger: TriggerKind,
    /// Items the scan surfaced.
    pub scanned_items: u32,
    /// Bytes the scan reported as reclaimable.
    pub reclaimable_bytes: u64,
    /// Items autopilot actually removed (0 when scan-only).
    pub cleaned_items: u32,
    /// Bytes autopilot actually reclaimed (0 when scan-only).
    pub reclaimed_bytes: u64,
    /// Whether this run was allowed to delete.
    pub auto_cleaned: bool,
    /// Items autopilot tried but couldn't remove.
    pub skipped_items: u32,
    pub duration_ms: u64,
    /// Set when the run failed outright.
    pub error: Option<String>,
}

/// How many audit-trail entries to keep. Enough to see a pattern, small enough
/// that the whole thing serializes instantly on every status push.
pub const HISTORY_LIMIT: usize = 20;

/// Scheduler bookkeeping that has to survive a restart.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScheduleState {
    /// Unix seconds of the last completed run of any kind.
    pub last_run_at: Option<u64>,
    /// Unix seconds of the last *threshold*-triggered run, for the cooldown.
    pub last_threshold_run_at: Option<u64>,
    /// Newest-first audit trail.
    pub history: Vec<RunRecord>,
}

impl ScheduleState {
    /// Push a record, keeping the list newest-first and capped.
    pub fn record(&mut self, record: RunRecord) {
        self.history.insert(0, record);
        self.history.truncate(HISTORY_LIMIT);
    }
}

// ---------------------------------------------------------------------------
// Schedule arithmetic
// ---------------------------------------------------------------------------

/// When the next cadence run is due, as unix seconds, or `None` for
/// [`Cadence::Manual`].
///
/// Anchored on the **last completed run** rather than a fixed grid, so the
/// interval means "N days since it last ran", and lands on `preferred_hour`
/// local time. Because the result is a wall-clock instant, a machine that was
/// asleep or powered off through its slot simply finds itself overdue on the
/// next tick and runs then — the missed-run catch-up behaviour you'd get from
/// Task Scheduler's `StartWhenAvailable`, without needing Task Scheduler.
///
/// `now` is only consulted to seed the very first run.
pub fn next_due_at(cfg: &ScheduleConfig, last_run_at: Option<u64>) -> Option<u64> {
    let days = cfg.cadence.interval_days()?;
    let hour = u32::from(cfg.preferred_hour.min(23));

    let base = match last_run_at {
        Some(secs) => Local.timestamp_opt(secs as i64, 0).single()?,
        // Never run before: schedule one full interval out, so switching
        // automation on doesn't immediately kick off a scan.
        None => Local::now(),
    };

    let target_date = (base + ChronoDuration::days(days)).date_naive();
    let naive = target_date.and_hms_opt(hour, 0, 0)?;

    // DST: a local time can be ambiguous (repeated) or nonexistent (skipped).
    // Take the earliest valid interpretation; for a maintenance window an hour
    // either way is irrelevant, but silently returning `None` would stall the
    // schedule until the next config change, so this must not fail.
    let dt = Local
        .from_local_datetime(&naive)
        .single()
        .or_else(|| Local.from_local_datetime(&naive).earliest())
        .or_else(|| {
            // Skipped-hour fallback: nudge past the DST gap.
            let bumped = target_date.and_hms_opt(hour.saturating_add(1).min(23), 0, 0)?;
            Local.from_local_datetime(&bumped).earliest()
        })?;

    Some(dt.timestamp().max(0) as u64)
}

/// Human summary of a cadence, for the tray tooltip and status line.
pub fn describe_cadence(cfg: &ScheduleConfig) -> String {
    let hour = cfg.preferred_hour.min(23);
    match cfg.cadence {
        Cadence::Manual => "Manual only".to_string(),
        Cadence::Daily => format!("Daily at {hour:02}:00"),
        Cadence::EveryThreeDays => format!("Every 3 days at {hour:02}:00"),
        Cadence::Weekly => format!("Weekly at {hour:02}:00"),
    }
}

/// Current unix time in seconds.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    #[test]
    fn manual_cadence_has_no_due_date() {
        let cfg = ScheduleConfig {
            cadence: Cadence::Manual,
            ..Default::default()
        };
        assert!(next_due_at(&cfg, Some(now_secs())).is_none());
    }

    #[test]
    fn due_date_lands_on_preferred_hour_one_interval_later() {
        let cfg = ScheduleConfig {
            cadence: Cadence::Daily,
            preferred_hour: 3,
            ..Default::default()
        };
        let last = Local
            .with_ymd_and_hms(2026, 3, 10, 22, 30, 0)
            .single()
            .expect("valid local time")
            .timestamp() as u64;

        let due = next_due_at(&cfg, Some(last)).expect("daily cadence is due");
        let due_local = Local.timestamp_opt(due as i64, 0).single().unwrap();

        assert_eq!(due_local.hour(), 3);
        assert_eq!(due_local.day(), 11, "one day after the anchor");
        assert!(due > last);
    }

    #[test]
    fn weekly_cadence_is_seven_days_out() {
        let cfg = ScheduleConfig {
            cadence: Cadence::Weekly,
            preferred_hour: 2,
            ..Default::default()
        };
        let last = Local
            .with_ymd_and_hms(2026, 7, 1, 12, 0, 0)
            .single()
            .unwrap()
            .timestamp() as u64;

        let due_local = Local
            .timestamp_opt(next_due_at(&cfg, Some(last)).unwrap() as i64, 0)
            .single()
            .unwrap();
        assert_eq!(due_local.day(), 8);
        assert_eq!(due_local.hour(), 2);
    }

    #[test]
    fn sanitize_rejects_caution_tier_and_clamps_ranges() {
        let cfg = ScheduleConfig {
            preferred_hour: 99,
            threshold_percent: 5,
            idle_minutes: 0,
            max_auto_clean_bytes: u64::MAX,
            auto_clean_tiers: vec![SafetyTier::Caution],
            ..Default::default()
        }
        .sanitized();

        assert_eq!(cfg.preferred_hour, 23);
        assert_eq!(cfg.threshold_percent, 50);
        assert_eq!(cfg.idle_minutes, 1);
        assert_eq!(cfg.max_auto_clean_bytes, AUTO_CLEAN_HARD_CAP_BYTES);
        assert!(!cfg.auto_clean_tiers.contains(&SafetyTier::Caution));
        assert_eq!(cfg.auto_clean_tiers, vec![SafetyTier::Safe]);
    }

    #[test]
    fn history_is_capped_newest_first() {
        let mut state = ScheduleState::default();
        for i in 0..(HISTORY_LIMIT as u64 + 5) {
            state.record(RunRecord {
                at: i,
                trigger: TriggerKind::Cadence,
                scanned_items: 0,
                reclaimable_bytes: 0,
                cleaned_items: 0,
                reclaimed_bytes: 0,
                auto_cleaned: false,
                skipped_items: 0,
                duration_ms: 0,
                error: None,
            });
        }
        assert_eq!(state.history.len(), HISTORY_LIMIT);
        assert_eq!(state.history[0].at, HISTORY_LIMIT as u64 + 4);
    }
}
