import { type Component } from "solid-js";
import type { SafetyTier } from "../lib/types";

/**
 * The safety-tier pill.
 *
 * Colours come from the theme tokens (`--ok` / `--amber` / `--rose`) rather than
 * fixed Tailwind classes. That matters: Void desaturates the whole palette, and
 * a hardcoded `sky-300` badge would stay vividly blue in a monochrome theme —
 * the one obviously-broken element on the screen.
 *
 * The tiers keep a trace of hue in every theme on purpose. Safety information is
 * the one thing that must not be conveyed by brightness alone.
 */
const TIER_LABEL: Record<SafetyTier, string> = {
  safe: "Safe",
  review: "Review",
  caution: "Caution",
};

/** Token that carries each tier's colour, per theme. */
const TIER_VAR: Record<SafetyTier, string> = {
  safe: "var(--ok)",
  review: "var(--amber)",
  caution: "var(--rose)",
};

// Shared tier → aura class helper so category cards can tint their glow with
// the same mapping the badge uses. (Presentational only.)
const TIER_AURA: Record<SafetyTier, string> = {
  safe: "aura-safe",
  review: "aura-review",
  caution: "aura-caution",
};

export function tierAura(tier: SafetyTier): string {
  return TIER_AURA[tier] ?? TIER_AURA.safe;
}

const TierBadge: Component<{ tier: SafetyTier }> = (props) => {
  const color = () => TIER_VAR[props.tier];

  return (
    <span
      class="tier-pill"
      data-tier={props.tier}
      style={{ color: color(), "border-color": color() }}
    >
      <span class="tier-dot" style={{ background: color() }} aria-hidden="true" />
      {TIER_LABEL[props.tier]}
    </span>
  );
};

export default TierBadge;
