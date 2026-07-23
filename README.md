<div align="center">

# 🧹 Safai

### Reclaim gigabytes of disk space on your dev machine — safely.

Safai finds the caches, build artifacts, and junk your tools quietly pile up,
tells you what's safe to remove, and cleans it up to the Recycle Bin in a couple
of clicks. No guesswork. No `rm -rf` regrets.

</div>

---

## The problem

If you write code, your drive is slowly drowning.

`node_modules` you forgot about. A `target/` folder that's bigger than your whole
project. `uv`, `npm`, `pnpm`, `gradle`, `bun`, `pip`, and `cargo` caches that grow
forever and never clean themselves. Editor databases and workspace history that
balloon to tens of gigabytes. Downloaded model weights you used once.

You *know* space is disappearing — but figuring out **what** is safe to delete
means digging through `AppData`, guessing at folder names, and hoping you don't
break a project. Most people just... don't. Until the disk is full.

## What Safai does

Safai scans the places developer junk actually hides and shows you a clear,
sorted picture of what you can get back:

- **📊 See where your space went** — a clean dashboard with your drive usage,
  a breakdown by category, and how much you've reclaimed over time.
- **🔍 One-click scan** — finds package-manager caches, `node_modules` and build
  outputs, editor/app data, temp files, downloaded models, and any other large
  folders — for *any* toolchain, not just a hard-coded few.
- **🟢 Safety, built in** — every finding is tagged **Safe** (regenerates on its
  own), **Review**, or **Caution** (your data — never selected for you). Safe
  items are pre-selected; the risky stuff always needs your explicit yes.
- **🗑️ Recycle Bin by default** — nothing is gone forever unless you deliberately
  choose permanent deletion. Guardrails stop anything outside your own folders
  from ever being touched.
- **⏹️ Stop whenever** — happy with what you've found mid-scan? Hit **Stop** and
  review exactly what's there so far.
- **🔔 Get notified** — kick off a scan or cleanup and walk away; Safai pings you
  when it's done.
- **🌌 Made to look at** — a calm, modern UI with two themes: **Nebula** (a deep
  night-sky gradient) and **Cursor Black** (flat true black).

## How it helps

- **Free up 10s of GBs in minutes** without a terminal or a wiki of folder paths.
- **Never delete something you'll regret** — clear safety tiers + Recycle Bin.
- **Works for your stack** — Node, Python, Rust, Go, Java/Gradle, Flutter/Dart,
  and more; plus generic large-folder discovery for everything else.
- **Stays out of your way** — scan, get notified, review the biggest wins first
  (results are sorted largest → smallest), clean, done.

---

## Getting started

Safai is a small native desktop app (built with [Tauri](https://tauri.app),
Rust, and SolidJS). It currently targets **Windows**.

### Prerequisites

- [**Rust**](https://www.rust-lang.org/tools/install) (stable toolchain)
- [**Node.js**](https://nodejs.org) 18+ and npm
- The [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS
  (on Windows: the WebView2 runtime, usually already installed)

### Run it

```bash
# 1. Clone
git clone <your-repo-url> safai
cd safai

# 2. Install frontend dependencies
npm install

# 3. Launch the app in development
npm run tauri dev
```

### Build an installer

```bash
npm run tauri build
```

The installer is produced under `src-tauri/target/release/bundle/`.

---

## Using Safai

1. **Scan** — open **Clean**, pick your scan mode (Quick or Deep), and hit scan.
   Deep scan also hunts for large folders no rule knows about.
2. **Review** — findings are grouped into collapsible sections, sorted biggest
   first. Flip a whole section on, or cherry-pick items. Safe items start
   selected; **Caution** items never do.
3. **Clean** — choose **Recycle Bin** (default, recoverable) or **Permanent**,
   confirm the preview, and Safai does the rest.

Your reclaimed total and history live on the **Dashboard**.

## Is it safe?

Yes — safety is the whole point:

- **Recycle Bin by default** — deletions are recoverable unless you opt into
  permanent removal.
- **Guardrails** — Safai only ever deletes inside your own known locations,
  never system paths.
- **You're always in control** — nothing is removed without a confirmation, and
  a dry-run preview shows exactly what will go.

---

## Contributing

Contributions are welcome! See **[CONTRIBUTING.md](./CONTRIBUTING.md)** to get set
up and learn how to propose changes.

## License

[MIT](./LICENSE) © Safai
