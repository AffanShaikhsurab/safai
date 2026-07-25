// TypeScript mirror of the Rust data contracts (implementation-plan.md §3.5).
// Field names and casing are normative: everything crossing the IPC boundary
// is camelCase. Do not rename fields.

export type Category =
  | "packageCache"
  | "editorStorage"
  | "buildArtifact"
  | "temp"
  | "model"
  | "browser"
  | "other";

export type SafetyTier = "safe" | "review" | "caution";

export interface CleanupItem {
  id: string;
  ruleId: string;
  label: string;
  category: Category;
  tier: SafetyTier;
  path: string;
  sizeBytes: number;
  regenerates: boolean;
  lastModifiedSecs: number | null;
  note: string;
  selectedByDefault: boolean;
}

export interface CategoryGroup {
  category: Category;
  label: string;
  totalBytes: number;
  items: CleanupItem[];
}

export interface ScanReport {
  totalReclaimableBytes: number;
  groups: CategoryGroup[];
  scannedRoots: string[];
  warnings: string[];
}

export type ScanEvent =
  | { event: "started"; data: { roots: string[] } }
  | {
      event: "progress";
      data: {
        currentPath: string;
        foundBytes: number;
        rulesChecked: number;
        rulesTotal: number;
      };
    }
  | { event: "found"; data: { item: CleanupItem } }
  | {
      event: "finished";
      data: { totalReclaimableBytes: number; itemCount: number };
    };

export interface DeletePlanItem {
  id: string;
  path: string;
  sizeBytes: number;
  tier: SafetyTier;
  allowed: boolean;
  reason: string | null;
}

export interface DeletePlan {
  items: DeletePlanItem[];
  totalBytes: number;
  blockedCount: number;
}

export type DeleteEvent =
  | { event: "started"; data: { total: number } }
  | { event: "deleted"; data: { id: string; path: string; sizeBytes: number } }
  | { event: "skipped"; data: { id: string; path: string; reason: string } }
  | {
      event: "finished";
      data: { deleted: number; reclaimedBytes: number; skipped: number };
    };

export interface DeleteReport {
  deleted: number;
  reclaimedBytes: number;
  skipped: string[];
}

export interface DriveInfo {
  mount: string;
  freeBytes: number;
  totalBytes: number;
}

export interface ToolInfo {
  id: string;
  label: string;
  detected: boolean;
}

// ---------------------------------------------------------------------------
// Automation (scheduled / proactive maintenance)
// ---------------------------------------------------------------------------

/** How often an automatic maintenance run happens. */
export type Cadence = "manual" | "daily" | "everyThreeDays" | "weekly";

/** What caused an automation run to start. */
export type TriggerKind = "cadence" | "threshold" | "manual";

/** Where an automation run currently is. */
export type RunPhase = "idle" | "scanning" | "cleaning";

/** One entry of the cleanup rule table (for the autopilot allow-list UI). */
export interface RuleInfo {
  id: string;
  label: string;
  category: Category;
  tier: SafetyTier;
  regenerates: boolean;
  note: string;
  patternBased: boolean;
}

/** Persisted automation settings. Mirrors the Rust `ScheduleConfig`. */
export interface ScheduleConfig {
  enabled: boolean;

  // Time trigger.
  cadence: Cadence;
  /** Local hour (0–23) the cadence run prefers. */
  preferredHour: number;

  // Capacity trigger.
  thresholdEnabled: boolean;
  /** Fire when *used* space reaches this percentage. */
  thresholdPercent: number;
  /** Drive to watch, as a path. Empty = the drive holding the profile. */
  thresholdPath: string;

  // Constraints.
  runOnlyWhenIdle: boolean;
  idleMinutes: number;
  skipOnBattery: boolean;

  // What an automatic run may do.
  autoClean: boolean;
  autoCleanTiers: SafetyTier[];
  autoCleanCategories: Category[];
  /** Empty = every rule allowed by the tier + category filters. */
  autoCleanRuleIds: string[];
  maxAutoCleanBytes: number;
  toRecycleBin: boolean;

  // Presence.
  autostart: boolean;
  minimizeToTray: boolean;
  notify: boolean;
}

/** One entry in the automation audit trail. */
export interface RunRecord {
  at: number;
  trigger: TriggerKind;
  scannedItems: number;
  reclaimableBytes: number;
  cleanedItems: number;
  reclaimedBytes: number;
  autoCleaned: boolean;
  skippedItems: number;
  durationMs: number;
  error: string | null;
}

/** High-frequency progress for a run in flight. */
export interface AutomationProgress {
  phase: RunPhase;
  currentPath: string;
  foundBytes: number;
  itemCount: number;
  deleted: number;
  reclaimedBytes: number;
  skipped: number;
}

/** Everything the Automation screen needs, in one payload. */
export interface AutomationStatus {
  config: ScheduleConfig;
  lastRunAt: number | null;
  nextDueAt: number | null;
  running: boolean;
  phase: RunPhase;
  currentTrigger: TriggerKind | null;
  /** Why the scheduler is holding back, if it is. */
  deferredReason: string | null;
  history: RunRecord[];
  disk: DriveInfo | null;
  diskUsedPercent: number | null;
  autostartRegistered: boolean;
  idleSecs: number;
  onBattery: boolean;
  cadenceLabel: string;
  progress: AutomationProgress;
}
