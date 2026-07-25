import { type Component, For } from "solid-js";
import type { ThemeName } from "../lib/prefs";

/**
 * A small preview tile for the theme picker.
 *
 * For the two sky themes this draws an actual miniature night sky — gradient,
 * square stars, one comet, the horizon ridge. A flat colour swatch can't
 * communicate "starfield with a comet in it", which is the entire difference
 * between these themes and any other dark mode.
 *
 * Star positions come from a fixed table rather than `Math.random()`, so the
 * swatch is identical every render. A preview that reshuffles whenever Solid
 * re-renders looks broken.
 */

/** Fixed star field: `[leftPct, topPct, opacity, big]`. */
const STARS: [number, number, number, boolean][] = [
  [8, 18, 0.5, false],
  [17, 46, 0.28, false],
  [24, 74, 0.42, false],
  [31, 22, 0.24, false],
  [39, 60, 0.55, true],
  [46, 12, 0.34, false],
  [53, 40, 0.2, false],
  [61, 70, 0.38, false],
  [68, 30, 0.3, false],
  [75, 55, 0.48, true],
  [82, 16, 0.26, false],
  [89, 44, 0.34, false],
  [94, 66, 0.22, false],
  [12, 62, 0.3, false],
  [35, 84, 0.26, false],
  [57, 26, 0.4, false],
  [71, 8, 0.24, false],
  [86, 78, 0.2, false],
];

interface SkyLook {
  grad: [string, string, string];
  ridge: string;
}

const LOOKS: Record<"nebula" | "void", SkyLook> = {
  nebula: { grad: ["#04061a", "#0a1132", "#15245a"], ridge: "#050c0c" },
  void: { grad: ["#070708", "#0d0d0f", "#191919"], ridge: "#080808" },
};

const ThemeSwatch: Component<{ theme: ThemeName }> = (props) => {
  // Pulsar has no sky, so its swatch previews the thing that actually defines
  // it: a dense panel with a treemap strip and table rows.
  if (props.theme === "pulsar") {
    return (
      <span class="sw sw-pulsar" aria-hidden="true">
        <span class="p-rail" />
        <span class="p-body">
          <span class="p-head" />
          <span class="p-map">
            <span style={{ flex: "3", background: "#7aa2f7" }} />
            <span style={{ flex: "2", background: "#5a7cc3" }} />
            <span style={{ flex: "1", background: "#3a568f" }} />
          </span>
          <span class="p-row" />
          <span class="p-row" />
        </span>
      </span>
    );
  }

  const look = LOOKS[props.theme === "void" ? "void" : "nebula"];

  return (
    <span
      class="sw sw-sky"
      aria-hidden="true"
      style={{
        background: `linear-gradient(180deg, ${look.grad[0]} 0%, ${look.grad[1]} 42%, ${look.grad[2]} 100%)`,
      }}
    >
      <For each={STARS}>
        {([x, y, o, big]) => (
          <span
            class="s-star"
            style={{
              left: `${x}%`,
              top: `${y}%`,
              width: big ? "2px" : "1px",
              height: big ? "2px" : "1px",
              background: `rgba(255,255,255,${o})`,
            }}
          />
        )}
      </For>
      {/* A single comet, built from stacked box-shadows so the trail tapers in
          discrete pixel steps rather than a smooth gradient. */}
      <span class="s-comet" />
      <span class="s-ridge" style={{ background: look.ridge }} />
    </span>
  );
};

export default ThemeSwatch;
