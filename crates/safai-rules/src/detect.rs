//! Environment path expansion, dev-tool detection, and default scan roots.
//!
//! Windows-first, but all OS-specific knowledge is funnelled through the env
//! variables (`LOCALAPPDATA`, `APPDATA`, `USERPROFILE`) so bringing up
//! macOS/Linux later is a config change, not a rewrite. We deliberately avoid
//! pulling in the `dirs` crate — `std::env` is enough for our needs.

use std::path::{Path, PathBuf};

/// Expand an env-templated path spec into a concrete, **existing** path.
///
/// Supported tokens (Windows style, case-insensitive on the token name):
///   * `%LOCALAPPDATA%` → env `LOCALAPPDATA`
///   * `%APPDATA%`      → env `APPDATA`
///   * `%USERPROFILE%`  → env `USERPROFILE`
///   * a leading `~`    → env `USERPROFILE`
///
/// Returns `None` when the referenced variable is missing/empty, when the spec
/// references an unknown `%VAR%`, or when the resolved path does not exist on
/// disk. The remainder of the spec (after the token) is appended, with both
/// `/` and `\` accepted as separators.
pub fn expand(spec: &str) -> Option<PathBuf> {
    let expanded = expand_str(spec)?;
    let path = PathBuf::from(expanded);
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

/// Textual expansion without the existence check (kept separate so tests can
/// assert the templating logic independently of the filesystem).
fn expand_str(spec: &str) -> Option<String> {
    // Leading `~` shorthand for the user profile.
    if let Some(rest) = spec.strip_prefix('~') {
        let home = env_nonempty("USERPROFILE")?;
        return Some(join_rest(&home, rest));
    }

    // `%VAR%\rest` form.
    if let Some(after) = spec.strip_prefix('%') {
        if let Some(end) = after.find('%') {
            let var = &after[..end];
            let rest = &after[end + 1..];
            let base = match var.to_ascii_uppercase().as_str() {
                "LOCALAPPDATA" => env_nonempty("LOCALAPPDATA")?,
                "APPDATA" => env_nonempty("APPDATA")?,
                "USERPROFILE" => env_nonempty("USERPROFILE")?,
                _ => return None, // unknown variable
            };
            return Some(join_rest(&base, rest));
        }
        // A lone unterminated `%` — not a valid spec.
        return None;
    }

    // No token: return the spec verbatim.
    Some(spec.to_string())
}

/// Join `base` with the remainder of a spec, tolerating a leading `/` or `\`
/// separator and normalizing inner separators to the platform separator.
fn join_rest(base: &str, rest: &str) -> String {
    let is_sep = |c: char| c == '/' || c == '\\';
    let trimmed = rest.trim_start_matches(is_sep);
    if trimmed.is_empty() {
        return base.to_string();
    }
    let mut path = PathBuf::from(base);
    // Split on both separators so specs can use `/` uniformly.
    for segment in trimmed.split(is_sep) {
        if !segment.is_empty() {
            path.push(segment);
        }
    }
    path.to_string_lossy().into_owned()
}

fn env_nonempty(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

// -------------------------------------------------------------------------
// Tool detection
// -------------------------------------------------------------------------

/// Return `true` when an executable named `exe` (or `exe.exe` / `exe.cmd`) is
/// found on any directory listed in the `PATH` environment variable.
pub fn tool_on_path(exe: &str) -> bool {
    let path_var = match std::env::var("PATH") {
        Ok(v) => v,
        Err(_) => return false,
    };

    // Candidate file names to probe for. On Windows the launcher is usually a
    // `.exe` or (for JS tooling) a `.cmd` shim; the bare name covers POSIX.
    let candidates = [exe.to_string(), format!("{exe}.exe"), format!("{exe}.cmd")];

    for dir in std::env::split_paths(&path_var) {
        for cand in &candidates {
            let full = dir.join(cand);
            if full.is_file() {
                return true;
            }
        }
    }
    false
}

/// Detect the dev tools Safai cares about for rule gating and UI chips.
///
/// Returns `(id, label, detected)` triples. The `id` matches the
/// `requires_tool` values used by the rule table in `rules.rs`.
pub fn detect_tools() -> Vec<(String, String, bool)> {
    // (id, label, list of executables that satisfy the tool).
    let specs: &[(&str, &str, &[&str])] = &[
        ("uv", "uv (Python)", &["uv"]),
        ("npm", "npm", &["npm"]),
        ("pnpm", "pnpm", &["pnpm"]),
        ("yarn", "Yarn", &["yarn"]),
        ("node", "Node.js", &["node"]),
        ("bun", "Bun", &["bun"]),
        ("gradle", "Gradle", &["gradle"]),
        ("cargo", "Cargo (Rust)", &["cargo"]),
        ("go", "Go", &["go"]),
        ("python", "Python", &["python", "python3"]),
        ("pip", "pip", &["pip", "pip3"]),
        ("dart", "Dart / Flutter", &["dart", "flutter"]),
        ("git", "Git", &["git"]),
    ];

    specs
        .iter()
        .map(|(id, label, exes)| {
            let detected = exes.iter().any(|e| tool_on_path(e));
            (id.to_string(), label.to_string(), detected)
        })
        .collect()
}

// -------------------------------------------------------------------------
// Default scan roots
// -------------------------------------------------------------------------

/// Suggested scan roots: the user profile plus any known cache parents that
/// currently exist on disk. Only existing directories are returned so callers
/// never have to filter.
pub fn default_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();

    // The user profile is the primary root.
    if let Some(home) = env_nonempty("USERPROFILE") {
        let home = PathBuf::from(home);
        if home.exists() {
            roots.push(home);
        }
    }

    // Known cache parents worth surfacing directly.
    let cache_parents = ["%LOCALAPPDATA%", "%APPDATA%"];
    for spec in cache_parents {
        if let Some(p) = expand(spec) {
            if !roots.iter().any(|existing| existing == &p) {
                roots.push(p);
            }
        }
    }

    roots
}

/// Targeted roots for the pattern-rule discovery walk (hunting `node_modules`,
/// `target`, build dirs, …).
///
/// This is deliberately **not** the whole user profile + `AppData`: a recursive
/// walk of `AppData` (npm/pip/browser caches, Electron app storage) is enormous
/// and virtually never contains the project artifacts we want, which is what
/// made scans stall. Instead we return the handful of common places developers
/// keep code — only those that actually exist on disk, so callers never filter.
pub fn project_scan_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();

    if let Some(home) = env_nonempty("USERPROFILE") {
        let home = PathBuf::from(home);
        // Common dev/project folder names directly under the user profile.
        // Windows paths are case-insensitive, so a single spelling suffices.
        const DEV_DIR_NAMES: &[&str] = &[
            "Desktop",
            "Documents",
            "Downloads",
            "source",
            "src",
            "code",
            "dev",
            "projects",
            "repos",
            "work",
            "git",
            "GitHub",
            "Development",
            "workspace",
        ];
        for name in DEV_DIR_NAMES {
            let p = home.join(name);
            if p.exists() && !roots.iter().any(|e| e == &p) {
                roots.push(p);
            }
        }
        // GitHub Desktop's default clone location.
        let gh = home.join("Documents").join("GitHub");
        if gh.exists() && !roots.iter().any(|e| e == &gh) {
            roots.push(gh);
        }
    }

    roots
}

/// Normalize a path for display: forward slashes, per §7.
pub fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_resolves_set_var() {
        // Set a variable and confirm the token resolves against it.
        std::env::set_var("USERPROFILE", "C:\\Users\\tester");
        // Use the internal textual expander to avoid depending on the path
        // actually existing on disk.
        let out = expand_str("%USERPROFILE%/.gradle/caches").expect("expands");
        let normalized = out.replace('\\', "/");
        assert!(
            normalized.ends_with("tester/.gradle/caches"),
            "unexpected expansion: {normalized}"
        );
    }

    #[test]
    fn expand_tilde_uses_userprofile() {
        std::env::set_var("USERPROFILE", "C:\\Users\\tilde");
        let out = expand_str("~/.bun/install/cache").expect("expands");
        let normalized = out.replace('\\', "/");
        assert!(
            normalized.ends_with("tilde/.bun/install/cache"),
            "unexpected expansion: {normalized}"
        );
    }

    #[test]
    fn expand_missing_var_returns_none() {
        std::env::remove_var("SAFAI_NOPE");
        // Unknown variable token yields None from the textual expander.
        assert!(expand_str("%SAFAI_NOPE%/x").is_none());
    }

    #[test]
    fn expand_nonexistent_path_returns_none() {
        std::env::set_var("USERPROFILE", "C:\\Users\\definitely-missing-xyz");
        // The full `expand` includes an existence check, so a bogus path is
        // rejected even though the variable is set.
        assert!(expand("%USERPROFILE%/this/does/not/exist/xyzzy").is_none());
    }
}
