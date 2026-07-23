// Lightweight persistent "lifetime" stats for the Dashboard, stored via
// tauri-plugin-store (capability `store:default` is already granted). All
// access is best-effort: if the store is unavailable we fall back to defaults
// so the UI never breaks.

import { load, type Store } from "@tauri-apps/plugin-store";

export interface LifetimeStats {
  /** Total bytes reclaimed across every cleanup, ever. */
  lifetimeReclaimedBytes: number;
  /** How many cleanup runs the user has completed. */
  cleanupCount: number;
  /** Unix seconds of the most recent cleanup, or null if none yet. */
  lastCleanupAt: number | null;
  /** Reclaimable bytes found by the most recent scan. */
  lastScanReclaimable: number | null;
  /** Item count from the most recent scan. */
  lastScanItems: number | null;
  /** Unix seconds of the most recent scan. */
  lastScanAt: number | null;
}

export const DEFAULT_STATS: LifetimeStats = {
  lifetimeReclaimedBytes: 0,
  cleanupCount: 0,
  lastCleanupAt: null,
  lastScanReclaimable: null,
  lastScanItems: null,
  lastScanAt: null,
};

const FILE = "safai-stats.json";
const KEY = "stats";

let storePromise: Promise<Store> | null = null;
function getStore(): Promise<Store> {
  if (!storePromise) storePromise = load(FILE, { autoSave: false });
  return storePromise;
}

export async function loadStats(): Promise<LifetimeStats> {
  try {
    const store = await getStore();
    const value = await store.get<LifetimeStats>(KEY);
    return { ...DEFAULT_STATS, ...(value ?? {}) };
  } catch {
    return { ...DEFAULT_STATS };
  }
}

export async function saveStats(stats: LifetimeStats): Promise<void> {
  try {
    const store = await getStore();
    await store.set(KEY, stats);
    await store.save();
  } catch {
    // Non-fatal: persistence is a nice-to-have, not required for the app.
  }
}
