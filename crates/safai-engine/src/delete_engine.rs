//! Robust, parallel file deletion engine for Windows.
//!
//! This module addresses the "stuck at 0/12" bug by:
//! 1. Running each deletion with a per-item timeout (no single item blocks forever)
//! 2. Processing items in parallel (one stuck item doesn't block others)
//! 3. Retrying transient sharing violations with exponential backoff
//! 4. Handling read-only files (strip attribute, retry)
//! 5. Supporting long paths (>260 chars) via extended-length prefix
//! 6. Pre-checking if a directory is accessible before attempting deletion
//!
//! References: docs/research-windows-file-deletion.md

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Maximum time to wait for a single item's deletion before skipping it.
const DELETE_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum retry attempts for transient lock errors.
const MAX_RETRIES: u32 = 5;

/// Initial retry delay (doubles each attempt: 10, 20, 40, 80, 160ms).
const INITIAL_RETRY_DELAY_MS: u64 = 10;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Result of attempting to delete a single item.
pub enum DeleteOutcome {
    /// Successfully deleted.
    Success,
    /// Failed with an error message.
    Error(String),
    /// Timed out — the deletion thread is still running but we moved on.
    Timeout,
}

/// Delete a single path (file or directory) with timeout protection.
///
/// If `to_recycle_bin` is true, uses the `trash` crate (IFileOperation COM).
/// Otherwise uses our robust recursive removal with retries.
///
/// Returns within `DELETE_TIMEOUT` regardless of whether the underlying
/// operation completes.
pub fn delete_with_timeout(path: &Path, to_recycle_bin: bool) -> DeleteOutcome {
    let path_owned = path.to_path_buf();
    let (tx, rx) = mpsc::channel();

    // Spawn a dedicated thread for this deletion so it can't block anyone else.
    thread::spawn(move || {
        let result = if to_recycle_bin {
            trash::delete(&path_owned).map_err(|e| format!("{e}"))
        } else if path_owned.is_dir() {
            remove_dir_all_robust(&path_owned).map_err(|e| format!("{e}"))
        } else {
            remove_file_robust(&path_owned).map_err(|e| format!("{e}"))
        };
        // Send result back; ignore errors (receiver may have timed out).
        let _ = tx.send(result);
    });

    match rx.recv_timeout(DELETE_TIMEOUT) {
        Ok(Ok(())) => DeleteOutcome::Success,
        Ok(Err(msg)) => DeleteOutcome::Error(msg),
        Err(mpsc::RecvTimeoutError::Timeout) => DeleteOutcome::Timeout,
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            DeleteOutcome::Error("deletion thread panicked".to_string())
        }
    }
}

/// Quick pre-check: can we likely access this path for deletion?
/// Returns `Ok(())` if the path looks deletable, `Err(reason)` if it's clearly
/// blocked (e.g. doesn't exist anymore, or can't even be stat'd).
///
/// This does NOT guarantee deletion will succeed, but catches the common case
/// of already-deleted items and obvious permission problems up front.
pub fn preflight_check(path: &Path) -> Result<(), String> {
    if !path.exists() {
        // Already gone — treat as success upstream.
        return Err("already deleted".to_string());
    }

    // On Windows: try opening the root for DELETE access. If this fails,
    // the directory/file is likely locked.
    #[cfg(windows)]
    can_open_for_delete(path)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Robust recursive directory removal
// ---------------------------------------------------------------------------

/// Recursively remove a directory, handling Windows-specific issues:
/// - Read-only files (strips attribute, retries)
/// - Transient sharing violations (retries with backoff)
/// - Long paths (uses extended-length prefix)
/// - Reparse points / junctions (removes without following)
pub fn remove_dir_all_robust(path: &Path) -> io::Result<()> {
    // Use extended-length path prefix for long path support on Windows.
    let work_path = to_extended_length(path);
    remove_dir_contents_robust(&work_path)?;
    remove_dir_with_retry(&work_path)
}

/// Remove a single file with retry logic for transient locks + read-only.
pub fn remove_file_robust(path: &Path) -> io::Result<()> {
    let work_path = to_extended_length(path);
    remove_file_with_retry(&work_path)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn remove_dir_contents_robust(dir: &Path) -> io::Result<()> {
    let entries: Vec<_> = match fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(e) => {
            // If we can't read the directory at all, report it.
            return Err(e);
        }
    };

    for entry in entries {
        let entry_path = entry.path();
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue, // Skip entries we can't classify
        };

        if ft.is_dir() {
            // Check for reparse points (junctions, symlinks to dirs).
            // These should be removed without recursing into them.
            if is_reparse_point(&entry_path) {
                // Remove the junction/symlink itself, don't follow it.
                let _ = remove_dir_with_retry(&entry_path);
            } else {
                // Regular directory: recurse.
                remove_dir_contents_robust(&entry_path)?;
                remove_dir_with_retry(&entry_path)?;
            }
        } else {
            // File or symlink-to-file.
            let _ = remove_file_with_retry(&entry_path);
            // Don't hard-fail on individual files — continue with the rest.
        }
    }

    Ok(())
}

fn remove_file_with_retry(path: &Path) -> io::Result<()> {
    let mut delay = Duration::from_millis(INITIAL_RETRY_DELAY_MS);

    for attempt in 0..MAX_RETRIES {
        match fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                let code = e.raw_os_error().unwrap_or(0);
                match code {
                    // ERROR_FILE_NOT_FOUND (2) / ERROR_PATH_NOT_FOUND (3)
                    // Already gone — success.
                    2 | 3 => return Ok(()),

                    // ERROR_ACCESS_DENIED (5) — might be read-only.
                    5 => {
                        if attempt == 0 {
                            // Try stripping read-only attribute.
                            if let Ok(meta) = fs::metadata(path) {
                                let mut perms = meta.permissions();
                                if perms.readonly() {
                                    // clippy::permissions_set_readonly_false
                                    // warns because on Unix this grants
                                    // world-writable permissions. Safai is
                                    // Windows-first and this path is reached
                                    // only after ERROR_ACCESS_DENIED, where
                                    // clearing the read-only attribute is
                                    // exactly the intended fix.
                                    #[allow(clippy::permissions_set_readonly_false)]
                                    perms.set_readonly(false);
                                    let _ = fs::set_permissions(path, perms);
                                    continue; // Retry immediately.
                                }
                            }
                        }
                        if attempt >= MAX_RETRIES - 1 {
                            return Err(e);
                        }
                        thread::sleep(delay);
                        delay = delay.saturating_mul(2).min(Duration::from_millis(500));
                    }

                    // ERROR_SHARING_VIOLATION (32) / ERROR_LOCK_VIOLATION (33)
                    // Transient — another process has a handle. Retry with backoff.
                    32 | 33 => {
                        if attempt >= MAX_RETRIES - 1 {
                            return Err(e);
                        }
                        thread::sleep(delay);
                        delay = delay.saturating_mul(2).min(Duration::from_millis(500));
                    }

                    // Anything else is a hard failure.
                    _ => return Err(e),
                }
            }
        }
    }

    // Final attempt.
    fs::remove_file(path)
}

fn remove_dir_with_retry(path: &Path) -> io::Result<()> {
    let mut delay = Duration::from_millis(INITIAL_RETRY_DELAY_MS);

    for attempt in 0..MAX_RETRIES {
        match fs::remove_dir(path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                let code = e.raw_os_error().unwrap_or(0);
                match code {
                    // Already gone.
                    2 | 3 => return Ok(()),

                    // ERROR_DIR_NOT_EMPTY (145): race with "delete pending" state.
                    // ERROR_SHARING_VIOLATION (32): handle still open.
                    // ERROR_ACCESS_DENIED (5): could be read-only dir on some FS.
                    5 | 32 | 145 => {
                        if attempt >= MAX_RETRIES - 1 {
                            return Err(e);
                        }
                        thread::sleep(delay);
                        delay = delay.saturating_mul(2).min(Duration::from_millis(500));
                    }

                    _ => return Err(e),
                }
            }
        }
    }

    fs::remove_dir(path)
}

/// Convert path to `\\?\` extended-length form for >260 char support on Windows.
/// On non-Windows, returns the path unchanged.
fn to_extended_length(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let s = path.to_string_lossy();
        if s.starts_with(r"\\?\") {
            path.to_path_buf()
        } else if let Ok(canonical) = fs::canonicalize(path) {
            // canonicalize on Windows already produces \\?\ prefix
            canonical
        } else {
            // Fallback: try to use the path as-is.
            path.to_path_buf()
        }
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

/// Check if a path is a reparse point (junction, symlink, mount point) on Windows.
fn is_reparse_point(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if let Ok(meta) = fs::symlink_metadata(path) {
            // FILE_ATTRIBUTE_REPARSE_POINT = 0x400
            meta.file_attributes() & 0x400 != 0
        } else {
            false
        }
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

/// Windows-specific: attempt to open the path for DELETE access.
/// If it fails, the file/directory is likely locked by another process.
#[cfg(windows)]
fn can_open_for_delete(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    // DELETE = 0x00010000
    const DELETE: u32 = 0x00010000;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();

    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            DELETE,
            FILE_SHARE_DELETE | FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS, // Required for directories
            std::ptr::null_mut(),
        )
    };

    if handle == INVALID_HANDLE_VALUE {
        let err = io::Error::last_os_error();
        let code = err.raw_os_error().unwrap_or(0);
        let msg = match code {
            5 => "access denied — may require elevated permissions".to_string(),
            32 => "file is being used by another program".to_string(),
            33 => "file is locked by another program".to_string(),
            _ => format!("cannot access for deletion: {err}"),
        };
        return Err(msg);
    }

    unsafe {
        CloseHandle(handle);
    }
    Ok(())
}

/// Map a Windows error code to a user-friendly message.
#[allow(dead_code)]
pub fn friendly_error_message(code: i32, path: &str) -> String {
    match code {
        2 => format!("'{}' not found (already deleted)", short_name(path)),
        3 => format!("path not found: '{}'", short_name(path)),
        5 => format!(
            "access denied: '{}' may be read-only or in use",
            short_name(path)
        ),
        32 => format!("'{}' is being used by another program", short_name(path)),
        33 => format!("'{}' is locked by another program", short_name(path)),
        145 => format!(
            "'{}' is not empty (files appeared during deletion)",
            short_name(path)
        ),
        206 => format!("path too long: '{}'", short_name(path)),
        _ => format!("failed to delete '{}': error {}", short_name(path), code),
    }
}

/// Extract just the last path component for display.
#[allow(dead_code)]
fn short_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
