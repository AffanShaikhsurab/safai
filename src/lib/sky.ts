// Pure night-sky logic: settings shape, validation, and the option tables the
// Settings screen renders.
//
// Deliberately free of Tauri imports so it can be unit-tested directly in node
// (see `sky.test.ts`). `prefs.ts` handles persistence and re-exports from here.

/**
 * Tuning for the canvas night sky. Only read when the theme is `nebula` or
 * `void`; Pulsar draws no sky and ignores all of it.
 */
export interface SkyPrefs {
  /**
   * Screen pixels per sky pixel. Higher = chunkier. The canvas buffer is the
   * window size divided by this, so it also sets the render cost.
   */
  pixel: 2 | 3 | 4;
  /** Starfield density multiplier. */
  density: number;
  /** Comet frequency multiplier. `0` disables them. */
  comets: number;
  /** Draw the dark ridge along the bottom of the window. */
  horizon: boolean;
  /** Twinkle + comet motion. Overridden by `prefers-reduced-motion`. */
  motion: boolean;
}

/**
 * Chosen during design review: chunky-but-legible pixels, a sparse field, and
 * comets rare enough that one still feels like an event.
 */
export const DEFAULT_SKY: SkyPrefs = {
  pixel: 3,
  density: 0.55,
  comets: 0.5,
  horizon: true,
  motion: true,
};

/** Upper bounds. Past these the sky stops being a backdrop and starts costing. */
export const MAX_DENSITY = 3;
export const MAX_COMETS = 4;

/**
 * Coerce anything a hand-edited prefs file (or an older/newer build) could
 * contain into a valid `SkyPrefs`.
 *
 * The prefs file lives in the user's app-data directory and outlives any single
 * release, so every field is treated as untrusted: wrong types fall back to the
 * default, and out-of-range numbers are clamped rather than rejected.
 */
export function normalizeSky(value: unknown): SkyPrefs {
  const v = (value ?? {}) as Partial<Record<keyof SkyPrefs, unknown>>;

  const pixel: SkyPrefs["pixel"] =
    v.pixel === 2 || v.pixel === 4 || v.pixel === 3 ? v.pixel : DEFAULT_SKY.pixel;

  const density =
    typeof v.density === "number" && Number.isFinite(v.density) && v.density > 0
      ? Math.min(MAX_DENSITY, v.density)
      : DEFAULT_SKY.density;

  // Zero is meaningful here (comets off), so the guard is `>= 0`, not `> 0`.
  const comets =
    typeof v.comets === "number" && Number.isFinite(v.comets) && v.comets >= 0
      ? Math.min(MAX_COMETS, v.comets)
      : DEFAULT_SKY.comets;

  return {
    pixel,
    density,
    comets,
    horizon: typeof v.horizon === "boolean" ? v.horizon : DEFAULT_SKY.horizon,
    motion: typeof v.motion === "boolean" ? v.motion : DEFAULT_SKY.motion,
  };
}

// ---------------------------------------------------------------------------
// Option tables (Settings → Night sky)
// ---------------------------------------------------------------------------

/** Pixel sizes, labelled by feel rather than by number. */
export const PIXEL_STEPS: { value: SkyPrefs["pixel"]; label: string }[] = [
  { value: 2, label: "Fine" },
  { value: 3, label: "Balanced" },
  { value: 4, label: "Chunky" },
];

export const STAR_STEPS: { value: number; label: string }[] = [
  { value: 0.55, label: "Sparse" },
  { value: 1, label: "Normal" },
  { value: 1.7, label: "Dense" },
];

export const COMET_STEPS: { value: number; label: string }[] = [
  { value: 0, label: "Off" },
  { value: 0.5, label: "Rare" },
  { value: 1, label: "Normal" },
  { value: 2.2, label: "Shower" },
];

// ---------------------------------------------------------------------------
// Drive meter (top bar)
// ---------------------------------------------------------------------------

/** Blocks in the top-bar drive meter. */
export const METER_CELLS = 14;

/**
 * Fraction of the meter past which cells render amber. Matches the scheduler's
 * default 85% capacity trigger, so the chrome shows where automation steps in.
 */
export const HOT_AT = 0.85;

/** Percentage of a volume in use, clamped to 0–100. */
export function usedPercent(freeBytes: number, totalBytes: number): number {
  if (!Number.isFinite(totalBytes) || totalBytes <= 0) return 0;
  const used = totalBytes - freeBytes;
  if (used <= 0) return 0;
  return Math.min(100, Math.round((used / totalBytes) * 100));
}

/** Fill state for one meter cell: filled, filled-and-hot, or empty. */
export type CellState = "true" | "hot" | "false";

/**
 * Fill states for the whole meter, left to right. Returned as an array so the
 * component just maps over it and holds no threshold logic of its own.
 */
export function meterCells(
  usedPct: number,
  cells: number = METER_CELLS,
): CellState[] {
  const filled = Math.round((usedPct / 100) * cells);
  return Array.from({ length: cells }, (_, i) =>
    i >= filled ? "false" : i / cells >= HOT_AT ? "hot" : "true",
  );
}
