---
name: safai-release
description: >-
  Builds Safai's Windows Tauri installer and publishes a GitHub Release so
  users can download the .exe. Use when asked to release, ship an installer,
  publish a version, or upload a Windows binary for Safai.
---

You are the Safai release agent. Your job is to produce a downloadable Windows
installer and attach it to a GitHub Release.

## Repo

Work in the Safai project root (Tauri + SolidJS). Product version lives in
`src-tauri/tauri.conf.json` (`version`) and `package.json`.

## Steps

1. Confirm `gh auth status` works and `origin` points at the GitHub repo.
2. Ensure the working tree for release assets is committed (or note uncommitted
   branding-only changes). Do not force-push.
3. Build the Windows bundle:

```powershell
npm install
npm run tauri build
```

4. Locate artifacts under `src-tauri/target/release/bundle/`:
   - Prefer NSIS `.exe` installer under `nsis/`
   - Also accept MSI under `msi/` if present
5. Create or update a GitHub release tagged `vX.Y.Z` matching the app version:

```powershell
gh release create "vX.Y.Z" `
  --title "Safai vX.Y.Z" `
  --notes "## Downloads`n- Windows installer (.exe)`n`nInstall and run Safai to reclaim developer disk space." `
  path\to\Safai_X.Y.Z_x64-setup.exe
```

If the tag already exists, upload assets with `gh release upload` instead of
recreating the release.

6. Return the release URL and the direct download URL for the `.exe`.

## Rules

- Never commit secrets or edit git config.
- Never use `--force` on main/master.
- If the build fails, diagnose (Rust toolchain, WebView2, missing deps) and fix
  or report clearly — do not invent a fake binary.
- Prefer a single clear Windows `.exe` installer as the primary download asset.
