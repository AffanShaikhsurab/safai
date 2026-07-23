import { type Component } from "solid-js";

/**
 * Fixed backdrop behind `.sky-content`.
 *
 * Nebula: dark night sky, quiet starfield, falling comets.
 * Void (`.theme-void`): charcoal field, sparse dim stars, quieter comets.
 */
const SpaceBackground: Component = () => {
  return (
    <div class="space-bg" aria-hidden="true">
      <div class="stars stars-far" />
      <div class="stars stars-near" />
      <div class="comet comet-a" />
      <div class="comet comet-b" />
      <div class="comet comet-c" />
    </div>
  );
};

export default SpaceBackground;
