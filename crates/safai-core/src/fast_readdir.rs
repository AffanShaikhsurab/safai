//! Windows-optimized directory reading using FindFirstFileExW + FIND_FIRST_EX_LARGE_FETCH.
//!
//! This module replaces `std::fs::read_dir` on Windows with a direct Win32 call that:
//! 1. Uses `FIND_FIRST_EX_LARGE_FETCH` — tells the kernel to batch directory entries
//!    in a 64KB+ buffer instead of fetching one-at-a-time. 10-30% faster on large dirs.
//! 2. Uses `FindExInfoBasic` — skips fetching legacy 8.3 short names (saves ~10%).
//! 3. Extracts file size directly from `WIN32_FIND_DATAW` — eliminates the need for a
//!    separate `symlink_metadata` call per file. This is the biggest win: avoids one
//!    extra syscall per file entirely.
//!
//! On non-Windows platforms, this module provides a thin wrapper around `std::fs::read_dir`
//! that reads file sizes from metadata.
//!
//! References: docs/research-parallel-scanning.md §2.2

use std::path::{Path, PathBuf};

/// A directory entry with pre-fetched file size and type info.
/// On Windows, all fields come from a single FindFirstFileEx call (no extra stat).
#[derive(Debug)]
pub struct FastEntry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    /// File size. For directories this is 0 (on-disk overhead, not content size).
    /// For regular files, this is the file size from the directory entry.
    pub size: u64,
    /// Raw file attributes (Windows-specific). 0 on other platforms.
    pub attributes: u32,
    /// Last-write time as unix seconds, or 0 when unknown. On Windows this
    /// comes straight from the directory listing (no extra syscall).
    pub modified_secs: u64,
}

/// Convert a Windows FILETIME (100ns ticks since 1601-01-01) to unix seconds.
/// Returns 0 for times before the unix epoch.
#[cfg(windows)]
fn filetime_to_unix(low: u32, high: u32) -> u64 {
    const TICKS_PER_SEC: u64 = 10_000_000;
    const EPOCH_DIFF_SECS: u64 = 11_644_473_600;
    let ticks = ((high as u64) << 32) | (low as u64);
    (ticks / TICKS_PER_SEC).saturating_sub(EPOCH_DIFF_SECS)
}

/// Read all entries in a directory using the fastest available platform method.
///
/// Returns entries with file size already populated (no extra stat needed for files).
/// Skips `.` and `..` entries. Returns an empty Vec on read errors (never panics).
pub fn read_dir_fast(dir: &Path) -> Vec<FastEntry> {
    #[cfg(windows)]
    {
        read_dir_win32(dir)
    }
    #[cfg(not(windows))]
    {
        read_dir_std(dir)
    }
}

// ---------------------------------------------------------------------------
// Windows implementation
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn read_dir_win32(dir: &Path) -> Vec<FastEntry> {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    // These constants come from the Windows SDK headers.
    // We define them inline to avoid pulling in the full `windows-sys` crate
    // into safai-core (which is a pure scanner library).
    const FIND_FIRST_EX_LARGE_FETCH: u32 = 0x00000002;
    const INVALID_HANDLE_VALUE: isize = -1;
    const ERROR_NO_MORE_FILES: u32 = 18;

    // FindExInfoBasic = 1 (skips short names)
    const FIND_EX_INFO_BASIC: u32 = 1;
    // FindExSearchNameMatch = 0
    const FIND_EX_SEARCH_NAME_MATCH: u32 = 0;

    // File attribute flags.
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

    // WIN32_FIND_DATAW layout (from winapi headers):
    #[repr(C)]
    #[allow(non_snake_case)]
    struct WIN32_FIND_DATAW {
        dwFileAttributes: u32,
        ftCreationTime: [u32; 2],
        ftLastAccessTime: [u32; 2],
        ftLastWriteTime: [u32; 2],
        nFileSizeHigh: u32,
        nFileSizeLow: u32,
        dwReserved0: u32,
        dwReserved1: u32,
        cFileName: [u16; 260],
        cAlternateFileName: [u16; 14],
    }

    extern "system" {
        fn FindFirstFileExW(
            lpFileName: *const u16,
            fInfoLevelId: u32,
            lpFindFileData: *mut WIN32_FIND_DATAW,
            fSearchOp: u32,
            lpSearchFilter: *const std::ffi::c_void,
            dwAdditionalFlags: u32,
        ) -> isize;
        fn FindNextFileW(hFindFile: isize, lpFindFileData: *mut WIN32_FIND_DATAW) -> i32;
        fn FindClose(hFindFile: isize) -> i32;
        fn GetLastError() -> u32;
    }

    // Build the search pattern: "dir\*"
    let pattern = dir.join("*");
    let wide: Vec<u16> = pattern.as_os_str().encode_wide().chain(Some(0)).collect();

    let mut find_data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };

    let handle = unsafe {
        FindFirstFileExW(
            wide.as_ptr(),
            FIND_EX_INFO_BASIC,
            &mut find_data,
            FIND_EX_SEARCH_NAME_MATCH,
            std::ptr::null(),
            FIND_FIRST_EX_LARGE_FETCH,
        )
    };

    if handle == INVALID_HANDLE_VALUE {
        return Vec::new(); // Can't read directory — return empty.
    }

    let mut entries = Vec::with_capacity(64);

    loop {
        // Parse the current find_data.
        let name_len = find_data
            .cFileName
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(260);
        let name_slice = &find_data.cFileName[..name_len];

        // Skip "." and ".."
        if !is_dot_or_dotdot(name_slice) {
            let name = OsString::from_wide(name_slice);
            let child_path = dir.join(&name);
            let attrs = find_data.dwFileAttributes;
            let is_dir = (attrs & FILE_ATTRIBUTE_DIRECTORY) != 0;
            let is_reparse = (attrs & FILE_ATTRIBUTE_REPARSE_POINT) != 0;
            // Symlinks / junctions are reparse points. A reparse point that is
            // also a "directory" is a directory symlink / junction.
            let is_symlink = is_reparse;
            let size = ((find_data.nFileSizeHigh as u64) << 32) | (find_data.nFileSizeLow as u64);

            entries.push(FastEntry {
                path: child_path,
                is_dir: is_dir && !is_reparse, // Real directories only
                is_symlink,
                size,
                attributes: attrs,
                modified_secs: filetime_to_unix(
                    find_data.ftLastWriteTime[0],
                    find_data.ftLastWriteTime[1],
                ),
            });
        }

        // Fetch next entry.
        if unsafe { FindNextFileW(handle, &mut find_data) } == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_NO_MORE_FILES {
                break;
            }
            // Any other error: stop iteration.
            break;
        }
    }

    unsafe {
        FindClose(handle);
    }

    entries
}

#[cfg(windows)]
fn is_dot_or_dotdot(name: &[u16]) -> bool {
    match name.len() {
        1 => name[0] == b'.' as u16,
        2 => name[0] == b'.' as u16 && name[1] == b'.' as u16,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Non-Windows fallback
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
fn read_dir_std(dir: &Path) -> Vec<FastEntry> {
    use std::fs;

    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };

    let mut entries = Vec::with_capacity(64);

    for entry in rd {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        let md = entry.metadata().ok();
        let size = if ft.is_file() {
            md.as_ref().map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };
        let modified_secs = md
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        entries.push(FastEntry {
            path: entry.path(),
            is_dir: ft.is_dir(),
            is_symlink: ft.is_symlink(),
            size,
            attributes: 0,
            modified_secs,
        });
    }

    entries
}
