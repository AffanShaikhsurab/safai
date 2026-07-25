//! Benchmark harness for Safai's directory-sizing path.
//!
//! Measures [`safai_core::measure::dir_size`] — the *production* code path the
//! scanner uses to size a folder — so the numbers reflect the real app rather
//! than a benchmark-only reimplementation.
//!
//! Emits a single line of JSON so `benchmarks/run-benchmark.ps1` can compare it
//! against the Windows built-in baselines.
//!
//! ```text
//! cargo run --release -p safai-core --example walk_bench -- <root> [iterations]
//! ```
//!
//! ## Methodology notes
//!
//! * One untimed warm-up pass runs first. Without it the first iteration pays
//!   for populating the NTFS metadata cache and every later one doesn't, which
//!   makes the result depend entirely on run order rather than on the code.
//!   Every tool in the comparison gets the same warm-up treatment.
//! * `min` is the headline figure: it's the run least polluted by unrelated
//!   system activity. Median and mean are reported too so a suspiciously fast
//!   outlier is visible rather than hidden.
//! * Release build only. A debug build of this code is several times slower and
//!   would flatter nothing.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use safai_core::measure::dir_size;

fn main() {
    let mut args = std::env::args().skip(1);

    let root = match args.next() {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!(
                "usage: walk_bench <root> [iterations]\n\
                 example: cargo run --release -p safai-core --example walk_bench -- C:/Users/me 5"
            );
            std::process::exit(2);
        }
    };

    let iterations: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(5).max(1);

    if !root.is_dir() {
        eprintln!("not a directory: {}", root.display());
        std::process::exit(2);
    }

    // `dir_size` takes a cancellation flag; benchmarks never cancel.
    let never_cancel = AtomicBool::new(false);

    // Warm-up: populate the filesystem metadata cache so the timed runs below
    // measure traversal, not cold-cache I/O.
    let warm_bytes = dir_size(&root, &never_cancel);

    let mut timings_ms: Vec<f64> = Vec::with_capacity(iterations);
    let mut bytes = warm_bytes;

    for _ in 0..iterations {
        let started = Instant::now();
        bytes = dir_size(&root, &never_cancel);
        timings_ms.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    let mut sorted = timings_ms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("timings are finite"));

    let min = sorted[0];
    let max = sorted[sorted.len() - 1];
    let median = if sorted.len().is_multiple_of(2) {
        (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
    } else {
        sorted[sorted.len() / 2]
    };
    let mean = timings_ms.iter().sum::<f64>() / timings_ms.len() as f64;

    // Single-line JSON keeps the PowerShell side trivial to parse.
    println!(
        r#"{{"tool":"safai","root":{root:?},"iterations":{iterations},"bytes":{bytes},"threads":{threads},"minMs":{min:.1},"medianMs":{median:.1},"meanMs":{mean:.1},"maxMs":{max:.1}}}"#,
        root = root.display().to_string(),
        iterations = iterations,
        bytes = bytes,
        threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1),
        min = min,
        median = median,
        mean = mean,
        max = max,
    );
}
