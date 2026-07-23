import { type Component, For, Show, createSignal } from "solid-js";
import { appStore } from "../state/store";
import { DEFAULT_STATS, saveStats } from "../lib/stats";
import { type ThemeName } from "../lib/prefs";

interface ThemeOption {
  id: ThemeName;
  label: string;
  detail: string;
  swatch: string;
}

const THEMES: ThemeOption[] = [
  {
    id: "nebula",
    label: "Nebula",
    detail: "Night sky — stars, glow & comets",
    swatch:
      "radial-gradient(circle at 15% 60%, #2a56d4, transparent 50%), radial-gradient(circle at 80% 20%, #1a2a5a, transparent 45%), #050814",
  },
  {
    id: "void",
    label: "Void",
    detail: "Charcoal dark — hairline dividers, dim comets",
    swatch: "linear-gradient(90deg, #0a0a0a 48%, #1a1a1a 49%, #0a0a0a 50%), #0a0a0a",
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
    color: "var(--mint-strong)",
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
    detail: "Your own data — never pre-selected, always your call.",
  },
];

/**
 * Settings — Appearance (the only place the theme switches), Scan preferences
 * (store-level defaults, persisted), the Safety tiers legend, About, and a
 * deliberately two-step, counter-only "reset lifetime stats".
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

  return (
    <div class="set-wrap animate-rise">
      <div>
        <div class="set-title">Settings</div>
        <div class="set-sub">Appearance, scanning, safety and about.</div>
      </div>

      {/* Appearance — the sole theme picker */}
      <div class="card set-card">
        <div class="h">Appearance</div>
        <div class="theme-grid">
          <For each={THEMES}>
            {(opt) => (
              <button
                type="button"
                class="theme-opt"
                data-sel={appStore.state.theme === opt.id ? "true" : "false"}
                aria-pressed={appStore.state.theme === opt.id}
                onClick={() => appStore.setTheme(opt.id)}
              >
                <span class="sw" style={{ background: opt.swatch }} aria-hidden="true" />
                <div>
                  <div class="n">{opt.label}</div>
                  <div class="d">{opt.detail}</div>
                </div>
              </button>
            )}
          </For>
        </div>
      </div>

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
          Safai <span class="ver">v0.1.0</span>
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
