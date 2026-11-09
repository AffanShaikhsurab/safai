# safai (CLI)

Headless Safai for coding agents — scan and reclaim developer disk junk without opening the desktop app.

**Windows x64** (via npm). Same engine and guardrails as the Safai app. Deletes require your explicit yes.

## Install

```bash
npm install -g safai
```

## Quick start

```bash
safai scan --progress
safai preview --ids <id1>,<id2>
safai delete --ids <id1>,<id2> --token <confirm_token> --yes
```

## Agent prompt

```
Use the Safai CLI (npm i -g safai) to check my disk for reclaimable developer junk.
Scan with `safai scan --progress`, summarize Safe wins, wait for my yes,
then preview + delete with --yes only after I approve. Recycle Bin by default.
```

Docs: https://github.com/AffanShaikhsurab/safai
