//! The cleanup rule table (implementation-plan.md §5 WS2).
//!
//! A [`CleanupRule`] describes *where* to look and *how safe* removal is. Two
//! shapes of location are supported:
//!   * [`PathSpec`] — a fixed, env-expandable known path (a cache dir or file).
//!   * [`DirPattern`] — artifact directory *names* to discover via the pruned
//!     walk (`node_modules`, `target`, …).
//!
//! Rules never touch the filesystem themselves; `scan.rs` drives them.

use crate::model::{Category, SafetyTier};

/// A fixed known path, expressed with env tokens understood by
/// [`crate::detect::expand`] (e.g. `%LOCALAPPDATA%/uv/cache`).
#[derive(Debug, Clone, Copy)]
pub struct PathSpec {
    pub raw: &'static str,
}

impl PathSpec {
    pub const fn new(raw: &'static str) -> Self {
        PathSpec { raw }
    }
}

/// Artifact directory names to locate via `safai_core::walk_pruned`. When a
/// directory whose name matches one of `names` is found, it is reported and the
/// walker does not descend into it.
#[derive(Debug, Clone, Copy)]
pub struct DirPattern {
    pub names: &'static [&'static str],
}

/// A single cleanup rule.
#[derive(Debug, Clone)]
pub struct CleanupRule {
    pub id: &'static str,
    pub label: &'static str,
    pub category: Category,
    pub tier: SafetyTier,
    /// Fixed known locations (env-expandable). May be empty for pattern rules.
    pub locations: Vec<PathSpec>,
    /// Directory-name pattern discovered via the pruned walk. `None` for
    /// fixed-location rules.
    pub pattern: Option<DirPattern>,
    /// Whether the target regenerates automatically after deletion.
    pub regenerates: bool,
    /// Human explanation of why removal is safe / what regenerates.
    pub note: &'static str,
    /// Gate: only surface this rule when the named tool is on PATH.
    pub requires_tool: Option<&'static str>,
}

/// The full seed rule set. Order is roughly by category for stable output.
pub fn all_rules() -> Vec<CleanupRule> {
    vec![
        // ---------------------------------------------------------------
        // Package-manager caches (regenerate on next install).
        // ---------------------------------------------------------------
        CleanupRule {
            id: "uv-cache",
            label: "Python uv cache",
            category: Category::PackageCache,
            tier: SafetyTier::Safe,
            locations: vec![PathSpec::new("%LOCALAPPDATA%/uv/cache")],
            pattern: None,
            regenerates: true,
            note: "Cached wheels and source distributions downloaded by uv. \
                   Safe to remove; uv re-downloads packages on the next install.",
            requires_tool: Some("uv"),
        },
        CleanupRule {
            id: "npm-cache",
            label: "npm cache",
            category: Category::PackageCache,
            tier: SafetyTier::Safe,
            locations: vec![PathSpec::new("%LOCALAPPDATA%/npm-cache")],
            pattern: None,
            regenerates: true,
            note: "npm's package download cache. Safe to remove; npm rebuilds \
                   it automatically. Equivalent to `npm cache clean --force`.",
            requires_tool: Some("npm"),
        },
        CleanupRule {
            id: "gradle-caches",
            label: "Gradle caches",
            category: Category::PackageCache,
            tier: SafetyTier::Safe,
            locations: vec![PathSpec::new("%USERPROFILE%/.gradle/caches")],
            pattern: None,
            regenerates: true,
            note: "Downloaded dependencies and build caches for Gradle. Safe to \
                   remove; Gradle re-downloads and re-derives them on the next build.",
            requires_tool: Some("gradle"),
        },
        CleanupRule {
            id: "bun-cache",
            label: "Bun install cache",
            category: Category::PackageCache,
            tier: SafetyTier::Safe,
            locations: vec![PathSpec::new("%USERPROFILE%/.bun/install/cache")],
            pattern: None,
            regenerates: true,
            note: "Bun's global package install cache. Safe to remove; Bun \
                   re-populates it on the next install.",
            requires_tool: Some("bun"),
        },
        CleanupRule {
            id: "dart-pub-cache",
            label: "Dart pub cache",
            category: Category::PackageCache,
            tier: SafetyTier::Safe,
            locations: vec![
                PathSpec::new("%LOCALAPPDATA%/Pub/Cache"),
                PathSpec::new("%LOCALAPPDATA%/.dartServer"),
            ],
            pattern: None,
            regenerates: true,
            note: "Dart/Flutter package cache (Pub) and analysis-server scratch \
                   data. Safe to remove; pub re-fetches packages and the analyzer \
                   rebuilds its index on the next run.",
            requires_tool: Some("dart"),
        },
        CleanupRule {
            id: "pnpm-store",
            label: "pnpm content store",
            category: Category::PackageCache,
            tier: SafetyTier::Safe,
            locations: vec![
                PathSpec::new("%LOCALAPPDATA%/pnpm/store"),
                PathSpec::new("%LOCALAPPDATA%/pnpm-cache"),
            ],
            pattern: None,
            regenerates: true,
            note: "pnpm's global content-addressable package store. Safe to \
                   remove; pnpm refetches packages on the next install.",
            requires_tool: Some("pnpm"),
        },
        CleanupRule {
            id: "yarn-cache",
            label: "Yarn cache",
            category: Category::PackageCache,
            tier: SafetyTier::Safe,
            locations: vec![PathSpec::new("%LOCALAPPDATA%/Yarn/Cache")],
            pattern: None,
            regenerates: true,
            note: "Yarn's global package cache. Safe to remove; Yarn rebuilds \
                   it on the next install.",
            requires_tool: Some("yarn"),
        },
        CleanupRule {
            id: "cargo-registry",
            label: "Cargo registry cache",
            category: Category::PackageCache,
            tier: SafetyTier::Safe,
            locations: vec![
                PathSpec::new("%USERPROFILE%/.cargo/registry/cache"),
                PathSpec::new("%USERPROFILE%/.cargo/registry/src"),
            ],
            pattern: None,
            regenerates: true,
            note: "Downloaded crate archives and unpacked sources for Cargo. \
                   Safe to remove; Cargo re-downloads crates on the next build.",
            requires_tool: Some("cargo"),
        },
        CleanupRule {
            id: "pip-cache",
            label: "pip download cache",
            category: Category::PackageCache,
            tier: SafetyTier::Safe,
            locations: vec![PathSpec::new("%LOCALAPPDATA%/pip/cache")],
            pattern: None,
            regenerates: true,
            note: "pip's wheel/download cache. Safe to remove; pip re-downloads \
                   packages when needed.",
            requires_tool: Some("pip"),
        },
        CleanupRule {
            id: "go-cache",
            label: "Go build & module cache",
            category: Category::PackageCache,
            tier: SafetyTier::Safe,
            locations: vec![
                PathSpec::new("%LOCALAPPDATA%/go-build"),
                PathSpec::new("%USERPROFILE%/go/pkg/mod/cache"),
            ],
            pattern: None,
            regenerates: true,
            note: "Go's compiled build cache and module download cache. Safe to \
                   remove; the toolchain rebuilds and re-downloads as needed.",
            requires_tool: Some("go"),
        },
        // ---------------------------------------------------------------
        // Browser binaries downloaded by tooling.
        // ---------------------------------------------------------------
        CleanupRule {
            id: "playwright-browsers",
            label: "Playwright browsers",
            category: Category::Browser,
            tier: SafetyTier::Safe,
            locations: vec![PathSpec::new("%LOCALAPPDATA%/ms-playwright")],
            pattern: None,
            regenerates: true,
            note: "Chromium/Firefox/WebKit builds downloaded by Playwright. \
                   Safe to remove; re-downloaded via `npx playwright install` \
                   when tests next need them.",
            requires_tool: None,
        },
        // ---------------------------------------------------------------
        // Temp.
        // ---------------------------------------------------------------
        CleanupRule {
            id: "windows-temp",
            label: "User temp files",
            category: Category::Temp,
            tier: SafetyTier::Safe,
            locations: vec![PathSpec::new("%LOCALAPPDATA%/Temp")],
            pattern: None,
            regenerates: true,
            note: "Per-user temporary files. Generally safe to remove; running \
                   programs recreate anything they still need. Files locked by a \
                   running process are skipped automatically.",
            requires_tool: None,
        },
        // ---------------------------------------------------------------
        // Editor storage.
        // ---------------------------------------------------------------
        CleanupRule {
            id: "editor-state-vscdb",
            label: "Editor UI state database",
            category: Category::EditorStorage,
            tier: SafetyTier::Caution,
            locations: vec![
                // VS Code and common forks all keep a global `state.vscdb`.
                PathSpec::new("%APPDATA%/Code/User/globalStorage/state.vscdb"),
                PathSpec::new("%APPDATA%/Code/User/globalStorage/state.vscdb.backup"),
                PathSpec::new("%APPDATA%/Cursor/User/globalStorage/state.vscdb"),
                PathSpec::new("%APPDATA%/Cursor/User/globalStorage/state.vscdb.backup"),
                PathSpec::new("%APPDATA%/Windsurf/User/globalStorage/state.vscdb"),
                PathSpec::new("%APPDATA%/Windsurf/User/globalStorage/state.vscdb.backup"),
                PathSpec::new("%APPDATA%/Trae/User/globalStorage/state.vscdb"),
                PathSpec::new("%APPDATA%/Trae/User/globalStorage/state.vscdb.backup"),
                PathSpec::new("%APPDATA%/VSCodium/User/globalStorage/state.vscdb"),
                PathSpec::new("%APPDATA%/VSCodium/User/globalStorage/state.vscdb.backup"),
            ],
            pattern: None,
            regenerates: true,
            note: "A VS Code-family editor's global UI state store (recent \
                   files, view layout, some extension state). Removing it resets \
                   UI state but not your code. Contains user data, so it is \
                   never pre-selected.",
            requires_tool: None,
        },
        CleanupRule {
            id: "editor-workspace-storage",
            label: "Editor workspace storage",
            category: Category::EditorStorage,
            tier: SafetyTier::Review,
            locations: vec![
                PathSpec::new("%APPDATA%/Code/User/workspaceStorage"),
                PathSpec::new("%APPDATA%/Cursor/User/workspaceStorage"),
                PathSpec::new("%APPDATA%/Windsurf/User/workspaceStorage"),
                PathSpec::new("%APPDATA%/Trae/User/workspaceStorage"),
                PathSpec::new("%APPDATA%/VSCodium/User/workspaceStorage"),
            ],
            pattern: None,
            regenerates: true,
            note: "Per-workspace cache for VS Code-family editors (indexes, \
                   panel layout, some extension caches). Regenerates as you \
                   reopen projects; review before removing since it clears \
                   per-project UI state.",
            requires_tool: None,
        },
        CleanupRule {
            id: "jetbrains-caches",
            label: "JetBrains IDE caches",
            category: Category::EditorStorage,
            tier: SafetyTier::Review,
            locations: vec![PathSpec::new("%LOCALAPPDATA%/JetBrains")],
            pattern: None,
            regenerates: true,
            note: "Caches and indexes for JetBrains IDEs (IntelliJ, PyCharm, \
                   etc.). Regenerated on next launch; review before removing.",
            requires_tool: None,
        },
        // ---------------------------------------------------------------
        // Downloaded models.
        // ---------------------------------------------------------------
        CleanupRule {
            id: "lmstudio-models",
            label: "LM Studio models",
            category: Category::Model,
            tier: SafetyTier::Caution,
            locations: vec![
                PathSpec::new("%USERPROFILE%/.lmstudio"),
                PathSpec::new("%USERPROFILE%/.cache/lm-studio"),
            ],
            pattern: None,
            regenerates: false,
            note: "Local LLM weights downloaded via LM Studio. These are large \
                   but must be re-downloaded (often many GB) if removed. Treated \
                   as user data and never pre-selected.",
            requires_tool: None,
        },
        CleanupRule {
            id: "huggingface-cache",
            label: "Hugging Face model cache",
            category: Category::Model,
            tier: SafetyTier::Caution,
            locations: vec![
                PathSpec::new("%USERPROFILE%/.cache/huggingface"),
                PathSpec::new("%USERPROFILE%/.cache/torch"),
            ],
            pattern: None,
            regenerates: false,
            note: "Model weights/datasets cached by Hugging Face / PyTorch. \
                   Large, but must be re-downloaded if removed. Treated as user \
                   data and never pre-selected.",
            requires_tool: None,
        },
        // ---------------------------------------------------------------
        // Build artifacts discovered by pattern (via walk_pruned).
        // ---------------------------------------------------------------
        CleanupRule {
            id: "node-modules",
            label: "node_modules folders",
            category: Category::BuildArtifact,
            tier: SafetyTier::Review,
            locations: Vec::new(),
            pattern: Some(DirPattern {
                names: &["node_modules"],
            }),
            regenerates: true,
            note: "Installed JavaScript dependencies for a project. Regenerated \
                   by `npm install` / `bun install`. Review before removing — \
                   reinstalling requires network access and time.",
            requires_tool: None,
        },
        CleanupRule {
            id: "build-dirs",
            label: "Build output folders",
            category: Category::BuildArtifact,
            tier: SafetyTier::Review,
            locations: Vec::new(),
            pattern: Some(DirPattern {
                names: &[
                    "target", "build", ".next", "dist", ".dart_tool", ".turbo",
                    ".parcel-cache", ".svelte-kit", ".nuxt", "__pycache__",
                    ".pytest_cache", ".mypy_cache",
                ],
            }),
            regenerates: true,
            note: "Compiler/bundler output and tool caches (Rust `target`, \
                   `build`, Next.js `.next`, `dist`, Dart `.dart_tool`, Turbo, \
                   Parcel, SvelteKit, Nuxt, Python `__pycache__`/`.pytest_cache`/\
                   `.mypy_cache`). Regenerated by the next build/run. Review \
                   before removing.",
            requires_tool: None,
        },
    ]
}
