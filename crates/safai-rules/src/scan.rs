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
use crate::model::{
    Category, CategoryGroup, CleanupItem, ScanEvent, ScanReport, SafetyTier,
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
    modified.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}

/// How often the streaming sizing loop republishes progress to the UI.
const SIZE_PROGRESS_INTERVAL: Duration = Duration::from_millis(120);

/// Maximum depth (relative to each project scan root) the pattern-discovery
/// walk descends. Artifact directories (`node_modules`, `target`, build dirs)
/// sit only a few levels below a project root, so a modest cap keeps the walk
/// bounded without missing real matches — and prevents it from crawling deep,
/// unrelated trees (which is what made scans appear to "hang").
const PROJECT_SCAN_MAX_DEPTH: usize = 8;

/// Wall-clock safety budget for the pattern-discovery walk. If discovery is
/// still running after this, it stops and records a warning. This guarantees
/// the walk can never hang the scan, mirroring `SIZE_TIME_BUDGET` for sizing.
const WALK_TIME_BUDGET: Duration = Duration::from_secs(45);

/// Directory names the pattern-discovery walk will report but never descend
/// into. These are large and/or irrelevant to project-artifact discovery:
/// `appdata` is huge and already swept one level deep as its own roots; VCS and
/// system folders never usefully contain the artifact dirs we hunt for.
/// Everything here is matched case-insensitively against the directory name.
const SKIP_DESCEND_NAMES: &[&str] = &[
    "appdata",
    ".git",
    ".hg",
    ".svn",
    "$recycle.bin",
    "windows",
    "system volume information",
];

/// Safety net: if sizing a single directory exceeds this budget, stop walking
/// it and report the partial size (with a warning). This guarantees the scan
/// can never hang indefinitely on a pathological tree (a giant model cache on a
/// slow disk, a network mount, a reparse-point maze, etc.). Sized generously so
/// it rarely triggers on normal local caches — the live progress below is the
/// primary "it's not stuck" signal.
const SIZE_TIME_BUDGET: Duration = Duration::from_secs(90);

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
            // poll, so the item counter advances live.
            for idx in 0..total_jobs {
                if !handled[idx] && per_job_done[idx].load(Ordering::Acquire) {
                    handled[idx] = true;
                    let size = per_job_total[idx].load(Ordering::Relaxed);
                    if let Some(item) = make_item(idx, size) {
                        on_event(ScanEvent::Found { item });
                    }
                }
            }

            let cur = progress_ref.load(Ordering::Relaxed);
            let done = completed_ref.load(Ordering::Relaxed).min(total_jobs);
            let remaining = total_jobs - done;
            let msg = if remaining > 0 {
                format!("Measuring {remaining} folder{}…", if remaining == 1 { "" } else { "s" })
            } else {
                "Finishing up…".to_string()
            };
            on_event(ScanEvent::Progress {
                current_path: msg,
                found_bytes: base_found.saturating_add(cur),
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
    let mut pattern_rules: Vec<&CleanupRule> = Vec::new();
    for rule in &rules {
        if rule.pattern.is_some() {
            pattern_rules.push(rule);
        } else {
            // Gate fixed rules on their required tool (if any).
            let gated_out = matches!(rule.requires_tool, Some(tool) if !tool_detected(tool));
            if !gated_out {
                fixed_rules.push(rule);
            }
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
                    };
                    found_bytes = found_bytes.saturating_add(size);
                    on_event(ScanEvent::Found { item: item.clone() });
                    items.push(item);
                }
            }
        }
    }

    // Pattern rules: one pruned walk to discover artifact directories.
    if !pattern_rules.is_empty() && !cfg.project_scan_roots.is_empty() {
        on_event(ScanEvent::Progress {
            current_path: "Scanning projects…".to_string(),
            found_bytes,
            rules_checked: 0,
            rules_total: 0,
        });

        let mut matches: Vec<PathBuf> = Vec::new();
        let walk_timed_out;
        {
            let is_target_name = |name: &str| -> bool {
                pattern_rules.iter().any(|r| {
                    r.pattern
                        .as_ref()
                        .map(|p| p.names.iter().any(|n| *n == name))
                        .unwrap_or(false)
                })
            };
            // Don't descend into artifact dirs (their contents are what we're
            // measuring, not searching) nor into the heavy/irrelevant folders
            // in `SKIP_DESCEND_NAMES`. Matched directories are still reported.
            let prune = |path: &Path| -> bool {
                match path.file_name().and_then(|n| n.to_str()) {
                    Some(name) => {
                        if is_target_name(name) {
                            return true;
                        }
                        let low = name.to_ascii_lowercase();
                        SKIP_DESCEND_NAMES.contains(&low.as_str())
                    }
                    None => false,
                }
            };
            // Throttled progress: `walk_pruned` reports every directory it
            // sees, so we count them and republish a live Progress event at
            // most every `SIZE_PROGRESS_INTERVAL`. This is what keeps the UI
            // moving during discovery instead of appearing frozen.
            let mut last_emit = Instant::now();
            let mut dirs_seen: u64 = 0;
            let mut on_dir = |path: &Path| {
                dirs_seen += 1;
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if is_target_name(name) {
                        matches.push(path.to_path_buf());
                    }
                }
                if last_emit.elapsed() >= SIZE_PROGRESS_INTERVAL {
                    last_emit = Instant::now();
                    on_event(ScanEvent::Progress {
                        current_path: format!("Scanning projects… ({dirs_seen} folders)"),
                        found_bytes,
                        rules_checked: 0,
                        rules_total: 0,
                    });
                }
            };
            let opts = safai_core::WalkOptions {
                max_depth: Some(PROJECT_SCAN_MAX_DEPTH),
                deadline: Some(Instant::now() + WALK_TIME_BUDGET),
            };
            walk_timed_out =
                safai_core::walk_pruned(&cfg.project_scan_roots, cancel, opts, &prune, &mut on_dir);
        }

        if walk_timed_out {
            warnings.push(format!(
                "Project scan stopped after {}s; some artifact folders may not be listed.",
                WALK_TIME_BUDGET.as_secs()
            ));
        }

        for matched in matches {
            let name = match matched.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            let rule = pattern_rules.iter().find(|r| {
                r.pattern
                    .as_ref()
                    .map(|p| p.names.iter().any(|n| *n == name))
                    .unwrap_or(false)
            });
            let rule = match rule {
                Some(r) => r,
                None => continue,
            };
            let disp = display_path(&matched);
            dir_jobs.push(PendingDir {
                path: matched.clone(),
                disp,
                rule_id: rule.id.to_string(),
                label: rule.label.to_string(),
                category: rule.category,
                tier: rule.tier,
                regenerates: rule.regenerates,
                note: rule.note.to_string(),
                selected_by_default: rule.tier == SafetyTier::Safe,
                is_large_folder: false,
            });
        }
    }

    // Large-folder discovery candidates.
    let candidate_start = dir_jobs.len();
    for cand in &candidates {
        let disp = display_path(cand);
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

    // Paths already attributed to a specific rule (for large-folder overlap).
    // Built before sizing so the overlap check is order-independent.
    let mut existing_lower: Vec<String> = items
        .iter()
        .map(|it| it.path.trim_end_matches('/').to_lowercase())
        .collect();
    // Also count the non-candidate dir jobs as "existing" for overlap.
    for job in dir_jobs.iter().take(candidate_start) {
        existing_lower.push(job.disp.trim_end_matches('/').to_lowercase());
    }

    {
        let mut emitted_large = 0usize;
        // Build a `CleanupItem` for job `idx` once its `size` is known. Returns
        // `None` when a large-folder candidate is filtered out (overlap, count
        // cap, or below the min-size threshold); rule-based dirs always pass.
        // Also records the item into `items` for final aggregation.
        let mut make_item = |idx: usize, size: u64| -> Option<CleanupItem> {
            let job = &dir_jobs[idx];

            if job.is_large_folder {
                let low = job.disp.trim_end_matches('/').to_lowercase();
                if path_overlaps(&low, &existing_lower) {
                    return None;
                }
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
