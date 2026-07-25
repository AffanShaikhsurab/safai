//! Proactive maintenance: run a scan (and optionally a cleanup) on a schedule
//! or when the drive fills up, without the user having to open the app.
//!
//! * [`config`] — the persisted settings, the audit trail, and the wall-clock
//!   arithmetic that decides when the next run is due.
//! * [`decision`] — a pure function from "state of the world" to "what to do".
//! * [`policy`] — the whitelist that decides what an unattended run may delete.
//! * [`runner`] — the tick loop, observation, and execution.
//!
//! The split between [`decision`] and [`runner`] is what keeps the feature
//! testable: all the branching lives in a function with no `AppHandle`, no Win32
//! calls and no clock, so the full trigger matrix is covered by unit tests.
//!
//! Why a coarse wall-clock tick rather than one long sleep, and why polling disk
//! space beats a WMI subscription, is documented at the top of [`runner`]. How
//! the constraints map onto Windows Task Scheduler's conditions is documented at
//! the top of [`decision`].

pub mod config;
pub mod decision;
pub mod policy;
pub mod runner;

pub use config::{RunPhase, ScheduleConfig};
pub use runner::{apply_config, init, push_status, runtime, status, AutomationStatus};
