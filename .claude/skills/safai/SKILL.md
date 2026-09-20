---
name: safai
description: >-
  Scan and reclaim developer disk junk with the Safai CLI (caches, node_modules,
  build artifacts). Use when the user mentions disk space, full drive, cleanup,
  reclaim space, Safai, node_modules, package caches, or wants an agent to scan
  without opening the desktop app. Deletes only after explicit user permission.
---

# Safai (agent CLI)

Headless disk cleanup for developer machines. Prefer the **CLI**, not the Tauri UI.

## Prerequisites

```bash
npm install -g safai
```

Fallback from this repo: `cargo build -p safai-cli --release` or `cargo run -p safai-cli -- …`.

## Workflow (required)

1. **Scan** — run `safai scan` (optional `--root`, `--quick`).
2. **Summarize** — list largest **Safe** wins with sizes; mention Review/Caution separately.
3. **Ask** — propose specific ids; wait for an explicit user yes in chat.
4. **Preview** — `safai preview --ids …` and show the plan.
5. **Confirm again** if needed, then **delete** with `--token` from preview and `--yes`.

Never pass `--yes` without clear user approval in this conversation. Never invent paths — delete by **ids** from the last scan only. Default destination is Recycle Bin; use `--permanent` only if the user asked for permanent deletion.

## Commands

```bash
safai roots
safai detect-tools
safai drive-info [--mount C:]
safai scan [--root PATH]... [--quick] [--out PATH] [--progress]
safai preview --ids id1,id2
safai delete --ids id1,id2 --token <confirm_token> --yes [--permanent]
```

All successful output is JSON on stdout. Errors are JSON with `error` + `code` (non-zero exit).

Session file (default): `%LOCALAPPDATA%/safai/last-scan.json`. Preview stores a one-shot `confirm_token`; delete refuses without a matching `--token` and `--yes`.

## Safety tiers

| Tier | Meaning | Agent behavior |
|------|---------|----------------|
| Safe | Regenerates; pre-selected in UI | Prefer these in proposals |
| Review | Regenerable but confirm | Include only if user wants |
| Caution | User data / not pure cache | Only if user names the item |

Guardrails (engine): deletes only under allowed scan roots; unknown ids are blocked.

## Talking to the user

Keep findings short: top reclaimable items, total bytes, tier mix. Ask clearly, e.g.:

> Delete these 4 Safe items (~12.3 GB) to the Recycle Bin?

Then run preview → show totals → on explicit yes → delete with token + `--yes`.

## More detail

See [reference.md](reference.md) for categories, example transcripts, and exit codes.
