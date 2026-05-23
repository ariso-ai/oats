# @ariso-ai/desktop

Tauri v2 desktop app for Ariso. Built with [Tauri](https://v2.tauri.app/), [Vite](https://vite.dev/), and [Vue 3](https://vuejs.org/).

This workspace is excluded from the monorepo's Turbo `build`/`lint`/`test` pipeline.

## Prerequisites

- [Rust](https://rustup.rs/) toolchain
- From the monorepo root: `npm install`
- `DEEPGRAM_API_KEY` in `.env` — must have **Member** role or higher (required for `/v1/auth/grant` token provisioning)

## Scripts

```bash
# Development (hot-reload frontend + Rust backend, uses localhost:4000)
npm run tauri:dev -w @ariso-ai/desktop

# Debug mode (enables MCP plugin + disables audio filters for loopback testing)
npm run tauri:dev:debug -w @ariso-ai/desktop

# Build (compile frontend + Rust, produce distributable)
npm run tauri:build -w @ariso-ai/desktop

# Build targeting dev API (https://api-dev.ari.ariso.ai)
npm run tauri:build -w @ariso-ai/desktop -- -- --features dev-api

# Build targeting prod API (https://api.ari.ariso.ai)
npm run tauri:build -w @ariso-ai/desktop -- -- --features prod-api
```

The API endpoint is controlled by Cargo feature flags and baked into the binary at compile time:

| Feature    | API endpoint                        |
|------------|-------------------------------------|
| *(default)* | `http://localhost:4000`             |
| `dev-api`  | `https://api-dev.ari.ariso.ai`      |
| `prod-api` | `https://api.ari.ariso.ai`          |

Debug mode sets `VITE_DEBUG_AUDIO=true`, which disables echo cancellation and noise suppression. This allows testing transcription with virtual audio devices (e.g., BlackHole) and the `say` command.

## Testing Transcription with a Virtual Audio Device

To test recording without a real microphone, route system audio back as mic input using an aggregate device.

### Setup (one-time)

1. Install [BlackHole](https://existential.audio/blackhole/) (virtual audio driver):
   ```bash
   brew install blackhole-2ch
   ```
2. Open **Audio MIDI Setup** (`/Applications/Utilities/Audio MIDI Setup.app`).
3. Click **+** in the bottom-left and create a **Multi-Output Device** that includes both your speakers (or headphones) and **BlackHole 2ch**. This lets you hear audio while it is also captured.
4. Set your **system output** to the Multi-Output Device (System Settings > Sound > Output, or Option-click the menu bar volume icon).
5. Set your **system input** to **BlackHole 2ch** (System Settings > Sound > Input).

### Running a test

```bash
# Start the app in debug mode (disables echo cancellation)
npm run tauri:dev:debug -w @ariso-ai/desktop

# In another terminal, play test audio
say -v Samantha "Hello everyone, welcome to today's standup."
```

Click **Start Recording** in the app before (or while) the `say` command is playing. The transcript should appear in real time.

> **Note:** Debug mode is required because the browser's echo cancellation and noise suppression filters strip out loopback audio. These filters are only disabled in dev builds when `VITE_DEBUG_AUDIO=true`.

## Output

| Path                | Contents                                      |
| ------------------- | --------------------------------------------- |
| `dist/`             | Compiled frontend (Vite output)               |
| `src-tauri/target/` | Rust build artifacts and packaged app bundles |

Both directories are git-ignored.

## **Architecture: WebSocket Streaming**

### Phase 1 — Direct Deepgram Connection

```
┌─────────────┐                       ┌──────────┐
│   Client    │── Audio (WebSocket) ─>│ Deepgram │
│             │<─ Transcripts (WS) ───│          │
└──────┬──────┘                       └──────────┘
       │
       │── HTTP (REST) ──> ┌──────────┐
       │                   │ web-api  │──Store──> Database
       └───────────────────└──────────┘
```

The client connects directly to Deepgram for real-time audio streaming. Meeting lifecycle (start, list, pause, terminate) is managed via HTTP API calls to `web-api`, which persists meeting metadata and transcripts.

### Phase 2 — Ariso Relay

```
┌─────────────┐                       ┌───────────────┐
│   Client    │── Audio (WebSocket) ─>│ Ariso Relay   │
│             │<─ Results (WebSocket)─│               │
└─────────────┘                       └──────┬────────┘
                                            │
                                            ├──WebSocket──> Deepgram
                                            │
                                            └──Store──> Database
```

Introduces the Ariso Relay as a WebSocket proxy between the client and Deepgram. The relay persists transcripts to the database and streams results back over the same WebSocket connection.
