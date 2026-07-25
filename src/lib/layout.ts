// Which layout family a theme belongs to.
//
// This is the single dispatch point for the app's two structural philosophies.
// It exists because Nebula/Void and Pulsar are not palette variants of one
// design — they are genuinely different DOM:
//
//   sky    a centred ~780px column, hairline-ruled rows, no card boxes, a pixel
//          display number as the hero. Nebula and Void share it exactly and
//          differ only in tokens (see theme.css).
//
//   dense  a full-width instrument panel: metric strip, proportional treemap,
//          sortable table, and a persistent inspector rail.
//
// Keying on *family* rather than on theme is deliberate. Nebula and Void are
// specified as one design in two palettes, and `theme.css` already proves that
// works with zero component branching. If components branched on `theme` it
// would become possible for Nebula and Void to drift apart — the exact failure
// the pair exists to prevent. With a family selector that drift is unexpressible,
// and adding a fourth theme is a one-line change here.

import type { ThemeName } from "./prefs";

export type LayoutFamily = "sky" | "dense";

export function layoutFamily(theme: ThemeName): LayoutFamily {
  return theme === "pulsar" ? "dense" : "sky";
}

/**
 * Per-family category colour ramps, used for row spines, bars and treemap tiles.
 *
 * Void gets greys rather than desaturated blues: the theme's premise is that the
 * hue is *gone*, so a blue-tinted chart would be the one coloured thing on a
 * monochrome screen.
 */
export const CATEGORY_RAMPS: Record<ThemeName, readonly string[]> = {
  nebula: ["#7fb2ff", "#5b9df0", "#4e7bc4", "#3c62a2", "#2f4d80", "#26405f", "#1d3049"],
  void: ["#e2e2e4", "#bcbcbf", "#98989c", "#78787c", "#59595d", "#404044", "#2e2e31"],
  pulsar: ["#7aa2f7", "#6a8fdd", "#5a7cc3", "#4a69a9", "#3a568f", "#2c4373", "#22355c"],
};

/** Colour for the category at `index` (largest-first order) under `theme`. */
export function categoryColor(theme: ThemeName, index: number): string {
  const ramp = CATEGORY_RAMPS[theme] ?? CATEGORY_RAMPS.nebula;
  return ramp[index % ramp.length];
}

/**
 * Is `color` light enough that text on top of it must be dark?
 *
 * Treemap tiles are filled with ramp colours and carry a label, so the label has
 * to flip. Uses the sRGB relative-luminance threshold rather than "index < 3",
 * which is what the mockup did and which breaks the moment a report has fewer
 * groups than the ramp has entries.
 */
export function needsDarkInk(color: string): boolean {
  const hex = color.replace("#", "");
  if (hex.length !== 6) return false;
  const r = parseInt(hex.slice(0, 2), 16) / 255;
  const g = parseInt(hex.slice(2, 4), 16) / 255;
  const b = parseInt(hex.slice(4, 6), 16) / 255;
  const lin = (c: number) =>
    c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  const luminance = 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
  return luminance > 0.45;
}
