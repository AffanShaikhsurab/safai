# Benchmarks

Safai's scanner has one expensive job: walk a directory tree and total up how
much space it occupies. This directory measures that job against the tools
Windows already ships with, on the same tree, answering the same question.

## Running it

```powershell
# Safai vs PowerShell over your user profile (recommended)
./benchmarks/run-benchmark.ps1 -SkipDir

# Include the cmd /c dir /s baseline (slow — see the caveat below)
./benchmarks/run-benchmark.ps1

# A specific tree, more timed iterations
./benchmarks/run-benchmark.ps1 -Root D:\code -Iterations 5 -SkipDir
```

The script builds `crates/safai-core/examples/walk_bench.rs` in release mode,
runs each contender, prints a comparison table, and writes `results.json`.

## What is actually being measured

Every contender computes **the total byte size of a directory tree, recursively**.
Nothing else. That is the operation that dominates a Safai scan, and it is
something all three tools can do natively, which is what makes the comparison
fair.

The Safai side calls `safai_core::measure::dir_size` — the real function the app
uses during a scan, not a benchmark-only reimplementation.

## Methodology

Filesystem benchmarks are easy to get wrong. These are the things this harness
controls for:

- **Cache warm-up.** Every contender gets one untimed pass before its timed runs.
  Without this the first tool to run pays for populating the NTFS metadata cache
  and the rest don't, so the "winner" is just whoever went second.
- **`min` is the headline.** It's the run least disturbed by unrelated system
  activity. Median, mean and max are reported too, so a suspicious outlier is
  visible rather than averaged away.
- **Equivalence check.** Byte totals are printed for each tool. If they diverge
  by more than 10% the script says the comparison is invalid, because that means
  the tools didn't examine the same set of files.
- **Release builds only.** A debug build of the Rust walker is several times
  slower; benchmarking it would prove nothing.
- **No PowerShell pipeline tax on the baselines.** The `dir /s` contender filters
  its output with `findstr` inside `cmd` and writes to a temp file. Capturing
  `& cmd /c dir /s` into a PowerShell variable would materialise one object per
  output line — 5.3M+ of them on a large profile — and the measurement would
  become PowerShell's overhead rather than `dir`'s work. That would unfairly
  flatter Safai.

## Reference run

Recorded 25 July 2026.

| | |
| --- | --- |
| OS | Windows 11 Home, build 26200 |
| CPU | Intel Core i5-13420H (8 cores / 12 threads) |
| RAM | 31.7 GB |
| Disk | NVMe SSD (SK hynix HFS512GEJ9X110N, 477 GB), NTFS |
| Toolchain | rustc 1.97.1, `--release` |
| Tree | `C:\Users\affan` — 5,333,449 files, 285.9 GiB |

| Tool | Best | Files/sec | Total measured |
| --- | --- | --- | --- |
| `Get-ChildItem -Recurse` | 676.4 s | ~7,900 | 306,955,475,218 B |
| **Safai** (`dir_size`) | **19.1 s** | **~279,000** | 306,947,828,097 B |

**≈35× faster.** Safai's timing is the best of 3 timed runs (median 20.9 s, mean
20.6 s, max 22.0 s) after a warm-up pass. The `Get-ChildItem` figure is a single
warm pass — it takes ~11 minutes each, so repeating it 3× wasn't worth the wall
clock. Both ran warm, against the same tree, on an otherwise idle machine.

`cmd /c dir /s` is **not** in the table: it failed to complete two passes over
this tree within 25 minutes, so there is no number to report. It's noticeably
slower than `Get-ChildItem` here, but "we gave up waiting" isn't a measurement,
so it isn't presented as one.

### Why the totals differ slightly

0.0025% — about 7.6 MB out of 286 GB. Two causes, both expected:

1. **Reparse points.** Safai deliberately does not follow junctions or symlinks,
   which is what stops a scan from double-counting or looping. `Get-ChildItem`
   treats some of them as ordinary entries.
2. **A live filesystem.** Browser caches, logs and temp files changed between the
   two runs. Nothing was holding the tree still.

The agreement being this tight is the point: it confirms both tools walked
essentially the same 5.3M files, so the timing difference reflects traversal
speed and nothing else.

## Where the speed comes from

- **Parallel work-stealing traversal.** `crates/safai-core/src/measure.rs` uses
  crossbeam work-stealing deques across all cores. Directory trees are wildly
  unbalanced, so a naive "one thread per root" split leaves most threads idle
  behind one huge folder; stealing means every thread piles onto whatever work is
  left.
- **One syscall per entry.** Sizes come from the directory enumeration itself
  (`read_dir_fast`) instead of a follow-up `metadata()` call per file. On 5.3M
  files that halves the syscall count.
- **Pruning.** The walker never descends into a directory it has already been
  told to treat as a unit, so sizing `node_modules` doesn't mean touching every
  file inside it twice.
- **No per-object allocation.** `Get-ChildItem` constructs a full `FileInfo`
  object per file. At 5.3M files that allocation and GC pressure is most of the
  cost.
