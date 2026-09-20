/**
 * Copy the release CLI binary into npm/safai/bin before pack/publish.
 * Run from repo root: node npm/safai/scripts/prepare-bin.mjs
 * Or via npm prepack inside this package (cwd = package dir).
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const pkgRoot = path.resolve(__dirname, "..");
const repoRoot = path.resolve(pkgRoot, "../..");
const src = path.join(repoRoot, "target", "release", "safai.exe");
const destDir = path.join(pkgRoot, "bin");
const dest = path.join(destDir, "safai.exe");

if (!fs.existsSync(src)) {
  console.error(
    `Missing ${src}\nBuild first: cargo build -p safai-cli --release`,
  );
  process.exit(1);
}

fs.mkdirSync(destDir, { recursive: true });
fs.copyFileSync(src, dest);
const mb = (fs.statSync(dest).size / (1024 * 1024)).toFixed(2);
console.log(`Prepared ${dest} (${mb} MiB)`);
