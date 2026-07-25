import { type Component, For, Show, createSignal } from "solid-js";
import { appStore } from "../state/store";
import { DEFAULT_STATS, saveStats } from "../lib/stats";
import { type ThemeName } from "../lib/prefs";
import { COMET_STEPS, PIXEL_STEPS, STAR_STEPS } from "../lib/sky";
import { APP_VERSION_LABEL } from "../lib/version";
import ThemeSwatch from "../components/ThemeSwatch";

interface ThemeOption {
  id: ThemeName;
  label: string;
  detail: string;
}

/**
 * The three themes. Nebula and Void are the same design in two palettes — both
 * render the pixel night sky — while Pulsar swaps the sky for a denser
 * instrument panel. The copy says so, because "why are there two dark themes"
 * is otherwise a fair question.
 */
const THEMES: ThemeOption[] = [
  {
    id: "nebula",
    label: "Nebula",
    detail: "Deep blue night sky. Pixel stars, rare comets, a dark horizon.",
  },
  {
    id: "void",
    label: "Void",
    detail: "The same sky with the colour drained out. Charcoal, no blue.",
  },
  {
    id: "pulsar",
    label: "Pulsar",
    detail: "Instrument panel. Dense, flat, no sky. Numbers over atmosphere.",
  },
];

interface Tier {
  label: string;
  color: string;
  detail: string;
}

const TIERS: Tier[] = [
  {
    label: "Safe",
    color: "var(--ok)",
    detail: "Regenerates on its own — pre-selected for you.",
  },
  {
    label: "Review",
    color: "var(--amber)",
    detail: "Regenerable, but worth a quick confirm before removing.",
  },
  {
    label: "Caution",
    color: "var(--rose)",
    detail: "Your own data — never pre-selected, never auto-cleaned.",
  },
];

/**
 * Settings — Appearance (the sole theme picker), the night-sky controls, scan
 * preferences, the safety-tier legend, About, and a deliberately two-step,
 * counter-only "reset lifetime stats".
 */
const Settings: Component = () => {
  const [confirming, setConfirming] = createSignal(false);

  const resetStats = () => {
    if (!confirming()) {
      setConfirming(true);
      return;
    }
    const fresh = { ...DEFAULT_STATS };
    appStore.setStats(fresh);
    void saveStats(fresh);
    setConfirming(false);
  };

  const sky = () => appStore.state.sky;
  /** The sky controls are meaningless under Pulsar, which draws no sky. */
  const hasSky = () => appStore.state.theme !== "pulsar";

  return (
    <div class="set-wrap animate-rise">
      <div>
        <div class="set-title">Settings</div>
        <div class="set-sub">Appearance, scanning, safety and about.</div>
      </div>

      {/* Appearance — the sole theme picker */}
      <div class="card set-card">
        <div class="h">Appearance</div>
        <div class="theme-grid three">
          <For each={THEMES}>
            {(opt) => (
              <button
                type="button"
                class="theme-opt"
                data-sel={appStore.state.theme === opt.id ? "true" : "false"}
                aria-pressed={appStore.state.theme === opt.id}
                onClick={() => appStore.setTheme(opt.id)}
              >
                {/* A live miniature of the real sky, not a static swatch — it's
                    the only honest way to preview a starfield. */}
                <ThemeSwatch theme={opt.id} />
                <div>
                  <div class="n">{opt.label}</div>
                  <div class="d">{opt.detail}</div>
                </div>
              </button>
            )}
          </For>
        </div>
      </div>

      {/* Night sky — Nebula and Void only */}
      <Show when={hasSky()}>
        <div class="card set-card">
          <div class="h">Night sky</div>

          <div class="set-row">
            <div>
              <div class="rl">Comets</div>
              <div class="rd">
                How often a meteor crosses the sky. Rare keeps it an event.
              </div>
            </div>
            <div class="segmented" role="group" aria-label="Comet frequency">
              <For each={COMET_STEPS}>
                {(step) => (
                  <button
                    type="button"
                    data-on={sky().comets === step.value ? "true" : "false"}
                    onClick={() => appStore.patchSky({ comets: step.value })}
                  >
                    {step.label}
                  </button>
                )}
              </For>
            </div>
          </div>

          <div class="set-row">
            <div>
              <div class="rl">Stars</div>
              <div class="rd">Density of the starfield.</div>
            </div>
            <div class="segmented" role="group" aria-label="Star density">
              <For each={STAR_STEPS}>
                {(step) => (
                  <button
                    type="button"
                    data-on={sky().density === step.value ? "true" : "false"}
                    onClick={() => appStore.patchSky({ density: step.value })}
                  >
                    {step.label}
                  </button>
                )}
              </For>
            </div>
          </div>

          <div class="set-row">
            <div>
              <div class="rl">Pixel size</div>
              <div class="rd">
                How chunky the sky renders. Also sets how much it costs to draw.
              </div>
            </div>
            <div class="segmented" role="group" aria-label="Pixel size">
              <For each={PIXEL_STEPS}>
                {(step) => (
                  <button
                    type="button"
                    data-on={sky().pixel === step.value ? "true" : "false"}
                    onClick={() => appStore.patchSky({ pixel: step.value })}
                  >
                    {step.label}
                  </button>
                )}
              </For>
            </div>
          </div>

          <div class="set-row">
            <div>
              <div class="rl">Horizon</div>
              <div class="rd">A dark ridge along the bottom of the window.</div>
            </div>
            <span
              class="switch"
              data-on={sky().horizon ? "true" : "false"}
              role="switch"
              aria-checked={sky().horizon}
              aria-label="Show horizon"
            >
              <input
                type="checkbox"
                class="sr-check"
                checked={sky().horizon}
                onChange={(e) =>
                  appStore.patchSky({ horizon: e.currentTarget.checked })
                }
              />
            </span>
          </div>

          <div class="set-row">
            <div>
              <div class="rl">Motion</div>
              <div class="rd">
                Twinkle and comets. Your system's reduce-motion setting always
                wins.
              </div>
            </div>
            <span
              class="switch"
              data-on={sky().motion ? "true" : "false"}
              role="switch"
              aria-checked={sky().motion}
              aria-label="Sky motion"
            >
              <input
                type="checkbox"
                class="sr-check"
                checked={sky().motion}
                onChange={(e) =>
                  appStore.patchSky({ motion: e.currentTarget.checked })
                }
              />
            </span>
          </div>
        </div>
      </Show>

      {/* Scan preferences — store-level defaults (no backend contract change) */}
      <div class="card set-card">
        <div class="h">Scan preferences</div>

        <div class="set-row">
          <div>
            <div class="rl">Deep scan by default</div>
            <div class="rd">Measure large folders across your drive.</div>
          </div>
          <span
            class="switch"
            data-on={appStore.state.deepScan ? "true" : "false"}
            role="switch"
            aria-checked={appStore.state.deepScan}
            aria-label="Deep scan by default"
          >
            <input
              type="checkbox"
              class="sr-check"
              checked={appStore.state.deepScan}
              onChange={(e) => appStore.setDeepScan(e.currentTarget.checked)}
            />
          </span>
        </div>

        <div class="set-row">
          <div>
            <div class="rl">Default destination</div>
            <div class="rd">Where cleaned items go. Recycle Bin is recoverable.</div>
          </div>
          <div class="segmented" role="group" aria-label="Default destination">
            <button
              type="button"
              data-on={appStore.state.toRecycleBin ? "true" : "false"}
              onClick={() => appStore.setDestination(true)}
            >
              Recycle
            </button>
            <button
              type="button"
              data-on={!appStore.state.toRecycleBin ? "true" : "false"}
              onClick={() => appStore.setDestination(false)}
            >
              Permanent
            </button>
          </div>
        </div>
      </div>

      {/* Safety tiers legend */}
      <div class="card set-card">
        <div class="h">Safety tiers</div>
        <For each={TIERS}>
          {(tier) => (
            <div class="tierline">
              <span class="dot" style={{ background: tier.color }} aria-hidden="true" />
              <div>
                <div class="tl" style={{ color: tier.color }}>
                  {tier.label}
                </div>
                <div class="td">{tier.detail}</div>
              </div>
            </div>
          )}
        </For>
      </div>

      {/* About */}
      <div class="card set-card">
        <div class="h">About</div>
        <div class="about-name">
          Safai <span class="ver">{APP_VERSION_LABEL}</span>
        </div>
        <div class="about-desc">
          A calm, developer-focused disk cleanup tool. Everything goes to the
          Recycle Bin by default, nothing is deleted without confirmation.
        </div>
      </div>

      {/* Lifetime stats — counter-only reset */}
      <div class="card set-card">
        <div class="h">Lifetime stats</div>
        <div class="set-row flush">
          <div class="rl-wrap">
            <div class="rl">Reset lifetime stats</div>
            <div class="rd">
              Clears your reclaimed total and cleanup history. Counters only — no
              files are deleted or changed.
            </div>
          </div>
          <div class="set-actions">
            <Show when={confirming()}>
              <button
                type="button"
                class="link-quiet"
                onClick={() => setConfirming(false)}
              >
                Cancel
              </button>
            </Show>
            <button
              type="button"
              class="btn btn-ghost btn-sm"
              classList={{ "btn-confirm-danger": confirming() }}
              onClick={resetStats}
            >
              {confirming() ? "Are you sure?" : "Reset lifetime stats"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

export default Settings;
