// Global app model. A single Tauri window drives a linear flow
// (welcome -> scanning -> results -> cleaning -> done), so one shared store is
// simpler than context. It's created inside `createRoot` so the `createMemo`
// selectors have a stable owner and don't leak.

import { createEffect, createMemo, createRoot, on } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { listen } from "@tauri-apps/api/event";
import type {
  AutomationProgress,
  AutomationStatus,
  Category,
  CleanupItem,
  DeleteEvent,
  DriveInfo,
  RuleInfo,
  SafetyTier,
  ScanEvent,
  ScanReport,
  ScheduleConfig,
  ToolInfo,
} from "../lib/types";
import { DEFAULT_STATS, saveStats, type LifetimeStats } from "../lib/stats";
import { formatBytes } from "../lib/format";
import { notify } from "../lib/notify";
import {
  automationStatus,
  cleanupRules,
  deleteItems,
  driveInfo,
  runAutomationNow,
  scan,
  setAutomationConfig,
  setUiEngaged,
  stopAutomation,
} from "../lib/tauri";
import { loadLastReport, saveLastReport } from "../lib/report";
import { isUiEngaged, shouldParkAtResults } from "../lib/automation";
import {
  applyThemeClass,
  DEFAULT_SCAN_PREFS,
  DEFAULT_SKY,
  DEFAULT_THEME,
  saveScanPrefs,
  saveSkyPrefs,
  saveTheme,
  type ScanPrefs,
  type SkyPrefs,
  type ThemeName,
} from "../lib/prefs";

export type Phase = "welcome" | "scanning" | "results" | "cleaning" | "done";

/** Top-level navigation sections (sidebar). */
export type View = "dashboard" | "clean" | "automation" | "settings";



/** A single item that couldn't be removed, with the reason why. */
export interface SkippedItem {
  id: string;
  path: string;
  reason: string;
}

export interface Progress {
  // Scan progress.
  currentPath: string;
  foundBytes: number;
  rulesChecked: number;
  rulesTotal: number;
  itemCount: number;
  // Delete progress.
  deleteTotal: number;
  deleted: number;
  reclaimedBytes: number;
  skipped: number;
  /** Details of every skipped item (path + reason) for the results view. */
  skippedItems: SkippedItem[];
}

export interface AppState {
  view: View;
  phase: Phase;
  roots: string[];
  tools: ToolInfo[];
  report: ScanReport | null;
  selected: Record<string, boolean>;
  progress: Progress;
  driveBefore: DriveInfo | null;
  driveAfter: DriveInfo | null;
  stats: LifetimeStats;
  theme: ThemeName;
  /** Night-sky tuning. Only meaningful for the `nebula` and `void` themes. */
  sky: SkyPrefs;
  /** UI scan preferences (persisted; never alters backend contracts). */
  deepScan: boolean;
  /** Default deletion destination: true = Recycle Bin, false = Permanent. */
  toRecycleBin: boolean;
  /**
   * Epoch ms when the current scan began (0 when idle). Lives in the store so
   * the Scanning screen's elapsed timer survives navigation — switching to
   * Dashboard and back must not reset it to 0.
   */
  scanStartedAt: number;
  /**
   * Backend automation status. `null` until the first hydrate; the Automation
   * screen renders a loading state rather than guessing at defaults, since the
   * real config lives in Rust.
   */
  automation: AutomationStatus | null;
  /** The cleanup rule table, for the autopilot allow-list. Lazy-loaded. */
  rules: RuleInfo[];
}

function emptyProgress(): Progress {
  return {
    currentPath: "",
    foundBytes: 0,
    rulesChecked: 0,
    rulesTotal: 0,
    itemCount: 0,
    deleteTotal: 0,
    deleted: 0,
    reclaimedBytes: 0,
    skipped: 0,
    skippedItems: [],
  };
}

function createAppStore() {
  const [state, setState] = createStore<AppState>({
    view: "dashboard",
    phase: "welcome",
    roots: [],
    tools: [],
    report: null,
    selected: {},
    progress: emptyProgress(),
    driveBefore: null,
    driveAfter: null,
    stats: { ...DEFAULT_STATS },
    theme: DEFAULT_THEME,
    sky: { ...DEFAULT_SKY },
    deepScan: DEFAULT_SCAN_PREFS.deepScan,
    toRecycleBin: DEFAULT_SCAN_PREFS.toRecycleBin,
    scanStartedAt: 0,
    automation: null,
    rules: [],
  });

  // Keep the backend's view of "the user is busy" in sync with the flow, so the
  // scheduler defers instead of invalidating a live selection.
  // Best-effort: a failed IPC call here must not break navigation.
  createEffect(
    on(
      () => isUiEngaged(state.phase, state.view),
      (engaged) => void setUiEngaged(engaged).catch(() => {}),
    ),
  );

  // ---- Derived index of every item across all groups (for byte math). ----
  const allItems = createMemo<CleanupItem[]>(() => {
    const report = state.report;
    if (!report) return [];
    return report.groups.flatMap((g) => g.items);
  });

  // ---- Derived selectors ----
  const selectedIds = createMemo<string[]>(() =>
    allItems()
      .filter((item) => state.selected[item.id])
      .map((item) => item.id),
  );

  const selectedCount = createMemo<number>(() => selectedIds().length);

  const reclaimableSelectedBytes = createMemo<number>(() =>
    allItems().reduce(
      (sum, item) => sum + (state.selected[item.id] ? item.sizeBytes : 0),
      0,
    ),
  );

  // ---- Actions ----
  function setPhase(phase: Phase) {
    setState("phase", phase);
  }

  function setRoots(roots: string[]) {
    setState("roots", roots);
  }

  function addRoot(root: string) {
    if (!state.roots.includes(root)) {
      setState("roots", state.roots.length, root);
    }
  }

  function removeRoot(root: string) {
    setState(
      "roots",
      state.roots.filter((r) => r !== root),
    );
  }

  function setTools(tools: ToolInfo[]) {
    setState("tools", tools);
  }

  function setDriveBefore(info: DriveInfo) {
    setState("driveBefore", info);
  }

  function setDriveAfter(info: DriveInfo) {
    setState("driveAfter", info);
  }

  // ---- Navigation + persistent lifetime stats ----
  function setView(view: View) {
    setState("view", view);
  }

  function setStats(stats: LifetimeStats) {
    setState("stats", stats);
  }

  /** Switch the visual theme: apply the <html> class + persist the choice. */
  function setTheme(theme: ThemeName) {
    applyThemeClass(theme);
    setState("theme", theme);
    void saveTheme(theme);
  }

  /** Hydrate sky settings from persisted storage (no persist write-back). */
  function setSkyPrefs(sky: SkyPrefs) {
    setState("sky", sky);
  }

  /**
   * Patch one or more sky settings (persisted). `PixelSky` tracks `state.sky`,
   * so the canvas rebuilds as soon as this lands — no explicit repaint call.
   */
  function patchSky(patch: Partial<SkyPrefs>) {
    const next: SkyPrefs = { ...state.sky, ...patch };
    setState("sky", next);
    void saveSkyPrefs(next);
  }

  /** Hydrate scan preferences from persisted storage (no persist write-back). */
  function setScanPrefs(prefs: ScanPrefs) {
    setState(
      produce((s) => {
        s.deepScan = prefs.deepScan;
        s.toRecycleBin = prefs.toRecycleBin;
      }),
    );
  }

  /** Toggle the deep-scan preference (persisted). */
  function setDeepScan(on: boolean) {
    setState("deepScan", on);
    void saveScanPrefs({ deepScan: on, toRecycleBin: state.toRecycleBin });
  }

  /** Set the default deletion destination (persisted). */
  function setDestination(toRecycleBin: boolean) {
    setState("toRecycleBin", toRecycleBin);
    void saveScanPrefs({ deepScan: state.deepScan, toRecycleBin });
  }

  /** Record the most recent scan's headline numbers (persisted). */
  function recordScan(reclaimable: number, itemCount: number) {
    const next: LifetimeStats = {
      ...state.stats,
      lastScanReclaimable: reclaimable,
      lastScanItems: itemCount,
      lastScanAt: Math.floor(Date.now() / 1000),
    };
    setState("stats", next);
    void saveStats(next);
  }

  /** Add a completed cleanup to the lifetime totals (persisted). */
  function recordCleanup(reclaimedBytes: number) {
    const prev = state.stats;
    const next: LifetimeStats = {
      ...prev,
      lifetimeReclaimedBytes: prev.lifetimeReclaimedBytes + reclaimedBytes,
      cleanupCount: prev.cleanupCount + 1,
      lastCleanupAt: Math.floor(Date.now() / 1000),
    };
    setState("stats", next);
    void saveStats(next);
  }

  /** Reset progress + report state before starting a new scan. */
  function beginScan(roots: string[]) {
    setState(
      produce((s) => {
        s.roots = roots;
        s.report = null;
        s.selected = {};
        s.progress = emptyProgress();
        s.driveAfter = null;
        s.scanStartedAt = Date.now();
        s.phase = "scanning";
      }),
    );
  }

  /** Consume a streamed scan event; keeps the scanning screen live. */
  function applyScanEvent(event: ScanEvent) {
    switch (event.event) {
      case "started":
        setState("roots", event.data.roots);
        break;
      case "progress":
        setState(
          produce((s) => {
            s.progress.currentPath = event.data.currentPath;
            s.progress.foundBytes = event.data.foundBytes;
            s.progress.rulesChecked = event.data.rulesChecked;
            s.progress.rulesTotal = event.data.rulesTotal;
          }),
        );
        break;
      case "found":
        setState(
          produce((s) => {
            const item = event.data.item;
            s.selected[item.id] = item.selectedByDefault;
            s.progress.itemCount += 1;
          }),
        );
        break;
      case "finished":
        setState(
          produce((s) => {
            s.progress.foundBytes = event.data.totalReclaimableBytes;
            s.progress.itemCount = event.data.itemCount;
          }),
        );
        break;
    }
  }

  /**
   * Store the final report and initialize per-item selection from
   * `selectedByDefault` (preserving any default already set from a `found`
   * event). Moves the app to the results phase.
   */
  function setReport(report: ScanReport) {
    // Sort largest-first so the biggest wins are easiest to find: categories
    // by total size, and items within each category by their own size.
    for (const group of report.groups) {
      group.items.sort((a, b) => b.sizeBytes - a.sizeBytes);
    }
    report.groups.sort((a, b) => b.totalBytes - a.totalBytes);

    setState(
      produce((s) => {
        s.report = report;
        for (const group of report.groups) {
          for (const item of group.items) {
            if (!(item.id in s.selected)) {
              s.selected[item.id] = item.selectedByDefault;
            }
          }
        }
        s.phase = "results";
      }),
    );
    const itemCount = report.groups.reduce((n, g) => n + g.items.length, 0);
    recordScan(report.totalReclaimableBytes, itemCount);
    // Persist the report locally so the Dashboard breakdown survives a restart.
    void saveLastReport(report);
    void notify(
      "Scan complete",
      itemCount > 0
        ? `Found ${formatBytes(report.totalReclaimableBytes)} of reclaimable space across ${itemCount} item${itemCount === 1 ? "" : "s"}.`
        : "No reclaimable space found — your disk is tidy.",
    );
  }

  /**
   * Restore a persisted scan report on launch (Dashboard breakdown, etc.).
   * Unlike `setReport`, this does NOT change the phase, record stats, or fire a
   * notification — it only rehydrates `state.report` and per-item selection.
   * Skipped if a fresh report is already present (e.g. a scan finished first).
   */
  async function hydrateLastReport() {
    if (state.report) return;
    const cached = await loadLastReport();
    if (!cached || state.report) return;
    const report = cached.report;
    setState(
      produce((s) => {
        s.report = report;
        for (const group of report.groups) {
          for (const item of group.items) {
            if (!(item.id in s.selected)) {
              s.selected[item.id] = item.selectedByDefault;
            }
          }
        }
      }),
    );
  }

  /**
   * Orchestrate a full scan. Owned by the store (not a screen component) so the
   * in-flight scan survives navigation between views — switching to Dashboard
   * and back never cancels or loses the run. Cancellation is explicit only
   * (via the Scanning screen's Stop/Cancel buttons calling `cancel_scan`).
   */
  async function runScan(roots: string[]) {
    // Guard against starting a second scan while one is already running.
    if (state.phase === "scanning") return;

    beginScan(roots);
    try {
      const report = await scan(roots, applyScanEvent);
      // Only apply the report if we're still scanning. If the user chose
      // "Cancel & discard" (which sets phase back to "welcome"), we must not
      // resurrect the results; "Stop & keep" leaves phase as "scanning" so the
      // partial report still advances to Review. (`state.phase` is a live
      // reactive read — cast defeats TS's stale control-flow narrowing from
      // the early-return guard above, since `beginScan` mutated it at runtime.)
      if ((state.phase as Phase) === "scanning") {
        setReport(report);
      }
    } catch (e) {
      console.error("scan failed", e);
      if ((state.phase as Phase) === "scanning") setPhase("welcome");
    }
  }

  /**
   * Orchestrate a full cleanup. Also store-owned so it survives navigation:
   * the deletion keeps streaming into the store even if the user switches
   * views mid-clean, and lands on Done when finished.
   */
  async function runClean(ids: string[], toRecycleBin: boolean) {
    if (ids.length === 0) return;
    if (state.phase === "cleaning") return;

    beginClean(ids.length);
    try {
      await deleteItems(ids, toRecycleBin, applyDeleteEvent);
      const before = state.driveBefore;
      if (before) {
        try {
          setDriveAfter(await driveInfo(before.mount));
        } catch {
          // Non-fatal: the Done screen falls back to the reclaimed total.
        }
      }
      setPhase("done");
    } catch (e) {
      console.error("delete failed", e);
      setPhase("results");
    }
  }

  function toggleItem(id: string, on: boolean) {
    setState("selected", id, on);
  }



  function toggleCategory(category: Category, on: boolean) {
    const report = state.report;
    if (!report) return;
    setState(
      produce((s) => {
        for (const group of report.groups) {
          if (group.category !== category) continue;
          for (const item of group.items) {
            s.selected[item.id] = on;
          }
        }
      }),
    );
  }

  function selectByTier(tier: SafetyTier, on: boolean) {
    setState(
      produce((s) => {
        for (const item of allItems()) {
          if (item.tier === tier) s.selected[item.id] = on;
        }
      }),
    );
  }

  /** Reset delete counters and enter the cleaning phase. */
  function beginClean(total: number) {
    setState(
      produce((s) => {
        s.progress.deleteTotal = total;
        s.progress.deleted = 0;
        s.progress.reclaimedBytes = 0;
        s.progress.skipped = 0;
        s.progress.skippedItems = [];
        s.phase = "cleaning";
      }),
    );
  }

  /** Consume a streamed delete event; keeps the cleaning screen live. */
  function applyDeleteEvent(event: DeleteEvent) {
    switch (event.event) {
      case "started":
        setState("progress", "deleteTotal", event.data.total);
        break;
      case "deleted":
        setState(
          produce((s) => {
            s.progress.deleted += 1;
            s.progress.reclaimedBytes += event.data.sizeBytes;
            s.progress.currentPath = event.data.path;
          }),
        );
        break;
      case "skipped":
        setState(
          produce((s) => {
            s.progress.skipped += 1;
            s.progress.currentPath = event.data.path;
            s.progress.skippedItems.push({
              id: event.data.id,
              path: event.data.path,
              reason: event.data.reason,
            });
          }),
        );
        break;
      case "finished":
        setState(
          produce((s) => {
            s.progress.deleted = event.data.deleted;
            s.progress.reclaimedBytes = event.data.reclaimedBytes;
            s.progress.skipped = event.data.skipped;
          }),
        );
        // Fold this cleanup into the persisted lifetime totals.
        recordCleanup(event.data.reclaimedBytes);
        void notify(
          "Cleanup complete",
          `Reclaimed ${formatBytes(event.data.reclaimedBytes)}${
            event.data.skipped > 0 ? ` · ${event.data.skipped} skipped` : ""
          }.`,
        );
        break;
    }
  }

  /** Return to the welcome screen for a fresh scan. */
  function reset() {
    setState(
      produce((s) => {
        s.phase = "welcome";
        s.report = null;
        s.selected = {};
        s.progress = emptyProgress();
        s.driveAfter = null;
      }),
    );
  }

  // -----------------------------------------------------------------------
  // Automation
  // -----------------------------------------------------------------------

  /**
   * Adopt a report produced by a background automation run.
   *
   * Never changes `view`: automation runs while the user is doing something
   * else, and yanking them onto another screen would be hostile. It does park
   * the flow at `results` when they aren't mid-task, so opening Clean lands
   * straight on the review list with everything ready to free — one click from
   * a notification to reclaimed space.
   */
  function applyAutomationReport(report: ScanReport) {
    for (const group of report.groups) {
      group.items.sort((a, b) => b.sizeBytes - a.sizeBytes);
    }
    report.groups.sort((a, b) => b.totalBytes - a.totalBytes);

    setState(
      produce((s) => {
        s.report = report;
        // Selection is rebuilt from scratch: the previous report's ids no
        // longer resolve in the backend, so keeping stale ticks would only
        // produce "item not found in last scan" skips.
        s.selected = {};
        for (const group of report.groups) {
          for (const item of group.items) {
            s.selected[item.id] = item.selectedByDefault;
          }
        }
        if (shouldParkAtResults(s.phase, report)) {
          s.phase = "results";
        }
      }),
    );

    const itemCount = report.groups.reduce((n, g) => n + g.items.length, 0);
    recordScan(report.totalReclaimableBytes, itemCount);
    void saveLastReport(report);
  }

  function setAutomation(status: AutomationStatus) {
    setState("automation", status);
  }

  /** Merge the high-frequency progress payload into the cached status. */
  function applyAutomationProgress(progress: AutomationProgress) {
    setState(
      produce((s) => {
        if (!s.automation) return;
        s.automation.progress = progress;
        s.automation.phase = progress.phase;
        s.automation.running = progress.phase !== "idle";
      }),
    );
  }

  /** Fetch the current automation status (and the rule table, once). */
  async function hydrateAutomation() {
    try {
      setAutomation(await automationStatus());
    } catch (e) {
      console.error("automation status failed", e);
    }
    if (state.rules.length === 0) {
      try {
        setState("rules", await cleanupRules());
      } catch {
        // Non-fatal: the allow-list UI degrades to category-level control.
      }
    }
  }

  /**
   * Subscribe to backend automation events. Returns an unsubscribe function;
   * call it if the app ever tears the store down.
   */
  async function initAutomationListeners(): Promise<() => void> {
    const unlisteners = await Promise.all([
      listen<AutomationStatus>("automation://status", (e) =>
        setAutomation(e.payload),
      ),
      listen<AutomationProgress>("automation://progress", (e) =>
        applyAutomationProgress(e.payload),
      ),
      listen<ScanReport>("automation://report", (e) =>
        applyAutomationReport(e.payload),
      ),
    ]);
    return () => unlisteners.forEach((un) => un());
  }

  /**
   * Patch the automation config. The backend sanitizes and returns the result,
   * so the store adopts the response rather than the optimistic patch — that
   * way a clamped value or a refused tier shows up in the UI immediately.
   */
  async function patchAutomation(patch: Partial<ScheduleConfig>) {
    const current = state.automation?.config;
    if (!current) return;
    try {
      setAutomation(await setAutomationConfig({ ...current, ...patch }));
    } catch (e) {
      console.error("saving automation config failed", e);
      // Re-read so the UI reflects what the backend actually holds.
      void hydrateAutomation();
    }
  }

  /** Kick off an automation run immediately. */
  async function triggerAutomationRun() {
    try {
      await runAutomationNow();
    } catch (e) {
      console.error("run automation failed", e);
    }
  }

  /** Ask the running automation run to stop. */
  async function haltAutomationRun() {
    try {
      await stopAutomation();
    } catch (e) {
      console.error("stop automation failed", e);
    }
  }

  return {
    state,
    // scan lifecycle
    beginScan,
    applyScanEvent,
    setReport,
    runScan,
    hydrateLastReport,
    // clean lifecycle
    beginClean,
    applyDeleteEvent,
    runClean,
    // selection
    toggleItem,
    toggleCategory,
    selectByTier,
    // roots / tools / drive
    setRoots,
    addRoot,
    removeRoot,
    setTools,
    setDriveBefore,
    setDriveAfter,
    // navigation + stats
    setView,
    setStats,
    setTheme,
    setSkyPrefs,
    patchSky,
    setScanPrefs,
    setDeepScan,
    setDestination,
    recordScan,
    recordCleanup,
    // automation
    hydrateAutomation,
    initAutomationListeners,
    patchAutomation,
    triggerAutomationRun,
    haltAutomationRun,
    // misc
    setPhase,
    reset,
    // derived selectors
    selectedIds,
    selectedCount,
    reclaimableSelectedBytes,
  };
}

// Single shared instance for the whole app.
export const appStore = createRoot(createAppStore);

export type AppStore = ReturnType<typeof createAppStore>;
