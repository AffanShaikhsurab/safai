// Render an HTML file to a PNG using whichever Chromium-based browser is
// already installed, at 2x for crispness.
//
//   node scripts/render-asset.mjs <input.html> <output.png> <width> <height>
//
// Why not Puppeteer: it would add a ~180MB dev dependency and download its own
// Chromium, to do something Edge and Chrome already expose via
// `--headless --screenshot`. Every Windows machine that can run this app already
// has Edge.
//
// The browser gets a throwaway --user-data-dir so this never touches (or is
// blocked by) a running browser profile.

import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const CANDIDATES = [
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Google/Chrome/Application/chrome.exe",
  "C:/Program Files (x86)/Google/Chrome/Application/chrome.exe",
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
];

const [input, output, width = "1280", height = "440"] = process.argv.slice(2);

if (!input || !output) {
  console.error(
    "usage: node scripts/render-asset.mjs <input.html> <output.png> [width] [height]",
  );
  process.exit(2);
}

const browser = CANDIDATES.find((p) => existsSync(p));
if (!browser) {
  console.error(
    "No Chromium-based browser found. Install Edge or Chrome, or add its path to CANDIDATES.",
  );
  process.exit(1);
}

const src = pathToFileURL(resolve(input)).href;
const dst = resolve(output);
const profile = mkdtempSync(join(tmpdir(), "safai-shot-"));

execFileSync(
  browser,
  [
    "--headless=new",
    "--disable-gpu",
    "--hide-scrollbars",
    // 2x so the committed PNG stays sharp on high-DPI displays.
    "--force-device-scale-factor=2",
    `--window-size=${width},${height}`,
    `--screenshot=${dst}`,
    `--user-data-dir=${profile}`,
    // Lets the inline script finish painting before the frame is captured.
    "--virtual-time-budget=4000",
    src,
  ],
  { stdio: "ignore" },
);

if (!existsSync(dst)) {
  console.error(`Render produced no file at ${dst}`);
  process.exit(1);
}

const kb = (statSync(dst).size / 1024).toFixed(1);
console.log(
  `${output}  ${Number(width) * 2}x${Number(height) * 2}  ${kb} KB  (via ${browser.split(/[/\\]/).pop()})`,
);
