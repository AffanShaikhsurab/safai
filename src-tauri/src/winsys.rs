//! Windows platform facilities used by the background scheduler.
//!
//! Everything Safai needs from Win32 lives here so the `unsafe` surface stays
//! in one audited place:
//!
//! * [`disk_free_total`] — volume free/total bytes (`GetDiskFreeSpaceExW`).
//! * [`idle_secs`] — how long the interactive session has had no input
//!   (`GetLastInputInfo`), the local equivalent of Task Scheduler's idle
//!   trigger.
//! * [`on_battery`] — AC vs. battery (`GetSystemPowerStatus`), so automatic
//!   runs can be deferred on battery like a Task Scheduler
//!   `DisallowStartIfOnBatteries` setting.
//! * [`BackgroundMode`] — an RAII guard that puts the process into Windows
//!   background processing mode *and* EcoQoS for the duration of an automatic
//!   run.
//!
//! ## Why background mode matters here
//!
//! `PROCESS_MODE_BACKGROUND_BEGIN` lowers the process's CPU **and I/O**
//! priority. The I/O half is the important one for Safai: an automatic scan
//! walks a lot of directory metadata, and at background I/O priority the
//! kernel keeps it out of the way of whatever the user is actually doing, so a
//! scheduled scan never shows up as a stutter. EcoQoS
//! (`PROCESS_POWER_THROTTLING_EXECUTION_SPEED`) additionally hints the
//! scheduler to favour efficiency cores and lower frequencies, which is what
//! Task Manager surfaces as "Efficiency mode".
//!
//! Non-Windows builds get inert stubs so the scheduler compiles everywhere
//! (Safai is Windows-first, but the crate should still build elsewhere).

use std::path::Path;

// ---------------------------------------------------------------------------
// Disk space
// ---------------------------------------------------------------------------

/// Returns `(free_bytes_available_to_caller, total_bytes)` for the volume
/// containing `path`, or `None` if the query fails.
///
/// This is a metadata-only call — it does not touch the platters — which is
/// what makes it cheap enough to poll on the scheduler's tick instead of
/// paying for a WMI `__InstanceModificationEvent` subscription (WMI implements
/// those by polling internally anyway, in a separate `WmiPrvSE` process).
#[cfg(windows)]
pub fn disk_free_total(path: &Path) -> Option<(u64, u64)> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    // Build a null-terminated UTF-16 path for the wide API.
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);

    let mut free_to_caller: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut total_free: u64 = 0;

    // SAFETY: `wide` is a valid, null-terminated UTF-16 buffer that outlives
    // the call. The three out-pointers reference stack locals that also
    // outlive it. The function only reads the path and writes the three u64
    // out-params.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_to_caller,
            &mut total_bytes,
            &mut total_free,
        )
    };

    if ok != 0 {
        Some((free_to_caller, total_bytes))
    } else {
        None
    }
}

#[cfg(not(windows))]
pub fn disk_free_total(_path: &Path) -> Option<(u64, u64)> {
    None
}

/// Percentage of the volume containing `path` that is in use (0–100), or
/// `None` if the volume can't be queried or reports a zero size.
pub fn disk_used_percent(path: &Path) -> Option<f64> {
    let (free, total) = disk_free_total(path)?;
    if total == 0 {
        return None;
    }
    let used = total.saturating_sub(free) as f64;
    Some((used / total as f64) * 100.0)
}

// ---------------------------------------------------------------------------
// User idle time
// ---------------------------------------------------------------------------

/// Seconds since the last keyboard/mouse input in this session.
///
/// `GetLastInputInfo` reports a tick count, so this is compared against
/// `GetTickCount` with wrapping arithmetic (the tick counter wraps roughly
/// every 49.7 days). Session-scoped by design — it answers "is the person at
/// this desk busy?", which is exactly the question the scheduler asks.
/// Returns `0` if the query fails, i.e. "assume the user is active", which is
/// the conservative answer.
#[cfg(windows)]
pub fn idle_secs() -> u64 {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };

    // SAFETY: `info.cbSize` is set to the struct's real size as the API
    // requires, and `info` is a live stack local for the duration of the call.
    let ok = unsafe { GetLastInputInfo(&mut info) };
    if ok == 0 {
        return 0;
    }

    // SAFETY: `GetTickCount` takes no arguments and has no preconditions.
    let now = unsafe { GetTickCount() };
    // Wrapping subtraction handles the ~49.7 day tick-counter rollover.
    u64::from(now.wrapping_sub(info.dwTime)) / 1000
}

#[cfg(not(windows))]
pub fn idle_secs() -> u64 {
    0
}

// ---------------------------------------------------------------------------
// Power source
// ---------------------------------------------------------------------------

/// Is the machine running on battery right now?
///
/// `false` when on AC, when the power state is unknown, or when the query
/// fails — a desktop with no battery reports "AC", which is what we want.
#[cfg(windows)]
pub fn on_battery() -> bool {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

    let mut status = SYSTEM_POWER_STATUS {
        ACLineStatus: 255,
        BatteryFlag: 255,
        BatteryLifePercent: 255,
        SystemStatusFlag: 0,
        BatteryLifeTime: 0,
        BatteryFullLifeTime: 0,
    };

    // SAFETY: `status` is a live stack local; the API only writes to it.
    let ok = unsafe { GetSystemPowerStatus(&mut status) };
    // ACLineStatus: 0 = offline (battery), 1 = online (AC), 255 = unknown.
    ok != 0 && status.ACLineStatus == 0
}

#[cfg(not(windows))]
pub fn on_battery() -> bool {
    false
}

// ---------------------------------------------------------------------------
// Background processing mode + EcoQoS
// ---------------------------------------------------------------------------

/// RAII guard that keeps the process in low-priority background mode (CPU and
/// I/O) plus EcoQoS while it is alive, restoring normal scheduling on drop.
///
/// Wrap every *automatic* run in one of these. Interactive runs deliberately
/// do **not** use it: when the user is watching a progress bar they want the
/// scan to be fast, not polite.
pub struct BackgroundMode {
    /// Whether the mode was actually applied (so `drop` knows to undo it).
    applied: bool,
}

impl BackgroundMode {
    /// Enter background processing mode. Always returns a guard; if the OS
    /// declines (or we're not on Windows) the guard is simply inert.
    pub fn enter() -> Self {
        BackgroundMode {
            applied: set_background(true),
        }
    }
}

impl Drop for BackgroundMode {
    fn drop(&mut self) {
        if self.applied {
            set_background(false);
        }
    }
}

/// Toggle background processing mode + EcoQoS. Returns whether it took effect.
#[cfg(windows)]
fn set_background(on: bool) -> bool {
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, ProcessPowerThrottling, SetPriorityClass, SetProcessInformation,
        PROCESS_MODE_BACKGROUND_BEGIN, PROCESS_MODE_BACKGROUND_END,
        PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        PROCESS_POWER_THROTTLING_STATE,
    };

    // SAFETY: `GetCurrentProcess` returns a pseudo-handle that needs no
    // cleanup and is always valid for the current process.
    let process = unsafe { GetCurrentProcess() };

    // 1. Background processing mode — lowers CPU *and* I/O priority. This is
    //    the setting that keeps a scheduled scan from stealing disk bandwidth
    //    from whatever the user is doing.
    //
    // SAFETY: `process` is the current-process pseudo-handle and the priority
    // class constants are the documented values for this API.
    let priority_ok = unsafe {
        SetPriorityClass(
            process,
            if on {
                PROCESS_MODE_BACKGROUND_BEGIN
            } else {
                PROCESS_MODE_BACKGROUND_END
            },
        ) != 0
    };

    // 2. EcoQoS — hints the scheduler to favour efficiency cores and lower
    //    clocks. Best-effort: unsupported on older Windows 10 builds, where
    //    the call simply fails and we keep the priority change.
    let mut state = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        StateMask: if on {
            PROCESS_POWER_THROTTLING_EXECUTION_SPEED
        } else {
            0
        },
    };

    // SAFETY: `state` is a live, fully-initialised struct of exactly the
    // length we pass, matching the `ProcessPowerThrottling` information class.
    unsafe {
        SetProcessInformation(
            process,
            ProcessPowerThrottling,
            (&mut state as *mut PROCESS_POWER_THROTTLING_STATE).cast(),
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );
    }

    priority_ok
}

#[cfg(not(windows))]
fn set_background(_on: bool) -> bool {
    false
}
