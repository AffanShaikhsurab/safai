// Pure helpers for the automation feature.
//
// Everything here is a plain function over plain data — no Tauri imports, no
// Solid reactivity, no DOM. The store and the Automation screen delegate their
// fiddly bits to this module so the logic can be unit-tested directly (see
// `automation.test.ts`) rather than only through the UI.

import type {
  Phase,
  View,
} from "../state/store";
import type {
  RuleInfo,
  ScanReport,
  ScheduleConfig,
} from "./types";

// ---------------------------------------------------------------------------
// Activity gate
// ---------------------------------------------------------------------------

/**
 * Is the user doing something an automatic scan would ruin?
 *
 * A live scan or cleanup always counts. The `results` phase only counts while
 * the Clean view is actually on screen: that's when a selection is being built
 * from ids the backend still holds. Once they navigate away, the phase is just
 * a bookmark — and if it kept the gate held, automation would defer forever,
 * since nothing ever clears `results` on its own.
 */
export function isUiEngaged(phase: Phase, view: View): boolean {
  if (phase === "scanning" || phase === "cleaning") return true;
  return phase === "results" && view === "clean";
}

/**
 * Should a background run's findings park the flow on the review screen?
 *
 * Only when there's nothing to interrupt, so opening Clean lands straight on a
 * ready-to-act list. `scanning`/`cleaning` mean a manual run is live (automation
 * shouldn't have started at all, but a race is cheap to guard against), and
 * `results` is already where we want to be.
 */
export function shouldParkAtResults(phase: Phase, report: ScanReport): boolean {
  if (report.groups.length === 0) return false;
  return phase === "welcome" || phase === "done";
}

// ---------------------------------------------------------------------------
// Autopilot rule allow-list
// ---------------------------------------------------------------------------

/**
 * Which rules the current tier + category choice lets autopilot touch.
 *
 * Mirrors the backend policy in `schedule/policy.rs` so the UI preview matches
 * what will actually happen. The `caution` and `regenerates` conditions are
 * repeated here on purpose: the backend enforces them regardless, and showing a
 * rule the backend would refuse would be a lie.
 */
export function eligibleRules(
  rules: RuleInfo[],
  cfg: Pick<ScheduleConfig, "autoCleanTiers" | "autoCleanCategories">,
): RuleInfo[] {
  return rules.filter(
    (rule) =>
      rule.tier !== "caution" &&
      rule.regenerates &&
      cfg.autoCleanTiers.includes(rule.tier) &&
      cfg.autoCleanCategories.includes(rule.category),
  );
}

/**
 * Is `ruleId` currently allowed?
 *
 * An empty allow-list is the "no narrowing applied" sentinel and means *every*
 * eligible rule, not none.
 */
export function isRuleAllowed(allowList: string[], ruleId: string): boolean {
  return allowList.length === 0 || allowList.includes(ruleId);
}

/**
 * The allow-list after toggling one rule.
 *
 * Two subtleties, both of which are easy to get wrong:
 *
 * 1. Un-ticking a rule while the list is the empty "all" sentinel has to start
 *    from the full eligible set. Starting from `[]` would collapse the list to a
 *    single entry and silently disable every other rule.
 * 2. When the result covers everything eligible again, it collapses back to `[]`
 *    so newly-added rules (from a Safai update, or from switching a category on)
 *    are picked up rather than being excluded by a stale explicit list.
 */
export function nextRuleAllowList(
  allowList: string[],
  allEligibleIds: string[],
  ruleId: string,
  allowed: boolean,
): string[] {
  const base = allowList.length > 0 ? allowList : allEligibleIds;
  const next = allowed
    ? [...new Set([...base, ruleId])]
    : base.filter((id) => id !== ruleId);

  const coversEverything =
    next.length === allEligibleIds.length &&
    allEligibleIds.every((id) => next.includes(id));

  return coversEverything ? [] : next;
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

/**
 * Short relative time: "just now", "40m ago", "in 6h", "3d ago".
 *
 * `nowMs` is injectable so tests don't depend on the wall clock.
 */
export function formatRelative(
  unixSecs: number | null,
  nowMs: number = Date.now(),
): string {
  if (!unixSecs) return "never";

  const deltaMs = nowMs - unixSecs * 1000;
  const mins = Math.round(Math.abs(deltaMs) / 60_000);
  if (mins < 1) return "just now";

  let magnitude: string;
  if (mins < 60) magnitude = `${mins}m`;
  else if (mins < 60 * 24) magnitude = `${Math.round(mins / 60)}h`;
  else magnitude = `${Math.round(mins / 1440)}d`;

  // Negative delta means the timestamp is in the future (a scheduled run).
  return deltaMs < 0 ? `in ${magnitude}` : `${magnitude} ago`;
}
