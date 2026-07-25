import { type Component, createEffect, onCleanup, onMount } from "solid-js";
import { appStore } from "../state/store";
import type { SkyPrefs, ThemeName } from "../lib/prefs";

/**
 * The night sky, painted on a low-resolution canvas and scaled up with
 * `image-rendering: pixelated`.
 *
 * ## Why a canvas and not CSS
 *
 * This replaced ~200 lines of hand-authored `radial-gradient` starfields plus
 * comet keyframes. Two things the CSS version could not do, both of which carry
 * the look:
 *
 * 1. **Star brightness distribution.** A believable sky is mostly very faint
 *    pinpoints with a few bright ones. That's a `pow(random, 2.6)` curve. A
 *    fixed list of gradient stops gives an evenly-lit, obviously fake field.
 * 2. **A tapered comet trail.** A CSS gradient tapers linearly, which reads as
 *    a laser pointer. Real meteors concentrate their light near the head; here
 *    each trail pixel is placed individually on a `^1.9` falloff.
 *
 * Rendering at 1/3 scale also means the pixelation is *real* — every star is
 * one canvas pixel blown up to a 3px square, and the gradient banding is
 * genuine low-bit-depth banding rather than a texture faked on top of a smooth
 * ramp. That holds at any pixel size, which is what makes the size adjustable.
 *
 * ## Cost
 *
 * At pixel size 3 on a 1100x720 window the buffer is roughly 367x240 — about
 * 88k pixels, with ~200 stars drawn as `fillRect` calls. Negligible. Under
 * Pulsar the component renders nothing at all, and with motion off it paints a
 * single frame and never starts a loop.
 */

/** Per-theme sky palette. Pulsar has none — it doesn't render a sky. */
interface SkyPalette {
  /** Vertical gradient: top, middle (42%), bottom. */
  grad: [string, string, string];
  /** Low-corner airglow, as `[r, g, b, alpha]`. */
  haze: [number, number, number, number];
  starBright: [number, number, number];
  starDim: [number, number, number];
  starArm: [number, number, number];
  trail: [number, number, number];
  head: [number, number, number];
  headHalo: [number, number, number];
  headRing: [number, number, number];
  ridge: string;
  ridgeGlow: [number, number, number];
}

const PALETTES: Record<"nebula" | "void", SkyPalette> = {
  // Deep navy, darkest at the top. Deliberately not near-black: a real night
  // sky still reads blue in its darkest corner.
  nebula: {
    grad: ["#04061a", "#0a1132", "#15245a"],
    haze: [70, 115, 205, 0.22],
    starBright: [255, 252, 255],
    starDim: [226, 238, 255],
    starArm: [190, 215, 255],
    trail: [216, 234, 255],
    head: [255, 255, 255],
    headHalo: [226, 240, 255],
    headRing: [160, 200, 255],
    ridge: "#050c0c",
    ridgeGlow: [24, 58, 44],
  },
  // Same sky with the hue removed, so it reads as a monochrome print of the
  // same photograph rather than a dimmer Nebula.
  void: {
    grad: ["#070708", "#0d0d0f", "#191919"],
    haze: [150, 150, 155, 0.16],
    starBright: [255, 255, 255],
    starDim: [222, 222, 226],
    starArm: [200, 200, 204],
    trail: [232, 232, 236],
    head: [255, 255, 255],
    headHalo: [238, 238, 240],
    headRing: [190, 190, 194],
    ridge: "#080808",
    ridgeGlow: [48, 48, 50],
  },
};

interface Star {
  x: number;
  y: number;
  b: number;
  big: boolean;
  tw: boolean;
  ph: number;
  sp: number;
}

interface Comet {
  x: number;
  y: number;
  dx: number;
  dy: number;
  len: number;
  peak: number;
  age: number;
}

/** Comet visible lifetime, in frames. */
const COMET_LIFE = 200;
/** Max concurrent comets. Two is already generous for "rare". */
const MAX_COMETS = 2;

const rand = (a: number, b: number) => a + Math.random() * (b - a);
const rgba = (c: readonly [number, number, number], a: number) =>
  `rgba(${c[0]},${c[1]},${c[2]},${a.toFixed(3)})`;

const PixelSky: Component = () => {
  let canvas: HTMLCanvasElement | undefined;

  onMount(() => {
    // Everything below is local to the mount so the effect can rebuild state
    // without leaking a previous animation frame.
    let ctx: CanvasRenderingContext2D | null = null;
    /** Which canvas `ctx` belongs to, so a stale context can never be reused. */
    let ctxOwner: HTMLCanvasElement | null = null;
    /**
     * Buffer size + density. The starfield is only regenerated when this
     * changes, so a palette switch preserves the exact sky.
     */
    let geometryKey = "";
    /** Set by `resize` when it rebuilt the starfield, so in-flight comets whose
     *  coordinates are now meaningless get dropped. */
    let geometryChanged = false;
    let w = 0;
    let h = 0;
    let stars: Star[] = [];
    let comets: Comet[] = [];
    let nextComet = 0;
    let raf: number | null = null;
    let observer: ResizeObserver | null = null;

    const palette = (): SkyPalette =>
      PALETTES[appStore.state.theme === "void" ? "void" : "nebula"];

    function buildStars(prefs: SkyPrefs) {
      // pow(r, 2.6) is what produces "mostly faint, a few bright". A flat
      // random() gives an evenly grey field that reads as noise.
      const count = Math.round(((w * h) / 240) * prefs.density);
      stars = new Array(count);
      for (let i = 0; i < count; i++) {
        const b = Math.pow(Math.random(), 2.6);
        stars[i] = {
          x: Math.floor(Math.random() * w),
          y: Math.floor(Math.random() * h),
          b: 0.16 + b * 0.84,
          big: b > 0.82,
          // Only a third twinkle. All of them shimmering reads as television
          // static, not a sky.
          tw: Math.random() < 0.34,
          ph: Math.random() * Math.PI * 2,
          sp: rand(0.6, 1.6),
        };
      }
    }

    /**
     * Size the low-resolution buffer from the element's laid-out size and
     * rebuild the starfield.
     *
     * Returns `false` when the element has no layout yet (hidden, or measured
     * before the browser has applied a class change). Callers must not paint on
     * a `false` — the buffer would be wrong.
     */
    function resize(prefs: SkyPrefs): boolean {
      if (!canvas || !ctx) return false;
      const rect = canvas.getBoundingClientRect();
      if (rect.width < 1 || rect.height < 1) return false;
      const before = geometryKey;
      // All maths happens in "sky pixels" (1 unit == prefs.pixel screen px) so
      // stars land on exact pixel boundaries. Computing in screen space and
      // dividing would reintroduce the soft half-pixel edges we're avoiding.
      w = Math.max(8, Math.ceil(rect.width / prefs.pixel));
      h = Math.max(8, Math.ceil(rect.height / prefs.pixel));
      canvas.width = w;
      canvas.height = h;
      // Setting canvas.width resets the whole 2D state, so this has to be
      // re-applied after every resize, not once at setup.
      ctx.imageSmoothingEnabled = false;

      // Star positions are random, so rebuilding them reshuffles the sky. Only
      // do that when the geometry actually changed — otherwise switching
      // Nebula <-> Void would scatter a brand-new starfield, when the whole
      // point of those two themes is that they're one sky in two palettes.
      geometryKey = `${w}x${h}:${prefs.density}`;
      if (geometryKey !== before || stars.length === 0) {
        buildStars(prefs);
        geometryChanged = true;
      }
      return true;
    }

    function spawnComet() {
      const fromLeft = Math.random() < 0.45;
      const ang = fromLeft ? rand(0.5, 0.72) : rand(2.42, 2.64);
      const speed = rand(0.5, 0.85);
      comets.push({
        x: fromLeft ? rand(-0.05, 0.35) * w : rand(0.6, 1.05) * w,
        y: rand(-0.08, 0.3) * h,
        dx: Math.cos(ang) * speed,
        dy: Math.sin(ang) * speed,
        len: Math.round(rand(0.16, 0.3) * Math.max(w, h)),
        peak: rand(0.6, 0.95),
        age: 0,
      });
    }

    function drawBackdrop(p: SkyPalette) {
      if (!ctx) return;
      const g = ctx.createLinearGradient(0, 0, 0, h);
      g.addColorStop(0, p.grad[0]);
      g.addColorStop(0.42, p.grad[1]);
      g.addColorStop(1, p.grad[2]);
      ctx.fillStyle = g;
      ctx.fillRect(0, 0, w, h);

      // Airglow low in one corner, as in a long-exposure night photo.
      const [hr, hg, hb, ha] = p.haze;
      const haze = ctx.createRadialGradient(
        w * 0.1,
        h * 1.02,
        0,
        w * 0.1,
        h * 1.02,
        w * 0.55,
      );
      haze.addColorStop(0, `rgba(${hr},${hg},${hb},${ha})`);
      haze.addColorStop(1, `rgba(${hr},${hg},${hb},0)`);
      ctx.fillStyle = haze;
      ctx.fillRect(0, 0, w, h);
    }

    function drawStars(p: SkyPalette, t: number, motion: boolean) {
      if (!ctx) return;
      for (const s of stars) {
        let a = s.b;
        if (s.tw && motion) {
          a *= 0.68 + 0.32 * Math.sin(t * 0.0011 * s.sp + s.ph);
        }
        ctx.fillStyle = rgba(s.big ? p.starBright : p.starDim, a);
        ctx.fillRect(s.x, s.y, 1, 1);
        if (s.big) {
          // 2x2 core plus faint arms: the pixel-art way to say "bright"
          // without a blur, which would smear the pixel grid.
          ctx.fillRect(s.x + 1, s.y, 1, 1);
          ctx.fillRect(s.x, s.y + 1, 1, 1);
          ctx.fillRect(s.x + 1, s.y + 1, 1, 1);
          ctx.fillStyle = rgba(p.starArm, a * 0.3);
          ctx.fillRect(s.x - 1, s.y, 1, 1);
          ctx.fillRect(s.x + 2, s.y, 1, 1);
          ctx.fillRect(s.x, s.y - 1, 1, 1);
          ctx.fillRect(s.x, s.y + 2, 1, 1);
        }
      }
    }

    function drawComets(p: SkyPalette) {
      if (!ctx) return;
      for (const c of comets) {
        // Fade in fast, fade out slowly — a meteor's actual light curve.
        const fade =
          Math.min(1, c.age / 14) * Math.max(0, 1 - c.age / (COMET_LIFE - 10));
        if (fade <= 0) continue;

        const m = Math.hypot(c.dx, c.dy) || 1;
        const nx = -c.dx / m;
        const ny = -c.dy / m;

        for (let i = 1; i <= c.len; i++) {
          const px = Math.round(c.x + nx * i);
          const py = Math.round(c.y + ny * i);
          if (px < -2 || py < -2 || px > w + 2 || py > h + 2) continue;
          // ^1.9 falloff concentrates the light near the head. Linear looks
          // like a laser pointer.
          const a = c.peak * fade * Math.pow(1 - i / c.len, 1.9);
          if (a < 0.012) continue;
          ctx.fillStyle = rgba(p.trail, a);
          ctx.fillRect(px, py, 1, 1);
        }

        const hx = Math.round(c.x);
        const hy = Math.round(c.y);
        ctx.fillStyle = rgba(p.head, 0.95 * fade);
        ctx.fillRect(hx, hy, 2, 2);
        ctx.fillStyle = rgba(p.headHalo, 0.5 * fade);
        ctx.fillRect(hx - 1, hy, 1, 2);
        ctx.fillRect(hx + 2, hy, 1, 2);
        ctx.fillRect(hx, hy - 1, 2, 1);
        ctx.fillRect(hx, hy + 2, 2, 1);
        ctx.fillStyle = rgba(p.headRing, 0.2 * fade);
        ctx.fillRect(hx - 2, hy, 1, 2);
        ctx.fillRect(hx + 3, hy, 1, 2);
        ctx.fillRect(hx, hy - 2, 2, 1);
        ctx.fillRect(hx, hy + 3, 2, 1);
      }
    }

    function drawHorizon(p: SkyPalette) {
      if (!ctx) return;
      const base = Math.round(h * 0.86);
      ctx.fillStyle = p.ridge;
      // Deterministic jitter (sin, not random) so the ridge doesn't crawl
      // between frames.
      for (let x = 0; x < w; x++) {
        const j = Math.round(
          Math.sin(x * 0.21) * 1.6 + Math.sin(x * 0.061) * 2.4,
        );
        ctx.fillRect(x, base + j, 1, h - base - j);
      }
      const g = ctx.createLinearGradient(0, base - 4, 0, h);
      g.addColorStop(0, rgba(p.ridgeGlow, 0.5));
      g.addColorStop(1, rgba(p.ridgeGlow, 0));
      ctx.fillStyle = g;
      ctx.fillRect(0, base - 4, w, h - base + 4);
    }

    function paint(t: number, prefs: SkyPrefs, motion: boolean) {
      const p = palette();
      drawBackdrop(p);
      drawStars(p, t, motion);

      if (motion) {
        if (prefs.comets > 0 && t > nextComet && comets.length < MAX_COMETS) {
          spawnComet();
          // Long, irregular gaps. The silence between meteors is what makes one
          // feel like an event rather than a loading animation.
          nextComet = t + rand(3800, 10500) / prefs.comets;
        }
        for (const c of comets) {
          c.x += c.dx;
          c.y += c.dy;
          c.age += 1;
        }
        comets = comets.filter(
          (c) =>
            c.age < COMET_LIFE &&
            c.y < h + c.len &&
            c.x > -c.len &&
            c.x < w + c.len,
        );
      }
      drawComets(p);
      if (prefs.horizon) drawHorizon(p);
    }

    // Re-runs whenever the theme or any sky preference changes. Solid tracks the
    // reads below, so flipping a switch in Settings rebuilds the sky at once.
    createEffect(() => {
      const theme: ThemeName = appStore.state.theme;
      const prefs = { ...appStore.state.sky };

      if (raf !== null) {
        cancelAnimationFrame(raf);
        raf = null;
      }
      // Pulsar draws no sky. The canvas stays mounted (see the note on the
      // return value) but we stop painting and leave it hidden by CSS.
      if (theme === "pulsar" || !canvas) return;

      // The canvas element is never recreated, so its context is stable — but
      // key the cache to the element anyway. Silently painting into a detached
      // canvas is invisible in every log and cost an hour to find once already.
      if (ctxOwner !== canvas) {
        ctx = canvas.getContext("2d", { alpha: false });
        ctxOwner = canvas;
      }
      if (!ctx) return; // No 2D context: the CSS fallback gradient shows through.

      // Respect the OS setting unless the user explicitly turned motion on.
      const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      const motion = prefs.motion && !reduced;

      // Returning from Pulsar un-hides the canvas via a class on <html>. If
      // that hasn't been laid out yet the element still measures 0x0, and
      // sizing the buffer from it would produce an 8x8 sky. Defer a frame and
      // let layout settle rather than painting something wrong.
      if (!resize(prefs)) {
        raf = requestAnimationFrame(() => {
          raf = null;
          if (!resize(prefs)) return;
          start(prefs, motion);
        });
        return;
      }

      start(prefs, motion);
    });

    /** Paint a static frame, or kick off the animation loop. */
    function start(prefs: SkyPrefs, motion: boolean) {
      // Only discard comets when the buffer was rebuilt under them; their
      // positions are in sky-pixel space and would be wrong at a new size. On a
      // plain palette switch a comet mid-flight simply carries on.
      if (geometryChanged) {
        comets = [];
        geometryChanged = false;
      }

      if (!motion) {
        // Single static frame, with one comet mid-flight so the sky still reads
        // as a sky rather than an empty gradient. Reset first: with motion off
        // there's no loop to retire an old comet.
        comets = [];
        if (prefs.comets > 0) {
          spawnComet();
          const c = comets[0];
          c.x += c.dx * 60;
          c.y += c.dy * 60;
          c.age = 40;
        }
        paint(performance.now(), prefs, false);
        return;
      }

      nextComet = performance.now() + 900;
      const loop = (t: number) => {
        paint(t, prefs, true);
        raf = requestAnimationFrame(loop);
      };
      raf = requestAnimationFrame(loop);
    }

    // Window resizes change the buffer size, so the starfield must be rebuilt.
    // Registered once, outside the effect: it has to work with motion off too,
    // and re-registering per effect run leaked observers on every theme change.
    if (canvas && "ResizeObserver" in window) {
      observer = new ResizeObserver(() => {
        if (appStore.state.theme === "pulsar") return;
        const prefs = { ...appStore.state.sky };
        if (!resize(prefs)) return;
        // With motion off nothing repaints on its own, so draw the new size now.
        if (raf === null) paint(performance.now(), prefs, false);
      });
      observer.observe(canvas);
    }

    onCleanup(() => {
      if (raf !== null) cancelAnimationFrame(raf);
      observer?.disconnect();
    });
  });

  /**
   * The canvas is mounted for every theme and hidden by CSS under Pulsar,
   * rather than being unmounted with `<Show>`.
   *
   * That's deliberate. Unmounting recreates the element on the way back, and
   * the element and the painting effect live in different reactive scopes — so
   * the effect could run against the previous, detached node and paint into
   * nothing. Keeping one element for the app's lifetime removes that ordering
   * hazard entirely, and an unpainted hidden canvas costs nothing.
   */
  return (
    <>
      {/* Sits behind the canvas so a failed 2D context still shows a night sky
          rather than a flat void. */}
      <div class="sky-fallback" aria-hidden="true" />
      <canvas ref={canvas} class="pixel-sky" aria-hidden="true" />
    </>
  );
};

export default PixelSky;
