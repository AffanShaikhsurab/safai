// Headless Clean model, shared by both layout families.
//
// The two families need the same items in two different shapes: `sky` groups
// them by category with expandable sub-lists, `dense` flattens them into one
// globally size-sorted table. Both derivations live here so neither variant
// re-implements the tier/selection arithmetic.

import { createMemo } from "solid-js";
import { appStore } from "../../state/store";
import { categoryColor } from "../../lib/layout";
import { categoryMeta } from "../../lib/categories";
import type { CategoryGroup, CleanupItem, SafetyTier } from "../../lib/types";

const TIER_RANK: Record<SafetyTier, number> = { safe: 0, review: 1, caution: 2 };

export interface CleanGroup {
  group: CategoryGroup;
  label: string;
  blurb: string;
  color: string;
  items: CleanupItem[];
  /** Most cautious tier present — showing "Safe" on a group holding a Caution
   *  item would be misleading. */
  tier: SafetyTier;
  allSelected: boolean;
  someSelected: boolean;
}

export interface FlatRow {
  item: CleanupItem;
  categoryLabel: string;
  color: string;
}

export function useClean() {
  const report = () => appStore.state.report;

  const hasItems = createMemo(() => {
    const r = report();
    return !!r && r.groups.some((g) => g.items.length > 0);
  });

  /** Categories largest-first, each with its items largest-first. */
  const groups = createMemo<CleanGroup[]>(() => {
    const r = report();
    if (!r) return [];
    const theme = appStore.state.theme;

    return [...r.groups]
      .sort((a, b) => b.totalBytes - a.totalBytes)
      .map((group, i) => {
        const items = [...group.items].sort((a, b) => b.sizeBytes - a.sizeBytes);
        const selectedCount = items.filter(
          (it) => appStore.state.selected[it.id],
        ).length;
        let tier: SafetyTier = "safe";
        for (const it of items) {
          if (TIER_RANK[it.tier] > TIER_RANK[tier]) tier = it.tier;
        }
        const meta = categoryMeta(group.category);
        return {
          group,
          label: meta.label,
          blurb: meta.blurb,
          color: categoryColor(theme, i),
          items,
          tier,
          allSelected: items.length > 0 && selectedCount === items.length,
          someSelected: selectedCount > 0 && selectedCount < items.length,
        };
      });
  });

  /**
   * Every item as one flat list, globally sorted largest-first.
   *
   * Colour is inherited from the item's *category* position, not its position in
   * the flat list, so a path's colour matches its category everywhere in the app.
   */
  const flat = createMemo<FlatRow[]>(() =>
    groups()
      .flatMap((g) =>
        g.items.map((item) => ({
          item,
          categoryLabel: g.label,
          color: g.color,
        })),
      )
      .sort((a, b) => b.item.sizeBytes - a.item.sizeBytes),
  );

  return {
    report,
    hasItems,
    groups,
    flat,
    selectedCount: () => appStore.selectedCount(),
    selectedBytes: () => appStore.reclaimableSelectedBytes(),
    toRecycleBin: () => appStore.state.toRecycleBin,
  };
}
