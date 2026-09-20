//! Safai headless CLI — agent-facing scan/cleanup without the desktop UI.

mod session;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use clap::{Parser, Subcommand};
use safai_engine::{
    build_scan_config, delete_blocking, disk, drive_mount, preview_delete, scan_blocking, DriveInfo,
};
use safai_rules::{CleanupItem, ScanEvent};
use serde::Serialize;

use session::{
    default_session_path, load_session, save_preview_token, save_session, verify_token, ScanSession,
};

#[derive(Parser, Debug)]
#[command(
    name = "safai",
    version,
    about = "Safai — headless disk cleanup for agents (scan, preview, delete)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Print default scan roots (JSON).
    Roots,

    /// Detect installed developer tools (JSON).
    #[command(name = "detect-tools")]
    DetectTools,

    /// Free/total space for a drive (JSON).
    #[command(name = "drive-info")]
    DriveInfo {
        /// Path on the volume to query (default: first default root or C:\).
        #[arg(long)]
        mount: Option<String>,
    },

    /// Scan for reclaimable developer junk (JSON report; persists session).
    Scan {
        /// Scan root (repeatable). Empty = Safai defaults.
        #[arg(long = "root")]
        roots: Vec<String>,

        /// Quick mode: skip large-folder discovery (default is deep).
        #[arg(long)]
        quick: bool,

        /// Write the session file here (default: %LOCALAPPDATA%/safai/last-scan.json).
        #[arg(long)]
        out: Option<PathBuf>,

        /// Print progress lines on stderr.
        #[arg(long)]
        progress: bool,
    },

    /// Dry-run a deletion plan for item ids from the last scan (JSON + confirm token).
    Preview {
        /// Comma-separated item ids from the last scan.
        #[arg(long, value_delimiter = ',')]
        ids: Vec<String>,

        /// Session file (default: last scan).
        #[arg(long)]
        session: Option<PathBuf>,
    },

    /// Delete items from the last scan. Requires --token from preview and --yes.
    Delete {
        /// Comma-separated item ids (must match the preview token).
        #[arg(long, value_delimiter = ',')]
        ids: Vec<String>,

        /// Confirm token returned by `safai preview`.
        #[arg(long)]
        token: String,

        /// Required. Only pass after the user explicitly approved the preview.
        #[arg(long)]
        yes: bool,

        /// Permanently delete instead of Recycle Bin (default: Recycle Bin).
        #[arg(long)]
        permanent: bool,

        /// Session file (default: last scan).
        #[arg(long)]
        session: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let payload = ErrorOut {
                error: err.message,
                code: err.code,
            };
            // Always emit JSON errors on stdout so agents can parse failures.
            if let Ok(json) = serde_json::to_string_pretty(&payload) {
                println!("{json}");
            } else {
                eprintln!("{}", payload.error);
            }
            ExitCode::from(err.exit)
        }
    }
}

struct CliError {
    message: String,
    code: String,
    exit: u8,
}

impl CliError {
    fn new(code: &str, message: impl Into<String>, exit: u8) -> Self {
        Self {
            message: message.into(),
            code: code.to_string(),
            exit,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorOut {
    error: String,
    code: String,
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Commands::Roots => {
            let roots: Vec<String> = safai_rules::default_roots()
                .into_iter()
                .map(|p| safai_engine::normalize_slashes(&p.to_string_lossy()))
                .collect();
            print_json(&roots)?;
        }
        Commands::DetectTools => {
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Tool {
                id: String,
                label: String,
                detected: bool,
            }
            let tools: Vec<Tool> = safai_rules::detect_tools()
                .into_iter()
                .map(|(id, label, detected)| Tool {
                    id,
                    label,
                    detected,
                })
                .collect();
            print_json(&tools)?;
        }
        Commands::DriveInfo { mount } => {
            let path = resolve_drive_path(mount.as_deref())?;
            let label = drive_mount(&path);
            let info = disk::drive_info_for(&path, label).unwrap_or(DriveInfo {
                mount: drive_mount(&path),
                free_bytes: 0,
                total_bytes: 0,
            });
            print_json(&info)?;
        }
        Commands::Scan {
            roots,
            quick,
            out,
            progress,
        } => {
            let discover = !quick;
            let cfg = build_scan_config(&roots, discover);
            let cancel = AtomicBool::new(false);
            let progress_flag = progress;
            let sink = move |ev: ScanEvent| {
                if !progress_flag {
                    return;
                }
                if let ScanEvent::Progress {
                    current_path,
                    found_bytes,
                    rules_checked,
                    rules_total,
                } = ev
                {
                    eprintln!(
                        "progress rules={rules_checked}/{rules_total} found={found_bytes} path={current_path}"
                    );
                }
            };
            let report = scan_blocking(&cfg, &cancel, &sink);

            let session_path = out.unwrap_or_else(default_session_path);
            let session = ScanSession::from_report(report.clone(), &cfg.roots);
            save_session(&session_path, &session).map_err(|e| {
                CliError::new("session_write", format!("failed to write session: {e}"), 2)
            })?;

            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct ScanOut {
                report: safai_rules::ScanReport,
                session_path: String,
            }
            print_json(&ScanOut {
                report,
                session_path: safai_engine::normalize_slashes(&session_path.to_string_lossy()),
            })?;
        }
        Commands::Preview { ids, session } => {
            if ids.is_empty() {
                return Err(CliError::new(
                    "missing_ids",
                    "pass --ids with at least one item id from the last scan",
                    2,
                ));
            }
            let path = session.unwrap_or_else(default_session_path);
            let sess = load_session(&path).map_err(|e| {
                CliError::new(
                    "session_missing",
                    format!("no scan session at {}: {e}", path.display()),
                    2,
                )
            })?;
            let plan = preview_delete(&ids, &sess.items, &sess.allowed_roots);
            let token = save_preview_token(&sess, &ids, &plan);
            // Persist token into session file for delete verification.
            let mut updated = sess;
            updated.pending_token = Some(token.clone());
            updated.pending_ids = ids.clone();
            save_session(&path, &updated).map_err(|e| {
                CliError::new("session_write", format!("failed to update session: {e}"), 2)
            })?;

            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct PreviewOut {
                plan: safai_engine::DeletePlan,
                confirm_token: String,
                session_path: String,
            }
            print_json(&PreviewOut {
                plan,
                confirm_token: token,
                session_path: safai_engine::normalize_slashes(&path.to_string_lossy()),
            })?;
        }
        Commands::Delete {
            ids,
            token,
            yes,
            permanent,
            session,
        } => {
            if !yes {
                return Err(CliError::new(
                    "confirmation_required",
                    "refusing to delete without --yes (ask the user first, then pass --yes)",
                    3,
                ));
            }
            if ids.is_empty() {
                return Err(CliError::new(
                    "missing_ids",
                    "pass --ids with the same ids used in preview",
                    2,
                ));
            }
            let path = session.unwrap_or_else(default_session_path);
            let mut sess = load_session(&path).map_err(|e| {
                CliError::new(
                    "session_missing",
                    format!("no scan session at {}: {e}", path.display()),
                    2,
                )
            })?;
            if !verify_token(&sess, &ids, &token) {
                return Err(CliError::new(
                    "invalid_token",
                    "confirm token is missing, expired, or does not match these ids — run `safai preview` again",
                    3,
                ));
            }

            let resolved: Vec<(String, Option<CleanupItem>)> = ids
                .iter()
                .map(|id| (id.clone(), sess.items.get(id).cloned()))
                .collect();
            let cancel = AtomicBool::new(false);
            let sink = |_ev| {};
            let to_recycle_bin = !permanent;
            let report = delete_blocking(
                resolved,
                &sess.allowed_roots,
                to_recycle_bin,
                &cancel,
                &sink,
            );

            // Invalidate the one-shot token after use.
            sess.pending_token = None;
            sess.pending_ids.clear();
            let _ = save_session(&path, &sess);

            print_json(&report)?;
        }
    }
    Ok(())
}

fn print_json<T: Serialize>(value: &T) -> Result<(), CliError> {
    match serde_json::to_string_pretty(value) {
        Ok(s) => {
            println!("{s}");
            Ok(())
        }
        Err(e) => Err(CliError::new(
            "serialize",
            format!("failed to serialize JSON: {e}"),
            2,
        )),
    }
}

fn resolve_drive_path(mount: Option<&str>) -> Result<PathBuf, CliError> {
    if let Some(m) = mount {
        return Ok(PathBuf::from(m));
    }
    if let Some(root) = safai_rules::default_roots().into_iter().next() {
        return Ok(root);
    }
    Ok(PathBuf::from(r"C:\"))
}
