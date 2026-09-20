# Safai CLI reference

## Categories (scan report)

- `packageCache` — uv, npm, pnpm, cargo registry, etc.
- `editorStorage` — VS Code / Cursor workspace and global storage
- `buildArtifact` — `node_modules`, `target`, `.next`, `dist`, …
- `temp` — OS / app temp directories
- `model` — downloaded ML weights
- `browser` — Playwright browsers, etc.
- `other` — large-folder discovery and leftovers

## Example agent transcript

```text
User: My C: drive is full. Can you find safe stuff to delete?

Agent: Running safai scan…
(shell) cargo run -p safai-cli -- scan --progress

Agent: Found ~48 GB reclaimable. Biggest Safe wins:
- npm cache — 12.1 GB (id: …)
- cargo registry — 8.4 GB (id: …)
- old node_modules under Projects — 6.2 GB (id: …)
Caution items skipped unless you name them.

Delete the three Safe items above to Recycle Bin (~26.7 GB)?

User: Yes, those three.

Agent: (shell) safai preview --ids a,b,c
Shows plan + confirm_token.

Agent: (shell) safai delete --ids a,b,c --token <token> --yes
Done — reclaimed N bytes; M skipped if any.
```

## Exit codes / error codes

| Exit | `code` | Meaning |
|------|--------|---------|
| 2 | `missing_ids`, `session_missing`, `session_write`, `serialize` | Bad args / missing scan |
| 3 | `confirmation_required` | `delete` without `--yes` |
| 3 | `invalid_token` | Token missing or does not match ids — re-run preview |

## Build & PATH

```bash
cargo build -p safai-cli --release
# Windows: target\release\safai.exe
```

Desktop app (`npm run tauri dev`) is unchanged and unrelated to this CLI path.
