#!/usr/bin/env node
"use strict";

const { spawn } = require("child_process");
const fs = require("fs");
const path = require("path");

function binaryPath() {
  if (process.platform !== "win32") {
    console.error(
      "safai CLI via npm is Windows-only for now. See https://github.com/AffanShaikhsurab/safai",
    );
    process.exit(1);
  }
  const name = "safai.exe";
  const candidate = path.join(__dirname, name);
  if (!fs.existsSync(candidate)) {
    console.error(
      `safai binary missing at ${candidate}. Reinstall with: npm install -g safai`,
    );
    process.exit(1);
  }
  return candidate;
}

const child = spawn(binaryPath(), process.argv.slice(2), {
  stdio: "inherit",
  windowsHide: true,
});

child.on("error", (err) => {
  console.error(`failed to start safai: ${err.message}`);
  process.exit(1);
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
  } else {
    process.exit(code ?? 1);
  }
});
