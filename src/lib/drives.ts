// Pure drive/path helpers.
//
// No Tauri imports: this is unit-tested in node, and every screen derives its
// per-drive arithmetic from here rather than parsing `C:/...` strings in five
// components.

import type { DriveEntry } from "./types";

/** The mount a display path lives on (`"C:"`), or null for UNC/relative paths. */
export function mountOfPath(path: string): string | null {
  const match = /^([A-Za-z]):(?:[\\/]|$)/.exec(path.trim());
  return match ? `${match[1].toUpperCase()}:` : null;
}

/** Normalize `"c:"`, `"C:/"`, `"C:\\"` to `"C:"`; null for anything else. */
export function normalizeMount(mount: string): string | null {
  const match = /^([A-Za-z]):[\\/]?$/.exec(mount.trim());
  return match ? `${match[1].toUpperCase()}:` : null;
}

/** Root display path for a mount (`"C:"` → `"C:/"`). */
export function driveRoot(mount: string): string {
  const normalized = normalizeMount(mount);
  return normalized ? `${normalized.slice(0, 2)}/` : mount;
}

export function isOnDrive(path: string, mount: string): boolean {
  const normalized = normalizeMount(mount);
  return normalized !== null && mountOfPath(path) === normalized;
}

export interface DriveTotal<T> {
  mount: string | null;
  bytes: number;
  count: number;
  items: T[];
}

/** Sum and group items by the drive their path lives on. */
export function perDriveTotals<T extends { path: string; sizeBytes: number }>(
  items: T[],
): DriveTotal<T>[] {
  const byMount = new Map<string | null, DriveTotal<T>>();
  for (const item of items) {
    const mount = mountOfPath(item.path);
    let total = byMount.get(mount);
    if (!total) {
      total = { mount, bytes: 0, count: 0, items: [] };
      byMount.set(mount, total);
    }
    total.bytes += item.sizeBytes;
    total.count += 1;
    total.items.push(item);
  }
  return [...byMount.values()].sort((a, b) => b.bytes - a.bytes);
}

/** Filter by drive. `null` means every drive. */
export function filterByDrive<T extends { path: string }>(
  items: T[],
  mount: string | null,
): T[] {
  if (!mount) return items;
  return items.filter((item) => mountOfPath(item.path) === mount);
}

export interface DriveAggregate {
  freeBytes: number;
  totalBytes: number;
  usedPercent: number;
  count: number;
}

/** Aggregate free/total/used across a set of drives. */
export function aggregateDrives(drives: DriveEntry[]): DriveAggregate {
  let freeBytes = 0;
  let totalBytes = 0;
  let count = 0;
  for (const drive of drives) {
    if (!drive.readable || drive.totalBytes <= 0) continue;
    freeBytes += drive.freeBytes;
    totalBytes += drive.totalBytes;
    count += 1;
  }
  return {
    freeBytes,
    totalBytes,
    usedPercent:
      totalBytes > 0
        ? Math.min(100, Math.round(((totalBytes - freeBytes) / totalBytes) * 100))
        : 0,
    count,
  };
}

/** Persisted scan scope. Kept separate from the shorter scan preferences. */
export interface ScopePrefs {
  drives: string[];
  allDrives: boolean;
  folders: string[];
}

export interface ReconciledScope {
  selectedDrives: string[];
  folders: string[];
  allDrives: boolean;
  /** Previously selected drives that are not present right now. */
  missing: string[];
  /** Drives that appeared since an "all drives" selection was saved. */
  added: string[];
}

/**
 * Reconcile the saved scope with the drives present right now.
 *
 * - No saved scope → select every readable fixed drive ("all drives").
 * - `allDrives` → select every present drive; newly appeared drives are
 *   reported in `added`, so the UI can mention them once.
 * - An explicit selection → keep only present drives and fall back to the
 *   system drive (or the first drive) if every saved drive disappeared.
 *
 * Folders are preserved verbatim; the backend validates existence on scan.
 */
export function reconcileScope(
  drives: DriveEntry[],
  saved: ScopePrefs | null,
): ReconciledScope {
  const present = drives
    .filter((drive) => drive.readable && drive.kind === "fixed")
    .map((drive) => drive.mount);
  const system =
    drives.find((drive) => drive.system && drive.readable)?.mount ??
    present[0] ??
    null;

  if (!saved) {
    return {
      selectedDrives: present,
      folders: [],
      allDrives: true,
      missing: [],
      added: [],
    };
  }

  if (saved.allDrives) {
    return {
      selectedDrives: present,
      folders: saved.folders ?? [],
      allDrives: true,
      missing: [],
      added: present.filter((mount) => !saved.drives.includes(mount)),
    };
  }

  const selected = saved.drives.filter((mount) => present.includes(mount));
  const missing = saved.drives.filter((mount) => !present.includes(mount));
  if (selected.length === 0 && system) selected.push(system);

  return {
    selectedDrives: selected,
    folders: saved.folders ?? [],
    allDrives: false,
    missing,
    added: [],
  };
}
