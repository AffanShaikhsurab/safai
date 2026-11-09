import { type Component, createEffect, onCleanup, onMount } from "solid-js";
import { theme, type LandingTheme } from "../theme";

/**
 * Night sky on a low-res canvas, scaled with `image-rendering: pixelated`.
 * Adapted from the desktop app's PixelSky — same palettes, pow(2.6) stars,
 * tapered comets, deterministic horizon. Theme switches recolour; geometry stays.
 */

interface SkyPalette {
  grad: [string, string, string];
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

const PALETTES: Record<LandingTheme, SkyPalette> = {
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

const COMET_LIFE = 200;
const MAX_COMETS = 2;
const PIXEL = 3;
const DENSITY = 0.55;
const COMETS = 0.5;

const rand = (a: number, b: number) => a + Math.random() * (b - a);
const rgba = (c: readonly [number, number, number], a: number) =>
  `rgba(${c[0]},${c[1]},${c[2]},${a.toFixed(3)})`;

const PixelSky: Component = () => {
  let canvas: HTMLCanvasElement | undefined;

  onMount(() => {
    let ctx: CanvasRenderingContext2D | null = null;
    let ctxOwner: HTMLCanvasElement | null = null;
    let geometryKey = "";
    let geometryChanged = false;
    let w = 0;
    let h = 0;
    let stars: Star[] = [];
    let comets: Comet[] = [];
    let nextComet = 0;
    let raf: number | null = null;
    let observer: ResizeObserver | null = null;

    const palette = (): SkyPalette => PALETTES[theme()];

    function buildStars() {
      const count = Math.round(((w * h) / 240) * DENSITY);
      stars = new Array(count);
      for (let i = 0; i < count; i++) {
        const b = Math.pow(Math.random(), 2.6);
        stars[i] = {
          x: Math.floor(Math.random() * w),
          y: Math.floor(Math.random() * h),
          b: 0.16 + b * 0.84,
          big: b > 0.82,
          tw: Math.random() < 0.34,
          ph: Math.random() * Math.PI * 2,
          sp: rand(0.6, 1.6),
        };
      }
    }

    function resize(): boolean {
      if (!canvas || !ctx) return false;
      const rect = canvas.getBoundingClientRect();
      if (rect.width < 1 || rect.height < 1) return false;
      const before = geometryKey;
      w = Math.max(8, Math.ceil(rect.width / PIXEL));
      h = Math.max(8, Math.ceil(rect.height / PIXEL));
      canvas.width = w;
      canvas.height = h;
      ctx.imageSmoothingEnabled = false;
      geometryKey = `${w}x${h}`;
      if (geometryKey !== before || stars.length === 0) {
        buildStars();
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

    function paint(t: number, motion: boolean) {
      const p = palette();
      drawBackdrop(p);
      drawStars(p, t, motion);
      if (motion) {
        if (COMETS > 0 && t > nextComet && comets.length < MAX_COMETS) {
          spawnComet();
          nextComet = t + rand(3800, 10500) / COMETS;
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
      drawHorizon(p);
    }

    function start(motion: boolean) {
      if (geometryChanged) {
        comets = [];
        geometryChanged = false;
      }
      if (!motion) {
        comets = [];
        if (COMETS > 0) {
          spawnComet();
          const c = comets[0];
          c.x += c.dx * 60;
          c.y += c.dy * 60;
          c.age = 40;
        }
        paint(performance.now(), false);
        return;
      }
      nextComet = performance.now() + 900;
      const loop = (t: number) => {
        paint(t, true);
        raf = requestAnimationFrame(loop);
      };
      raf = requestAnimationFrame(loop);
    }

    createEffect(() => {
      theme(); // re-run on Nebula ↔ Void; palette only
      if (raf !== null) {
        cancelAnimationFrame(raf);
        raf = null;
      }
      if (!canvas) return;
      if (ctxOwner !== canvas) {
        ctx = canvas.getContext("2d", { alpha: false });
        ctxOwner = canvas;
      }
      if (!ctx) return;

      const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      const motion = !reduced;

      if (!resize()) {
        raf = requestAnimationFrame(() => {
          raf = null;
          if (!resize()) return;
          start(motion);
        });
        return;
      }
      start(motion);
    });

    if (canvas && "ResizeObserver" in window) {
      observer = new ResizeObserver(() => {
        const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
        if (!resize()) return;
        if (raf === null) paint(performance.now(), false);
        else if (reduced) paint(performance.now(), false);
      });
      observer.observe(canvas);
    }

    onCleanup(() => {
      if (raf !== null) cancelAnimationFrame(raf);
      observer?.disconnect();
    });
  });

  return (
    <>
      <div class="sky-fallback" aria-hidden="true" />
      <canvas ref={canvas} class="pixel-sky" aria-hidden="true" />
    </>
  );
};

export default PixelSky;
