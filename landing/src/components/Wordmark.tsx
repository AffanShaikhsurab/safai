import { type Component, For } from "solid-js";

/**
 * Hand-authored 5×7 pixel SAFAI wordmark — same glyphs as assets/banner.html.
 * Drawn as blocks so the brand never falls back to the wrong typeface.
 */

const GLYPHS: Record<string, string[]> = {
  S: ["01110", "10001", "10000", "01110", "00001", "10001", "01110"],
  A: ["01110", "10001", "10001", "11111", "10001", "10001", "10001"],
  F: ["11111", "10000", "10000", "11110", "10000", "10000", "10000"],
  I: ["11111", "00100", "00100", "00100", "00100", "00100", "11111"],
};

const Wordmark: Component<{
  class?: string;
  /** Pixel size of one bitmap cell. */
  cell?: number;
  gap?: number;
  track?: number;
}> = (props) => {
  const cell = () => props.cell ?? 10;
  const gap = () => props.gap ?? 2;
  const track = () => props.track ?? 14;

  const cells = () => {
    const out: { x: number; y: number; s: number }[] = [];
    let penX = 0;
    const c = cell();
    const g = gap();
    for (const ch of "SAFAI") {
      const glyph = GLYPHS[ch];
      for (let row = 0; row < glyph.length; row++) {
        for (let col = 0; col < glyph[row].length; col++) {
          if (glyph[row][col] !== "1") continue;
          out.push({
            x: penX + col * c,
            y: row * c,
            s: c - g,
          });
        }
      }
      penX += 5 * c + track();
    }
    return { bits: out, width: penX - track(), height: 7 * c - g };
  };

  return (
    <svg
      class={`wordmark ${props.class ?? ""}`}
      viewBox={`0 0 ${cells().width} ${cells().height}`}
      preserveAspectRatio="xMinYMid meet"
      role="img"
      aria-label="Safai"
      style={{
        width: "auto",
        "aspect-ratio": `${cells().width} / ${cells().height}`,
      }}
    >
      <For each={cells().bits}>
        {(b) => (
          <rect x={b.x} y={b.y} width={b.s} height={b.s} fill="currentColor" />
        )}
      </For>
    </svg>
  );
};

export default Wordmark;
