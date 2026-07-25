import { describe, expect, it } from "vitest";
import {
  eligibleRules,
  formatRelative,
  isRuleAllowed,
  isUiEngaged,
  nextRuleAllowList,
  shouldParkAtResults,
} from "./automation";
import type { Category, RuleInfo, SafetyTier, ScanReport } from "./types";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function rule(
  id: string,
  category: Category,
  tier: SafetyTier = "safe",
  regenerates = true,
): RuleInfo {
  return {
    id,
    label: id,
    category,
    tier,
    regenerates,
    note: "",
    patternBased: false,
  };
}

function report(groupCount: number): ScanReport {
  return {
    totalReclaimableBytes: 0,
    groups: Array.from({ length: groupCount }, () => ({
      category: "packageCache" as Category,
      label: "Package caches",
      totalBytes: 0,
      items: [],
    })),
    scannedRoots: [],
    warnings: [],
  };
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

// ---------------------------------------------------------------------------

describe("isUiEngaged", () => {
  it("treats a live scan or cleanup as engaged from any view", () => {
    expect(isUiEngaged("scanning", "clean")).toBe(true);
    expect(isUiEngaged("scanning", "dashboard")).toBe(true);
    expect(isUiEngaged("cleaning", "settings")).toBe(true);
  });

  it("treats the results screen as engaged only while it is on screen", () => {
    expect(isUiEngaged("results", "clean")).toBe(true);
  });

  // The regression this guards: if `results` counted as engaged everywhere,
  // nothing would ever clear it (no phase transition happens on its own) and
  // automation would defer forever after the user's first scan.
  it("releases the gate once the user navigates away from results", () => {
    expect(isUiEngaged("results", "dashboard")).toBe(false);
    expect(isUiEngaged("results", "automation")).toBe(false);
    expect(isUiEngaged("results", "settings")).toBe(false);
  });

  it("treats idle phases as not engaged", () => {
    expect(isUiEngaged("welcome", "clean")).toBe(false);
    expect(isUiEngaged("done", "clean")).toBe(false);
  });
});

describe("shouldParkAtResults", () => {
  it("parks at results when the user has nothing in progress", () => {
    expect(shouldParkAtResults("welcome", report(2))).toBe(true);
    expect(shouldParkAtResults("done", report(2))).toBe(true);
  });

  it("never interrupts a live manual run", () => {
    expect(shouldParkAtResults("scanning", report(2))).toBe(false);
    expect(shouldParkAtResults("cleaning", report(2))).toBe(false);
  });

  it("leaves an existing results view alone", () => {
    expect(shouldParkAtResults("results", report(2))).toBe(false);
  });

  it("does not park on an empty report", () => {
    // Nothing was found, so there is nothing to review.
    expect(shouldParkAtResults("welcome", report(0))).toBe(false);
  });
});

describe("eligibleRules", () => {
  const cfg = {
    autoCleanTiers: ["safe"] as SafetyTier[],
    autoCleanCategories: ["packageCache", "temp"] as Category[],
  };

  it("keeps rules inside the chosen tiers and categories", () => {
    const rules = [
      rule("npm-cache", "packageCache"),
      rule("windows-temp", "temp"),
    ];
    expect(eligibleRules(rules, cfg).map((r) => r.id)).toEqual([
      "npm-cache",
      "windows-temp",
    ]);
  });

  it("excludes categories the user did not opt into", () => {
    const rules = [rule("node-modules", "buildArtifact")];
    expect(eligibleRules(rules, cfg)).toEqual([]);
  });

  it("excludes tiers the user did not opt into", () => {
    const rules = [rule("jetbrains-caches", "packageCache", "review")];
    expect(eligibleRules(rules, cfg)).toEqual([]);
  });

  // Mirrors the backend's hard floors, so the UI can't advertise a rule the
  // backend would refuse to run.
  it("excludes caution-tier rules even when the tier is somehow selected", () => {
    const permissive = {
      autoCleanTiers: ["safe", "caution"] as SafetyTier[],
      autoCleanCategories: ["packageCache", "model"] as Category[],
    };
    const rules = [rule("lmstudio-models", "model", "caution", false)];
    expect(eligibleRules(rules, permissive)).toEqual([]);
  });

  it("excludes rules that do not regenerate", () => {
    const rules = [rule("huggingface-cache", "packageCache", "safe", false)];
    expect(eligibleRules(rules, cfg)).toEqual([]);
  });
});

describe("isRuleAllowed", () => {
  it("treats an empty allow-list as 'everything eligible'", () => {
    expect(isRuleAllowed([], "npm-cache")).toBe(true);
  });

  it("respects an explicit narrowing", () => {
    expect(isRuleAllowed(["npm-cache"], "npm-cache")).toBe(true);
    expect(isRuleAllowed(["npm-cache"], "pip-cache")).toBe(false);
  });
});

describe("nextRuleAllowList", () => {
  const all = ["npm-cache", "pip-cache", "uv-cache"];

  // The bug this guards: starting from `[]` when un-ticking would produce
  // `[]`.filter(...) === `[]`, which reads as "all rules" — the exact opposite
  // of what the user just asked for.
  it("expands the empty sentinel before excluding the first rule", () => {
    expect(nextRuleAllowList([], all, "pip-cache", false).sort()).toEqual([
      "npm-cache",
      "uv-cache",
    ]);
  });

  it("removes a rule from an explicit list", () => {
    expect(
      nextRuleAllowList(["npm-cache", "pip-cache"], all, "npm-cache", false),
    ).toEqual(["pip-cache"]);
  });

  it("adds a rule back to an explicit list", () => {
    expect(
      nextRuleAllowList(["npm-cache"], all, "pip-cache", true).sort(),
    ).toEqual(["npm-cache", "pip-cache"]);
  });

  // Collapsing back to the sentinel matters for forward compatibility: a Safai
  // update that adds a rule should be picked up, not silently excluded by a
  // stale explicit list.
  it("collapses back to the empty sentinel once everything is allowed again", () => {
    expect(
      nextRuleAllowList(["npm-cache", "pip-cache"], all, "uv-cache", true),
    ).toEqual([]);
  });

  it("does not duplicate a rule that is already allowed", () => {
    expect(nextRuleAllowList(["npm-cache"], all, "npm-cache", true)).toEqual([
      "npm-cache",
    ]);
  });

  it("can empty the list down to a single allowed rule", () => {
    const afterTwo = nextRuleAllowList([], all, "pip-cache", false);
    const afterThree = nextRuleAllowList(afterTwo, all, "uv-cache", false);
    expect(afterThree).toEqual(["npm-cache"]);
  });
});

describe("formatRelative", () => {
  const now = 1_700_000_000_000;
  const at = (offsetMs: number) => Math.floor((now + offsetMs) / 1000);

  it("reports a missing timestamp as never", () => {
    expect(formatRelative(null, now)).toBe("never");
    expect(formatRelative(0, now)).toBe("never");
  });

  it("collapses sub-minute differences", () => {
    expect(formatRelative(at(-20_000), now)).toBe("just now");
  });

  it("formats the past with an 'ago' suffix", () => {
    expect(formatRelative(at(-40 * MINUTE), now)).toBe("40m ago");
    expect(formatRelative(at(-6 * HOUR), now)).toBe("6h ago");
    expect(formatRelative(at(-3 * DAY), now)).toBe("3d ago");
  });

  it("formats the future with an 'in' prefix", () => {
    expect(formatRelative(at(45 * MINUTE), now)).toBe("in 45m");
    expect(formatRelative(at(5 * HOUR), now)).toBe("in 5h");
    expect(formatRelative(at(2 * DAY), now)).toBe("in 2d");
  });

  it("switches units at the hour and day boundaries", () => {
    expect(formatRelative(at(-59 * MINUTE), now)).toBe("59m ago");
    expect(formatRelative(at(-60 * MINUTE), now)).toBe("1h ago");
    expect(formatRelative(at(-23 * HOUR), now)).toBe("23h ago");
    expect(formatRelative(at(-25 * HOUR), now)).toBe("1d ago");
  });
});
