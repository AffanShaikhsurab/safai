import { type Component, For, type JSX, Show } from "solid-js";
import { appStore } from "../state/store";
import { layoutFamily } from "../lib/layout";

/* ===========================================================================
   FAMILY-AWARE FORM PRIMITIVES
   ===========================================================================

   Automation and Settings carry the same controls in both layout families —
   the same toggles, the same order, the same copy. Only the container and row
   markup differs: hairline `.sky-*` rows in a centred column versus bordered
   `.dense-*` panels on the instrument panel.

   So rather than two screen variants (the pattern Overview and Clean need,
   where the *information architecture* genuinely differs), these two screens
   stay single-source and the family branch lives down here in the primitives.
   That matters more than it sounds: the shipped Automation screen has 20-odd
   controls with interdependent visibility, and duplicating that markup is how
   two copies of a settings screen quietly drift out of sync.
   =========================================================================== */

/** Current layout family. The single read point for every primitive below. */
export const isSky = (): boolean => layoutFamily(appStore.state.theme) === "sky";

/* ---------------------------------------------------------------------------
   Hero
   --------------------------------------------------------------------------- */

/**
 * Screen hero: eyebrow, display-size title, lede.
 *
 * Sky only. The dense shell already prints the view name in its status bar, and
 * an instrument panel that spent 200px of vertical space restating it would be
 * the reskin this family exists to avoid.
 */
export const Hero: Component<{
  eyebrow: string;
  title: JSX.Element;
  lede?: JSX.Element;
  /** Renders the "alive" dot beside the title. One of the app's only glows. */
  live?: "on" | "armed" | "off";
}> = (props) => (
  <Show when={isSky()}>
    <div class="sky-eyebrow">{props.eyebrow}</div>
    <div class="sky-display sm" classList={{ "sky-hero-live": !!props.live }}>
      <Show when={props.live}>
        <span class="sky-live" data-state={props.live} aria-hidden="true" />
      </Show>
      {props.title}
    </div>
    <Show when={props.lede}>
      <p class="sky-lede">{props.lede}</p>
    </Show>
  </Show>
);

/* ---------------------------------------------------------------------------
   Section
   --------------------------------------------------------------------------- */

/**
 * A titled group of rows.
 *
 * `plain` skips the row-list wrapper for children that aren't rows (a theme
 * grid, a log): sky drops the hairline list, dense keeps the panel but pads it.
 */
export const Section: Component<{
  title: string;
  meta?: JSX.Element;
  plain?: boolean;
  children: JSX.Element;
}> = (props) => (
  <Show
    when={isSky()}
    fallback={
      <div class="dense-panel">
        <div class="dense-phead">
          {props.title}
          <Show when={props.meta}>
            <span class="r">{props.meta}</span>
          </Show>
        </div>
        <Show when={props.plain} fallback={props.children}>
          <div class="dense-pad">{props.children}</div>
        </Show>
      </div>
    }
  >
    <div class="sky-sec">
      <h3>{props.title}</h3>
      <Show when={props.meta}>
        <span class="m">{props.meta}</span>
      </Show>
    </div>
    <Show when={props.plain} fallback={<div class="sky-list">{props.children}</div>}>
      {props.children}
    </Show>
  </Show>
);

/* ---------------------------------------------------------------------------
   Row
   --------------------------------------------------------------------------- */

/** Label + description on the left, control on the right. */
export const Row: Component<{
  label: JSX.Element;
  desc?: JSX.Element;
  children?: JSX.Element;
}> = (props) => (
  <div class={isSky() ? "sky-arow" : "dense-parow"}>
    <div class="t">
      <div class="l">{props.label}</div>
      <Show when={props.desc}>
        <div class="d">{props.desc}</div>
      </Show>
    </div>
    <Show when={props.children}>
      <div class="c">{props.children}</div>
    </Show>
  </div>
);

/* ---------------------------------------------------------------------------
   Toggle
   --------------------------------------------------------------------------- */

/**
 * On/off switch.
 *
 * A real `<input type="checkbox">` sits invisibly on top rather than a
 * `role="switch"` div with a click handler: it gets keyboard operation, focus
 * and form semantics from the platform instead of from us re-implementing them.
 */
export const Toggle: Component<{
  on: boolean;
  onChange: (next: boolean) => void;
  label: string;
}> = (props) => (
  <span
    class={isSky() ? "sky-toggle" : "dense-toggle"}
    data-on={props.on ? "true" : "false"}
  >
    <input
      type="checkbox"
      class="sr-check"
      role="switch"
      aria-checked={props.on}
      aria-label={props.label}
      checked={props.on}
      onChange={(e) => props.onChange(e.currentTarget.checked)}
    />
  </span>
);

/* ---------------------------------------------------------------------------
   Segmented control
   --------------------------------------------------------------------------- */

export interface SegOption<T> {
  value: T;
  label: string;
}

/** Mutually exclusive choice, two to four options. */
export function Seg<T>(props: {
  label: string;
  options: readonly SegOption<T>[];
  value: T;
  onChange: (next: T) => void;
}): JSX.Element {
  return (
    <div
      class={isSky() ? "sky-seg" : "dense-seg"}
      role="group"
      aria-label={props.label}
    >
      <For each={props.options}>
        {(opt) => (
          <button
            type="button"
            data-on={props.value === opt.value ? "true" : "false"}
            aria-pressed={props.value === opt.value}
            onClick={() => props.onChange(opt.value)}
          >
            {opt.label}
          </button>
        )}
      </For>
    </div>
  );
}

/* ---------------------------------------------------------------------------
   Checkbox
   --------------------------------------------------------------------------- */

/** Square check, for allow-lists. */
export const Check: Component<{
  on: boolean;
  onChange: (next: boolean) => void;
  label: string;
}> = (props) => (
  <button
    type="button"
    class={isSky() ? "sky-check" : "dense-chk"}
    data-on={props.on ? "true" : "false"}
    role="checkbox"
    aria-checked={props.on}
    aria-label={props.label}
    onClick={() => props.onChange(!props.on)}
  >
    {/* Sky draws its tick with box-shadow (see .sky-check); dense uses a glyph. */}
    <Show when={!isSky() && props.on}>
      <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
        <path
          d="M20 6 9 17l-5-5"
          stroke="currentColor"
          stroke-width="3.5"
          stroke-linecap="square"
        />
      </svg>
    </Show>
  </button>
);

/* ---------------------------------------------------------------------------
   Note
   --------------------------------------------------------------------------- */

/** Explanatory copy inside a section. `tone="warn"` for policy limits. */
export const Note: Component<{ tone?: "warn"; children: JSX.Element }> = (
  props,
) => (
  <p
    class={isSky() ? "sky-note" : "dense-empty"}
    classList={{ warn: props.tone === "warn" }}
  >
    {props.children}
  </p>
);

/* ---------------------------------------------------------------------------
   Pick
   --------------------------------------------------------------------------- */

/** A two-up card choice. Used for scan-and-notify versus autopilot. */
export const Pick: Component<{
  on: boolean;
  onClick: () => void;
  name: string;
  desc: JSX.Element;
}> = (props) => (
  <button
    type="button"
    class={isSky() ? "sky-pick" : "dense-pick"}
    data-on={props.on ? "true" : "false"}
    aria-pressed={props.on}
    onClick={props.onClick}
  >
    <span class="n">{props.name}</span>
    <span class="d">{props.desc}</span>
  </button>
);
