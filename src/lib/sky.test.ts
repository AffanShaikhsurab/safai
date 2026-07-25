import { describe, expect, it } from "vitest";
import {
  COMET_STEPS,
  DEFAULT_SKY,
  HOT_AT,
  MAX_COMETS,
  MAX_DENSITY,
  METER_CELLS,
  PIXEL_STEPS,
  STAR_STEPS,
  meterCells,
  normalizeSky,
  usedPercent,
} from "./sky";

describe("normalizeSky", () => {
  it("returns the defaults for missing or non-object input", () => {
    expect(normalizeSky(undefined)).toEqual(DEFAULT_SKY);
    expect(normalizeSky(null)).toEqual(DEFAULT_SKY);
    expect(normalizeSky({})).toEqual(DEFAULT_SKY);
  });

  it("keeps a fully valid object intact", () => {
    const valid = {
      pixel: 4 as const,
      density: 1.7,
      comets: 2.2,
      horizon: false,
      motion: false,
    };
    expect(normalizeSky(valid)).toEqual(valid);
  });

  it("accepts every offered pixel size", () => {
    for (const step of PIXEL_STEPS) {
      expect(normalizeSky({ pixel: step.value }).pixel).toBe(step.value);
    }
  });

  it("falls back on an unsupported pixel size", () => {
    // 8 would produce a buffer so small the UI text sits on visible blocks.
    expect(normalizeSky({ pixel: 8 }).pixel).toBe(DEFAULT_SKY.pixel);
    expect(normalizeSky({ pixel: "3" }).pixel).toBe(DEFAULT_SKY.pixel);
  });

  it("clamps density rather than rejecting it", () => {
    expect(normalizeSky({ density: 99 }).density).toBe(MAX_DENSITY);
    expect(normalizeSky({ density: 0.8 }).density).toBe(0.8);
  });

  it("rejects a non-positive density", () => {
    // Zero stars is an empty gradient, not a sky — that's what the theme
    // switcher is for.
    expect(normalizeSky({ density: 0 }).density).toBe(DEFAULT_SKY.density);
    expect(normalizeSky({ density: -1 }).density).toBe(DEFAULT_SKY.density);
  });

  // Zero is a real setting here (comets off), which is why the guard differs
  // from density's. Easy thing to get wrong in a refactor.
  it("treats zero comets as valid, not as missing", () => {
    expect(normalizeSky({ comets: 0 }).comets).toBe(0);
  });

  it("clamps the comet rate", () => {
    expect(normalizeSky({ comets: 500 }).comets).toBe(MAX_COMETS);
    expect(normalizeSky({ comets: -3 }).comets).toBe(DEFAULT_SKY.comets);
  });

  it("rejects non-finite numbers", () => {
    expect(normalizeSky({ density: NaN }).density).toBe(DEFAULT_SKY.density);
    expect(normalizeSky({ comets: Infinity }).comets).toBe(DEFAULT_SKY.comets);
  });

  it("only accepts real booleans for the toggles", () => {
    expect(normalizeSky({ horizon: false, motion: false })).toMatchObject({
      horizon: false,
      motion: false,
    });
    // A truthy string must not be coerced — that would silently flip a stored
    // `false` back on.
    expect(normalizeSky({ horizon: "yes" }).horizon).toBe(DEFAULT_SKY.horizon);
  });

  it("ignores unknown keys from a newer build", () => {
    const out = normalizeSky({ pixel: 2, nebulaBrightness: 12 });
    expect(out.pixel).toBe(2);
    expect(out).not.toHaveProperty("nebulaBrightness");
  });
});

describe("defaults match the reviewed design", () => {
  // These are the values chosen in the design review. If someone changes a
  // default, this fails and makes them say so out loud.
  it("is balanced pixels, a sparse field and rare comets", () => {
    expect(DEFAULT_SKY).toEqual({
      pixel: 3,
      density: 0.55,
      comets: 0.5,
      horizon: true,
      motion: true,
    });
  });

  it("offers the defaults as selectable options", () => {
    // Otherwise the Settings UI shows nothing highlighted on a fresh install.
    expect(PIXEL_STEPS.map((s) => s.value)).toContain(DEFAULT_SKY.pixel);
    expect(STAR_STEPS.map((s) => s.value)).toContain(DEFAULT_SKY.density);
    expect(COMET_STEPS.map((s) => s.value)).toContain(DEFAULT_SKY.comets);
  });

  it("keeps every option within the clamped range", () => {
    for (const s of STAR_STEPS) expect(s.value).toBeLessThanOrEqual(MAX_DENSITY);
    for (const s of COMET_STEPS) expect(s.value).toBeLessThanOrEqual(MAX_COMETS);
    // Round-tripping any option through the validator must be a no-op.
    for (const s of STAR_STEPS)
      expect(normalizeSky({ density: s.value }).density).toBe(s.value);
    for (const s of COMET_STEPS)
      expect(normalizeSky({ comets: s.value }).comets).toBe(s.value);
  });
});

describe("usedPercent", () => {
  it("computes used space from free and total", () => {
    expect(usedPercent(25, 100)).toBe(75);
    // The real drive on the dev machine: 56.7 of 475.7 GB free.
    expect(usedPercent(56.7, 475.7)).toBe(88);
  });

  it("returns 0 for an unreadable volume instead of NaN", () => {
    expect(usedPercent(0, 0)).toBe(0);
    expect(usedPercent(10, -1)).toBe(0);
    expect(usedPercent(10, NaN)).toBe(0);
  });

  it("clamps out-of-range inputs", () => {
    // free > total shouldn't render a negative meter.
    expect(usedPercent(150, 100)).toBe(0);
    expect(usedPercent(0, 100)).toBe(100);
  });
});

describe("meterCells", () => {
  it("fills cells in proportion to usage", () => {
    const cells = meterCells(50, 10);
    expect(cells.filter((c) => c !== "false")).toHaveLength(5);
  });

  it("leaves every cell empty at 0%", () => {
    expect(meterCells(0, 10).every((c) => c === "false")).toBe(true);
  });

  it("fills every cell at 100%", () => {
    expect(meterCells(100, 10).every((c) => c !== "false")).toBe(true);
  });

  // The amber cells are how the UI shows where the scheduler's capacity trigger
  // sits, so the threshold has to line up with it.
  it("marks only the cells past the automation threshold as hot", () => {
    const cells = meterCells(100, METER_CELLS);
    const firstHot = cells.indexOf("hot");
    expect(firstHot).toBeGreaterThan(0);
    expect(firstHot / METER_CELLS).toBeGreaterThanOrEqual(HOT_AT);
    // Everything before it is plain fill.
    expect(cells.slice(0, firstHot).every((c) => c === "true")).toBe(true);
  });

  it("shows no hot cells while usage is comfortable", () => {
    expect(meterCells(60, METER_CELLS)).not.toContain("hot");
  });

  it("defaults to the shared cell count", () => {
    expect(meterCells(50)).toHaveLength(METER_CELLS);
  });
});
