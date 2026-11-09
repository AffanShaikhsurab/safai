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

/// Root paths (`C:\`, `D:\`, …) of every local fixed drive.
///
/// Removable, network, optical and RAM drives are left out on purpose: the
/// dev-project scan walks whole drives, and a USB stick or network share is
/// slow to walk and not where anyone keeps an active `node_modules`.
#[cfg(windows)]
pub fn fixed_drives() -> Vec<std::path::PathBuf> {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    const DRIVE_FIXED: u32 = 3;

    // SAFETY: takes no arguments; returns a bitmask of present drive letters.
    let mask = unsafe { GetLogicalDrives() };
    (0..26u32)
        .filter(|bit| mask & (1 << bit) != 0)
        .filter_map(|bit| {
            let letter = char::from(b'A' + bit as u8);
            let root = format!("{letter}:\\");
            let wide: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
            // SAFETY: `wide` is a valid, null-terminated UTF-16 root path that
            // outlives the call; the API only reads it.
            let kind = unsafe { GetDriveTypeW(wide.as_ptr()) };
            (kind == DRIVE_FIXED).then(|| std::path::PathBuf::from(root))
        })
        .collect()
}

#[cfg(not(windows))]
pub fn fixed_drives() -> Vec<std::path::PathBuf> {
    Vec::new()
}

// ---------------------------------------------------------------------------
// Volume details (labels, serials, free space)
// ---------------------------------------------------------------------------

/// Everything the UI needs to describe one fixed volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeDetail {
    /// Display mount, e.g. `"C:"`.
    pub mount: String,
    /// Root path safe to pass to scan commands, e.g. `"C:/"`.
    pub root: String,
    pub label: String,
    pub file_system: String,
    pub free_bytes: u64,
    pub total_bytes: u64,
    /// Windows volume serial number, or `0` when it could not be read.
    pub serial: u32,
    /// False when the volume is locked, empty, or inaccessible.
    pub readable: bool,
    pub system: bool,
    pub kind: String,
}

/// Enumerate ready fixed drives with label, filesystem and free space.
///
/// Fixed drives only, matching [`fixed_drives`]: removable, network, optical and
/// RAM volumes are deliberately excluded. A volume that exists but cannot be
/// read (BitLocker locked, no media, access denied) is still returned with
/// `readable = false`, so the UI can show it as unavailable rather than
/// silently pretending it does not exist.
#[cfg(windows)]
pub fn drive_details() -> Vec<VolumeDetail> {
    use std::collections::HashSet;
    use windows_sys::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };

    const DRIVE_FIXED: u32 = 3;
    const MAX_VOLUME_NAME: usize = 261; // MAX_PATH + 1, in TCHARs

    let system = system_drive_mount();
    // SAFETY: GetLogicalDrives takes no arguments and returns a bitmask.
    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    // SUBST drives report DRIVE_FIXED and share the host volume's serial.
    // Keeping the first (lowest-letter) entry avoids scanning the same tree
    // twice, which would double-count every byte.
    let mut seen: HashSet<(u32, u64)> = HashSet::new();

    for bit in 0..26u32 {
        if mask & (1 << bit) == 0 {
            continue;
        }
        let letter = char::from(b'A' + bit as u8);
        let root = format!("{letter}:\\");
        let wide: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();

        // SAFETY: `wide` is a valid, NUL-terminated root path, and the API only
        // reads it.
        let drive_type = unsafe { GetDriveTypeW(wide.as_ptr()) };
        if drive_type != DRIVE_FIXED {
            continue;
        }

        let mut label = [0u16; MAX_VOLUME_NAME];
        let mut fs_name = [0u16; MAX_VOLUME_NAME];
        let mut serial = 0u32;
        let mut max_component = 0u32;
        let mut flags = 0u32;

        // SAFETY: both buffers are valid for MAX_VOLUME_NAME u16s and outlive
        // the call; every out-pointer references a live stack local.
        let info_ok = unsafe {
            GetVolumeInformationW(
                wide.as_ptr(),
                label.as_mut_ptr(),
                label.len() as u32,
                &mut serial,
                &mut max_component,
                &mut flags,
                fs_name.as_mut_ptr(),
                fs_name.len() as u32,
            )
        } != 0;

        let mount = format!("{letter}:");
        let space = disk_free_total(std::path::Path::new(&root));
        let (free_bytes, total_bytes) = space.unwrap_or((0, 0));

        if serial != 0 && !seen.insert((serial, total_bytes)) {
            continue;
        }

        out.push(VolumeDetail {
            mount: mount.clone(),
            root: format!("{letter}:/"),
            label: utf16z(&label),
            file_system: utf16z(&fs_name),
            free_bytes,
            total_bytes,
            serial,
            readable: info_ok && space.is_some() && total_bytes > 0,
            system: system.as_deref() == Some(mount.as_str()),
            kind: "fixed".to_string(),
        });
    }

    out.sort_by(|a, b| b.system.cmp(&a.system).then_with(|| a.mount.cmp(&b.mount)));
    out
}

#[cfg(not(windows))]
pub fn drive_details() -> Vec<VolumeDetail> {
    Vec::new()
}

/// The drive Windows itself boots from, normally `"C:"`.
///
/// `%SystemDrive%` can legitimately be `D:` on a custom install, so this is
/// never hard-coded. Falls back to `%SystemRoot%`, then the first fixed drive.
pub fn system_drive_mount() -> Option<String> {
    let system_drive = std::env::var("SystemDrive").ok();
    let system_root = std::env::var("SystemRoot").ok();
    let drives: Vec<String> = fixed_drives()
        .iter()
        .filter_map(|p| safai_rules::mount_key(p))
        .collect();
    pick_system_mount(system_drive.as_deref(), system_root.as_deref(), &drives)
}

/// Pure resolver for [`system_drive_mount`], kept separate so it can be tested
/// without touching the real environment.
pub fn pick_system_mount(
    system_drive: Option<&str>,
    system_root: Option<&str>,
    drives: &[String],
) -> Option<String> {
    if let Some(mount) = system_drive.and_then(safai_rules::normalize_mount) {
        return Some(mount);
    }
    if let Some(root) = system_root {
        if let Some(mount) = safai_rules::mount_key(std::path::Path::new(root)) {
            return Some(mount);
        }
    }
    drives.first().cloned()
}

/// Decode a NUL-terminated UTF-16 buffer.
fn utf16z(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// Percentage of the volume containing `path` that is in use (0–100), or
/// `None` if the volume can't be queried or reports a zero size.
#[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_drive_env_wins() {
        assert_eq!(
            pick_system_mount(Some("d:"), Some(r"C:\Windows"), &["C:".into()]),
            Some("D:".into())
        );
    }

    #[test]
    fn system_drive_falls_back_to_system_root() {
        assert_eq!(
            pick_system_mount(None, Some(r"D:\Windows"), &["C:".into()]),
            Some("D:".into())
        );
    }

    #[test]
    fn system_drive_falls_back_to_the_first_enumerated_drive() {
        assert_eq!(
            pick_system_mount(None, None, &["E:".into(), "F:".into()]),
            Some("E:".into())
        );
        assert_eq!(pick_system_mount(None, None, &[]), None);
    }

    #[test]
    fn utf16_buffers_are_trimmed_at_the_first_nul() {
        assert_eq!(utf16z(&[b'C' as u16, b':' as u16, 0, b'X' as u16]), "C:");
        assert_eq!(utf16z(&[]), "");
        assert_eq!(utf16z(&[b'A' as u16, b'B' as u16]), "AB");
    }

    #[cfg(windows)]
    #[test]
    fn drive_enumeration_finds_the_system_volume() {
        let details = drive_details();
        assert!(
            !details.is_empty(),
            "a Windows machine has at least one fixed drive"
        );
        let system = system_drive_mount().expect("system drive");
        assert!(
            details
                .iter()
                .any(|d| d.system && d.mount.eq_ignore_ascii_case(&system)),
            "the system drive {system} should be flagged"
        );
        assert!(details.iter().all(|d| d.kind == "fixed"));
        assert!(details.iter().any(|d| d.readable));
        if std::path::Path::new(r"D:\").is_dir() {
            assert!(
                details.iter().any(|d| d.mount == "D:"),
                "a mounted D: should be enumerated"
            );
        }
    }
}
