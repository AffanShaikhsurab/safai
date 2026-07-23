// Typed wrappers over the Tauri command API (implementation-plan.md §4).
// Screens/components never touch raw command names or arg shapes — they go
// through these helpers. All argument keys are camelCase to match the Rust
// commands' serde `rename_all = "camelCase"`.

import { invoke, Channel } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  DeleteEvent,
  DeletePlan,
  DeleteReport,
  DriveInfo,
  ScanEvent,
  ScanReport,
  ToolInfo,
} from "./types";

/**
 * Run a rules-based scan. Streams `ScanEvent`s to `onEvent` while running and
 * resolves with the full `ScanReport` when the backend finishes.
 */
export function scan(
  roots: string[],
  onEvent: (event: ScanEvent) => void,
): Promise<ScanReport> {
  const channel = new Channel<ScanEvent>();
  channel.onmessage = onEvent;
  return invoke<ScanReport>("scan", { roots, onEvent: channel });
}

/** Flip the backend cancel flag for an in-flight scan. */
export function cancelScan(): Promise<void> {
  return invoke<void>("cancel_scan");
}

/** Dry-run: validate + size the selected items before any deletion. */
export function previewDelete(ids: string[]): Promise<DeletePlan> {
  return invoke<DeletePlan>("preview_delete", { ids });
}

/**
 * Delete the selected items (Recycle Bin by default). Streams `DeleteEvent`s to
 * `onEvent` and resolves with a `DeleteReport`.
 */
export function deleteItems(
  ids: string[],
  toRecycleBin: boolean,
  onEvent: (event: DeleteEvent) => void,
): Promise<DeleteReport> {
  const channel = new Channel<DeleteEvent>();
  channel.onmessage = onEvent;
  return invoke<DeleteReport>("delete", { ids, toRecycleBin, onEvent: channel });
}

/** Reveal a path in the system file explorer. */
export function openPath(path: string): Promise<void> {
  return invoke<void>("open_path", { path });
}

/** Which dev tools are installed (UI chips + rule gating). */
export function detectTools(): Promise<ToolInfo[]> {
  return invoke<ToolInfo[]>("detect_tools");
}

/** Suggested scan roots (home, known cache dirs). */
export function defaultRoots(): Promise<string[]> {
  return invoke<string[]>("default_roots");
}

/** Free/total space for the header gauge. */
export function driveInfo(path: string): Promise<DriveInfo> {
  return invoke<DriveInfo>("drive_info", { path });
}

/**
 * Open a native folder picker. Returns the chosen path, or `null` if the user
 * cancelled. With `directory: true` + `multiple: false`, the dialog resolves to
 * a single path string or null.
 */
export async function pickFolder(): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}
