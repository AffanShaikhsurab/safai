//! Dev-project artifact discovery.
//!
//! For most developers the bulk of "mystery" disk usage isn't source code or
//! app data — it's the dependency and build folders every project downloads or
//! generates: `node_modules`, Rust `target`, Gradle `.gradle`/`build`, Flutter
//! `.dart_tool`, CocoaPods `Pods`, Python virtualenvs, .NET `bin`/`obj`, …
//! They all regenerate with one command (`npm install`, `cargo build`,
//! `flutter pub get`, …), so the ones belonging to projects you haven't touched
//! in weeks are the safest big wins on the disk.
//!
//! This module:
//!   1. walks whole drives in parallel, skipping system/app folders, and finds
//!      folders whose *name* is a known artifact ([`SPECS`]);
//!   2. **validates** each hit with marker files next to it (`target` only
//!      counts beside a `Cargo.toml`/`pom.xml`, `vendor` only when it's also
//!      git-ignored, …) so generic names never cause false positives;
//!   3. groups artifacts into projects (git root, or the topmost folder in a
//!      contiguous chain of project manifests);
//!   4. computes each project's **last activity** from its source files and
//!      git metadata — never from the artifact folders, which installs touch.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use safai_core::read_dir_fast;

// ---------------------------------------------------------------------------
// Artifact table
// ---------------------------------------------------------------------------

/// How a folder with a matching name proves it really is an artifact.
#[derive(Debug, Clone, Copy)]
pub enum Check {
    /// The parent folder contains at least one of these entries. Entries may
    /// be relative paths (`ProjectSettings/ProjectVersion.txt`) or `*.ext`
    /// globs (any file in the parent with that extension).
    Sibling(&'static [&'static str]),
    /// A sibling marker *and* the folder itself contains one of these.
    SiblingAndInside(&'static [&'static str], &'static [&'static str]),
    /// The folder itself contains one of these (e.g. `pyvenv.cfg`).
    Inside(&'static [&'static str]),
    /// A sibling marker *and* the parent's `.gitignore` lists the folder. For
    /// names that are sometimes committed on purpose (`vendor`).
    SiblingAndIgnored(&'static [&'static str]),
    /// `node_modules`: a sibling `package.json` plus evidence this is a real
    /// project (a lockfile or a git repo above it). Installed Electron apps
    /// ship `package.json` + `node_modules` but neither of those, so they are
    /// never mistaken for a project.
    NodeModules,
    /// The name alone is unambiguous (`.pytest_cache`).
    Unique,
}

/// One kind of regenerable project folder.
#[derive(Debug)]
pub struct ArtifactSpec {
    pub rule_id: &'static str,
    /// Folder name (compared case-insensitively).
    pub name: &'static str,
    pub ecosystem: &'static str,
    pub label: &'static str,
    pub check: Check,
    /// What regenerates it.
    pub note: &'static str,
}

const GRADLE_PROJECT: &[&str] = &[
    "settings.gradle",
    "settings.gradle.kts",
    "build.gradle",
    "build.gradle.kts",
    "gradlew",
];
const GRADLE_MODULE: &[&str] = &["build.gradle", "build.gradle.kts"];
const PACKAGE_JSON: &[&str] = &["package.json"];
const VENV_NAMES_NOTE: &str = "A Python virtual environment. Recreate it with \
    `python -m venv` and reinstall from requirements.txt / pyproject.toml (or `uv sync`).";

/// Every artifact kind Safai knows. When several specs share a name (`target`,
/// `build`, `vendor`) the first whose check passes wins.
pub static SPECS: &[ArtifactSpec] = &[
    // -- JavaScript / TypeScript (web, React Native, Electron, Tauri…) ------
    ArtifactSpec {
        rule_id: "node-modules",
        name: "node_modules",
        ecosystem: "Node.js",
        label: "node_modules",
        check: Check::NodeModules,
        note: "Installed JavaScript packages. Restored by `npm install` / `pnpm install` / `yarn` / `bun install`.",
    },
    ArtifactSpec {
        rule_id: "js-build-cache",
        name: ".next",
        ecosystem: "Node.js",
        label: "Next.js build cache",
        check: Check::Sibling(PACKAGE_JSON),
        note: "Next.js build output and cache. Rebuilt by `next dev` / `next build`.",
    },
    ArtifactSpec {
        rule_id: "js-build-cache",
        name: ".nuxt",
        ecosystem: "Node.js",
        label: "Nuxt build cache",
        check: Check::Sibling(PACKAGE_JSON),
        note: "Nuxt generated build folder. Rebuilt by `nuxt dev` / `nuxt build`.",
    },
    ArtifactSpec {
        rule_id: "js-build-cache",
        name: ".svelte-kit",
        ecosystem: "Node.js",
        label: "SvelteKit build cache",
        check: Check::Sibling(PACKAGE_JSON),
        note: "SvelteKit generated output. Rebuilt on the next dev/build run.",
    },
    ArtifactSpec {
        rule_id: "js-build-cache",
        name: ".turbo",
        ecosystem: "Node.js",
        label: "Turborepo cache",
        check: Check::Sibling(PACKAGE_JSON),
        note: "Turborepo task cache. Rebuilt by the next `turbo run`.",
    },
    ArtifactSpec {
        rule_id: "js-build-cache",
        name: ".parcel-cache",
        ecosystem: "Node.js",
        label: "Parcel cache",
        check: Check::Sibling(PACKAGE_JSON),
        note: "Parcel bundler cache. Rebuilt by the next build.",
    },
    ArtifactSpec {
        rule_id: "js-build-cache",
        name: ".angular",
        ecosystem: "Node.js",
        label: "Angular CLI cache",
        check: Check::Sibling(PACKAGE_JSON),
        note: "Angular CLI build cache. Rebuilt by `ng serve` / `ng build`.",
    },
    // -- Rust --------------------------------------------------------------
    ArtifactSpec {
        rule_id: "rust-target",
        name: "target",
        ecosystem: "Rust",
        label: "Rust target folder",
        check: Check::Sibling(&["Cargo.toml"]),
        note: "Cargo build output (debug/release binaries, incremental cache). Rebuilt by `cargo build`.",
    },
    // -- JVM: Maven / Gradle / Android ----------------------------------------
    ArtifactSpec {
        rule_id: "maven-target",
        name: "target",
        ecosystem: "Java (Maven)",
        label: "Maven target folder",
        check: Check::Sibling(&["pom.xml"]),
        note: "Maven build output. Rebuilt by `mvn package`.",
    },
    ArtifactSpec {
        rule_id: "gradle-project-cache",
        name: ".gradle",
        ecosystem: "Gradle / Android",
        label: "Gradle project cache",
        check: Check::Sibling(GRADLE_PROJECT),
        note: "Per-project Gradle cache. Recreated by the next Gradle sync/build.",
    },
    ArtifactSpec {
        rule_id: "gradle-build",
        name: "build",
        ecosystem: "Gradle / Android",
        label: "Gradle build output",
        check: Check::Sibling(GRADLE_MODULE),
        note: "Gradle/Android build output (APKs, intermediates). Rebuilt by `gradlew assemble`.",
    },
    ArtifactSpec {
        rule_id: "gradle-build",
        name: ".cxx",
        ecosystem: "Gradle / Android",
        label: "Android native build cache",
        check: Check::Sibling(GRADLE_MODULE),
        note: "Android NDK/CMake intermediates. Rebuilt by the next Gradle build.",
    },
    ArtifactSpec {
        rule_id: "gradle-build",
        name: ".externalNativeBuild",
        ecosystem: "Gradle / Android",
        label: "Android native build cache",
        check: Check::Sibling(GRADLE_MODULE),
        note: "Legacy Android native build intermediates. Rebuilt by the next Gradle build.",
    },
    ArtifactSpec {
        rule_id: "gradle-project-cache",
        name: ".kotlin",
        ecosystem: "Gradle / Android",
        label: "Kotlin compiler cache",
        check: Check::Sibling(GRADLE_PROJECT),
        note: "Kotlin Gradle plugin session data. Recreated on the next build.",
    },
    // -- Flutter / Dart ------------------------------------------------------
    ArtifactSpec {
        rule_id: "dart-tool",
        name: ".dart_tool",
        ecosystem: "Flutter / Dart",
        label: "Dart tool cache",
        check: Check::Sibling(&["pubspec.yaml"]),
        note: "Resolved packages and build cache for Dart/Flutter. Restored by `flutter pub get`.",
    },
    ArtifactSpec {
        rule_id: "flutter-build",
        name: "build",
        ecosystem: "Flutter / Dart",
        label: "Flutter build output",
        check: Check::Sibling(&["pubspec.yaml"]),
        note: "Flutter build output for every platform. Rebuilt by `flutter run` / `flutter build`.",
    },
    // -- iOS / macOS -----------------------------------------------------------
    ArtifactSpec {
        rule_id: "cocoapods",
        name: "Pods",
        ecosystem: "CocoaPods (iOS)",
        label: "CocoaPods",
        check: Check::Sibling(&["Podfile"]),
        note: "Installed iOS pods. Restored by `pod install`.",
    },
    ArtifactSpec {
        rule_id: "swift-build",
        name: ".build",
        ecosystem: "Swift",
        label: "Swift package build",
        check: Check::Sibling(&["Package.swift"]),
        note: "SwiftPM build output. Rebuilt by `swift build`.",
    },
    // -- Python ----------------------------------------------------------------
    ArtifactSpec {
        rule_id: "python-venv",
        name: ".venv",
        ecosystem: "Python",
        label: "Python virtualenv",
        check: Check::Inside(&["pyvenv.cfg"]),
        note: VENV_NAMES_NOTE,
    },
    ArtifactSpec {
        rule_id: "python-venv",
        name: "venv",
        ecosystem: "Python",
        label: "Python virtualenv",
        check: Check::Inside(&["pyvenv.cfg"]),
        note: VENV_NAMES_NOTE,
    },
    ArtifactSpec {
        rule_id: "python-venv",
        name: "env",
        ecosystem: "Python",
        label: "Python virtualenv",
        check: Check::Inside(&["pyvenv.cfg"]),
        note: VENV_NAMES_NOTE,
    },
    ArtifactSpec {
        rule_id: "python-venv",
        name: ".env",
        ecosystem: "Python",
        label: "Python virtualenv",
        check: Check::Inside(&["pyvenv.cfg"]),
        note: VENV_NAMES_NOTE,
    },
    ArtifactSpec {
        rule_id: "python-cache",
        name: ".tox",
        ecosystem: "Python",
        label: "tox environments",
        check: Check::Unique,
        note: "tox test environments. Recreated by the next `tox` run.",
    },
    ArtifactSpec {
        rule_id: "python-cache",
        name: ".nox",
        ecosystem: "Python",
        label: "nox environments",
        check: Check::Unique,
        note: "nox test environments. Recreated by the next `nox` run.",
    },
    ArtifactSpec {
        rule_id: "python-cache",
        name: ".pytest_cache",
        ecosystem: "Python",
        label: "pytest cache",
        check: Check::Unique,
        note: "pytest's run cache. Recreated automatically.",
    },
    ArtifactSpec {
        rule_id: "python-cache",
        name: ".mypy_cache",
        ecosystem: "Python",
        label: "mypy cache",
        check: Check::Unique,
        note: "mypy's type-check cache. Recreated automatically.",
    },
    ArtifactSpec {
        rule_id: "python-cache",
        name: ".ruff_cache",
        ecosystem: "Python",
        label: "Ruff cache",
        check: Check::Unique,
        note: "Ruff's lint cache. Recreated automatically.",
    },
    // -- .NET ------------------------------------------------------------------
    ArtifactSpec {
        rule_id: "dotnet-build",
        name: "bin",
        ecosystem: ".NET",
        label: ".NET bin folder",
        check: Check::Sibling(&["*.csproj", "*.fsproj", "*.vbproj"]),
        note: ".NET build output. Rebuilt by `dotnet build`.",
    },
    ArtifactSpec {
        rule_id: "dotnet-build",
        name: "obj",
        ecosystem: ".NET",
        label: ".NET obj folder",
        check: Check::Sibling(&["*.csproj", "*.fsproj", "*.vbproj"]),
        note: ".NET intermediate build files. Rebuilt by `dotnet build`.",
    },
    // -- Go ----------------------------------------------------------------------
    // Go keeps dependencies in a global module cache (covered by the Go cache
    // rule), so the only per-project folder is an *ignored* `vendor`.
    ArtifactSpec {
        rule_id: "go-vendor",
        name: "vendor",
        ecosystem: "Go",
        label: "Go vendor folder",
        check: Check::SiblingAndIgnored(&["go.mod"]),
        note: "Vendored Go modules (git-ignored). Restored by `go mod vendor`.",
    },
    // -- PHP ---------------------------------------------------------------------
    ArtifactSpec {
        rule_id: "composer-vendor",
        name: "vendor",
        ecosystem: "PHP",
        label: "Composer vendor folder",
        check: Check::SiblingAndIgnored(&["composer.json"]),
        note: "Installed Composer packages (git-ignored). Restored by `composer install`.",
    },
    // -- Elixir ------------------------------------------------------------------
    ArtifactSpec {
        rule_id: "elixir-build",
        name: "_build",
        ecosystem: "Elixir",
        label: "Mix build output",
        check: Check::Sibling(&["mix.exs"]),
        note: "Mix build output. Rebuilt by `mix compile`.",
    },
    ArtifactSpec {
        rule_id: "elixir-build",
        name: "deps",
        ecosystem: "Elixir",
        label: "Mix dependencies",
        check: Check::Sibling(&["mix.exs"]),
        note: "Fetched Mix dependencies. Restored by `mix deps.get`.",
    },
    // -- C / C++ (CMake) ---------------------------------------------------------
    ArtifactSpec {
        rule_id: "cmake-build",
        name: "build",
        ecosystem: "C/C++ (CMake)",
        label: "CMake build folder",
        check: Check::SiblingAndInside(&["CMakeLists.txt"], &["CMakeCache.txt"]),
        note: "CMake build tree. Regenerated by `cmake -B build` and the next build.",
    },
    // -- Zig / Haskell -------------------------------------------------------------
    ArtifactSpec {
        rule_id: "zig-build",
        name: "zig-out",
        ecosystem: "Zig",
        label: "Zig output",
        check: Check::Sibling(&["build.zig"]),
        note: "Zig build output. Rebuilt by `zig build`.",
    },
    ArtifactSpec {
        rule_id: "zig-build",
        name: ".zig-cache",
        ecosystem: "Zig",
        label: "Zig cache",
        check: Check::Sibling(&["build.zig"]),
        note: "Zig build cache. Rebuilt by `zig build`.",
    },
    ArtifactSpec {
        rule_id: "zig-build",
        name: "zig-cache",
        ecosystem: "Zig",
        label: "Zig cache",
        check: Check::Sibling(&["build.zig"]),
        note: "Zig build cache. Rebuilt by `zig build`.",
    },
    ArtifactSpec {
        rule_id: "haskell-build",
        name: ".stack-work",
        ecosystem: "Haskell",
        label: "Stack work folder",
        check: Check::Sibling(&["stack.yaml"]),
        note: "Stack build output. Rebuilt by `stack build`.",
    },
    ArtifactSpec {
        rule_id: "haskell-build",
        name: "dist-newstyle",
        ecosystem: "Haskell",
        label: "Cabal build folder",
        check: Check::Sibling(&["cabal.project", "*.cabal"]),
        note: "Cabal build output. Rebuilt by `cabal build`.",
    },
    // -- Infra / game engines --------------------------------------------------
    ArtifactSpec {
        rule_id: "terraform",
        name: ".terraform",
        ecosystem: "Terraform",
        label: "Terraform providers",
        check: Check::Sibling(&["*.tf"]),
        note: "Downloaded Terraform providers and modules. Restored by `terraform init`.",
    },
    ArtifactSpec {
        rule_id: "unity-library",
        name: "Library",
        ecosystem: "Unity",
        label: "Unity Library cache",
        check: Check::Sibling(&["ProjectSettings/ProjectVersion.txt"]),
        note: "Unity's imported-asset cache. Rebuilt when the project is next opened (can take a while).",
    },
];

/// Is `name` the name of any artifact spec (validated or not)?
pub fn is_artifact_name(name: &str) -> bool {
    SPECS.iter().any(|s| s.name.eq_ignore_ascii_case(name))
}

// ---------------------------------------------------------------------------
// Where never to look
// ---------------------------------------------------------------------------

/// Skipped at any depth. Installed apps, package managers and SDKs bundle
/// `node_modules`/`build` folders that are *not* user projects.
const SKIP_ANYWHERE: &[&str] = &[
    "appdata",
    "$recycle.bin",
    "system volume information",
    "windowsapps",
    "scoop",
    "nvm",
    "nvm4w",
    "steamapps",
    "steamlibrary",
    "epic games",
    "anaconda3",
    "miniconda3",
    "miniforge3",
    "mambaforge",
    "site-packages",
    "__pycache__",
];

/// Skipped only directly under a drive (or walk) root: the OS and installed
/// software.
const SKIP_AT_ROOT: &[&str] = &[
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "recovery",
    "perflogs",
    "$windows.~bt",
    "$windows.~ws",
    "$winreagent",
    "msocache",
    "config.msi",
    "onedrivetemp",
    "inetpub",
    "xboxgames",
    "drivers",
    "intel",
    "amd",
    "nvidia",
];

/// Maximum depth below a walk root. Drive-relative, so `C:\Users\me\code\
/// monorepo\packages\app\node_modules` (depth 7) is comfortably inside.
pub const MAX_DEPTH: usize = 14;

/// Should the walk refuse to descend into this (non-artifact) folder?
/// `depth` is the folder's depth below the walk root (1 = direct child).
fn skip_descent(path: &Path, name: &str, depth: usize) -> bool {
    if name.starts_with('.') {
        // Hidden tool folders: .git, .vscode, .cargo, .rustup, .nvm, …
        return true;
    }
    let low = name.to_ascii_lowercase();
    if SKIP_ANYWHERE.contains(&low.as_str()) {
        return true;
    }
    if depth == 1 && SKIP_AT_ROOT.contains(&low.as_str()) {
        return true;
    }
    // Go module cache (`…/pkg/mod`) holds read-only third-party sources.
    if low == "mod"
        && path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("pkg"))
    {
        return true;
    }
    // A Flutter SDK checkout contains real Dart packages with `.dart_tool`;
    // it is an SDK, not a project.
    if low.contains("flutter") && path.join("bin").join("internal").is_dir() {
        return true;
    }
    false
}

/// Is `path` somewhere Safai must never treat as a dev project, no matter what
/// the folder looks like? Used to re-check right before deletion.
pub fn is_protected_location(path: &Path) -> bool {
    let parent = match path.parent() {
        Some(p) => p,
        None => return true,
    };
    let mut depth = 0usize;
    for comp in parent.components() {
        let Component::Normal(os) = comp else {
            continue;
        };
        depth += 1;
        let name = os.to_string_lossy();
        let low = name.to_ascii_lowercase();
        if name.starts_with('.') || SKIP_ANYWHERE.contains(&low.as_str()) {
            return true;
        }
        if depth == 1 && SKIP_AT_ROOT.contains(&low.as_str()) {
            return true;
        }
    }
    // A bare drive root or a top-level folder is never an artifact.
    depth == 0
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Does `dir` contain an entry matching `marker` (`name`, `a/b`, or `*.ext`)?
fn has_marker(dir: &Path, marker: &str) -> bool {
    if let Some(ext) = marker.strip_prefix("*.") {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return false;
        };
        return rd.flatten().any(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case(ext))
                && e.file_type().map(|t| t.is_file()).unwrap_or(false)
        });
    }
    dir.join(marker).exists()
}

fn has_any(dir: &Path, markers: &[&str]) -> bool {
    markers.iter().any(|m| has_marker(dir, m))
}

/// Does `parent/.gitignore` ignore the folder `name`?
fn gitignore_lists(parent: &Path, name: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(parent.join(".gitignore")) else {
        return false;
    };
    text.lines().any(|line| {
        let l = line.trim();
        let l = l.strip_prefix('/').unwrap_or(l);
        let l = l.strip_suffix('/').unwrap_or(l);
        let l = l.strip_suffix("/*").unwrap_or(l);
        l == name
    })
}

/// Project lockfiles: proof `node_modules` came from a real install.
const NODE_LOCKFILES: &[&str] = &[
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lockb",
    "bun.lock",
    "npm-shrinkwrap.json",
];

/// How far up to look for a lockfile / git repo (monorepo workspaces).
const ANCESTOR_LOOKUP: usize = 6;

fn node_project_evidence(parent: &Path) -> bool {
    let mut cur = Some(parent);
    for _ in 0..ANCESTOR_LOOKUP {
        let Some(dir) = cur else { break };
        if has_any(dir, NODE_LOCKFILES) || dir.join(".git").exists() {
            return true;
        }
        cur = dir.parent();
    }
    false
}

fn check_passes(check: Check, dir: &Path, parent: &Path, name: &str) -> bool {
    match check {
        Check::Sibling(m) => has_any(parent, m),
        Check::SiblingAndInside(s, i) => has_any(parent, s) && has_any(dir, i),
        Check::Inside(i) => has_any(dir, i),
        Check::SiblingAndIgnored(s) => has_any(parent, s) && gitignore_lists(parent, name),
        Check::NodeModules => has_any(parent, PACKAGE_JSON) && node_project_evidence(parent),
        Check::Unique => true,
    }
}

/// Is `dir` a validated artifact? Returns the matching spec. Purely about the
/// folder's name and its markers — callers decide *where* to look.
pub fn validate(dir: &Path) -> Option<&'static ArtifactSpec> {
    let name = dir.file_name()?.to_str()?;
    let parent = dir.parent()?;
    SPECS
        .iter()
        .filter(|s| s.name.eq_ignore_ascii_case(name))
        .find(|s| check_passes(s.check, dir, parent, s.name))
}

/// Deletion-time re-check: the path is still a real directory (not a symlink
/// or junction), still validates as an artifact, and isn't in a protected
/// location. The deletion engine calls this for dev artifacts outside the
/// regular allow-listed roots.
pub fn revalidate(path: &Path) -> Option<&'static ArtifactSpec> {
    let md = std::fs::symlink_metadata(path).ok()?;
    if !md.file_type().is_dir() || md.file_type().is_symlink() {
        return None;
    }
    if is_protected_location(path) {
        return None;
    }
    validate(path)
}

// ---------------------------------------------------------------------------
// Discovery walk
// ---------------------------------------------------------------------------

/// A validated artifact found by [`discover`].
#[derive(Debug, Clone)]
pub struct DevArtifact {
    pub path: PathBuf,
    pub spec: &'static ArtifactSpec,
}

/// Walk `roots` in parallel and return every validated artifact.
///
/// Matched artifacts are never descended into, nor are system/app folders or
/// hidden tool folders. Junctions and symlinks are never followed (the fast
/// readdir reports them as non-directories). `dirs_seen` is bumped per folder
/// for live progress. Returns `(artifacts, timed_out)`.
pub fn discover(
    roots: &[PathBuf],
    cancel: &AtomicBool,
    deadline: Option<Instant>,
    dirs_seen: &AtomicU64,
) -> (Vec<DevArtifact>, bool) {
    struct Ctx<'a> {
        cancel: &'a AtomicBool,
        deadline: Option<Instant>,
        timed_out: AtomicBool,
        dirs_seen: &'a AtomicU64,
        found: Mutex<Vec<DevArtifact>>,
    }

    fn visit<'s>(s: &rayon::Scope<'s>, ctx: &'s Ctx<'s>, dir: PathBuf, depth: usize) {
        if ctx.cancel.load(Ordering::Relaxed) || ctx.timed_out.load(Ordering::Relaxed) {
            return;
        }
        if ctx.deadline.is_some_and(|d| Instant::now() >= d) {
            ctx.timed_out.store(true, Ordering::Relaxed);
            return;
        }
        ctx.dirs_seen.fetch_add(1, Ordering::Relaxed);

        for entry in read_dir_fast(&dir) {
            if !entry.is_dir {
                continue;
            }
            let Some(name) = entry.path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let child_depth = depth + 1;

            if is_artifact_name(name) {
                if let Some(spec) = validate(&entry.path) {
                    ctx.found.lock().unwrap().push(DevArtifact {
                        path: entry.path.clone(),
                        spec,
                    });
                    continue; // never descend into an artifact
                }
                // An unvalidated `node_modules` (e.g. inside an installed
                // Electron app) is still never worth descending into.
                if name.eq_ignore_ascii_case("node_modules") {
                    continue;
                }
            }

            if child_depth >= MAX_DEPTH || skip_descent(&entry.path, name, child_depth) {
                continue;
            }
            let child = entry.path;
            s.spawn(move |s| visit(s, ctx, child, child_depth));
        }
    }

    let ctx = Ctx {
        cancel,
        deadline,
        timed_out: AtomicBool::new(false),
        dirs_seen,
        found: Mutex::new(Vec::new()),
    };
    rayon::scope(|s| {
        for root in roots {
            let root = root.clone();
            let ctx = &ctx;
            s.spawn(move |s| visit(s, ctx, root, 0));
        }
    });

    let timed_out = ctx.timed_out.load(Ordering::Relaxed);
    let mut out = ctx.found.into_inner().unwrap();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out.dedup_by(|a, b| a.path == b.path);
    (out, timed_out)
}

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

/// Files whose presence marks a folder as (part of) a project. Used to climb
/// from e.g. `android/app/build` up to the React Native project root when
/// there is no git repo to anchor on.
const PROJECT_MANIFESTS: &[&str] = &[
    "package.json",
    "pubspec.yaml",
    "Cargo.toml",
    "go.mod",
    "settings.gradle",
    "settings.gradle.kts",
    "build.gradle",
    "build.gradle.kts",
    "pom.xml",
    "Podfile",
    "pyproject.toml",
    "requirements.txt",
    "setup.py",
    "composer.json",
    "mix.exs",
    "Package.swift",
    "CMakeLists.txt",
    "build.zig",
    "stack.yaml",
];

fn is_home_or_root(dir: &Path) -> bool {
    if dir.parent().is_none() {
        return true;
    }
    match std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        Some(home) => {
            let home = PathBuf::from(home);
            dir == home.as_path() || home.starts_with(dir)
        }
        None => false,
    }
}

/// How far up to look for the enclosing git repository.
const GIT_LOOKUP: usize = 12;

/// The git repository root above `dir`, if any (never the user profile or a
/// drive root — some people version their dotfiles at `~`).
pub fn git_root_for(dir: &Path) -> Option<PathBuf> {
    let mut cur = Some(dir);
    for _ in 0..GIT_LOOKUP {
        let d = cur?;
        if is_home_or_root(d) {
            return None;
        }
        if d.join(".git").exists() {
            return Some(d.to_path_buf());
        }
        cur = d.parent();
    }
    None
}

/// The project folder an artifact belongs to: the nearest git root above it,
/// else the topmost folder in a chain of project manifests (tolerating one
/// manifest-less level such as `packages/` or `crates/` in a monorepo), else
/// the artifact's parent.
pub fn project_root_for(artifact: &Path) -> PathBuf {
    let parent = artifact.parent().unwrap_or(artifact).to_path_buf();
    if let Some(git) = git_root_for(&parent) {
        return git;
    }

    let mut best = parent.clone();
    let mut gap = 0usize;
    let mut cur = parent.parent();
    for _ in 0..ANCESTOR_LOOKUP {
        let Some(dir) = cur else { break };
        if is_home_or_root(dir) {
            break;
        }
        if has_any(dir, PROJECT_MANIFESTS) {
            best = dir.to_path_buf();
            gap = 0;
        } else {
            gap += 1;
            if gap > 1 {
                break;
            }
        }
        cur = dir.parent();
    }
    best
}

/// Is anything inside `artifact` tracked by the git repo at `repo`? Some
/// repos deliberately commit `node_modules`/`vendor`/`build` (test fixtures,
/// vendored deps); deleting those would corrupt the working tree, so such
/// folders are never offered. Returns `false` when git isn't available.
pub fn is_tracked_by_git(repo: &Path, artifact: &Path) -> bool {
    let Ok(rel) = artifact.strip_prefix(repo) else {
        return false;
    };
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-C")
        .arg(repo)
        .args(["ls-files", "-z", "--"])
        .arg(rel)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flash from the GUI app.
        cmd.creation_flags(0x0800_0000);
    }
    match cmd.output() {
        Ok(out) => out.status.success() && !out.stdout.is_empty(),
        Err(_) => false,
    }
}

/// Upper bound on entries examined when sampling a project for activity.
const ACTIVITY_ENTRY_BUDGET: usize = 50_000;
/// Depth bound for the activity sample.
const ACTIVITY_MAX_DEPTH: usize = 10;

/// Newest mtime among a project's own files (BFS, skipping artifact folders,
/// hidden folders and `.git`), bounded by [`ACTIVITY_ENTRY_BUDGET`]. Files
/// dated more than a day in the future are ignored (clock skew, archives).
fn newest_source_mtime(root: &Path, now: u64) -> Option<u64> {
    let mut newest: Option<u64> = None;
    let mut seen = 0usize;
    let mut queue: std::collections::VecDeque<(PathBuf, usize)> =
        std::collections::VecDeque::from([(root.to_path_buf(), 0)]);
    let future_limit = now.saturating_add(86_400);

    while let Some((dir, depth)) = queue.pop_front() {
        for entry in read_dir_fast(&dir) {
            seen += 1;
            if seen > ACTIVITY_ENTRY_BUDGET {
                return newest;
            }
            let Some(name) = entry.path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if entry.is_dir {
                if depth + 1 < ACTIVITY_MAX_DEPTH
                    && !is_artifact_name(name)
                    && !name.eq_ignore_ascii_case("node_modules")
                    && !skip_descent(&entry.path, name, usize::MAX)
                {
                    queue.push_back((entry.path, depth + 1));
                }
            } else if !entry.is_symlink
                && entry.modified_secs > 0
                && entry.modified_secs <= future_limit
            {
                newest = Some(newest.map_or(entry.modified_secs, |n| n.max(entry.modified_secs)));
            }
        }
    }
    newest
}

fn mtime_secs(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Newest git activity: commits, checkouts, staging. `FETCH_HEAD` is ignored
/// on purpose — editors auto-fetch in the background.
fn newest_git_mtime(root: &Path) -> Option<u64> {
    let git = root.join(".git");
    if !git.is_dir() {
        return None;
    }
    ["HEAD", "index", "ORIG_HEAD", "COMMIT_EDITMSG", "logs/HEAD"]
        .iter()
        .filter_map(|f| mtime_secs(&git.join(f)))
        .max()
}

/// When was this project last worked on? Max of source-file and git activity.
pub fn last_active_secs(root: &Path, now: u64) -> Option<u64> {
    let files = newest_source_mtime(root, now);
    let git = newest_git_mtime(root);
    match (files, git) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

fn read_small(path: &Path) -> Option<String> {
    const LIMIT: u64 = 512 * 1024;
    let md = std::fs::metadata(path).ok()?;
    if md.len() > LIMIT {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// Best-effort label for what kind of project lives at `root`.
pub fn project_kind(root: &Path, fallback: &str) -> String {
    if let Some(pubspec) = read_small(&root.join("pubspec.yaml")) {
        return if pubspec.contains("flutter:") {
            "Flutter".into()
        } else {
            "Dart".into()
        };
    }
    if let Some(pkg) = read_small(&root.join("package.json")) {
        let has = |dep: &str| pkg.contains(&format!("\"{dep}\""));
        let kind = if has("expo") {
            "Expo (React Native)"
        } else if has("react-native") {
            "React Native"
        } else if has("electron") {
            "Electron"
        } else if pkg.contains("\"@tauri-apps/") {
            "Tauri"
        } else if has("next") {
            "Next.js"
        } else if has("nuxt") {
            "Nuxt"
        } else if has("@angular/core") {
            "Angular"
        } else if has("svelte") || has("@sveltejs/kit") {
            "Svelte"
        } else if has("vue") {
            "Vue"
        } else if has("react") || has("solid-js") || has("vite") {
            "Web app"
        } else {
            "Node.js"
        };
        return kind.into();
    }
    let checks: &[(&[&str], &str)] = &[
        (&["Cargo.toml"], "Rust"),
        (&["go.mod"], "Go"),
        (&["app/src/main/AndroidManifest.xml"], "Android"),
        (GRADLE_PROJECT, "Gradle"),
        (&["pom.xml"], "Java (Maven)"),
        (
            &["pyproject.toml", "requirements.txt", "setup.py", "Pipfile"],
            "Python",
        ),
        (&["*.sln", "*.csproj", "*.fsproj"], ".NET"),
        (&["composer.json"], "PHP"),
        (&["mix.exs"], "Elixir"),
        (&["Package.swift"], "Swift"),
        (&["ProjectSettings/ProjectVersion.txt"], "Unity"),
        (&["CMakeLists.txt"], "C/C++"),
        (&["build.zig"], "Zig"),
        (&["stack.yaml", "cabal.project"], "Haskell"),
    ];
    for (markers, kind) in checks {
        if has_any(root, markers) {
            return (*kind).into();
        }
    }
    fallback.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(path: &Path) {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(path, b"x").unwrap();
    }

    fn mkdir(path: &Path) {
        fs::create_dir_all(path).unwrap();
    }

    fn found_names(root: &Path) -> Vec<String> {
        let (found, timed_out) = discover(
            &[root.to_path_buf()],
            &AtomicBool::new(false),
            None,
            &AtomicU64::new(0),
        );
        assert!(!timed_out);
        found
            .iter()
            .map(|a| {
                a.path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn generic_names_need_their_marker_files() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        // Real ones.
        touch(&r.join("rust/Cargo.toml"));
        mkdir(&r.join("rust/target/debug"));
        touch(&r.join("flutter/pubspec.yaml"));
        mkdir(&r.join("flutter/build"));
        mkdir(&r.join("flutter/.dart_tool"));
        touch(&r.join("android/app/build.gradle"));
        mkdir(&r.join("android/app/build"));
        touch(&r.join("dotnet/App.csproj"));
        mkdir(&r.join("dotnet/bin"));
        // Look-alikes with no marker: must be ignored.
        mkdir(&r.join("docs/build"));
        mkdir(&r.join("game/target"));
        mkdir(&r.join("tools/bin"));
        // electron-builder keeps *source* icons in `build/` next to package.json.
        touch(&r.join("electron/package.json"));
        touch(&r.join("electron/build/icon.png"));

        let mut names = found_names(r);
        names.sort();
        assert_eq!(
            names,
            vec![
                "android/app/build",
                "dotnet/bin",
                "flutter/.dart_tool",
                "flutter/build",
                "rust/target",
            ]
        );
    }

    #[test]
    fn node_modules_needs_a_lockfile_or_git() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        touch(&r.join("web/package.json"));
        touch(&r.join("web/package-lock.json"));
        mkdir(&r.join("web/node_modules/react"));
        // Monorepo package: lockfile lives at the workspace root.
        touch(&r.join("mono/pnpm-lock.yaml"));
        touch(&r.join("mono/packages/ui/package.json"));
        mkdir(&r.join("mono/packages/ui/node_modules"));
        // Installed Electron app: package.json + node_modules, no lockfile/git.
        touch(&r.join("Apps/Thing/resources/app/package.json"));
        mkdir(&r.join("Apps/Thing/resources/app/node_modules/dep"));

        let mut names = found_names(r);
        names.sort();
        assert_eq!(
            names,
            vec!["mono/packages/ui/node_modules", "web/node_modules"]
        );
    }

    #[test]
    fn vendor_counts_only_when_git_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        touch(&r.join("committed/go.mod"));
        mkdir(&r.join("committed/vendor/x"));
        touch(&r.join("ignored/go.mod"));
        fs::write(r.join("ignored/.gitignore"), "/vendor/\n").unwrap();
        mkdir(&r.join("ignored/vendor/x"));

        assert_eq!(found_names(r), vec!["ignored/vendor"]);
    }

    #[test]
    fn venvs_are_recognised_by_pyvenv_cfg() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        touch(&r.join("py/.venv/pyvenv.cfg"));
        mkdir(&r.join("other/env/config")); // just a folder called env
        assert_eq!(found_names(r), vec!["py/.venv"]);
    }

    #[test]
    fn system_and_app_folders_are_never_walked() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        for skipped in ["Program Files/x", "AppData/Local/y", ".vscode/extensions/z"] {
            let base = r.join(skipped);
            touch(&base.join("package.json"));
            touch(&base.join("package-lock.json"));
            mkdir(&base.join("node_modules"));
        }
        assert!(found_names(r).is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn protected_locations_are_refused_at_delete_time() {
        assert!(is_protected_location(Path::new(
            r"C:\Users\me\AppData\Roaming\Code\node_modules"
        )));
        assert!(is_protected_location(Path::new(
            r"C:\Program Files\nodejs\node_modules"
        )));
        assert!(is_protected_location(Path::new(
            r"C:\Users\me\.vscode\extensions\x\node_modules"
        )));
        assert!(is_protected_location(Path::new(r"C:\node_modules")));
        assert!(!is_protected_location(Path::new(
            r"C:\Users\me\code\app\node_modules"
        )));
        assert!(!is_protected_location(Path::new(r"D:\work\app\target")));
    }

    #[test]
    fn nested_artifacts_group_under_the_project_root() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path().join("rn-app");
        touch(&r.join("package.json"));
        touch(&r.join("android/settings.gradle"));
        touch(&r.join("android/app/build.gradle"));
        mkdir(&r.join("android/app/build"));
        assert_eq!(project_root_for(&r.join("android/app/build")), r);

        // A git root wins over the manifest chain.
        let g = tmp.path().join("repo");
        mkdir(&g.join(".git"));
        touch(&g.join("crates/core/Cargo.toml"));
        mkdir(&g.join("crates/core/target"));
        assert_eq!(project_root_for(&g.join("crates/core/target")), g);
    }

    #[test]
    fn activity_ignores_artifact_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        touch(&r.join("src/main.rs"));
        touch(&r.join("target/debug/app.exe"));
        touch(&r.join("node_modules/x/index.js"));

        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        let set = |p: &Path, t| {
            fs::File::options()
                .write(true)
                .open(p)
                .unwrap()
                .set_modified(t)
                .unwrap()
        };
        set(&r.join("src/main.rs"), old);
        // Artifacts are "new" (just installed/built) but must not count.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(last_active_secs(r, now), Some(1_000_000));
    }

    #[test]
    fn project_kind_reads_manifests() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        fs::write(
            r.join("package.json"),
            r#"{"dependencies":{"react-native":"0.76.0"}}"#,
        )
        .unwrap();
        assert_eq!(project_kind(r, "x"), "React Native");

        let f = tempfile::tempdir().unwrap();
        fs::write(
            f.path().join("pubspec.yaml"),
            "name: a\nflutter:\n  uses: 1\n",
        )
        .unwrap();
        assert_eq!(project_kind(f.path(), "x"), "Flutter");
    }
}
