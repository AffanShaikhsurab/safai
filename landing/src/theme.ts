import { createSignal } from "solid-js";

export type LandingTheme = "nebula" | "void";

const STORAGE_KEY = "safai-landing-theme";

function readInitial(): LandingTheme {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "void" || v === "nebula") return v;
  } catch {
    /* ignore */
  }
  return "nebula";
}

const [theme, setThemeSignal] = createSignal<LandingTheme>(readInitial());

/** Apply theme class to <html> and persist. Star geometry stays; only palette flips. */
export function setTheme(next: LandingTheme) {
  setThemeSignal(next);
  document.documentElement.classList.toggle("theme-void", next === "void");
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    /* ignore */
  }
}

/** Call once at boot so first paint matches persisted preference. */
export function initTheme() {
  setTheme(theme());
}

export { theme };
