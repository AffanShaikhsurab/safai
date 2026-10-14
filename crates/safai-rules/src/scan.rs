//! The scan driver (implementation-plan.md §5 WS2).
//!
//! [`run_scan`] walks the rule table, measures matching locations via the
//! `safai-core` `measure` API, streams [`ScanEvent`]s through a plain callback,
//! and aggregates the findings into a [`ScanReport`]. The callback shape keeps
//! this crate free of any Tauri dependency — WS3 adapts it to a `Channel`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, UNIX_EPOCH};

use crate::detect::{detect_tools, display_path, expand};
use crate::dev::{self, DevArtifact};
use crate::model::{
    Category, CategoryGroup, CleanupItem, DevInfo, SafetyTier, ScanEvent, ScanReport,
};
use crate::rules::{all_rules, CleanupRule};

/// Configuration for a scan run.
pub struct ScanConfig {
    /// Roots reported to the UI (typically the user profile + cache parents).
    /// Also swept (one level deep) for the generic large-folder discovery.
    pub roots: Vec<PathBuf>,
    /// Where to hunt for pattern targets (`node_modules`, build dirs) via the
    /// pruned walk. The caller chooses these (e.g. common dev folders) so this
    /// crate stays policy-free.
    pub project_scan_roots: Vec<PathBuf>,
    /// When true, run the generic "large folders" discovery pass: surface big
    /// directories under `roots` that no specific rule matched, so any user's
    /// software shows up (Caution tier, never pre-selected).
    pub discover_large_folders: bool,
}

/// Minimum size for a folder to be surfaced by the generic discovery pass.
const MIN_LARGE_FOLDER_BYTES: u64 = 1024 * 1024 * 1024; // 1 GiB
/// Cap on how many generic large folders to report (largest first).
const MAX_LARGE_FOLDERS: usize = 20;

/// Stable category ordering for deterministic group output.
const CATEGORY_ORDER: [Category; 7] = [
    Category::PackageCache,
    Category::EditorStorage,
    Category::BuildArtifact,
    Category::Temp,
    Category::Model,
    Category::Browser,
    Category::Other,
];

/// Human-readable label for a category group header.
pub fn label_for(category: Category) -> &'static str {
    match category {
        Category::PackageCache => "Package caches",
        Category::EditorStorage => "Editor storage",
        Category::BuildArtifact => "Build artifacts",
        Category::Temp => "Temporary files",
        Category::Model => "Downloaded models",
        Category::Browser => "Browser binaries",
        Category::Other => "Other large items",
    }
}

/// Is `candidate` (normalized, lowercased) equal to, an ancestor of, or a
/// descendant of any already-found path? Used by the generic discovery pass to
/// avoid double-counting space already attributed to a specific rule.
fn path_overlaps(candidate: &str, existing_lower: &[String]) -> bool {
    existing_lower.iter().any(|p| {
        candidate == p
            || candidate.starts_with(&format!("{p}/"))
            || p.starts_with(&format!("{candidate}/"))
    })
}

/// Well-known user/system folder names that the generic discovery skips: they
/// are large but are user *data* (not reclaimable software space) and/or are
/// slow to size. Skipping them keeps discovery fast and its results relevant.
/// `appdata` is skipped here because we already sweep `Local`/`Roaming`
/// directly as roots — re-sizing the whole `AppData` tree would be huge and
/// redundant.
const DISCOVERY_SKIP_NAMES: &[&str] = &[
    "documents",
    "pictures",
    "videos",
    "music",
    "desktop",
    "downloads",
    "favorites",
    "contacts",
    "links",
    "searches",
    "saved games",
    "3d objects",
    "appdata",
    "onedrive",
    "onedrive - personal",
    "dropbox",
    "google drive",
];

/// Cheaply enumerate the *direct child* directories of each root that are
/// candidates for the generic large-folder pass. This does **no** sizing (just
/// `read_dir` + a name denylist), so the caller can learn the candidate count
/// up front and drive an accurate, smoothly-advancing progress bar — then size
/// each candidate one at a time, streaming a `Progress` event per folder.
fn enumerate_large_folder_candidates(parents: &[PathBuf], cancel: &AtomicBool) -> Vec<PathBuf> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<PathBuf> = Vec::new();
    for parent in parents {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let rd = match std::fs::read_dir(parent) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for entry in rd.flatten() {
            let ft = match entry.file_type() {
                Ok(f) => f,
                Err(_) => continue,
            };
            if !ft.is_dir() {
                continue;
            }
            let path = entry.path();
            let name_low = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();
            if DISCOVERY_SKIP_NAMES.contains(&name_low.as_str()) {
                continue;
            }
            let low = display_path(&path).trim_end_matches('/').to_lowercase();
            if seen.insert(low) {
                out.push(path);
            }
        }
    }
    out
}

/// 64-bit FNV-1a hash — chosen over `DefaultHasher` because FNV is a fixed,
/// specified algorithm, so an item's `id` is stable across runs and builds
/// (the same path always yields the same id). Rendered as 16 hex chars.
fn fnv1a_hex(input: &str) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for byte in input.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

/// Stable, unique id for a finding: a short hash of its (display) path.
pub fn stable_id(path: &str) -> String {
    fnv1a_hex(path)
}

/// Unix seconds of a path's last-modified time, if available (for staleness).
fn last_modified_secs(path: &Path) -> Option<u64> {
    let md = std::fs::metadata(path).ok()?;
    let modified = md.modified().ok()?;
    modified
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// How often the streaming sizing loop republishes progress to the UI.
const SIZE_PROGRESS_INTERVAL: Duration = Duration::from_millis(120);

/// Wall-clock safety budget for the dev-project discovery walk (whole drives,
/// in parallel). If discovery is still running after this, it stops and
/// records a warning, so the walk can never hang the scan.
const WALK_TIME_BUDGET: Duration = Duration::from_secs(120);

/// Safety net: if sizing a single directory exceeds this budget, stop walking
/// it and report the partial size (with a warning). This guarantees the scan
/// can never hang indefinitely on a pathological tree (a giant model cache on a
/// slow disk, a network mount, a reparse-point maze, etc.). Sized generously so
/// it rarely triggers — dozens of `node_modules` folders across a whole disk
/// are millions of files — and the live progress below is the primary "it's
/// not stuck" signal.
const SIZE_TIME_BUDGET: Duration = Duration::from_secs(180);

/// Measure a file's length. Directories are handled by [`measure_dir_streaming`].
fn measure_file(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// A directory whose size we still need to measure, plus everything needed to
/// build its [`CleanupItem`] once the size is known. Collected up front so all
/// directories can be sized together in one shared work-stealing pool (see
/// [`size_dirs_with_progress`]) instead of one-at-a-time — which is what
/// prevents one giant folder (e.g. the Hugging Face cache) from blocking the
/// whole scan.
struct PendingDir {
    path: PathBuf,
    disp: String,
    rule_id: String,
    label: String,
    category: Category,
    tier: SafetyTier,
    regenerates: bool,
    note: String,
    selected_by_default: bool,
    /// Large-folder discovery items get post-sizing filtering (min size,
    /// overlap dedup, count cap); rule-based items are always surfaced.
    is_large_folder: bool,
    /// Project context for dev artifacts.
    dev: Option<DevInfo>,
}

/// Size every directory job in `paths` **concurrently** in one shared
/// work-stealing pool (`safai_core::size_many`), while this thread polls the
/// live counters.
///
/// As each folder finishes sizing, `make_item` is called with its
/// `(index, size)`; when it returns `Some(item)` a `Found` event is emitted
/// **immediately**, so the UI's found-item counter climbs live during sizing
/// instead of jumping only when the whole pool drains (the symptom that made
/// scans look stuck near the end). `Progress` events are forwarded throughout.
///
/// Both callbacks run on this single polling thread, so neither needs a `Sync`
/// bound. Bounded by `SIZE_TIME_BUDGET` so the scan can never hang, even on a
/// pathological tree. Every job is handed to `make_item` exactly once.
#[allow(clippy::too_many_arguments)]
fn size_dirs_with_progress(
    paths: &[PathBuf],
    cancel: &AtomicBool,
    base_found: u64,
    base_checked: u32,
    rules_total: u32,
    on_event: &mut dyn FnMut(ScanEvent),
    make_item: &mut dyn FnMut(usize, u64) -> Option<CleanupItem>,
    warnings: &mut Vec<String>,
) {
    let total_jobs = paths.len();
    if total_jobs == 0 {
        return;
    }

    let progress = AtomicU64::new(0);
    let completed = std::sync::atomic::AtomicUsize::new(0);
    let per_job_total: Vec<AtomicU64> = (0..total_jobs).map(|_| AtomicU64::new(0)).collect();
    let per_job_done: Vec<AtomicBool> = (0..total_jobs).map(|_| AtomicBool::new(false)).collect();
    let local_cancel = AtomicBool::new(false);
    let start = Instant::now();

    // Jobs already handed to `make_item`, so each is emitted exactly once.
    let mut handled: Vec<bool> = vec![false; total_jobs];

    // Running total of bytes for folders that were actually *surfaced* (passed
    // `make_item`). The live "Reclaimable" figure is `base_found + surfaced`,
    // so it tracks the real, de-duplicated total and matches the review page —
    // instead of the raw sum of every byte touched (which over-counts folders
    // that get filtered out by the size/cap rules).
    let mut surfaced: u64 = 0;

    std::thread::scope(|s| {
        let progress_ref = &progress;
        let completed_ref = &completed;
        let per_job_total_ref = &per_job_total;
        let per_job_done_ref = &per_job_done;
        let local_ref = &local_cancel;
        let paths_ref = paths;
        let handle = s.spawn(move || {
            safai_core::size_many(
                paths_ref,
                local_ref,
                progress_ref,
                completed_ref,
                per_job_total_ref,
                per_job_done_ref,
            );
        });

        while !handle.is_finished() {
            std::thread::sleep(SIZE_PROGRESS_INTERVAL);

            // Propagate global cancellation or a blown time budget to the walk.
            if cancel.load(Ordering::Relaxed) || start.elapsed() >= SIZE_TIME_BUDGET {
                local_cancel.store(true, Ordering::Relaxed);
            }

            // Emit Found for every folder that finished sizing since the last
            // poll, so the item counter advances live and the reclaimable
            // total climbs by only the surfaced (de-duplicated) bytes.
            for idx in 0..total_jobs {
                if !handled[idx] && per_job_done[idx].load(Ordering::Acquire) {
                    handled[idx] = true;
                    let size = per_job_total[idx].load(Ordering::Relaxed);
                    if let Some(item) = make_item(idx, size) {
                        surfaced = surfaced.saturating_add(item.size_bytes);
                        on_event(ScanEvent::Found { item });
                    }
                }
            }

            let done = completed_ref.load(Ordering::Relaxed).min(total_jobs);
            let remaining = total_jobs - done;
            let msg = if remaining > 0 {
                format!(
                    "Measuring {remaining} folder{}…",
                    if remaining == 1 { "" } else { "s" }
                )
            } else {
                "Finishing up…".to_string()
            };
            on_event(ScanEvent::Progress {
                current_path: msg,
                found_bytes: base_found.saturating_add(surfaced),
                rules_checked: base_checked + done as u32,
                rules_total,
            });
        }

        let _ = handle.join();
    });

    // Final sweep: hand over any jobs not yet emitted (finished between the
    // last poll and join, or left partial by cancel/timeout).
    for idx in 0..total_jobs {
        if !handled[idx] {
            handled[idx] = true;
            let size = per_job_total[idx].load(Ordering::Relaxed);
            if let Some(item) = make_item(idx, size) {
                surfaced = surfaced.saturating_add(item.size_bytes);
                on_event(ScanEvent::Found { item });
            }
        }
    }

    if start.elapsed() >= SIZE_TIME_BUDGET && !cancel.load(Ordering::Relaxed) {
        warnings.push(format!(
            "Sizing stopped after {}s; some folder sizes may be underestimated.",
            SIZE_TIME_BUDGET.as_secs()
        ));
    }
}

/// Run [`dev::discover`] on a worker while this thread streams live progress
/// (the walk itself is parallel and can't call the non-`Sync` sink).
fn discover_with_progress(
    roots: &[PathBuf],
    cancel: &AtomicBool,
    found_bytes: u64,
    on_event: &mut dyn FnMut(ScanEvent),
) -> (Vec<DevArtifact>, bool) {
    let dirs_seen = AtomicU64::new(0);
    let deadline = Instant::now() + WALK_TIME_BUDGET;
    std::thread::scope(|s| {
        let dirs_ref = &dirs_seen;
        let handle = s.spawn(move || dev::discover(roots, cancel, Some(deadline), dirs_ref));
        while !handle.is_finished() {
            std::thread::sleep(SIZE_PROGRESS_INTERVAL);
            on_event(ScanEvent::Progress {
                current_path: format!(
                    "Looking for dev projects… ({} folders)",
                    dirs_seen.load(Ordering::Relaxed)
                ),
                found_bytes,
                rules_checked: 0,
                rules_total: 0,
            });
        }
        handle.join().unwrap_or_default()
    })
}

/// Attach project context to each artifact: project root, kind, and last
/// activity. Activity is computed once per project, in parallel.
fn describe_projects(artifacts: Vec<DevArtifact>) -> Vec<(DevArtifact, DevInfo)> {
    use rayon::prelude::*;
    use std::collections::HashMap;

    let now = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Resolve each artifact's project, dropping any folder the project's git
    // repo actually tracks (committed fixtures / vendored deps are source).
    let rooted: Vec<(DevArtifact, PathBuf)> = artifacts
        .into_par_iter()
        .filter_map(|a| {
            let root = dev::project_root_for(&a.path);
            if root.join(".git").exists() && dev::is_tracked_by_git(&root, &a.path) {
                return None;
            }
            Some((a, root))
        })
        .collect();

    // One activity probe per distinct project root.
    let mut roots: Vec<(PathBuf, &'static str)> = Vec::new();
    for (a, root) in &rooted {
        if !roots.iter().any(|(r, _)| r == root) {
            roots.push((root.clone(), a.spec.ecosystem));
        }
    }
    let infos: HashMap<PathBuf, (String, Option<u64>)> = roots
        .into_par_iter()
        .map(|(root, eco)| {
            let kind = dev::project_kind(&root, eco);
            let active = dev::last_active_secs(&root, now);
            (root, (kind, active))
        })
        .collect();

    rooted
        .into_iter()
        .map(|(a, root)| {
            let (kind, active) = infos
                .get(&root)
                .cloned()
                .unwrap_or_else(|| (a.spec.ecosystem.to_string(), None));
            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| display_path(&root));
            let info = DevInfo {
                project_root: display_path(&root),
                project_name: name,
                project_kind: kind,
                ecosystem: a.spec.ecosystem.to_string(),
                last_active_secs: active,
            };
            (a, info)
        })
        .collect()
}

/// Run the rules-based scan.
///
/// Emits [`ScanEvent`]s through `on_event` and returns the aggregated
/// [`ScanReport`]. Honors `cancel` throughout: when set, the scan stops adding
/// new findings and finishes with whatever it has gathered so far.
pub fn run_scan(
    cfg: &ScanConfig,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(ScanEvent),
) -> ScanReport {
    // ------------------------------------------------------------------
    // Scanned roots (deduped) + Started event.
    // ------------------------------------------------------------------
    let mut scanned_roots: Vec<String> = Vec::new();
    for r in cfg.roots.iter().chain(cfg.project_scan_roots.iter()) {
        let d = display_path(r);
        if !scanned_roots.contains(&d) {
            scanned_roots.push(d);
        }
    }
    on_event(ScanEvent::Started {
        roots: scanned_roots.clone(),
    });

    let rules = all_rules();

    // Detected tool ids, for `requires_tool` gating.
    let detected: Vec<String> = detect_tools()
        .into_iter()
        .filter(|(_, _, ok)| *ok)
        .map(|(id, _, _)| id)
        .collect();
    let tool_detected = |id: &str| detected.iter().any(|d| d == id);

    // Partition rules; count only rules that will actually be processed so the
    // progress denominator is meaningful.
    let mut fixed_rules: Vec<&CleanupRule> = Vec::new();
    for rule in &rules {
        // Gate fixed rules on their required tool (if any). Pattern rules are
        // no longer part of the table — dev artifacts come from `crate::dev`.
        let gated_out = matches!(rule.requires_tool, Some(tool) if !tool_detected(tool));
        if rule.pattern.is_none() && !gated_out {
            fixed_rules.push(rule);
        }
    }
    // Enumerate generic large-folder candidates up front (cheap `read_dir`, no
    // sizing).
    let candidates: Vec<PathBuf> = if cfg.discover_large_folders {
        enumerate_large_folder_candidates(&cfg.roots, cancel)
    } else {
        Vec::new()
    };

    let mut items: Vec<CleanupItem> = Vec::new();
    let mut found_bytes: u64 = 0;
    let mut warnings: Vec<String> = Vec::new();

    // ------------------------------------------------------------------
    // Phase 1 — collect work WITHOUT sizing.
    //   * Fixed-rule *file* locations are measured inline (a single cheap
    //     metadata call each) and become items immediately.
    //   * Every directory (fixed-rule dirs, pattern matches, discovery
    //     candidates) becomes a `PendingDir` job so they can all be sized
    //     together in one shared work-stealing pool below.
    // ------------------------------------------------------------------
    let mut dir_jobs: Vec<PendingDir> = Vec::new();

    // Fixed-location rules.
    for rule in &fixed_rules {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        for spec in &rule.locations {
            if let Some(path) = expand(spec.raw) {
                let disp = display_path(&path);
                if path.is_dir() {
                    dir_jobs.push(PendingDir {
                        path: path.clone(),
                        disp,
                        rule_id: rule.id.to_string(),
                        label: rule.label.to_string(),
                        category: rule.category,
                        tier: rule.tier,
                        regenerates: rule.regenerates,
                        note: rule.note.to_string(),
                        selected_by_default: rule.tier == SafetyTier::Safe,
                        is_large_folder: false,
                        dev: None,
                    });
                } else {
                    // A file (e.g. `state.vscdb`): measure inline.
                    let size = measure_file(&path);
                    let item = CleanupItem {
                        id: stable_id(&disp),
                        rule_id: rule.id.to_string(),
                        label: rule.label.to_string(),
                        category: rule.category,
                        tier: rule.tier,
                        path: disp.clone(),
                        size_bytes: size,
                        regenerates: rule.regenerates,
                        last_modified_secs: last_modified_secs(&path),
                        note: rule.note.to_string(),
                        selected_by_default: rule.tier == SafetyTier::Safe,
                        dev: None,
                    };
                    found_bytes = found_bytes.saturating_add(size);
                    on_event(ScanEvent::Found { item: item.clone() });
                    items.push(item);
                }
            }
        }
    }

    // Dev projects: walk the project roots (whole drives in the app) for
    // validated artifact folders, then work out which project each belongs to
    // and when that project was last worked on.
    if !cfg.project_scan_roots.is_empty() && !cancel.load(Ordering::Relaxed) {
        let (artifacts, timed_out) =
            discover_with_progress(&cfg.project_scan_roots, cancel, found_bytes, on_event);
        if timed_out {
            warnings.push(format!(
                "Project scan stopped after {}s; some project folders may not be listed.",
                WALK_TIME_BUDGET.as_secs()
            ));
        }
        on_event(ScanEvent::Progress {
            current_path: format!(
                "Checking when {} project folder{} were last used…",
                artifacts.len(),
                if artifacts.len() == 1 { "" } else { "s" }
            ),
            found_bytes,
            rules_checked: 0,
            rules_total: 0,
        });
        for (artifact, dev) in describe_projects(artifacts) {
            let spec = artifact.spec;
            dir_jobs.push(PendingDir {
                disp: display_path(&artifact.path),
                path: artifact.path,
                rule_id: spec.rule_id.to_string(),
                label: format!("{} · {}", dev.project_name, spec.label),
                category: Category::BuildArtifact,
                tier: SafetyTier::Review,
                regenerates: true,
                note: spec.note.to_string(),
                // The UI pre-selects by inactivity threshold instead.
                selected_by_default: false,
                is_large_folder: false,
                dev: Some(dev),
            });
        }
    }

    // Large-folder discovery candidates.
    //
    // Dedup BEFORE sizing. A generic large-folder candidate that overlaps a
    // path already attributed to a specific rule (the candidate is an ancestor
    // of a detected `node_modules`, or the parent of a known cache, etc.) must
    // not be sized at all — otherwise its bytes are counted twice: once for the
    // parent candidate and once for the rule match nested inside it. That
    // double-counting is exactly what made the live "Reclaimable" total balloon
    // far above the real figure shown on the review page. Filtering here means
    // those overlapping folders are never sized (so the scan is faster too) and
    // never counted.
    //
    // `existing_lower` holds every rule-attributed path (fixed-rule files and
    // dirs + pattern matches) in normalized, lowercased form. At this point
    // `dir_jobs` contains only rule dirs (candidates are added below), so it is
    // the complete set of rule paths.
    let mut existing_lower: Vec<String> = items
        .iter()
        .map(|it| it.path.trim_end_matches('/').to_lowercase())
        .collect();
    for job in &dir_jobs {
        existing_lower.push(job.disp.trim_end_matches('/').to_lowercase());
    }

    for cand in &candidates {
        let disp = display_path(cand);
        let low = disp.trim_end_matches('/').to_lowercase();
        // Skip candidates that overlap (are an ancestor/descendant of) a path a
        // specific rule already owns — dedup before we spend time sizing it.
        if path_overlaps(&low, &existing_lower) {
            continue;
        }
        let name = cand
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("folder")
            .to_string();
        dir_jobs.push(PendingDir {
            path: cand.clone(),
            disp,
            rule_id: "large-folder".to_string(),
            label: name,
            category: Category::Other,
            tier: SafetyTier::Caution,
            regenerates: false,
            note: "A large folder that no specific rule matched. It may hold \
                   important data — use Reveal to inspect it before removing. \
                   Never pre-selected."
                .to_string(),
            selected_by_default: false,
            is_large_folder: true,
            dev: None,
        });
    }

    // ------------------------------------------------------------------
    // Phase 2 — size ALL directory jobs together in one shared work-stealing
    // pool. Every worker steals across all folders, so a single huge folder
    // (Hugging Face cache, a massive node_modules, …) is sized with full
    // parallelism and can never block the folders behind it. Crucially, each
    // folder's `Found` event is emitted the moment its size is known (via
    // `make_item` below), so the UI's found-item counter climbs live during
    // sizing instead of jumping only when the whole pool drains.
    //
    // Rule-based dirs are always surfaced; large-folder candidates get the
    // min-size / overlap / count-cap filtering applied as each one completes.
    // ------------------------------------------------------------------
    let file_items = items.len() as u32;
    let rules_total: u32 = file_items + dir_jobs.len() as u32;
    let job_paths: Vec<PathBuf> = dir_jobs.iter().map(|j| j.path.clone()).collect();

    {
        let mut emitted_large = 0usize;
        // Build a `CleanupItem` for job `idx` once its `size` is known. Returns
        // `None` when a large-folder candidate is filtered out by the count cap
        // or the min-size threshold; rule-based dirs always pass. (Overlap dedup
        // already happened before sizing, so there is no overlap check here.)
        // Also records the item into `items` for final aggregation.
        let mut make_item = |idx: usize, size: u64| -> Option<CleanupItem> {
            let job = &dir_jobs[idx];

            if job.is_large_folder {
                if emitted_large >= MAX_LARGE_FOLDERS {
                    return None;
                }
                if size < MIN_LARGE_FOLDER_BYTES {
                    return None;
                }
                emitted_large += 1;
            }

            let item = CleanupItem {
                id: stable_id(&job.disp),
                rule_id: job.rule_id.clone(),
                label: job.label.clone(),
                category: job.category,
                tier: job.tier,
                path: job.disp.clone(),
                size_bytes: size,
                regenerates: job.regenerates,
                last_modified_secs: last_modified_secs(&job.path),
                note: job.note.clone(),
                selected_by_default: job.selected_by_default,
                dev: job.dev.clone(),
            };
            items.push(item.clone());
            Some(item)
        };

        size_dirs_with_progress(
            &job_paths,
            cancel,
            found_bytes,
            file_items,
            rules_total,
            on_event,
            &mut make_item,
            &mut warnings,
        );
    }

    if cancel.load(Ordering::Relaxed) {
        warnings.push("Scan was cancelled; results may be incomplete.".to_string());
    }

    // ------------------------------------------------------------------
    // Aggregate into category groups (stable order) + totals.
    // ------------------------------------------------------------------
    let total_reclaimable_bytes: u64 = items
        .iter()
        .fold(0u64, |acc, it| acc.saturating_add(it.size_bytes));

    let mut groups: Vec<CategoryGroup> = Vec::new();
    for category in CATEGORY_ORDER {
        let group_items: Vec<CleanupItem> = items
            .iter()
            .filter(|it| it.category == category)
            .cloned()
            .collect();
        if group_items.is_empty() {
            continue;
        }
        let total_bytes = group_items
            .iter()
            .fold(0u64, |acc, it| acc.saturating_add(it.size_bytes));
        groups.push(CategoryGroup {
            category,
            label: label_for(category).to_string(),
            total_bytes,
            items: group_items,
        });
    }

    on_event(ScanEvent::Finished {
        total_reclaimable_bytes,
        item_count: items.len() as u32,
    });

    ScanReport {
        total_reclaimable_bytes,
        groups,
        scanned_roots,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn stable_id_is_deterministic() {
        let a = stable_id("C:/Users/x/node_modules");
        let b = stable_id("C:/Users/x/node_modules");
        let c = stable_id("C:/Users/x/other");
        assert_eq!(a, b, "same path must yield same id");
        assert_ne!(a, c, "different paths should differ");
        assert_eq!(a.len(), 16, "id is 16 hex chars");
    }

    #[test]
    fn aggregates_node_modules_from_tempdir() {
        // Build: root/proj/node_modules/pkg/a.bin (30 bytes)
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().to_path_buf();
        let nm = root.join("proj").join("node_modules");
        let pkg = nm.join("pkg");
        fs::create_dir_all(&pkg).expect("create tree");
        let mut f = File::create(pkg.join("a.bin")).expect("file");
        f.write_all(&[0u8; 30]).expect("write");
        drop(f); // Flush and close before scanning.
                 // A real project: manifest + lockfile next to node_modules.
        fs::write(root.join("proj").join("package.json"), b"{}").expect("manifest");
        fs::write(root.join("proj").join("package-lock.json"), b"{}").expect("lock");
        fs::create_dir_all(root.join("proj").join("src")).expect("src");
        fs::write(root.join("proj").join("src").join("index.js"), b"1").expect("src file");

        let cfg = ScanConfig {
            roots: vec![root.clone()],
            project_scan_roots: vec![root.clone()],
            discover_large_folders: false,
        };
        let cancel = AtomicBool::new(false);

        let mut found_count = 0usize;
        let report = run_scan(&cfg, &cancel, &mut |ev| {
            if let ScanEvent::Found { item } = ev {
                if item.rule_id == "node-modules" {
                    found_count += 1;
                }
            }
        });

        // Exactly one node_modules Found item, sized 30 bytes.
        assert_eq!(found_count, 1, "should find the single node_modules");
        let nm_items: Vec<&CleanupItem> = report
            .groups
            .iter()
            .flat_map(|g| g.items.iter())
            .filter(|it| it.rule_id == "node-modules")
            .collect();
        assert_eq!(nm_items.len(), 1);
        assert_eq!(nm_items[0].size_bytes, 30);
        assert_eq!(nm_items[0].category, Category::BuildArtifact);
        assert_eq!(nm_items[0].tier, SafetyTier::Review);
        assert!(!nm_items[0].selected_by_default);
        // Project context is attached, with activity from the source file.
        let dev = nm_items[0].dev.as_ref().expect("dev info");
        assert_eq!(dev.project_name, "proj");
        assert_eq!(dev.ecosystem, "Node.js");
        assert!(dev.last_active_secs.is_some());
        // The build-artifact bytes contribute to the reclaimable total.
        assert!(report.total_reclaimable_bytes >= 30);
    }

    #[test]
    fn cancelled_scan_records_warning() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg = ScanConfig {
            roots: vec![tmp.path().to_path_buf()],
            project_scan_roots: vec![tmp.path().to_path_buf()],
            discover_large_folders: false,
        };
        let cancel = AtomicBool::new(true); // pre-cancelled
        let report = run_scan(&cfg, &cancel, &mut |_ev| {});
        assert!(!report.warnings.is_empty(), "cancellation should warn");
    }
}
