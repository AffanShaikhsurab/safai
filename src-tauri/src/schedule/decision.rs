//! The scheduler's decision logic, as a pure function.
//!
//! [`decide`] is deliberately free of `AppHandle`, Win32 calls and global
//! state: everything it needs arrives in [`Conditions`]. That's what makes the
//! whole trigger matrix — two triggers times three constraints times the
//! cooldown and grace windows — testable without a running app, a real disk, or
//! a real clock.
//!
//! [`crate::schedule::runner`] is responsible for gathering the conditions from
//! the world and acting on the result; it holds no policy of its own.
//!
//! # Constraint model
//!
//! Modelled on Windows Task Scheduler's conditions, with strictness scaled to
//! urgency:
//!
//! | Trigger   | Waits for idle?          | Waits for AC power?         |
//! |-----------|--------------------------|-----------------------------|
//! | Manual    | no                       | no                          |
//! | Threshold | no — the disk is filling | yes, unless nearly full     |
//! | Cadence   | yes, if configured       | yes, if configured          |
//!
//! Cadence deferral is bounded by [`CADENCE_GRACE_SECS`]: a run held back long
//! enough eventually happens regardless, so "only when idle" can't mean "never"
//! for someone who is always at their desk.

use super::config::{next_due_at, ScheduleConfig, TriggerKind};

/// Minimum gap between two threshold-triggered runs. Without this, a drive that
/// stays above the threshold (because the reclaimable space simply isn't there)
/// would re-scan on every tick.
pub const THRESHOLD_COOLDOWN_SECS: u64 = 6 * 60 * 60;

/// How long a cadence run may be held back by idle/battery constraints before
/// it runs anyway.
pub const CADENCE_GRACE_SECS: u64 = 12 * 60 * 60;

/// Above this used-percentage a threshold run ignores the battery constraint —
/// at that point running out of disk is the bigger problem.
pub const CRITICAL_USED_PERCENT: f64 = 95.0;

/// Everything [`decide`] needs from the outside world.
#[derive(Debug, Clone)]
pub struct Conditions {
    /// Current unix time, in seconds.
    pub now: u64,
    /// A "Run now" request is waiting to be honoured.
    pub manual_requested: bool,
    /// An automation run is already executing.
    pub run_in_flight: bool,
    /// A scan or cleanup (from either side) holds the activity gate.
    pub activity_busy: bool,
    /// The user is mid-flow in the Clean screens.
    pub ui_engaged: bool,
    /// Used percentage of the watched volume, or `None` if unavailable.
    pub disk_used_percent: Option<f64>,
    /// Seconds since the last user input.
    pub idle_secs: u64,
    pub on_battery: bool,
    pub last_run_at: Option<u64>,
    pub last_threshold_run_at: Option<u64>,
}

impl Conditions {
    /// A baseline set of conditions: idle machine, on AC, nothing running,
    /// plenty of disk free. Tests override only the field under examination.
    #[cfg(test)]
    pub fn idle_desktop(now: u64) -> Self {
        Conditions {
            now,
            manual_requested: false,
            run_in_flight: false,
            activity_busy: false,
            ui_engaged: false,
            disk_used_percent: Some(40.0),
            idle_secs: 60 * 60,
            on_battery: false,
            last_run_at: None,
            last_threshold_run_at: None,
        }
    }
}

/// What the scheduler should do on this tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Nothing to do.
    Idle,
    /// Something is due but a condition isn't met yet.
    Defer {
        /// User-facing explanation, shown in the UI and the tray.
        reason: String,
        /// The caller must put the manual request back: it was consumed to
        /// build [`Conditions`], but hasn't been honoured.
        requeue_manual: bool,
    },
    Run(TriggerKind),
}

impl Decision {
    fn defer(reason: impl Into<String>) -> Self {
        Decision::Defer {
            reason: reason.into(),
            requeue_manual: false,
        }
    }
}

/// Decide what to do, given the config and the state of the world.
pub fn decide(cfg: &ScheduleConfig, c: &Conditions) -> Decision {
    // An automation run is already executing; let it finish.
    if c.run_in_flight {
        return Decision::Idle;
    }

    // A scan or cleanup is running. Both sides share the activity gate because
    // both replace the backend's id→item map, and two concurrent scans would
    // leave it describing neither of them.
    if c.activity_busy {
        return if c.manual_requested {
            Decision::Defer {
                reason: "waiting for the current scan or cleanup to finish".to_string(),
                requeue_manual: true,
            }
        } else {
            Decision::Idle
        };
    }

    // The user is building a selection from the current results. Starting a
    // scan now would invalidate every id they've ticked.
    if c.ui_engaged && !c.manual_requested {
        return Decision::defer("paused while you're using the Clean screen");
    }

    // An explicit request bypasses the cadence, the constraints, and even the
    // master switch — the user just asked for it.
    if c.manual_requested {
        return Decision::Run(TriggerKind::Manual);
    }

    if !cfg.enabled {
        return Decision::Idle;
    }

    // --- Capacity trigger (checked first: it's the urgent one) -------------
    if cfg.threshold_enabled {
        if let Some(used) = c.disk_used_percent {
            let over = used >= f64::from(cfg.threshold_percent);
            let cooled = c
                .last_threshold_run_at
                .map(|t| c.now.saturating_sub(t) >= THRESHOLD_COOLDOWN_SECS)
                .unwrap_or(true);
            if over && cooled {
                // Disk pressure overrides the idle requirement. It still waits
                // for power, but not once things are critical.
                if cfg.skip_on_battery && c.on_battery && used < CRITICAL_USED_PERCENT {
                    return Decision::defer(format!(
                        "drive is {used:.0}% full — waiting until you're on power"
                    ));
                }
                return Decision::Run(TriggerKind::Threshold);
            }
        }
    }

    // --- Cadence trigger --------------------------------------------------
    let Some(due) = next_due_at(cfg, c.last_run_at) else {
        return Decision::Idle; // Cadence::Manual
    };
    if c.now < due {
        return Decision::Idle;
    }

    // Overdue. Honour the constraints, but not forever.
    if c.now.saturating_sub(due) < CADENCE_GRACE_SECS {
        if cfg.skip_on_battery && c.on_battery {
            return Decision::defer("scheduled run is waiting until you're on power");
        }
        if cfg.run_only_when_idle && c.idle_secs < u64::from(cfg.idle_minutes) * 60 {
            return Decision::defer(format!(
                "scheduled run is waiting for {} minutes of idle time",
                cfg.idle_minutes
            ));
        }
    }

    Decision::Run(TriggerKind::Cadence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::config::{now_secs, Cadence};

    /// An enabled config with the capacity trigger off, so cadence behaviour
    /// can be examined in isolation.
    fn cadence_only(cadence: Cadence) -> ScheduleConfig {
        ScheduleConfig {
            enabled: true,
            cadence,
            threshold_enabled: false,
            ..Default::default()
        }
        .sanitized()
    }

    /// An enabled config with only the capacity trigger active.
    fn threshold_only(percent: u8) -> ScheduleConfig {
        ScheduleConfig {
            enabled: true,
            cadence: Cadence::Manual,
            threshold_enabled: true,
            threshold_percent: percent,
            ..Default::default()
        }
        .sanitized()
    }

    /// A cadence run that came due `overdue_secs` ago.
    fn overdue_by(cfg: &ScheduleConfig, overdue_secs: u64) -> Conditions {
        let last = now_secs() - 60 * 60 * 24 * 30;
        let due = next_due_at(cfg, Some(last)).expect("cadence has a due date");
        Conditions {
            last_run_at: Some(last),
            ..Conditions::idle_desktop(due + overdue_secs)
        }
    }

    // -- Master switch and gating ---------------------------------------

    #[test]
    fn disabled_config_does_nothing() {
        let cfg = ScheduleConfig::default(); // enabled: false
        assert_eq!(
            decide(&cfg, &Conditions::idle_desktop(now_secs())),
            Decision::Idle
        );
    }

    #[test]
    fn disabled_config_still_honours_an_explicit_request() {
        let cfg = ScheduleConfig::default();
        let c = Conditions {
            manual_requested: true,
            ..Conditions::idle_desktop(now_secs())
        };
        assert_eq!(decide(&cfg, &c), Decision::Run(TriggerKind::Manual));
    }

    #[test]
    fn a_run_already_in_flight_wins() {
        let cfg = cadence_only(Cadence::Daily);
        let c = Conditions {
            run_in_flight: true,
            manual_requested: true,
            ..overdue_by(&cfg, 0)
        };
        assert_eq!(decide(&cfg, &c), Decision::Idle);
    }

    #[test]
    fn a_busy_activity_gate_blocks_scheduled_runs_silently() {
        let cfg = cadence_only(Cadence::Daily);
        let c = Conditions {
            activity_busy: true,
            ..overdue_by(&cfg, 0)
        };
        assert_eq!(decide(&cfg, &c), Decision::Idle);
    }

    #[test]
    fn a_busy_activity_gate_requeues_a_manual_request_instead_of_dropping_it() {
        let cfg = cadence_only(Cadence::Daily);
        let c = Conditions {
            activity_busy: true,
            manual_requested: true,
            ..Conditions::idle_desktop(now_secs())
        };
        match decide(&cfg, &c) {
            Decision::Defer { requeue_manual, .. } => {
                assert!(requeue_manual, "the request must not be lost");
            }
            other => panic!("expected a deferral, got {other:?}"),
        }
    }

    #[test]
    fn an_engaged_ui_defers_scheduled_runs_but_not_manual_ones() {
        let cfg = cadence_only(Cadence::Daily);
        let engaged = Conditions {
            ui_engaged: true,
            ..overdue_by(&cfg, 0)
        };
        assert!(matches!(decide(&cfg, &engaged), Decision::Defer { .. }));

        let manual = Conditions {
            manual_requested: true,
            ..engaged
        };
        assert_eq!(decide(&cfg, &manual), Decision::Run(TriggerKind::Manual));
    }

    // -- Cadence trigger -------------------------------------------------

    #[test]
    fn cadence_manual_never_fires_on_time() {
        let cfg = ScheduleConfig {
            enabled: true,
            cadence: Cadence::Manual,
            threshold_enabled: false,
            ..Default::default()
        };
        let c = Conditions {
            last_run_at: Some(0), // as overdue as it gets
            ..Conditions::idle_desktop(now_secs())
        };
        assert_eq!(decide(&cfg, &c), Decision::Idle);
    }

    #[test]
    fn cadence_does_not_fire_before_it_is_due() {
        let cfg = cadence_only(Cadence::Weekly);
        let last = now_secs();
        let due = next_due_at(&cfg, Some(last)).unwrap();
        let c = Conditions {
            last_run_at: Some(last),
            ..Conditions::idle_desktop(due - 60)
        };
        assert_eq!(decide(&cfg, &c), Decision::Idle);
    }

    #[test]
    fn cadence_fires_once_due() {
        let cfg = cadence_only(Cadence::Daily);
        assert_eq!(
            decide(&cfg, &overdue_by(&cfg, 0)),
            Decision::Run(TriggerKind::Cadence)
        );
    }

    /// The missed-run catch-up guarantee: a machine that was off through its
    /// window must run at the next opportunity, not skip to the next slot.
    #[test]
    fn cadence_catches_up_after_the_machine_was_off_for_days() {
        let cfg = cadence_only(Cadence::Daily);
        let c = overdue_by(&cfg, 60 * 60 * 24 * 5);
        assert_eq!(decide(&cfg, &c), Decision::Run(TriggerKind::Cadence));
    }

    #[test]
    fn a_first_ever_run_is_not_immediately_due() {
        // Switching automation on should not kick off a scan on the spot.
        let cfg = cadence_only(Cadence::Daily);
        let c = Conditions {
            last_run_at: None,
            ..Conditions::idle_desktop(now_secs())
        };
        assert_eq!(decide(&cfg, &c), Decision::Idle);
    }

    // -- Cadence constraints ---------------------------------------------

    #[test]
    fn cadence_waits_for_idle_time() {
        let cfg = ScheduleConfig {
            run_only_when_idle: true,
            idle_minutes: 10,
            ..cadence_only(Cadence::Daily)
        };
        let c = Conditions {
            idle_secs: 60, // 1 minute — not enough
            ..overdue_by(&cfg, 0)
        };
        match decide(&cfg, &c) {
            Decision::Defer { reason, .. } => assert!(
                reason.contains("idle"),
                "reason should explain the wait, got {reason:?}"
            ),
            other => panic!("expected a deferral, got {other:?}"),
        }
    }

    #[test]
    fn cadence_runs_once_the_idle_threshold_is_met() {
        let cfg = ScheduleConfig {
            run_only_when_idle: true,
            idle_minutes: 10,
            ..cadence_only(Cadence::Daily)
        };
        let c = Conditions {
            idle_secs: 11 * 60,
            ..overdue_by(&cfg, 0)
        };
        assert_eq!(decide(&cfg, &c), Decision::Run(TriggerKind::Cadence));
    }

    #[test]
    fn cadence_waits_for_ac_power() {
        let cfg = ScheduleConfig {
            skip_on_battery: true,
            run_only_when_idle: false,
            ..cadence_only(Cadence::Daily)
        };
        let c = Conditions {
            on_battery: true,
            ..overdue_by(&cfg, 0)
        };
        match decide(&cfg, &c) {
            Decision::Defer { reason, .. } => assert!(reason.contains("power")),
            other => panic!("expected a deferral, got {other:?}"),
        }
    }

    #[test]
    fn cadence_ignores_the_battery_constraint_when_switched_off() {
        let cfg = ScheduleConfig {
            skip_on_battery: false,
            run_only_when_idle: false,
            ..cadence_only(Cadence::Daily)
        };
        let c = Conditions {
            on_battery: true,
            ..overdue_by(&cfg, 0)
        };
        assert_eq!(decide(&cfg, &c), Decision::Run(TriggerKind::Cadence));
    }

    /// "Only when idle" must not mean "never" for someone always at their desk.
    #[test]
    fn constraints_stop_applying_once_past_the_grace_window() {
        let cfg = ScheduleConfig {
            run_only_when_idle: true,
            idle_minutes: 30,
            skip_on_battery: true,
            ..cadence_only(Cadence::Daily)
        };
        // Actively typing, on battery — every constraint failing.
        let hostile = Conditions {
            idle_secs: 0,
            on_battery: true,
            ..overdue_by(&cfg, CADENCE_GRACE_SECS - 60)
        };
        assert!(
            matches!(decide(&cfg, &hostile), Decision::Defer { .. }),
            "inside the grace window the constraints still hold"
        );

        let past_grace = Conditions {
            idle_secs: 0,
            on_battery: true,
            ..overdue_by(&cfg, CADENCE_GRACE_SECS + 60)
        };
        assert_eq!(
            decide(&cfg, &past_grace),
            Decision::Run(TriggerKind::Cadence),
            "past the grace window the run happens regardless"
        );
    }

    // -- Capacity trigger ------------------------------------------------

    #[test]
    fn threshold_fires_when_the_drive_is_over_the_limit() {
        let cfg = threshold_only(85);
        let c = Conditions {
            disk_used_percent: Some(85.0),
            ..Conditions::idle_desktop(now_secs())
        };
        assert_eq!(decide(&cfg, &c), Decision::Run(TriggerKind::Threshold));
    }

    #[test]
    fn threshold_stays_quiet_below_the_limit() {
        let cfg = threshold_only(85);
        let c = Conditions {
            disk_used_percent: Some(84.9),
            ..Conditions::idle_desktop(now_secs())
        };
        assert_eq!(decide(&cfg, &c), Decision::Idle);
    }

    #[test]
    fn threshold_respects_its_cooldown() {
        let cfg = threshold_only(85);
        let now = now_secs();
        let hot = Conditions {
            disk_used_percent: Some(97.0),
            last_threshold_run_at: Some(now - (THRESHOLD_COOLDOWN_SECS - 60)),
            ..Conditions::idle_desktop(now)
        };
        assert_eq!(
            decide(&cfg, &hot),
            Decision::Idle,
            "a still-full drive must not be re-scanned every tick"
        );

        let cooled = Conditions {
            last_threshold_run_at: Some(now - (THRESHOLD_COOLDOWN_SECS + 60)),
            ..hot
        };
        assert_eq!(decide(&cfg, &cooled), Decision::Run(TriggerKind::Threshold));
    }

    /// Disk pressure is urgent, so it doesn't wait for the user to leave.
    #[test]
    fn threshold_ignores_the_idle_requirement() {
        let cfg = ScheduleConfig {
            run_only_when_idle: true,
            idle_minutes: 30,
            ..threshold_only(85)
        };
        let c = Conditions {
            disk_used_percent: Some(90.0),
            idle_secs: 0, // actively typing
            ..Conditions::idle_desktop(now_secs())
        };
        assert_eq!(decide(&cfg, &c), Decision::Run(TriggerKind::Threshold));
    }

    #[test]
    fn threshold_still_waits_for_power_when_not_critical() {
        let cfg = ScheduleConfig {
            skip_on_battery: true,
            ..threshold_only(85)
        };
        let c = Conditions {
            disk_used_percent: Some(90.0),
            on_battery: true,
            ..Conditions::idle_desktop(now_secs())
        };
        assert!(matches!(decide(&cfg, &c), Decision::Defer { .. }));
    }

    #[test]
    fn a_nearly_full_drive_overrides_the_battery_constraint() {
        let cfg = ScheduleConfig {
            skip_on_battery: true,
            ..threshold_only(85)
        };
        let c = Conditions {
            disk_used_percent: Some(CRITICAL_USED_PERCENT + 1.0),
            on_battery: true,
            ..Conditions::idle_desktop(now_secs())
        };
        assert_eq!(decide(&cfg, &c), Decision::Run(TriggerKind::Threshold));
    }

    #[test]
    fn an_unreadable_drive_does_not_trigger_anything() {
        let cfg = threshold_only(85);
        let c = Conditions {
            disk_used_percent: None,
            ..Conditions::idle_desktop(now_secs())
        };
        assert_eq!(decide(&cfg, &c), Decision::Idle);
    }

    #[test]
    fn a_disabled_threshold_ignores_a_full_drive() {
        let cfg = cadence_only(Cadence::Manual); // threshold_enabled: false
        let c = Conditions {
            disk_used_percent: Some(99.0),
            ..Conditions::idle_desktop(now_secs())
        };
        assert_eq!(decide(&cfg, &c), Decision::Idle);
    }

    // -- Trigger precedence ----------------------------------------------

    #[test]
    fn a_full_drive_takes_precedence_over_an_overdue_cadence() {
        let cfg = ScheduleConfig {
            threshold_enabled: true,
            threshold_percent: 85,
            ..cadence_only(Cadence::Daily)
        };
        let c = Conditions {
            disk_used_percent: Some(92.0),
            ..overdue_by(&cfg, 0)
        };
        assert_eq!(decide(&cfg, &c), Decision::Run(TriggerKind::Threshold));
    }

    #[test]
    fn cadence_still_fires_when_the_threshold_is_in_cooldown() {
        let cfg = ScheduleConfig {
            threshold_enabled: true,
            threshold_percent: 85,
            run_only_when_idle: false,
            ..cadence_only(Cadence::Daily)
        };
        let base = overdue_by(&cfg, 0);
        let c = Conditions {
            disk_used_percent: Some(92.0),
            last_threshold_run_at: Some(base.now - 60),
            ..base
        };
        assert_eq!(decide(&cfg, &c), Decision::Run(TriggerKind::Cadence));
    }
}
