// Persisted UI preferences (currently just the visual theme), stored via
// tauri-plugin-store. Best-effort: failures fall back to defaults so the UI
// never breaks.

import { load, type Store } from "@tauri-apps/plugin-store";

/** Background themes the user can pick in Settings → Appearance. */
export type ThemeName = "nebula" | "void";

export const DEFAULT_THEME: ThemeName = "nebula";

const FILE = "safai-prefs.json";
const KEY = "theme";
const KEY_DEEP = "deepScan";
const KEY_RECYCLE = "toRecycleBin";

let storePromise: Promise<Store> | null = null;
function getStore(): Promise<Store> {
  if (!storePromise) storePromise = load(FILE, { autoSave: false });
  return storePromise;
}

function normalizeTheme(value: unknown): ThemeName {
  // Legacy id from older builds.
  if (value === "cursor" || value === "void") return "void";
  if (value === "nebula") return "nebula";
  return DEFAULT_THEME;
}

/**
 * Apply the theme by toggling a class on <html>. Nebula is the default (no
 * class); "void" adds `theme-void` — charcoal dark, hairline dividers, dim comets.
 */
export function applyThemeClass(theme: ThemeName): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.classList.toggle("theme-void", theme === "void");
  // Drop legacy class if present.
  root.classList.remove("theme-cursor");
}

export async function loadTheme(): Promise<ThemeName> {
  try {
    const store = await getStore();
    const value = await store.get<string>(KEY);
    return normalizeTheme(value);
  } catch {
    return DEFAULT_THEME;
  }
}

export async function saveTheme(theme: ThemeName): Promise<void> {
  try {
    const store = await getStore();
    await store.set(KEY, theme);
    await store.save();
  } catch {
    // Non-fatal.
  }
}

/**
 * Store-level scan preferences (Settings → Scan preferences, and the Clean
 * setup panel). These are UI defaults only — they never change any backend
 * command signature. `deepScan` toggles the deep-scan affordance; `toRecycleBin`
 * is the default deletion destination (Recycle Bin vs Permanent).
 */
export interface ScanPrefs {
  deepScan: boolean;
  toRecycleBin: boolean;
}

export const DEFAULT_SCAN_PREFS: ScanPrefs = {
  deepScan: true,
  toRecycleBin: true,
};

export async function loadScanPrefs(): Promise<ScanPrefs> {
  try {
    const store = await getStore();
    const deep = await store.get<boolean>(KEY_DEEP);
    const recycle = await store.get<boolean>(KEY_RECYCLE);
    return {
      deepScan: typeof deep === "boolean" ? deep : DEFAULT_SCAN_PREFS.deepScan,
      toRecycleBin:
        typeof recycle === "boolean"
          ? recycle
          : DEFAULT_SCAN_PREFS.toRecycleBin,
    };
  } catch {
    return { ...DEFAULT_SCAN_PREFS };
  }
}

export async function saveScanPrefs(prefs: ScanPrefs): Promise<void> {
  try {
    const store = await getStore();
    await store.set(KEY_DEEP, prefs.deepScan);
    await store.set(KEY_RECYCLE, prefs.toRecycleBin);
    await store.save();
  } catch {
    // Non-fatal.
  }
}
