//! Read-only dry run of the dev-project scan. Deletes nothing.
//!
//! ```text
//! cargo run --release -p safai-rules --example dev_scan            # all drives
//! cargo run --release -p safai-rules --example dev_scan -- D:\code # one folder
//! ```
//!
//! Prints every project with regenerable artifacts, most inactive first, and
//! what an inactivity threshold of 15 / 30 days would select.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use safai_rules::{run_scan, ScanConfig, ScanEvent};

fn gb(bytes: u64) -> String {
    format!("{:.2} GB", bytes as f64 / 1024f64.powi(3))
}

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let roots = if args.is_empty() {
        // Every drive letter that exists (the app itself restricts this to
        // fixed drives via Win32).
        ('A'..='Z')
            .map(|c| PathBuf::from(format!("{c}:\\")))
            .filter(|p| p.is_dir())
            .collect()
    } else {
        args
    };
    eprintln!("Scanning {roots:?} …");

    let cfg = ScanConfig {
        roots: Vec::new(),
        project_scan_roots: roots,
        discover_large_folders: false,
    };
    let started = Instant::now();
    let report = run_scan(&cfg, &AtomicBool::new(false), &mut |ev| {
        if let ScanEvent::Progress { current_path, .. } = ev {
            eprint!("\r{current_path:<70}");
        }
    });
    eprintln!(
        "\rDone in {:.1}s{:<60}",
        started.elapsed().as_secs_f64(),
        ""
    );

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // project root -> (name, kind, last active, total, artifacts)
    type Row = (String, String, Option<u64>, u64, Vec<(String, u64)>);
    let mut projects: BTreeMap<String, Row> = BTreeMap::new();
    for item in report.groups.iter().flat_map(|g| g.items.iter()) {
        let Some(dev) = &item.dev else { continue };
        let row = projects.entry(dev.project_root.clone()).or_insert_with(|| {
            (
                dev.project_name.clone(),
                dev.project_kind.clone(),
                dev.last_active_secs,
                0,
                Vec::new(),
            )
        });
        row.3 += item.size_bytes;
        let rel = item
            .path
            .strip_prefix(&dev.project_root)
            .unwrap_or(&item.path)
            .trim_start_matches('/')
            .to_string();
        row.4.push((rel, item.size_bytes));
    }

    let mut rows: Vec<(String, Row)> = projects.into_iter().collect();
    let idle_days = |a: Option<u64>| a.map(|t| now.saturating_sub(t) / 86_400);
    rows.sort_by_key(|(_, r)| std::cmp::Reverse(idle_days(r.2).unwrap_or(0)));

    println!("\n{:>9}  {:>10}  {:<20} PROJECT", "SIZE", "IDLE", "KIND");
    for (root, (_, kind, active, total, arts)) in &rows {
        let idle = idle_days(*active)
            .map(|d| format!("{d}d"))
            .unwrap_or_else(|| "unknown".into());
        println!("{:>9}  {:>10}  {:<20} {}", gb(*total), idle, kind, root);
        for (rel, size) in arts {
            println!("{:>9}  {:>10}  {:<20}   └ {}", gb(*size), "", "", rel);
        }
    }

    let total: u64 = rows.iter().map(|(_, r)| r.3).sum();
    println!(
        "\n{} projects, {} in regenerable folders.",
        rows.len(),
        gb(total)
    );
    for days in [15u64, 30] {
        let (n, bytes) = rows
            .iter()
            .filter(|(_, r)| idle_days(r.2).is_some_and(|d| d >= days))
            .fold((0, 0u64), |(n, b), (_, r)| (n + 1, b + r.3));
        println!(
            "Inactive {days}+ days: {n} projects, {} would be freed.",
            gb(bytes)
        );
    }
    for w in &report.warnings {
        println!("warning: {w}");
    }
}
