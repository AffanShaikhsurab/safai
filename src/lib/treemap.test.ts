import { describe, expect, it } from "vitest";
import { squarify, type TreemapInput } from "./treemap";
import { categoryColor, layoutFamily, needsDarkInk } from "./layout";

const item = (id: string, value: number): TreemapInput => ({ id, value });

/** Total area of all tiles, in percent² of the container. */
const area = (tiles: { w: number; h: number }[]) =>
  tiles.reduce((sum, t) => sum + t.w * t.h, 0);

describe("squarify", () => {
  it("returns nothing for an empty list", () => {
    expect(squarify([])).toEqual([]);
  });

  it("gives a single item the whole container", () => {
    const [tile] = squarify([item("a", 10)]);
    expect(tile).toMatchObject({ x: 0, y: 0, w: 100, h: 100 });
  });

  it("fills the container exactly", () => {
    const tiles = squarify([
      item("a", 24.8),
      item("b", 18.2),
      item("c", 9.1),
      item("d", 6.4),
      item("e", 2.6),
      item("f", 1.3),
    ]);
    // Float tolerance only — no gaps and no overlap.
    expect(area(tiles)).toBeCloseTo(100 * 100, 3);
  });

  it("keeps every tile inside the container", () => {
    const tiles = squarify(
      Array.from({ length: 7 }, (_, i) => item(`c${i}`, (i + 1) * 3.3)),
    );
    for (const t of tiles) {
      expect(t.x).toBeGreaterThanOrEqual(-0.001);
      expect(t.y).toBeGreaterThanOrEqual(-0.001);
      expect(t.x + t.w).toBeLessThanOrEqual(100.001);
      expect(t.y + t.h).toBeLessThanOrEqual(100.001);
    }
  });

  it("sizes tiles in proportion to value", () => {
    const tiles = squarify([item("big", 75), item("small", 25)]);
    const big = tiles.find((t) => t.item.id === "big")!;
    const small = tiles.find((t) => t.item.id === "small")!;
    expect(big.w * big.h).toBeCloseTo(7500, 2);
    expect(small.w * small.h).toBeCloseTo(2500, 2);
  });

  it("orders output largest-first regardless of input order", () => {
    const tiles = squarify([item("s", 1), item("l", 50), item("m", 10)]);
    expect(tiles.map((t) => t.item.id)).toEqual(["l", "m", "s"]);
  });

  it("drops zero and negative values instead of emitting empty tiles", () => {
    // A zero-area tile would still be hoverable and focusable — an invisible
    // click target is worse than an absent one.
    const tiles = squarify([item("a", 10), item("zero", 0), item("neg", -5)]);
    expect(tiles.map((t) => t.item.id)).toEqual(["a"]);
  });

  it("drops non-finite values", () => {
    const tiles = squarify([item("a", 10), item("nan", NaN), item("inf", Infinity)]);
    expect(tiles.map((t) => t.item.id)).toEqual(["a"]);
  });

  it("returns nothing when every value is unusable", () => {
    expect(squarify([item("a", 0), item("b", -1)])).toEqual([]);
  });

  // The whole reason to squarify rather than slice: comparing areas by eye only
  // works when the rectangles aren't slivers.
  it("keeps aspect ratios reasonable for a realistic report", () => {
    const tiles = squarify([
      item("pkg", 24.8),
      item("build", 18.2),
      item("editor", 9.1),
      item("model", 6.4),
      item("temp", 2.6),
      item("browser", 1.3),
    ]);
    for (const t of tiles) {
      const ratio = Math.max(t.w / t.h, t.h / t.w);
      // A naive slice layout produces ratios in the tens for the small tiles.
      expect(ratio).toBeLessThan(6);
    }
  });

  it("handles one dominant group without collapsing the rest", () => {
    // Common in practice: one enormous node_modules and a few small caches.
    const tiles = squarify([item("huge", 500), item("a", 1), item("b", 1)]);
    expect(tiles).toHaveLength(3);
    for (const t of tiles) {
      expect(t.w).toBeGreaterThan(0);
      expect(t.h).toBeGreaterThan(0);
    }
    expect(area(tiles)).toBeCloseTo(10000, 2);
  });

  it("does not overlap any two tiles", () => {
    const tiles = squarify(
      Array.from({ length: 6 }, (_, i) => item(`c${i}`, 10 - i)),
    );
    for (let i = 0; i < tiles.length; i++) {
      for (let j = i + 1; j < tiles.length; j++) {
        const a = tiles[i];
        const b = tiles[j];
        const disjoint =
          a.x + a.w <= b.x + 0.001 ||
          b.x + b.w <= a.x + 0.001 ||
          a.y + a.h <= b.y + 0.001 ||
          b.y + b.h <= a.y + 0.001;
        expect(disjoint).toBe(true);
      }
    }
  });
});

describe("layoutFamily", () => {
  // Nebula and Void must resolve to the same family: they're specified as one
  // design in two palettes, and branching them apart is the failure this
  // indirection exists to make impossible.
  it("groups nebula and void together", () => {
    expect(layoutFamily("nebula")).toBe("sky");
    expect(layoutFamily("void")).toBe("sky");
    expect(layoutFamily("nebula")).toBe(layoutFamily("void"));
  });

  it("puts pulsar in its own family", () => {
    expect(layoutFamily("pulsar")).toBe("dense");
  });
});

describe("categoryColor", () => {
  it("gives every theme its own ramp", () => {
    expect(categoryColor("nebula", 0)).not.toBe(categoryColor("void", 0));
    expect(categoryColor("void", 0)).not.toBe(categoryColor("pulsar", 0));
  });

  it("wraps rather than returning undefined past the end of the ramp", () => {
    expect(categoryColor("nebula", 99)).toMatch(/^#[0-9a-f]{6}$/i);
  });

  it("is stable for the same index", () => {
    expect(categoryColor("nebula", 2)).toBe(categoryColor("nebula", 2));
  });

  it("uses true greys for void, not desaturated blues", () => {
    // The premise of Void is that the hue is gone, so R, G and B should match.
    for (let i = 0; i < 7; i++) {
      const hex = categoryColor("void", i).replace("#", "");
      const r = hex.slice(0, 2);
      const g = hex.slice(2, 4);
      expect(g).toBe(r);
    }
  });
});

describe("needsDarkInk", () => {
  it("asks for dark ink on light fills and light ink on dark fills", () => {
    expect(needsDarkInk("#e2e2e4")).toBe(true);
    expect(needsDarkInk("#22355c")).toBe(false);
  });

  it("handles the brightest entry of each ramp", () => {
    // Guards the treemap label contrast flip across all three themes.
    expect(needsDarkInk(categoryColor("void", 0))).toBe(true);
    expect(needsDarkInk(categoryColor("nebula", 6))).toBe(false);
  });

  it("degrades safely on a malformed colour", () => {
    expect(needsDarkInk("not-a-colour")).toBe(false);
    expect(needsDarkInk("#fff")).toBe(false);
  });
});
