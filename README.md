<div align="center">

<img src="src/assets/oats-dark.png" alt="oats logo" width="96" height="96" />

# oats

**Record, transcribe, and summarize your meetings on macOS — for free in the cloud, or 100% offline on your own device.**

[![Desktop App](https://github.com/ariso-ai/oats/actions/workflows/desktop.yaml/badge.svg)](https://github.com/ariso-ai/oats/actions/workflows/desktop.yaml)
[![Release](https://github.com/ariso-ai/oats/actions/workflows/release.yaml/badge.svg)](https://github.com/ariso-ai/oats/actions/workflows/release.yaml)
[![Latest Release](https://img.shields.io/github/v/release/ariso-ai/oats?label=download&logo=apple&color=000000)](https://github.com/ariso-ai/oats/releases/latest)

[![macOS Apple Silicon](https://img.shields.io/badge/macOS-Apple%20Silicon-000000?logo=apple&logoColor=white)](https://github.com/ariso-ai/oats/releases/latest)
[![Built with Tauri](https://img.shields.io/badge/Built%20with-Tauri%20v2-24C8DB?logo=tauri&logoColor=white)](https://v2.tauri.app/)
[![Vue 3](https://img.shields.io/badge/Vue-3-4FC08D?logo=vuedotjs&logoColor=white)](https://vuejs.org/)
[![Built with Claude Code](https://img.shields.io/badge/Built%20with-Claude%20Code-D97757?logo=anthropic&logoColor=white)](https://claude.com/claude-code)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](CONTRIBUTING.md)

</div>

---

**oats** is a lightweight menu-bar app that captures your meetings, turns them into accurate transcripts in real time, and writes you a clean set of meeting notes when you're done. It runs entirely from the macOS tray — start a recording, keep working, and your transcript and summary are waiting for you in the Library.

You choose how it works:

- ☁️ **Free, in the cloud** — sign in and let the [Ariso](https://ariso.ai) backend do the heavy lifting. Real-time streaming transcription with no setup.
- 🔒 **Private, on-device** — flip one switch and everything — recording, transcription, speaker labels, and summary notes — happens **entirely offline on your Mac**. No login. No upload. Nothing leaves your machine.

## ✨ Features

- 🎙️ **One-click recording** from the menu bar — pause, resume, and stop without leaving your workflow.
- ⚡ **Real-time transcription** that streams in as people speak.
- 🗣️ **Speaker diarization** — see who said what.
- 📝 **Automatic meeting notes** — a tidy Markdown summary generated for every recording.
- 📚 **Library** — every past recording, transcript, and note kept in one place.
- 🔗 **Share** — send a meeting summary out with a native macOS share sheet, or share to the web (Ariso backend).
- 🔄 **Auto-updates** — signed and notarized releases update themselves in the background.

## 📥 Install

> **Requires Apple Silicon (M-series) and macOS 14 or later.**

1. Download the latest `oats.dmg` from the [**Releases page**](https://github.com/ariso-ai/oats/releases/latest).
2. Open the DMG and drag **oats** into your Applications folder.
3. Launch it from Applications. oats lives in your menu bar — look for the <img src="src/assets/oats-dark.png" alt="oats icon" width="16" height="16" valign="middle" /> icon.

The app is **code-signed and notarized by Apple**, and updates itself automatically as new versions ship.

## 🚀 Getting started

When you first launch oats, pick a transcription backend in **Settings → Transcription Backend**:

### ☁️ Ariso — free, in the cloud

The default. Sign in with your Ariso account and start recording. Audio streams to the Ariso backend, which transcribes it in real time and stores your meetings so you can revisit and share them from anywhere. **Free to use** — perfect if you want zero setup and don't mind your transcripts living in the cloud.

### 🔒 Local — private, 100% offline

For sensitive conversations, switch the backend to **Local**. Now oats does *everything* on your Mac:

- **Recording** is captured and saved locally.
- **Transcription** runs on the Apple Neural Engine ([Parakeet](https://github.com/FluidInference/FluidAudio) ASR + speaker diarization).
- **Summary notes** are written by an on-device language model — no API calls.

There is **no login and no network upload** — your audio, transcripts, and notes never leave your machine. The one-time setup downloads the on-device models: open **Settings → On-device models** and install the **speech voice model** and **language model** (each shows a green tick when ready). After that, oats works completely offline.

Everything is stored locally under `~/.ariso/recordings/`:

| File            | Contents                          |
| --------------- | --------------------------------- |
| `recording.mp3` | The audio of your meeting         |
| `transcript.md` | The full transcript               |
| `note.md`       | The generated meeting summary     |

## 🔐 Privacy at a glance

| | ☁️ Ariso (cloud) | 🔒 Local (on-device) |
| --- | --- | --- |
| **Cost** | Free | Free |
| **Account / login** | Required | None |
| **Audio leaves your Mac** | Yes (to Ariso) | **Never** |
| **Transcription** | Ariso backend | Apple Neural Engine |
| **Summary notes** | Ariso backend | On-device LLM |
| **Works offline** | No | **Yes** |
| **Best for** | Convenience, sharing, any Mac | Confidential meetings, air-gapped use |

## 🤝 Contributing

oats is open source and contributions are welcome! Whether it's a bug report, a feature idea, or a pull request, we'd love your help.

👉 See **[CONTRIBUTING.md](CONTRIBUTING.md)** for how to set up a development environment, build the app and the on-device sidecar, run the tests, and cut a release.

## 📄 License

oats is open source under the [MIT License](LICENSE).

## 🛠️ Built with

[Tauri v2](https://v2.tauri.app/) · [Vue 3](https://vuejs.org/) · [Vite](https://vite.dev/) · [Rust](https://www.rust-lang.org/) · [FluidAudio](https://github.com/FluidInference/FluidAudio) · [MLX](https://github.com/ml-explore/mlx-swift-lm) · and a lot of [Claude Code](https://claude.com/claude-code).

<div align="center">
<sub>Made with 🌾 by <a href="https://ariso.ai">Ariso</a></sub>
</div>
