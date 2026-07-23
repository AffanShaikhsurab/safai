import { type Component } from "solid-js";
import type { SafetyTier } from "../lib/types";

// Tier colors: Safe=sky blue, Review=amber, Caution=rose.
const TIER_LABEL: Record<SafetyTier, string> = {
  safe: "Safe",
  review: "Review",
  caution: "Caution",
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
  return (
    <span
      class="pill inline-flex items-center gap-1.5 !py-0.5 !text-[0.7rem] font-medium tracking-wide"
      classList={{
        "!border-sky-400/40 !bg-sky-400/10 text-sky-300":
          props.tier === "safe",
        "!border-amber-400/40 !bg-amber-400/10 text-amber-300":
          props.tier === "review",
        "!border-rose-400/50 !bg-rose-400/10 text-rose-300":
          props.tier === "caution",
      }}
    >
      <span
        class="h-1.5 w-1.5 rounded-full"
        classList={{
          "bg-sky-300": props.tier === "safe",
          "bg-amber-300": props.tier === "review",
          "bg-rose-300": props.tier === "caution",
        }}
        style={{
          "box-shadow":
            props.tier === "safe"
              ? "0 0 6px rgba(110,168,255,0.9)"
              : props.tier === "review"
                ? "0 0 6px rgba(251,191,36,0.9)"
                : "0 0 6px rgba(251,113,133,0.9)",
        }}
        aria-hidden="true"
      />
      {TIER_LABEL[props.tier]}
    </span>
  );
};

export default TierBadge;
