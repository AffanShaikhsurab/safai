// Headless Overview model.
//
// Both layout families show the same numbers in completely different DOM, so the
// arithmetic lives here once. Only markup is duplicated between Sky.tsx and
// Dense.tsx — never a calculation.

import { createMemo } from "solid-js";
import { appStore } from "../../state/store";
import { categoryColor } from "../../lib/layout";
import { categoryMeta } from "../../lib/categories";
import type { CategoryGroup } from "../../lib/types";

export interface GroupRow {
  group: CategoryGroup;
  label: string;
  /** One short clause, sized to fit a single line. */
  blurb: string;
  color: string;
  /** Share of the total reclaimable figure, 0–100. */
  share: number;
  /** Share of the *largest* group, for bar length — makes small groups visible. */
  relative: number;
  items: number;
  /** Worst (most cautious) tier present in the group. */
  tier: "safe" | "review" | "caution";
}

const TIER_RANK = { safe: 0, review: 1, caution: 2 } as const;

/**
 * A group's headline tier is its most cautious member. Showing "Safe" on a group
 * that contains a Caution item would be actively misleading.
 */
function groupTier(group: CategoryGroup): "safe" | "review" | "caution" {
  let worst: "safe" | "review" | "caution" = "safe";
  for (const item of group.items) {
    if (TIER_RANK[item.tier] > TIER_RANK[worst]) worst = item.tier;
  }
  return worst;
}

export function useOverview() {
  const report = () => appStore.state.report;
  const stats = () => appStore.state.stats;
  const drive = () => appStore.state.driveBefore;

  const reclaimable = () => report()?.totalReclaimableBytes ?? 0;

  const itemCount = createMemo(
    () => report()?.groups.reduce((n, g) => n + g.items.length, 0) ?? 0,
  );

  const usedPct = createMemo(() => {
    const d = drive();
    if (!d || d.totalBytes <= 0) return 0;
    return Math.min(
      100,
      Math.round(((d.totalBytes - d.freeBytes) / d.totalBytes) * 100),
    );
  });

  /** Projected free space after cleaning everything currently reclaimable. */
  const freeAfter = createMemo(() => {
    const d = drive();
    if (!d) return 0;
    return d.freeBytes + reclaimable();
  });

  const usedAfterPct = createMemo(() => {
    const d = drive();
    if (!d || d.totalBytes <= 0) return 0;
    return Math.max(
      0,
      Math.round(((d.totalBytes - freeAfter()) / d.totalBytes) * 100),
    );
  });

  /** Groups largest-first, with everything the row renderers need. */
  const rows = createMemo<GroupRow[]>(() => {
    const r = report();
    if (!r) return [];
    const total = r.totalReclaimableBytes || 1;
    const sorted = [...r.groups].sort((a, b) => b.totalBytes - a.totalBytes);
    const largest = sorted[0]?.totalBytes || 1;
    const theme = appStore.state.theme;

    return sorted.map((group, i) => {
      const meta = categoryMeta(group.category);
      return {
        group,
        label: meta.label,
        blurb: meta.blurb,
        color: categoryColor(theme, i),
        share: Math.round((group.totalBytes / total) * 100),
        // At least 3% so a tiny group still shows one lit cell.
        relative: Math.max(3, Math.round((group.totalBytes / largest) * 100)),
        items: group.items.length,
        tier: groupTier(group),
      };
    });
  });

  return {
    report,
    stats,
    drive,
    reclaimable,
    itemCount,
    usedPct,
    freeAfter,
    usedAfterPct,
    rows,
  };
}
