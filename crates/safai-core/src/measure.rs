// Cleanup measure helpers used by the Safai rules engine (WS2).
//
// This module adds two focused capabilities on top of the extracted
// `wiztree-metafile` scanner:
//
//   * [`dir_size`]  — a fast, cancellable, recursive byte sum of a single
//     directory. Unlike the full analyzer it does **not** allocate a
//     `FileEntry` per file; it only sums `symlink_metadata().len()`. It uses
//     a rayon work-stealing parallel walk (modelled on `traversal::parallel`)
//     so large trees (e.g. a `node_modules`) are summed across all cores.
//
//   * [`walk_pruned`] — a directory walker that visits directories but can be
//     told to NOT descend into a directory (via the `prune` predicate). This
//     lets the rules engine locate artifact directories such as
//     `node_modules`/`target` without paying to traverse their millions of
//     inner files.
//
// Both helpers respect a shared `&AtomicBool` cancellation flag and never
// panic on unreadable files/directories — they simply skip what they cannot
// read.

use crossbeam_deque::{Injector, Steal, Stealer, Worker};
use std::fs::{self, FileType};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

/// How often (in directory-entry iterations) the cancel flag is consulted
/// inside a single directory listing. Checking every entry is cheap (a
/// relaxed atomic load) but we also check once per directory before reading.
const CANCEL_CHECK_STRIDE: usize = 256;

/// Upper bound on worker threads for a single `dir_size` call. Metadata-bound
/// directory walking benefits from oversubscription (many outstanding
/// requests saturate SSD/NVMe queues) but we cap it to avoid spawning an
/// excessive number of OS threads for every sizing call.
fn size_thread_count() -> usize {
    num_cpus::get().clamp(1, 16)
}

/// Recursive byte sum of a single directory subtree.
///
/// * On Windows: uses `FindFirstFileExW` with `FIND_FIRST_EX_LARGE_FETCH` to
///   get file sizes directly from directory entries (zero extra stat calls).
/// * Sums file sizes; symlinks are counted by their entry size and are **never
///   followed**, so a symlink can never cause the walk to escape the subtree.
/// * Directories are traversed in parallel using a **crossbeam work-stealing
///   deque** — the same proven design as `traversal::parallel` in the original
///   scanner. This avoids the task explosion / poor cancellation of a naive
///   recursive `par_iter`, and stays responsive to `cancel`.
/// * Files and directories that cannot be read are silently skipped (no
///   panics), matching the analyzer's permission-denied behaviour.
/// * `cancel` is checked at the top of every worker loop iteration, so
///   cancellation is observed within microseconds even mid-scan. On cancel the
///   function returns the **partial** sum accumulated so far.
pub fn dir_size(path: &Path, cancel: &AtomicBool) -> u64 {
    let progress = AtomicU64::new(0);
    dir_size_into(path, cancel, &progress)
}

/// Like [`dir_size`], but adds bytes to `progress` **as it walks** so a caller
/// on another thread can observe the running total live (e.g. to drive a
/// smoothly-advancing progress bar while sizing a huge directory such as the
/// Hugging Face model cache). Returns the final total for `path`.
///
/// `progress` should normally start at `0`; the returned value equals the
/// amount this call added to it.
pub fn dir_size_into(path: &Path, cancel: &AtomicBool, progress: &AtomicU64) -> u64 {
    if cancel.load(Ordering::Relaxed) {
        return 0;
    }

    // Read the root directory once. This lets us take a cheap single-threaded
    // fast path for small/leaf directories (the common case for build dirs,
    // caches with few subfolders) and avoids the overhead of spawning a whole
    // worker pool when there is little to parallelize.
    let root_entries = crate::fast_readdir::read_dir_fast(path);
    let mut files_total: u64 = 0;
    let mut subdirs: Vec<PathBuf> = Vec::with_capacity(root_entries.len());

    for entry in &root_entries {
        if entry.is_dir {
            subdirs.push(entry.path.clone());
        } else {
            // File or symlink — size already known from the directory listing.
            files_total = files_total.saturating_add(entry.size);
        }
    }

    if files_total > 0 {
        progress.fetch_add(files_total, Ordering::Relaxed);
    }

    // No subdirectories: we're done without spawning any threads.
    if subdirs.is_empty() {
        return files_total;
    }

    // Otherwise fan the subdirectories out to a work-stealing pool, which adds
    // to `progress` as it goes.
    let sub_total = size_subdirs_parallel(subdirs, cancel, progress);
    files_total.saturating_add(sub_total)
}

/// Sum the total bytes across a set of subdirectory roots using a crossbeam
/// work-stealing deque and scoped OS threads. Workers add discovered bytes to
/// the shared `progress` counter live; the function returns the delta they
/// contributed (so callers can compose it into a parent total).
fn size_subdirs_parallel(roots: Vec<PathBuf>, cancel: &AtomicBool, progress: &AtomicU64) -> u64 {
    let thread_count = size_thread_count();
    let before = progress.load(Ordering::Relaxed);

    // Shared state.
    let injector: Injector<PathBuf> = Injector::new();
    let active = AtomicUsize::new(0);

    for root in roots {
        injector.push(root);
        active.fetch_add(1, Ordering::SeqCst);
    }

    // Per-worker LIFO local queues (DFS flavour: good locality). Stealers are
    // shared so idle workers can steal from busy ones.
    let workers: Vec<Worker<PathBuf>> =
        (0..thread_count).map(|_| Worker::new_lifo()).collect();
    let stealers: Vec<Stealer<PathBuf>> = workers.iter().map(|w| w.stealer()).collect();

    // Scoped threads borrow `injector`, `active`, `progress`, `stealers`,
    // `cancel` directly — no Arc needed. Each worker MOVES one `Worker` into
    // itself (crossbeam's `Worker` is not `Sync`).
    std::thread::scope(|s| {
        for worker in workers {
            let injector = &injector;
            let stealers = &stealers;
            let active = &active;
            let progress = &*progress;
            s.spawn(move || {
                size_worker(worker, injector, stealers, active, progress, cancel);
            });
        }
    });

    progress.load(Ordering::Relaxed).saturating_sub(before)
}

/// One work-stealing worker: pulls directory tasks, lists each directory,
/// sums file sizes into the shared `progress` counter, and pushes child
/// directories back onto its own queue.
fn size_worker(
    worker: Worker<PathBuf>,
    injector: &Injector<PathBuf>,
    stealers: &[Stealer<PathBuf>],
    active: &AtomicUsize,
    progress: &AtomicU64,
    cancel: &AtomicBool,
) {
    loop {
        // Responsive cancellation: checked once per loop iteration.
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        match find_size_task(&worker, injector, stealers) {
            Some(dir) => {
                let entries = crate::fast_readdir::read_dir_fast(&dir);
                let mut local: u64 = 0;

                for (i, entry) in entries.iter().enumerate() {
                    if i % CANCEL_CHECK_STRIDE == 0 && cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    if entry.is_dir {
                        // Queue the child directory; keep the active count in
                        // sync so the pool knows there is outstanding work.
                        active.fetch_add(1, Ordering::SeqCst);
                        worker.push(entry.path.clone());
                    } else {
                        // File or symlink — size already known from listing.
                        local = local.saturating_add(entry.size);
                    }
                }

                if local > 0 {
                    // Publish incrementally so observers see the total climb.
                    progress.fetch_add(local, Ordering::Relaxed);
                }
                // This directory task is now complete.
                active.fetch_sub(1, Ordering::SeqCst);
            }
            None => {
                // No task available anywhere. If nothing is outstanding, the
                // walk is finished; otherwise yield and retry (another worker
                // may push more tasks).
                if active.load(Ordering::Acquire) == 0 {
                    break;
                }
                std::thread::yield_now();
            }
        }
    }
}

/// Find a directory task: own queue first, then the injector, then steal from
/// other workers. Mirrors `traversal::parallel::find_task`.
fn find_size_task(
    worker: &Worker<PathBuf>,
    injector: &Injector<PathBuf>,
    stealers: &[Stealer<PathBuf>],
) -> Option<PathBuf> {
    if let Some(t) = worker.pop() {
        return Some(t);
    }
    loop {
        match injector.steal() {
            Steal::Empty => break,
            Steal::Success(t) => return Some(t),
            Steal::Retry => continue,
        }
    }
    for stealer in stealers {
        loop {
            match stealer.steal() {
                Steal::Empty => break,
                Steal::Success(t) => return Some(t),
                Steal::Retry => continue,
            }
        }
    }
    None
}

/// Walk directories starting at `roots`, invoking `on_dir` for every
/// subdirectory discovered. When `prune(path, file_type)` returns `true` for a
/// directory, that directory is still reported via `on_dir` but is **not**
/// descended into — this is what lets the rules engine find artifact
/// directories (e.g. `node_modules`, `target`) without traversing the millions
/// of files inside them.
///
/// Uses the optimized `read_dir_fast` on Windows for faster enumeration.
///
/// Design note: this walker is intentionally **single-threaded**. Correctness
/// and simplicity matter more than raw speed here because the pruning is what
/// makes it fast — once a heavyweight directory is pruned we never touch its
/// contents at all. A single-threaded walker also lets `on_dir` be a plain
/// `&mut dyn FnMut` (no `Sync`/locking requirement), which keeps the API
/// ergonomic for the caller accumulating findings.
///
/// Symlinked directories are not descended into (detected via `is_symlink`
/// flag from the fast readdir), preventing the walk from escaping or looping.
///
/// `cancel` is checked before reading each directory and while iterating
/// entries; when set, the walk stops promptly.
pub fn walk_pruned(
    roots: &[PathBuf],
    cancel: &AtomicBool,
    prune: &(dyn Fn(&Path, &FileType) -> bool + Sync),
    on_dir: &mut dyn FnMut(&Path),
) {
    // Explicit LIFO stack of directories still to descend into.
    let mut stack: Vec<PathBuf> = roots.to_vec();

    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            return;
        }

        let entries = crate::fast_readdir::read_dir_fast(&dir);

        for (i, entry) in entries.iter().enumerate() {
            if i % CANCEL_CHECK_STRIDE == 0 && cancel.load(Ordering::Relaxed) {
                return;
            }

            // Only real directories are of interest. Symlinks are ignored
            // (is_symlink entries cannot cause the walk to escape).
            if entry.is_dir {
                let child = &entry.path;
                // Always report the directory to the caller.
                on_dir(child);

                // Build a synthetic FileType for the prune predicate.
                // We know it's a directory, so we get the real FileType via a
                // quick metadata call. If that fails, assume not-pruned and descend.
                let should_prune = match fs::symlink_metadata(child) {
                    Ok(meta) => {
                        let ft = meta.file_type();
                        prune(child, &ft)
                    }
                    Err(_) => false,
                };

                if !should_prune {
                    stack.push(child.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs::File;
    use std::io::Write;
    use std::sync::atomic::AtomicBool;

    fn write_file(path: &Path, bytes: &[u8]) {
        let mut f = File::create(path).expect("create file");
        f.write_all(bytes).expect("write file");
    }

    #[test]
    fn dir_size_sums_known_sizes() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        // root/a.bin = 10 bytes, root/b.bin = 25 bytes
        write_file(&root.join("a.bin"), &[0u8; 10]);
        write_file(&root.join("b.bin"), &[0u8; 25]);

        // root/sub/c.bin = 100 bytes, root/sub/deep/d.bin = 5 bytes
        let sub = root.join("sub");
        fs::create_dir(&sub).expect("create sub");
        write_file(&sub.join("c.bin"), &[0u8; 100]);
        let deep = sub.join("deep");
        fs::create_dir(&deep).expect("create deep");
        write_file(&deep.join("d.bin"), &[0u8; 5]);

        let cancel = AtomicBool::new(false);
        let total = dir_size(root, &cancel);

        assert_eq!(total, 10 + 25 + 100 + 5);
    }

    #[test]
    fn dir_size_cancelled_returns_partial_not_panic() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write_file(&root.join("a.bin"), &[0u8; 42]);

        // Pre-cancelled: must return 0 (nothing summed) and never panic.
        let cancel = AtomicBool::new(true);
        let total = dir_size(root, &cancel);
        assert_eq!(total, 0);
    }

    #[test]
    fn walk_pruned_does_not_visit_children_of_pruned_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        // Layout:
        //   root/keep/inner_keep/
        //   root/node_modules/pkg/   <- pruned; children must NOT be visited
        let keep = root.join("keep");
        let inner_keep = keep.join("inner_keep");
        fs::create_dir_all(&inner_keep).expect("create keep tree");

        let node_modules = root.join("node_modules");
        let pkg = node_modules.join("pkg");
        fs::create_dir_all(&pkg).expect("create node_modules tree");

        let cancel = AtomicBool::new(false);
        let mut visited: HashSet<PathBuf> = HashSet::new();

        let prune = |path: &Path, ft: &FileType| -> bool {
            ft.is_dir()
                && path
                    .file_name()
                    .map(|n| n == "node_modules")
                    .unwrap_or(false)
        };

        walk_pruned(
            std::slice::from_ref(&root.to_path_buf()),
            &cancel,
            &prune,
            &mut |p: &Path| {
                visited.insert(p.to_path_buf());
            },
        );

        // The pruned directory itself IS reported.
        assert!(visited.contains(&node_modules), "node_modules should be reported");
        // But its children are NOT descended into / visited.
        assert!(!visited.contains(&pkg), "pruned dir's child must not be visited");

        // Non-pruned directories are fully walked.
        assert!(visited.contains(&keep), "keep should be visited");
        assert!(visited.contains(&inner_keep), "inner_keep should be visited");
    }
}
