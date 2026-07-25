// The app version, injected at build time from package.json by vite.config.ts.
//
// Anything user-visible must read it from here rather than hardcoding a string.
// It had already drifted once — the About panel said v0.1.0 while both manifests
// said 0.1.2 — and a version the UI reports incorrectly is worse than none,
// because bug reports then cite the wrong build.
//
// `vitest` runs through the same Vite config, so the define is present in tests
// too. The fallback only matters if this module is ever loaded by a bundler that
// doesn't apply the define.
declare const __APP_VERSION__: string;

export const APP_VERSION: string =
  typeof __APP_VERSION__ === "string" ? __APP_VERSION__ : "0.0.0";

/** Display form, e.g. `v0.2.0`. */
export const APP_VERSION_LABEL = `v${APP_VERSION}`;
