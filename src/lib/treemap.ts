// Squarified treemap layout.
//
// The approved mockup drew its treemap with three hardcoded columns and a fixed
// assignment of exactly six categories (`c[0]` | `c[1]+c[2]` | `c[3]+c[4]+c[5]`).
// That is fine for a static picture and useless as an implementation: a real
// scan report has anywhere from one to seven groups in arbitrary proportions, so
// the mockup's DOM is a reference for *appearance*, not a template.
//
// This is the standard squarified algorithm (Bruls, Huizing & van Wijk, 1999).
// It lays values out in rows/columns chosen to keep each rectangle's aspect
// ratio as close to 1 as possible, because long thin slivers are both ugly and
// genuinely hard to compare by area — which is the entire reason to draw a
// treemap instead of a bar chart.
//
// Pure and deterministic, so it's unit-tested directly (`treemap.test.ts`).

export interface TreemapInput {
  /** Stable identity, passed through to the output. */
  id: string;
  /** Must be >= 0. Zero-valued entries are dropped: they have no area. */
  value: number;
}

export interface TreemapTile<T extends TreemapInput = TreemapInput> {
  item: T;
  /** Percentages of the container, ready for CSS `left`/`top`/`width`/`height`. */
  x: number;
  y: number;
  w: number;
  h: number;
}

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Worst (largest) aspect ratio in a row of areas laid along `side`. */
function worstRatio(areas: number[], side: number, sum: number): number {
  if (sum <= 0 || side <= 0) return Infinity;
  const rowThickness = sum / side;
  let worst = 0;
  for (const area of areas) {
    const length = area / rowThickness;
    // Ratio is always >= 1, whichever dimension is longer.
    const ratio = Math.max(rowThickness / length, length / rowThickness);
    if (ratio > worst) worst = ratio;
  }
  return worst;
}

/**
 * Lay `items` out over a 100x100 space, returning one tile per item in
 * percentage units.
 *
 * Items are sorted largest-first (the algorithm requires it). Entries with a
 * non-positive or non-finite value are dropped rather than producing
 * zero-area tiles that would still be focusable and hoverable.
 */
export function squarify<T extends TreemapInput>(items: T[]): TreemapTile<T>[] {
  const usable = items
    .filter((i) => Number.isFinite(i.value) && i.value > 0)
    .sort((a, b) => b.value - a.value);

  if (usable.length === 0) return [];

  const total = usable.reduce((sum, i) => sum + i.value, 0);
  // Work in area units where the whole container is 100 * 100.
  const scale = (100 * 100) / total;

  const tiles: TreemapTile<T>[] = [];
  let rect: Rect = { x: 0, y: 0, w: 100, h: 100 };

  let queue = usable.slice();
  let row: T[] = [];
  let rowAreas: number[] = [];

  /** Place the accumulated row along the short side of `rect` and shrink it. */
  function flushRow() {
    if (row.length === 0) return;
    const sum = rowAreas.reduce((a, b) => a + b, 0);
    const vertical = rect.w >= rect.h;
    // Row runs along the shorter side, so the layout stays chunky.
    const side = vertical ? rect.h : rect.w;
    const thickness = sum / side;

    let offset = 0;
    for (let i = 0; i < row.length; i++) {
      const length = rowAreas[i] / thickness;
      if (vertical) {
        tiles.push({
          item: row[i],
          x: rect.x,
          y: rect.y + offset,
          w: thickness,
          h: length,
        });
      } else {
        tiles.push({
          item: row[i],
          x: rect.x + offset,
          y: rect.y,
          w: length,
          h: thickness,
        });
      }
      offset += length;
    }

    if (vertical) {
      rect = { x: rect.x + thickness, y: rect.y, w: rect.w - thickness, h: rect.h };
    } else {
      rect = { x: rect.x, y: rect.y + thickness, w: rect.w, h: rect.h - thickness };
    }
    row = [];
    rowAreas = [];
  }

  while (queue.length > 0) {
    const next = queue[0];
    const nextArea = next.value * scale;
    const side = Math.min(rect.w, rect.h);
    const currentSum = rowAreas.reduce((a, b) => a + b, 0);

    const worstNow = row.length === 0 ? Infinity : worstRatio(rowAreas, side, currentSum);
    const worstWith = worstRatio([...rowAreas, nextArea], side, currentSum + nextArea);

    // Adding the next item improves (or holds) the worst ratio: keep filling.
    if (row.length === 0 || worstWith <= worstNow) {
      row.push(next);
      rowAreas.push(nextArea);
      queue = queue.slice(1);
    } else {
      flushRow();
    }
  }
  flushRow();

  return tiles;
}
