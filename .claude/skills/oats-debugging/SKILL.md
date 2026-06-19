---
name: oats-debugging
description: Use when reproducing or diagnosing runtime behavior in the running oats desktop app — driving and inspecting its windows via the oats-desktop MCP server (execute JS, read DOM, invoke backend commands). Pairs with systematic-debugging.
---

# Debugging the running oats app via MCP

oats ships a Tauri MCP server (`tauri-plugin-mcp`) that lets you drive the live app —
list windows, screenshot, execute JS in a webview, and call backend commands. Use it to
verify a fix in the real app, not just in unit tests. Pair this with superpowers'
`systematic-debugging` for the methodology.

## Enable the server

It is registered in `.mcp.json` as **`oats-desktop`** but disabled by default in
`.claude/settings.local.json` (`disabledMcpjsonServers`). Remove it from that list (or
approve the server when prompted) to use it. The app must be launched with the MCP
feature:

```
npm run tauri:dev:debug      # = tauri dev --features mcp
```

Run it in the **background with the sandbox disabled** (cargo must compile and the GUI /
network must spawn). It's a never-exiting dev server — don't wait on it to "finish".

## Windows (what you'll see)

Routes live in `src/main.ts`. See `oats-architecture` for the full map. Quick version:
- `settings` — the settings window (pre-created hidden).
- `library` — titled **"Meetings"**; the main user-facing window.
- `main` — **headless/blank** BootstrapView; a blank screenshot of it is expected.

## execute_js gotchas (these will bite you)

- **Hidden WKWebViews are JS-suspended on macOS** — `execute_js` times out until the
  window is **shown and focused**. Show the window first.
- **Backend bridge**: `window.__TAURI__` is **not** exposed (the app uses ESM
  `@tauri-apps/api`). Call commands via
  `window.__TAURI_INTERNALS__.invoke('cmd_name', args)`.
- **No top-level `await`** — make the last expression a promise; the tool resolves it.
- A blank window with no JS bridge usually means the **Vue bundle failed to load** —
  check the vite dev log for an unresolved import (the router statically imports every
  view, so one bad import breaks the whole graph).

## Typical loop

1. Launch `tauri:dev:debug` in the background.
2. List windows; `show` the target window before any `execute_js`.
3. Inspect via JS / screenshot; reproduce the issue.
4. Drive the backend with `__TAURI_INTERNALS__.invoke(...)` to isolate frontend vs Rust.
5. Once root cause is found, fix per `oats-vue` / `oats-tauri` and re-verify here.
