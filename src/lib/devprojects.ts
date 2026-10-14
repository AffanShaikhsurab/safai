// Pure helpers for dev-project artifacts (node_modules, target, .gradle…).
//
// The scan reports each artifact folder as its own item with a `dev` block
// naming its project. The UI works per *project*: "this app hasn't been
// touched in 40 days, its 3 folders free 2.1 GB". Everything here is plain
// data in, plain data out, so it is unit-tested in `devprojects.test.ts`.

import type { CleanupItem } from "./types";

/** Inactivity thresholds offered in the UI, in days. */
export const INACTIVE_DAY_CHOICES = [15, 30, 60, 90] as const;

export const DEFAULT_INACTIVE_DAYS = 30;

const DAY_SECS = 86_400;

export interface DevProject {
  root: string;
  name: string;
  kind: string;
  /** Distinct ecosystems among the project's artifacts. */
  ecosystems: string[];
  lastActiveSecs: number | null;
  /** Whole days since last activity; null when unknown. */
  idleDays: number | null;
  totalBytes: number;
  items: CleanupItem[];
}

export function isDevItem(item: CleanupItem): boolean {
  return !!item.dev;
}

/** Whole days between `lastActiveSecs` and `nowSecs`, or null if unknown. */
export function idleDays(lastActiveSecs: number | null, nowSecs: number): number | null {
  if (lastActiveSecs === null || lastActiveSecs === undefined) return null;
  return Math.max(0, Math.floor((nowSecs - lastActiveSecs) / DAY_SECS));
}

/**
 * Group dev items into projects, most inactive first (unknown activity last,
 * since those are never auto-selected), ties broken by size.
 */
export function groupProjects(items: CleanupItem[], nowSecs: number): DevProject[] {
  const byRoot = new Map<string, DevProject>();
  for (const item of items) {
    const dev = item.dev;
    if (!dev) continue;
    let p = byRoot.get(dev.projectRoot);
    if (!p) {
      p = {
        root: dev.projectRoot,
        name: dev.projectName,
        kind: dev.projectKind,
        ecosystems: [],
        lastActiveSecs: dev.lastActiveSecs ?? null,
        idleDays: idleDays(dev.lastActiveSecs ?? null, nowSecs),
        totalBytes: 0,
        items: [],
      };
      byRoot.set(dev.projectRoot, p);
    }
    p.items.push(item);
    p.totalBytes += item.sizeBytes;
    if (!p.ecosystems.includes(dev.ecosystem)) p.ecosystems.push(dev.ecosystem);
  }
  const projects = [...byRoot.values()];
  for (const p of projects) p.items.sort((a, b) => b.sizeBytes - a.sizeBytes);
  return projects.sort((a, b) => {
    const ai = a.idleDays ?? -1;
    const bi = b.idleDays ?? -1;
    if (ai !== bi) return bi - ai;
    return b.totalBytes - a.totalBytes;
  });
}

/** Has `project` been idle for at least `days`? Unknown activity never is. */
export function isInactive(project: DevProject, days: number): boolean {
  return project.idleDays !== null && project.idleDays >= days;
}

/**
 * Selection for every dev item under a threshold: items of inactive projects
 * on, everything else off. Non-dev items are not included.
 */
export function selectionForThreshold(
  projects: DevProject[],
  days: number,
): Record<string, boolean> {
  const out: Record<string, boolean> = {};
  for (const p of projects) {
    const on = isInactive(p, days);
    for (const item of p.items) out[item.id] = on;
  }
  return out;
}

/** "today", "1 day", "40 days", "3 months", "unknown". */
export function formatIdle(days: number | null): string {
  if (days === null) return "unknown";
  if (days === 0) return "today";
  if (days < 60) return `${days} day${days === 1 ? "" : "s"}`;
  const months = Math.floor(days / 30);
  if (months < 24) return `${months} months`;
  return `${Math.floor(days / 365)} years`;
}

/** Normalize a persisted threshold. */
export function normalizeInactiveDays(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.min(365, Math.max(7, Math.round(value)))
    : DEFAULT_INACTIVE_DAYS;
}
