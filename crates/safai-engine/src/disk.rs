//! Volume free/total space queries.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Free/total space for the drive containing a path.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveInfo {
    pub mount: String,
    pub free_bytes: u64,
    pub total_bytes: u64,
}

/// Returns `(free_bytes_available_to_caller, total_bytes)` for the volume
/// containing `path`, or `None` if the query fails.
#[cfg(windows)]
pub fn disk_free_total(path: &Path) -> Option<(u64, u64)> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);

    let mut free_to_caller: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut total_free: u64 = 0;

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

/// Build a [`DriveInfo`] for `path`, using `mount` as the display label.
pub fn drive_info_for(path: &Path, mount: String) -> Option<DriveInfo> {
    let (free_bytes, total_bytes) = disk_free_total(path)?;
    Some(DriveInfo {
        mount,
        free_bytes,
        total_bytes,
    })
}
