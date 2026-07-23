// Persists the most recent scan report locally via tauri-plugin-store, so the
// Dashboard's "Last scan breakdown" (and any other report-derived view)
// survives an app restart. Best-effort: failures fall back to no cached report
// so the UI never breaks. Uses its own file to keep the (potentially larger)
// report JSON separate from the small stats/prefs files.

import { load, type Store } from "@tauri-apps/plugin-store";
import type { ScanReport } from "./types";

const FILE = "safai-last-scan.json";
const KEY = "report";
const KEY_AT = "savedAt";

let storePromise: Promise<Store> | null = null;
function getStore(): Promise<Store> {
  if (!storePromise) storePromise = load(FILE, { autoSave: false });
  return storePromise;
}

/** The persisted report plus when it was saved (unix seconds). */
export interface CachedReport {
  report: ScanReport;
  savedAt: number | null;
}

/** Load the last saved scan report, or `null` if none/unavailable. */
export async function loadLastReport(): Promise<CachedReport | null> {
  try {
    const store = await getStore();
    const report = await store.get<ScanReport>(KEY);
    if (!report || !Array.isArray(report.groups)) return null;
    const savedAt = (await store.get<number>(KEY_AT)) ?? null;
    return { report, savedAt };
  } catch {
    return null;
  }
}

/** Save the latest scan report (or clear it when passed `null`). */
export async function saveLastReport(report: ScanReport | null): Promise<void> {
  try {
    const store = await getStore();
    if (report === null) {
      await store.delete(KEY);
      await store.delete(KEY_AT);
    } else {
      await store.set(KEY, report);
      await store.set(KEY_AT, Math.floor(Date.now() / 1000));
    }
    await store.save();
  } catch {
    // Non-fatal: persistence is a nice-to-have, not required for the app.
  }
}
