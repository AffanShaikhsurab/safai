import {
  type Component,
  type JSX,
  createSignal,
  For,
  onCleanup,
  onMount,
} from "solid-js";
import PixelSky from "./components/PixelSky";
import Wordmark from "./components/Wordmark";
import { setTheme, theme } from "./theme";

const DOWNLOAD =
  "https://github.com/AffanShaikhsurab/safai/releases/latest";
const GITHUB = "https://github.com/AffanShaikhsurab/safai";

const FEATURES: { label: string; body: JSX.Element }[] = [
  {
    label: "See it",
    body: (
      <>
        A clear picture of where your space went — drive usage, category
        breakdown, and how much you've reclaimed.
      </>
    ),
  },
  {
    label: "Scan once",
    body: (
      <>
        Finds package-manager caches, <b>node_modules</b> and build outputs,
        editor data, temps, models, and large folders — for any toolchain.
      </>
    ),
  },
  {
    label: "Safety first",
    body: (
      <>
        Every finding is tagged Safe, Review, or Caution. Safe items are
        pre-selected; anything risky always needs your explicit yes.
      </>
    ),
  },
  {
    label: "Recycle Bin",
    body: (
      <>
        Nothing is gone forever unless you deliberately choose permanent
        deletion. Guardrails keep anything outside your folders untouched.
      </>
    ),
  },
  {
    label: "Automation",
    body: (
      <>
        Daily or weekly from the tray, or when the drive crosses a threshold you
        pick. It waits until you're idle — background I/O so you never feel it.
      </>
    ),
  },
];

const App: Component = () => {
  const [scrolled, setScrolled] = createSignal(false);

  onMount(() => {
    const onScroll = () => setScrolled(window.scrollY > 24);
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    onCleanup(() => window.removeEventListener("scroll", onScroll));
  });

  return (
    <>
      <PixelSky />

      <div class="sky-content">
        <header class="lp-nav" classList={{ scrolled: scrolled() }}>
          <a class="lp-nav-mark" href="#top" aria-label="Safai home">
            <Wordmark cell={3} gap={1} track={5} />
          </a>
          <div class="lp-nav-actions">
            <a class="lp-nav-link" href="#how">
              How it works
            </a>
            <a class="lp-nav-link" href="#speed">
              Speed
            </a>
            <div
              class="lp-theme"
              role="group"
              aria-label="Theme"
            >
              <button
                type="button"
                data-on={theme() === "nebula"}
                aria-pressed={theme() === "nebula"}
                onClick={() => setTheme("nebula")}
              >
                Nebula
              </button>
              <button
                type="button"
                data-on={theme() === "void"}
                aria-pressed={theme() === "void"}
                onClick={() => setTheme("void")}
              >
                Void
              </button>
            </div>
            <a class="sky-btn" href={DOWNLOAD} style={{ padding: "9px 14px", "font-size": "9.5px" }}>
              Download
            </a>
          </div>
        </header>

        {/* ---- Hero ---- */}
        <section class="lp-hero" id="top">
          <div class="lp-hero-fade" aria-hidden="true" />
          <div class="lp-col lp-hero-inner">
            <div class="lp-hero-brand animate-rise">
              <Wordmark cell={11} gap={2} track={16} />
            </div>
            <h1 class="sky-display animate-rise d1">
              Reclaim your disk.
              <br />
              Safely.
            </h1>
            <p class="sky-lede animate-rise d2">
              Finds the caches, build output, and junk your tools quietly pile up —
              and tells you what's safe to clear.
            </p>
            <div class="sky-acts animate-rise d3">
              <a class="sky-btn" href={DOWNLOAD}>
                Download for Windows
              </a>
              <a class="sky-btn quiet" href={GITHUB}>
                View source
              </a>
            </div>
          </div>
        </section>

        {/* ---- Problem ---- */}
        <section class="lp-section" id="problem">
          <div class="lp-col">
            <div class="sky-eyebrow">The problem</div>
            <div class="sky-display sm">Your drive is slowly drowning.</div>
            <p class="sky-lede">
              <b>node_modules</b> you forgot about. A <b>target/</b> bigger than the
              project. Caches from npm, pnpm, uv, cargo, gradle that grow forever.
              Editor databases that balloon to tens of gigabytes.
            </p>
            <p class="sky-lede" style={{ "margin-top": "16px" }}>
              You know space is disappearing — figuring out what's safe means digging
              through AppData and hoping you don't break a project. Most people just
              don't. Until the disk is full.
            </p>
          </div>
        </section>

        {/* ---- How ---- */}
        <section class="lp-section" id="how">
          <div class="lp-col">
            <div class="sky-eyebrow">What Safai does</div>
            <div class="sky-display sm">
              <span class="sky-live" aria-hidden="true" />
              Scan. Review. Reclaim.
            </div>
            <p class="sky-lede" style={{ "margin-bottom": "32px" }}>
              One app for the places developer junk actually hides — with safety
              tiers built in, and the Recycle Bin as the default landing pad.
            </p>

            <div class="sky-list">
              <For each={FEATURES}>
                {(f) => (
                  <div class="sky-row">
                    <div class="label">{f.label}</div>
                    <div class="body">{f.body}</div>
                  </div>
                )}
              </For>
            </div>

            <div class="tier-row" aria-label="Safety tiers">
              <span class="tier-pill" style={{ color: "var(--ok)" }}>
                <span class="tier-dot" style={{ background: "var(--ok)" }} />
                Safe
              </span>
              <span class="tier-pill" style={{ color: "var(--amber)" }}>
                <span class="tier-dot" style={{ background: "var(--amber)" }} />
                Review
              </span>
              <span class="tier-pill" style={{ color: "var(--rose)" }}>
                <span class="tier-dot" style={{ background: "var(--rose)" }} />
                Caution
              </span>
            </div>
          </div>
        </section>

        {/* ---- Speed ---- */}
        <section class="lp-section" id="speed">
          <div class="lp-col">
            <div class="sky-eyebrow">Performance</div>
            <div class="sky-display sm">Written in Rust for a reason.</div>
            <p class="sky-lede">
              Measuring a disk means touching millions of files. Anything slower than
              a few seconds turns into an app nobody opens twice.
            </p>

            <div class="lp-perf">
              <div>
                <div class="lp-perf-big">
                  ≈35×
                  <span class="unit">faster than PowerShell</span>
                </div>
                <div class="pxbar" aria-hidden="true">
                  <For each={Array.from({ length: 14 }, (_, i) => i)}>
                    {(i) => (
                      <i
                        data-on={i < 12}
                        data-hot={i >= 10 && i < 12}
                      />
                    )}
                  </For>
                </div>
              </div>
              <div class="lp-perf-compare">
                <div class="row">
                  <span class="name">Get-ChildItem</span>
                  <span class="val">676.4 s</span>
                </div>
                <div class="row win">
                  <span class="name">Safai</span>
                  <span class="val">19.1 s</span>
                </div>
                <div class="row">
                  <span class="name">Same tree</span>
                  <span class="val">5.3M files</span>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* ---- Themes ---- */}
        <section class="lp-section" id="look">
          <div class="lp-col">
            <div class="sky-eyebrow">Made to look at</div>
            <div class="sky-display sm">
              {theme() === "nebula" ? "Nebula" : "Void"}
            </div>
            <p class="sky-lede">
              {theme() === "nebula" ? (
                <>
                  A pixel-art night sky rendered on canvas. Real starfields, a rare
                  comet, a dark horizon. Content sits in a centred column so the sky
                  has room to be seen.
                </>
              ) : (
                <>
                  The same sky with the colour drained out — a monochrome print of the
                  same photograph, not a dimmed copy. Flip the toggle anytime.
                </>
              )}
            </p>
            <div class="sky-acts">
              <button
                type="button"
                class="sky-btn"
                onClick={() => setTheme(theme() === "nebula" ? "void" : "nebula")}
              >
                Switch to {theme() === "nebula" ? "Void" : "Nebula"}
              </button>
            </div>
          </div>
        </section>

        {/* ---- Close ---- */}
        <section class="lp-close" id="download">
          <div class="lp-col">
            <div class="sky-eyebrow">Ready when you are</div>
            <div class="sky-display sm">
              Free up tens of GBs
              <br />
              without a terminal.
            </div>
            <p class="sky-lede">
              Windows-first. Recycle Bin by default. Open source.
            </p>
            <div class="sky-acts">
              <a class="sky-btn" href={DOWNLOAD}>
                Download for Windows
              </a>
              <a class="sky-btn quiet" href={GITHUB}>
                Star on GitHub
              </a>
            </div>
          </div>
        </section>

        <footer class="lp-foot">
          <div class="lp-col" style={{ display: "flex", "flex-wrap": "wrap", "justify-content": "space-between", gap: "16px", width: "100%" }}>
            <span>© {new Date().getFullYear()} Safai</span>
            <div class="lp-foot-links">
              <a href={GITHUB}>GitHub</a>
              <a href={`${GITHUB}/releases`}>Releases</a>
              <a href="#top">Back up</a>
            </div>
          </div>
        </footer>
      </div>
    </>
  );
};

export default App;
