#!/usr/bin/env node
// Deterministic, cross-platform E2E smoke test for the oats desktop app.
//
//   npm run e2e:smoke          # macOS and Windows
//
// It owns the whole run — launch the app, wait for it, assert, tear down — so CI
// is a single step and the same command reproduces a red build locally. There is
// no LLM in the loop: the checks are fixed, so a red gate means the app broke,
// not that a model wandered.
//
// It drives the LIVE app over tauri-plugin-mcp's socket, using the same
// `socketClient` the MCP server itself calls. It is intentionally audio/TCC-free
// — no recording, no transcription — so it only proves the app booted, the
// Rust<->webview bridge answers, and the Meetings window opens and renders. The
// full record->transcribe tier is a separate, self-hosted follow-up.
//
// Env knobs:
//   E2E_ATTACH=1             drive an app you already launched (skips spawn/kill)
//   E2E_ARTIFACT_DIR=<dir>   where the dev log and result JSON land
//   E2E_LAUNCH_TIMEOUT_MS    how long to wait for the app (default 20m: a cold
//                            Rust compile dominates the first run)

import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { closeSync, mkdirSync, openSync, readFileSync, writeFileSync } from "node:fs";
import { createConnection } from "node:net";
import { homedir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const IS_WINDOWS = process.platform === "win32";
const ATTACH = process.env.E2E_ATTACH === "1";

const ARTIFACT_DIR = process.env.E2E_ARTIFACT_DIR
  ? path.resolve(process.env.E2E_ARTIFACT_DIR)
  : path.join(REPO_ROOT, "e2e-artifacts");
const RESULT_PATH = path.join(ARTIFACT_DIR, "e2e-smoke-result.json");
const DEV_LOG_PATH = path.join(ARTIFACT_DIR, "tauri-dev.log");

const LAUNCH_TIMEOUT_MS = Number(process.env.E2E_LAUNCH_TIMEOUT_MS ?? 20 * 60_000);
const PROBE_INTERVAL_MS = 1_000;
// Per attempt, not per check: a webview that has not registered the plugin's
// listener yet never answers, so this is how long one lost emit costs us.
const JS_TIMEOUT_MS = 5_000;
// The socket binds while the plugin initialises, which is before setup() has
// built a window — this covers the rest of setup() (vault migration, tray,
// notification wiring) up to the point `main` exists.
const BOOT_TIMEOUT_MS = 60_000;
// The socket accepts as soon as the Rust side is up, which is well before the
// main webview has booted its bundle on a cold CI run.
const BRIDGE_TIMEOUT_MS = 120_000;
const RENDER_TIMEOUT_MS = 30_000;

// --- IPC endpoint -----------------------------------------------------------
//
// Both ends must land on one endpoint, and they negotiate it differently:
//
//   macOS   — a Unix socket. main.rs derives <home>/.ariso/run/oats-mcp.sock, the
//             plugin's TAURI_MCP_IPC_PATH override takes precedence over that, and
//             the node client dials whatever TAURI_MCP_IPC_PATH says. One value,
//             both ends.
//   Windows — a named pipe. The node client hardcodes \\.\pipe\tmp\tauri-mcp.sock
//             and ignores TAURI_MCP_IPC_PATH entirely (client.js
//             getEffectiveIpcPath), while the Rust side maps its configured path
//             through interprocess' GenericNamespaced, which prepends \\.\pipe\.
//             So the app is handed the *relative* name `tmp\tauri-mcp.sock`, which
//             resolves to exactly the pipe the client dials. (main.rs' own
//             USERPROFILE default would become \\.\pipe\C:\Users\...\oats-mcp.sock,
//             which nothing dials.)
const IPC_PATH = IS_WINDOWS
  ? String.raw`tmp\tauri-mcp.sock`
  : path.join(homedir(), ".ariso", "run", "oats-mcp.sock");
const IPC_DIAL_PATH = IS_WINDOWS ? String.raw`\\.\pipe\tmp\tauri-mcp.sock` : IPC_PATH;

// Both ends read TAURI_MCP_AUTH_TOKEN, and it wins over the token the plugin
// generates at init. Pinning one here skips the `<socket>.token` sidecar
// handshake, which the client cannot perform on Windows — it would look for that
// file *inside* the pipe namespace. The token is per-run and never leaves this
// machine; it only gates local access to the socket.
//
// Attaching is the exception: that app already minted its own token, so we have
// to adopt it rather than invent one it would reject.
if (ATTACH && !process.env.TAURI_MCP_AUTH_TOKEN && !IS_WINDOWS) {
  try {
    process.env.TAURI_MCP_AUTH_TOKEN = readFileSync(`${IPC_PATH}.token`, "utf8").trim();
  } catch {
    // No token file — fall through and let the connection fail loudly.
  }
}
process.env.TAURI_MCP_AUTH_TOKEN ??= randomUUID();
// Read at import time by the client singleton, and inherited by the app we spawn.
process.env.TAURI_MCP_IPC_PATH = IPC_PATH;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// --- app lifecycle ----------------------------------------------------------

function launchApp() {
  // `tauri dev` compiles Rust and starts vite itself (beforeDevCommand). It never
  // exits, so nothing waits on it — readiness is the socket accepting a connection.
  const log = openSync(DEV_LOG_PATH, "w");
  const child = spawn("npm", ["run", "tauri:dev:debug"], {
    cwd: REPO_ROOT,
    stdio: ["ignore", log, log],
    detached: !IS_WINDOWS, // own process group, so teardown reaches cargo and vite too
    shell: IS_WINDOWS, // npm is npm.cmd here
  });
  closeSync(log);
  return child;
}

function stopApp(child) {
  if (!child || child.exitCode !== null || child.signalCode !== null) return;
  if (IS_WINDOWS) {
    // No process groups; taskkill /T walks the tree (npm -> cargo -> app, vite).
    spawn("taskkill", ["/PID", String(child.pid), "/T", "/F"], { stdio: "ignore" });
  } else {
    try {
      process.kill(-child.pid, "SIGTERM");
    } catch {
      // Already gone.
    }
  }
}

function endpointAccepts() {
  return new Promise((resolve) => {
    const socket = createConnection({ path: IPC_DIAL_PATH });
    const settle = (ok) => {
      socket.destroy();
      resolve(ok);
    };
    socket.once("connect", () => settle(true));
    socket.once("error", () => settle(false));
  });
}

async function waitForApp(child) {
  const deadline = Date.now() + LAUNCH_TIMEOUT_MS;
  while (Date.now() < deadline) {
    if (await endpointAccepts()) return;
    if (child && child.exitCode !== null) {
      throw new Error(`tauri dev exited with code ${child.exitCode} before the app came up`);
    }
    await sleep(PROBE_INTERVAL_MS);
  }
  throw new Error(
    `timed out after ${Math.round(LAUNCH_TIMEOUT_MS / 1000)}s waiting for ${IPC_DIAL_PATH}`,
  );
}

// --- checks -----------------------------------------------------------------

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

async function runChecks(send) {
  // `send` resolves the command's structured data and rejects on failure, so
  // every check below reads real values rather than scraping formatted text.
  const listWindows = async () => (await send("list_windows", {})).windows ?? [];
  // The socket protocol's own field is `operation`; `action` is only the MCP
  // tool's public spelling, and the Rust side rejects it as a missing field.
  const manageWindow = (operation, window_label) =>
    send("manage_window", { operation, window_label });
  // execute_js answers { result, type } — the value, not the wrapper, is what a
  // check wants to assert on.
  const evalJs = async (window_label, code) => {
    const res = await send("execute_js", { code, window_label, timeout_ms: JS_TIMEOUT_MS });
    return res && typeof res === "object" && "result" in res ? res.result : res;
  };
  // The plugin emits its request event into the webview exactly once and drops
  // it when no listener has registered yet, so a webview that just opened burns
  // the whole timeout instead of answering. (Its own `wait_for` loses to that
  // race outright — one emit, no retry — which is why the render check polls
  // from here instead.) Every attempt is a fresh emit, so a lost one costs a
  // poll rather than the run.
  const evalUntil = async (window_label, code, timeoutMs, accept) => {
    const deadline = Date.now() + timeoutMs;
    let last = "never answered";
    for (;;) {
      try {
        const value = await evalJs(window_label, code);
        if (accept(value)) return value;
        last = `got ${JSON.stringify(value)}`;
      } catch (e) {
        last = String(e?.message ?? e);
      }
      if (Date.now() >= deadline) {
        throw new Error(`gave up after ${Math.round(timeoutMs / 1000)}s — ${last}`);
      }
      await sleep(PROBE_INTERVAL_MS);
    }
  };

  // 1. The app is alive and enumerating its windows. tauri.conf.json declares no
  //    windows — every one is built partway through setup() — while the plugin's
  //    socket binds during plugin init, before setup() runs at all. So a socket
  //    that accepts proves only that the process started, and asking right then
  //    reports zero windows. Wait for `main` to actually exist.
  await check("app_up", async () => {
    const deadline = Date.now() + BOOT_TIMEOUT_MS;
    for (;;) {
      const windows = await listWindows();
      if (windows.some((w) => w.label === "main")) {
        return `windows: ${windows.map((w) => w.label).join(", ")}`;
      }
      if (Date.now() >= deadline) {
        const seen = windows.length ? windows.map((w) => w.label).join(", ") : "none";
        throw new Error(
          `no 'main' window ${Math.round(BOOT_TIMEOUT_MS / 1000)}s after the socket came up (saw: ${seen})`,
        );
      }
      await sleep(PROBE_INTERVAL_MS);
    }
  });

  // 2. The Rust<->JS bridge answers — round-trip a read-only backend command.
  //    `main` is the headless BootstrapView window and starts hidden; hidden
  //    WKWebViews are JS-suspended on macOS, so it has to be shown and focused
  //    before any execute_js. That is an OS behavior, not an app defect.
  await check("backend_roundtrip", async () => {
    await manageWindow("show", "main");
    await manageWindow("focus", "main");
    const raw = await evalUntil(
      "main",
      "window.__TAURI_INTERNALS__.invoke('get_desktop_config').then(c => JSON.stringify(c))",
      BRIDGE_TIMEOUT_MS,
      (v) => typeof v === "string" && v.startsWith("{"),
    );
    const config = JSON.parse(raw);
    if (!config.webAppBaseUrl) throw new Error(`webAppBaseUrl missing in ${raw}`);
    return `webAppBaseUrl=${config.webAppBaseUrl}`;
  });

  // 3. The Meetings window opens *and presents itself*. Deliberately no show/focus
  //    from here: making it visible is part of what create_library_window does, so
  //    forcing it would mask the regression this check exists to catch.
  await check("library_opens", async () => {
    await evalJs("main", "window.__TAURI_INTERNALS__.invoke('create_library_window')");
    const library = (await listWindows()).find((w) => w.label === "library");
    if (!library) throw new Error("no window labelled 'library' after create_library_window");
    if (!library.visible) throw new Error("the library window was created but never shown");
    return `title=${library.title}`;
  });

  // 4. LibraryView.vue actually mounted. A blank webview means the Vue bundle
  //    failed to load — the router statically imports every view, so one bad
  //    import breaks the whole graph. Assert on the aria contract rather than a
  //    scoped styling class, so a CSS refactor can't silently turn this red.
  await check("library_rendered", async () => {
    await evalUntil(
      "library",
      `String((() => {
        const el = document.querySelector('[aria-label="Toggle meetings list"]');
        return !!el && el.getBoundingClientRect().width > 0;
      })())`,
      RENDER_TIMEOUT_MS,
      (v) => v === "true",
    );
    return "meetings list toggle visible";
  });
}

// --- main -------------------------------------------------------------------

mkdirSync(ARTIFACT_DIR, { recursive: true });

let app = null;
let failure = null;

try {
  if (ATTACH) {
    console.log(`oats E2E smoke: attaching to a running app on ${IPC_DIAL_PATH}`);
  } else {
    console.log(`oats E2E smoke: launching the app (log: ${DEV_LOG_PATH})`);
    app = launchApp();
  }
  await waitForApp(app);

  // The client singleton reads TAURI_MCP_IPC_PATH at import time, so it must be
  // loaded after the env is set above.
  const { socketClient } = await import("tauri-plugin-mcp-server/build/tools/index.js");
  console.log("oats E2E smoke:");
  await runChecks((command, payload) => socketClient.sendCommand(command, payload));
} catch (e) {
  failure = String(e?.message ?? e);
  console.log(`  FAIL launch — ${failure}`);
  checks.push({ name: "launch", pass: false, detail: failure });
} finally {
  stopApp(app);
}

const passed = checks.filter((c) => c.pass).length;
const pass = checks.length > 0 && passed === checks.length;
writeFileSync(RESULT_PATH, JSON.stringify({ pass, checks }, null, 2));

if (!pass && !ATTACH) {
  // The dev log is the only place a compile or vite failure shows up.
  console.log(`\n--- tail of ${DEV_LOG_PATH} ---`);
  try {
    // cargo redraws its progress bar with \r, so a whole build can be a couple
    // of "lines" — split on those too or the tail shows the top of the file.
    const lines = readFileSync(DEV_LOG_PATH, "utf8")
      .split(/[\r\n]/)
      .filter((l) => l.trim() !== "");
    console.log(lines.slice(-100).join("\n"));
  } catch {
    console.log("(no dev log was written)");
  }
}

console.log(`\nSMOKE ${pass ? "PASS" : "FAIL"} (${passed}/${checks.length}) — ${RESULT_PATH}`);
process.exit(pass ? 0 : 1);
