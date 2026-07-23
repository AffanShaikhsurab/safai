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
