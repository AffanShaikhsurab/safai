import { type Component, Show } from "solid-js";

/**
 * The signature dial — a 270° gauge with a big center value.
 * `pct` (0–100) fills the progress arc (electric blue on Nebula); `big`/`unit`
 * render the headline number; `cap` is the bold uppercase caption; `sub` is quiet.
 *
 * Geometry: r=144 → circumference 904.8, a 270° arc spans 678.6, rotated 135°
 * so the gap sits at the bottom. Gradient stops use theme CSS vars.
 */
const ARC = 678.6;
const CIRC = 904.8;

const Dial: Component<{
  pct: number;
  big: string;
  unit?: string;
  cap: string;
  sub?: string;
}> = (props) => {
  const dash = () => {
    const p = Math.max(0, Math.min(100, props.pct));
    return `${((p / 100) * ARC).toFixed(1)} ${CIRC}`;
  };

  return (
    <div class="dial">
      <svg viewBox="0 0 360 360" aria-hidden="true">
        <defs>
          <linearGradient id="dialGrad" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0%" stop-color="var(--progress)" />
            <stop offset="100%" stop-color="var(--progress-2)" />
          </linearGradient>
        </defs>
        <g transform="rotate(135 180 180)">
          <circle
            class="dial-track"
            cx="180"
            cy="180"
            r="144"
            stroke-dasharray={`${ARC} ${CIRC}`}
          />
          <circle
            class="dial-value"
            cx="180"
            cy="180"
            r="144"
            stroke-dasharray={dash()}
          />
        </g>
      </svg>
      <div class="dial-center">
        <div class="dial-big">
          {props.big}
          <Show when={props.unit}>
            <span>{props.unit}</span>
          </Show>
        </div>
        <div class="dial-cap">{props.cap}</div>
        <Show when={props.sub}>
          <div class="dial-sub" title={props.sub}>
            {props.sub}
          </div>
        </Show>
      </div>
    </div>
  );
};

export default Dial;
