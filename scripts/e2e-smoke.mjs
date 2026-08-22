// Deterministic E2E smoke test for the oats desktop app.
//
// Drives the LIVE app through the tauri-plugin-mcp unix socket — the same
// primitives the MCP server exposes (manage_window, execute_js) — but WITHOUT an
// LLM in the loop, so it's fast, free, and repeatable. It is the regression gate
// for the autonomous-refactor workflow (see issue "Autonomous refactoring in CI,
// gated by an E2E smoke test").
//
// Prereqs (the workflow handles these):
//   - the app is running with `--features mcp` (debug build; the MCP plugin is
//     #[cfg(all(debug_assertions, feature = "mcp"))]).
//   - TAURI_MCP_IPC_PATH points at its socket (~/.ariso/run/oats-mcp.sock). The
//     socket client reads this env at import time, so it MUST be set before node
//     loads this module.
//
// It is intentionally audio/TCC-free: no recording, no transcription. It only
// proves the app booted, the Rust↔webview bridge answers, and the Meetings window
// renders. Writes /tmp/e2e-smoke-result.json and exits non-zero on any failure.
//
// Note: we reuse the package's own tool handlers (registered into a stub server)
// rather than re-implementing the action→socket mapping. That couples us to the
// package's internal build path; it's pinned at ^0.1.0.

import { writeFileSync } from "node:fs";
import { registerAllTools } from "tauri-plugin-mcp-server/build/tools/index.js";

const RESULT_PATH = process.env.E2E_RESULT_PATH || "/tmp/e2e-smoke-result.json";

// Capture each tool's handler by registering into a stub `server`. The package
// calls server.tool(name, desc, schema, annotations, handler); grab the last
// function arg so we don't depend on the exact arity.
const handlers = {};
registerAllTools({
  tool: (name, ...rest) => {
    handlers[name] = rest.find((a) => typeof a === "function");
  },
});

async function call(name, args) {
  const handler = handlers[name];
  if (!handler) throw new Error(`tool '${name}' not registered`);
  const res = await handler(args);
  const text = res?.content?.[0]?.text ?? "";
  if (res?.isError) throw new Error(`${name}: ${text}`);
  return text;
}

const evalJs = (window_label, code, timeout_ms = 10000) =>
  call("execute_js", { code, window_label, timeout_ms });

async function showWindow(label) {
  // Hidden macOS WKWebViews are JS-suspended, so show + focus before execute_js.
  await call("manage_window", { action: "show", window_label: label });
  await call("manage_window", { action: "focus", window_label: label });
}

// Poll execute_js until the snippet resolves to boolean true, or give up.
async function waitForTrue(label, code, { tries = 20, delayMs = 500 } = {}) {
  let last = "";
  for (let i = 0; i < tries; i++) {
    last = await evalJs(label, code);
    if (last === "true" || last === true) return;
    await new Promise((r) => setTimeout(r, delayMs));
  }
  throw new Error(`condition never became true (last=${JSON.stringify(last)})`);
}

const checks = [];
async function check(name, fn) {
  try {
    const detail = (await fn()) || "";
    checks.push({ name, pass: true, detail });
    console.log(`  PASS ${name}${detail ? ` — ${detail}` : ""}`);
  } catch (e) {
    const detail = String(e?.message ?? e);
    checks.push({ name, pass: false, detail });
    console.log(`  FAIL ${name} — ${detail}`);
  }
}

console.log("oats E2E smoke:");

// 1. App is alive — at least the headless `main` window responds.
await check("app_up", async () => {
  const text = await call("manage_window", { action: "list" });
  if (!text || text === "[]") throw new Error("no windows reported");
  const labels = [...text.matchAll(/"label"\s*:\s*"([^"]+)"/g)].map((m) => m[1]);
  return `windows: ${labels.length ? labels.join(", ") : text.slice(0, 80)}`;
});

// 2. The Rust↔JS bridge works — round-trip a read-only backend command.
await check("backend_roundtrip", async () => {
  await showWindow("main");
  const text = await evalJs(
    "main",
    "window.__TAURI_INTERNALS__.invoke('get_desktop_config').then(c => JSON.stringify(c))",
  );
  const cfg = JSON.parse(text);
  if (!cfg.webAppBaseUrl) throw new Error(`webAppBaseUrl missing in ${text}`);
  return `webAppBaseUrl=${cfg.webAppBaseUrl}`;
});

// 3. The Meetings window opens.
await check("library_opens", async () => {
  await evalJs(
    "main",
    "window.__TAURI_INTERNALS__.invoke('create_library_window').then(() => 'ok')",
  );
  await showWindow("library");
  return "";
});

// 4. LibraryView.vue actually mounted (a blank webview = the Vue bundle failed
//    to load — one bad static import breaks the whole router graph).
await check("library_rendered", async () => {
  await waitForTrue(
    "library",
    "new Promise(r => r(!!document.querySelector('.library') && !!document.querySelector('.titlebar')))",
  );
  return ".library + .titlebar present";
});

const passed = checks.filter((c) => c.pass).length;
const pass = passed === checks.length;
writeFileSync(RESULT_PATH, JSON.stringify({ pass, checks }, null, 2));
console.log(`\nSMOKE ${pass ? "PASS" : "FAIL"} (${passed}/${checks.length})`);
process.exit(pass ? 0 : 1);
