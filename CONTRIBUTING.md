# Contributing to Safai

Thanks for your interest in improving Safai! This guide covers everything you
need to get set up and contribute changes.

## Ground rules

- Be respectful and constructive.
- Keep changes focused — one logical change per pull request.
- Safety is Safai's core promise. Anything that deletes or moves files must stay
  guardrailed and default to the Recycle Bin. Never weaken those defaults.

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable) with `rustfmt` and `clippy`
- [Node.js](https://nodejs.org) 18+ and npm
- The [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS

## Local setup

```bash
git clone <your-fork-url> safai
cd safai
npm install
npm run tauri dev
```

## Project layout (quick orientation)

| Path | What lives there |
| --- | --- |
| `src/` | SolidJS frontend (screens, components, state, helpers) |
| `src-tauri/` | Tauri app + Rust command layer (the thin backend) |
| `crates/safai-core/` | The fast filesystem scanner |
| `crates/safai-rules/` | Cleanup detection engine (rules, categories, scan driver) |

The frontend talks to the backend through a small set of typed commands in
`src/lib/tauri.ts`; the data shapes are mirrored in `src/lib/types.ts`. Keep the
two sides in sync when you change a command.

## Making changes

1. Create a branch: `git checkout -b feat/short-description` (or `fix/…`).
2. Make your change with tests where it makes sense.
3. Run the checks below and make sure they pass.
4. Open a pull request with a clear description of **what** changed and **why**,
   and note what you tested.

## Before you push — run the checks

**Frontend**

```bash
npx tsc --noEmit        # type-check (must be clean)
npm run build           # production build must succeed
```

**Rust**

```bash
cargo fmt --all
cargo clippy --all-targets
cargo test -p safai-core -p safai-rules
```

## Coding style

- **Rust**: idiomatic, `rustfmt`-formatted, clippy-clean. Keep the backend thin —
  scanning/rules logic belongs in the crates, not in the Tauri command layer.
- **SolidJS/TypeScript**: follow Solid idioms — components run once, **never
  destructure props**, use `<For>` / `<Show>` / `<Switch>`, `createStore`, and
  clean up channels/intervals with `onCleanup`. No React patterns.
- Match the existing visual language (design tokens and classes in
  `src/index.css`). Keep the UI calm and uncluttered.

## Commit messages

Short, imperative summaries are preferred, optionally with a type prefix:

```
feat: add per-folder scan timeout
fix: keep partial results when a scan is stopped
docs: clarify Windows build steps
```

## Reporting bugs & requesting features

Open an issue with:

- What you expected vs. what happened (and steps to reproduce).
- Your OS/version and how you built/ran Safai.
- For features: the problem you're trying to solve, not just a proposed solution.

Thanks for helping make Safai better! 🧹
