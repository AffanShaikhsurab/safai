//! Desktop scan-scope planning.
//!
//! The shared scan/delete execution cores live in `safai-engine` so the Tauri
//! app, the scheduler and the CLI all use one deletion path. This module keeps
//! the parts that are specific to the desktop app: turning the user's
//! drive/folder selection into a [`ScanConfig`], and committing the resulting
//! scan scope (including the deletion allow-list) into [`SafaiState`].
//!
//! The safety/performance table in one place:
//! * the system drive contributes its profile/AppData rule roots, never `C:\`;
//! * data drives contribute `D:\` as a Deep discovery root (and as an allowed
//!   deletion root);
//! * the dev walk starts at each selected drive root, where it prunes
//!   system/app folders itself;
//! * custom folders are discovery roots and allowed deletion roots.

pub use safai_engine::{
    delete_blocking, drive_mount, normalize_slashes, preview_delete, scan_blocking,
};

use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use safai_rules::{ScanConfig, ScanReport};
use serde::Deserialize;

use crate::error::{Result, SafaiError};
use crate::state::SafaiState;

/// The scope the UI asks for: which drives, which extra folders, and how deep.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScanRequest {
    /// Drive mounts, e.g. `["C:", "D:"]`. `"C:/"` is accepted too.
    pub drives: Vec<String>,
    /// Extra absolute folders picked by the user.
    pub folders: Vec<String>,
    /// Generic large-folder discovery. Quick = false, Deep = true.
    pub deep: bool,
}

impl Default for ScanRequest {
    fn default() -> Self {
        Self {
            drives: Vec::new(),
            folders: Vec::new(),
            // Preserve the old hard-coded behaviour for callers that omit it.
            deep: true,
        }
    }
}

/// A validated, on-disk scope.
#[derive(Debug, Clone)]
pub struct ResolvedScope {
    pub drives: Vec<String>,
    pub data_drives: Vec<String>,
    pub folders: Vec<PathBuf>,
    pub system_mount: Option<String>,
    pub deep: bool,
    pub warnings: Vec<String>,
}

/// A scan plan: the config the scan engine runs, plus the exact roots a later
/// deletion is allowed to touch.
#[derive(Debug, Clone)]
pub struct PreparedScan {
    pub cfg: ScanConfig,
    pub allowed_roots: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

const FORBIDDEN_FOLDER_NAMES: &[&str] = &[
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "$recycle.bin",
    "system volume information",
    "recovery",
    "$windows.~bt",
    "$windows.~ws",
    "$winreagent",
    "perflogs",
    "msocache",
    "config.msi",
];

// Deliberately *not* forbidden as custom folders: `drivers`, `intel`, `amd`,
// `nvidia`. Those are system-ish names at a drive root but also perfectly
// plausible project folder names, so Deep discovery skips them while an
// explicitly picked folder is allowed.

fn is_forbidden_folder(path: &Path) -> bool {
    path.components().any(|component| {
        let Component::Normal(name) = component else {
            return false;
        };
        let low = name.to_string_lossy().to_ascii_lowercase();
        FORBIDDEN_FOLDER_NAMES.contains(&low.as_str())
    })
}

fn push_unique(list: &mut Vec<PathBuf>, path: PathBuf) {
    let path_text = path.to_string_lossy();
    if !list
        .iter()
        .any(|existing| existing.to_string_lossy().eq_ignore_ascii_case(&path_text))
    {
        list.push(path);
    }
}

/// Validate a request against the live machine.
///
/// Empty drive + folder lists fall back to the system drive (or the profile
/// folders when no drive can be enumerated). An explicitly selected drive that
/// is unavailable is dropped with a warning rather than failing the whole scan;
/// only an entirely empty result is an error.
pub fn resolve_scope(request: &ScanRequest) -> Result<ResolvedScope> {
    let mut warnings = Vec::new();

    let fixed: Vec<String> = crate::winsys::fixed_drives()
        .iter()
        .filter_map(|p| safai_rules::mount_key(p))
        .collect();

    let mut drives: Vec<String> = Vec::new();
    for raw in &request.drives {
        let Some(mount) = safai_rules::normalize_mount(raw) else {
            warnings.push(format!("{raw} is not a drive letter; skipped."));
            continue;
        };
        if !fixed.iter().any(|m| m.eq_ignore_ascii_case(&mount)) {
            warnings.push(format!("{mount} is not an available fixed drive; skipped."));
            continue;
        }
        if !drives.iter().any(|m| m.eq_ignore_ascii_case(&mount)) {
            drives.push(mount);
        }
    }

    let mut folders: Vec<PathBuf> = Vec::new();
    let mut folder_canon: Vec<PathBuf> = Vec::new();
    for raw in &request.folders {
        let path = PathBuf::from(raw);
        if !path.is_absolute() {
            warnings.push(format!("{raw} is not an absolute folder; skipped."));
            continue;
        }
        if path.parent().is_none() {
            warnings.push(format!(
                "{raw} is a drive root; select it as a drive instead."
            ));
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            warnings.push(format!("{raw} does not exist; skipped."));
            continue;
        };
        if !meta.file_type().is_dir() || meta.file_type().is_symlink() {
            warnings.push(format!("{raw} is not a real folder; skipped."));
            continue;
        }
        if is_forbidden_folder(&path) {
            warnings.push(format!("{raw} is a system folder; skipped."));
            continue;
        }
        let Ok(canon) = std::fs::canonicalize(&path) else {
            warnings.push(format!("{raw} could not be resolved; skipped."));
            continue;
        };
        if folder_canon.iter().any(|existing| existing == &canon) {
            continue;
        }
        folder_canon.push(canon);
        folders.push(path);
    }

    let system_mount = crate::winsys::system_drive_mount();

    if drives.is_empty() && folders.is_empty() {
        if let Some(system) = system_mount.clone() {
            drives.push(system);
            warnings.push("No scope selected; defaulting to the system drive.".to_string());
        } else {
            folders = safai_rules::default_roots();
            warnings.push("No fixed drives found; defaulting to your profile folders.".to_string());
        }
    }

    if drives.is_empty() && folders.is_empty() {
        return Err(SafaiError::Other(
            "none of the selected drives or folders are available".to_string(),
        ));
    }

    let data_drives = drives
        .iter()
        .filter(|drive| system_mount.as_deref() != Some(drive.as_str()))
        .cloned()
        .collect();

    Ok(ResolvedScope {
        drives,
        data_drives,
        folders,
        system_mount,
        deep: request.deep,
        warnings,
    })
}

/// Turn a resolved scope into a concrete scan plan.
pub fn plan_scan(scope: &ResolvedScope, default_roots: &[PathBuf]) -> PreparedScan {
    let warnings = scope.warnings.clone();
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut discovery_roots: Vec<PathBuf> = Vec::new();
    let mut project_roots: Vec<PathBuf> = Vec::new();

    let system_selected = scope.system_mount.as_ref().is_some_and(|system| {
        scope
            .drives
            .iter()
            .any(|drive| drive.eq_ignore_ascii_case(system))
    });

    if system_selected {
        let system = scope.system_mount.as_deref();
        for root in default_roots {
            // The profile can live on a different volume in a custom Windows
            // install. Only add roots that are actually on the selected
            // system drive; otherwise a C:-only scan would measure D:'s
            // profile while fixed rules on D: are filtered out.
            let on_system = safai_rules::mount_key(root).as_deref() == system;
            if on_system {
                push_unique(&mut roots, root.clone());
            }
        }
    }

    for drive in &scope.drives {
        let root = PathBuf::from(format!("{drive}/"));
        push_unique(&mut project_roots, root.clone());
        let is_system = scope
            .system_mount
            .as_deref()
            .is_some_and(|system| system.eq_ignore_ascii_case(drive));
        if !is_system {
            push_unique(&mut discovery_roots, root);
        }
    }

    for folder in &scope.folders {
        push_unique(&mut roots, folder.clone());
        let covered_by_drive = scope.drives.iter().any(|drive| {
            let root = PathBuf::from(format!("{drive}/"));
            folder.starts_with(root)
        });
        if !covered_by_drive {
            push_unique(&mut project_roots, folder.clone());
            push_unique(&mut discovery_roots, folder.clone());
        }
    }

    // Deletion scope: the rule roots (profile/AppData/custom folders) plus
    // data-drive roots. The system drive root is never allow-listed; dev
    // artifacts there still delete via `dev::revalidate`.
    let mut allowed = roots.clone();
    for drive in &scope.data_drives {
        push_unique(&mut allowed, PathBuf::from(format!("{drive}/")));
    }
    let allowed_roots: Vec<PathBuf> = allowed
        .into_iter()
        .filter_map(|path| std::fs::canonicalize(&path).ok())
        .collect();

    let scopes = scope.drives.len().max(1) + scope.folders.len();
    let extra = scopes.saturating_sub(1) as u64;
    let walk_secs = (120 + 45 * extra).min(300);
    let size_secs = (180 + 60 * extra).min(600);
    let max_large = (20usize + 8 * extra as usize).min(40);

    let cfg = ScanConfig {
        roots,
        project_scan_roots: project_roots,
        discovery_roots,
        drives: scope.drives.clone(),
        restrict_rules: true,
        discover_large_folders: scope.deep,
        walk_time_budget: Duration::from_secs(walk_secs),
        size_time_budget: Duration::from_secs(size_secs),
        max_large_folders: max_large,
    };

    PreparedScan {
        cfg,
        allowed_roots,
        warnings,
    }
}

/// Resolve a request and plan it in one step.
pub fn prepare_scan(request: &ScanRequest) -> Result<PreparedScan> {
    let scope = resolve_scope(request)?;
    Ok(plan_scan(&scope, &safai_rules::default_roots()))
}

/// Commit a completed scan: replace the id map and the deletion allow-list
/// atomically from the UI's point of view (the caller holds the activity gate).
pub fn commit_scan(state: &SafaiState, prepared: &PreparedScan, report: &ScanReport) {
    {
        let mut roots = state.allowed_roots.lock().unwrap();
        *roots = prepared.allowed_roots.clone();
    }
    let mut items = state.last_items.lock().unwrap();
    items.clear();
    for group in &report.groups {
        for item in &group.items {
            items.insert(item.id.clone(), item.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- scan scope planning ----------------------------------------------
    //
    // These pin the two rules that keep multi-drive scanning both safe and
    // fast: the system root is never allow-listed, and a data drive's root is
    // a Deep discovery root.

    fn scope(drives: &[&str], system: Option<&str>, folders: Vec<PathBuf>) -> ResolvedScope {
        let drives: Vec<String> = drives.iter().map(|d| d.to_string()).collect();
        let data_drives = drives
            .iter()
            .filter(|d| system != Some(d.as_str()))
            .cloned()
            .collect();
        ResolvedScope {
            drives,
            data_drives,
            folders,
            system_mount: system.map(|s| s.to_string()),
            deep: true,
            warnings: Vec::new(),
        }
    }

    #[test]
    fn a_system_drive_scan_uses_profile_roots_and_never_the_system_root() {
        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join("profile");
        std::fs::create_dir(&profile).unwrap();

        let prepared = plan_scan(
            &scope(&["C:"], Some("C:"), Vec::new()),
            std::slice::from_ref(&profile),
        );

        assert_eq!(prepared.cfg.discovery_roots.len(), 0);
        assert_eq!(prepared.cfg.project_scan_roots, vec![PathBuf::from("C:/")]);
        assert!(prepared.cfg.roots.iter().any(|p| p == &profile));
        assert!(
            !prepared
                .allowed_roots
                .iter()
                .any(|p| p.to_string_lossy().eq_ignore_ascii_case(r"C:\")),
            "the system root must never be allow-listed"
        );
    }

    #[test]
    fn a_data_drive_scan_discovers_its_root() {
        let prepared = plan_scan(&scope(&["D:"], Some("C:"), Vec::new()), &[]);
        assert_eq!(prepared.cfg.discovery_roots, vec![PathBuf::from("D:/")]);
        assert_eq!(prepared.cfg.project_scan_roots, vec![PathBuf::from("D:/")]);
        assert!(prepared.cfg.roots.is_empty());
        // `allowed_roots` is canonicalized, so this assertion only makes sense
        // on a machine that actually has D: (GitHub runners often do not).
        if std::path::Path::new(r"D:\").is_dir() {
            assert!(
                prepared
                    .allowed_roots
                    .iter()
                    .any(|p| drive_mount(p) == "D:"),
                "a selected data drive must be an allowed deletion root"
            );
        }
    }

    #[test]
    fn deep_is_the_only_quick_vs_deep_difference() {
        let mut quick = scope(&["D:"], Some("C:"), Vec::new());
        quick.deep = false;
        let prepared = plan_scan(&quick, &[]);
        assert!(!prepared.cfg.discover_large_folders);
        assert_eq!(prepared.cfg.project_scan_roots, vec![PathBuf::from("D:/")]);
    }
}
