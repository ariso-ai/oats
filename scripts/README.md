## Sync model files from huggingface to cloudflare R2

Make sure you've configured R2 user api token as aws credential.

```SHELL
export R2_ENDPOINT=https://020cfd316d4853132dc053030d7d4653.r2.cloudflarestorage.com
export R2_BUCKET=ariso-app
AWS_PROFILE=r2 ./sync-stt-models.sh
```

## Windows Local smoke harness

`import-windows-build-env.ps1` loads the Visual Studio C++ build environment when `link.exe` is not
already on `PATH`.

`build-windows-sidecar.ps1` builds the Windows `ariso-stt.exe` sidecar for
`x86_64-pc-windows-msvc` and copies it to Tauri's target-named external binary path:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-windows-sidecar.ps1
```

`build-windows-installers.ps1` builds the Windows installer artifacts through Tauri:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw "C:\path\to\tauri.key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "..."
powershell -ExecutionPolicy Bypass -File scripts\build-windows-installers.ps1
```

Release and desktop CI use these helpers before building or validating the Windows app.

`windows-local-smoke.ps1` runs the Windows `ariso-stt.exe` sidecar against already-staged
model artifacts and reports JSON timing/output checks. It does not download models. The
output includes a hardware summary plus STT audio duration and real-time factor when the
audio fixture is a WAV file.

`sync-windows-local-models.ps1` stages the Windows spike artifacts into the canonical R2 layout
and writes a `windows-local-manifest.json` containing size, SHA-256, public URL, and sidecar env
override metadata. It also writes `windows-local-sidecar-pins.txt`, a copy-pasteable checklist for
updating the Rust sidecar pins if any artifact changes:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\sync-windows-local-models.ps1 `
  -Models "$env:TEMP\oats-smoke\models" `
  -StageDir "$env:TEMP\oats-windows-local-r2"
```

If GitHub is not reachable but you already have the official sherpa-onnx segmentation archive
and llama.cpp runtime archive, pass them explicitly:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\sync-windows-local-models.ps1 `
  -Models "$env:TEMP\oats-smoke\models" `
  -SegmentationArchive "$env:TEMP\sherpa-onnx-pyannote-segmentation-3-0.tar.bz2" `
  -LlamaRuntimeArchive "$env:TEMP\llama-b9940-bin-win-cpu-x64.zip" `
  -StageDir "$env:TEMP\oats-windows-local-r2"
```

If you have accepted the Google Gemma terms and downloaded the official gated QAT GGUF manually,
pass it with `-GemmaGguf`. The helper stages it under the sidecar's expected filename and writes
new size/SHA-256 values to `windows-local-sidecar-pins.txt` so the Rust downloader pins can be
updated before changing production defaults.

To upload after setting R2 credentials, add `-Upload`; the script uses `R2_ENDPOINT` and
`R2_BUCKET`.

The speech model bundle must include Parakeet plus speaker diarization artifacts. The
canonical Windows layout is versioned under
`windows/parakeet-tdt-0.6b-v3/v1` and `windows/speaker-diarization/v1`, including
`sherpa-onnx-pyannote-segmentation-3-0/model.int8.onnx` and
`3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx`. The sidecar also accepts
the earlier flat spike directories for manual smoke runs.

The Windows sidecar can install the spike speech and notes bundles directly:

```powershell
src-tauri\ariso-stt-cross\target\debug\ariso-stt.exe download --models "$env:TEMP\oats-smoke\models"
src-tauri\ariso-stt-cross\target\debug\ariso-stt.exe download-notes --models "$env:TEMP\oats-smoke\models"
```

By default those commands fetch the public spike sources, but every download still verifies a
pinned byte size and SHA-256. To test a mirrored R2 bundle, set any of these before running the
same commands:

```powershell
$env:ARISO_WINDOWS_PARAKEET_BASE_URL = "https://pub.example.r2.dev/models/windows/parakeet-tdt-0.6b-v3/v1"
$env:ARISO_WINDOWS_DIARIZATION_SEGMENTATION_URL = "https://pub.example.r2.dev/models/windows/speaker-diarization/v1/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2"
$env:ARISO_WINDOWS_DIARIZATION_EMBEDDING_URL = "https://pub.example.r2.dev/models/windows/speaker-diarization/v1/3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx"
$env:ARISO_WINDOWS_GEMMA_GGUF_URL = "https://pub.example.r2.dev/models/windows/gemma-3-1b-it-qat-4bit/v1/gemma-3-1b-it-q4_0.gguf"
$env:ARISO_WINDOWS_LLAMA_RUNTIME_URL = "https://pub.example.r2.dev/models/windows/gemma-3-1b-it-qat-4bit/v1/llama-b9940-bin-win-cpu-x64.zip"
```

Example STT smoke:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\windows-local-smoke.ps1 `
  -Models "$env:TEMP\oats-parakeet-smoke-curl\models" `
  -Audio "$env:TEMP\oats-parakeet-smoke-curl\audio\en.wav"
```

Example notes smoke:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\windows-local-smoke.ps1 `
  -Models "$env:TEMP\oats-gemma-smoke\models" `
  -Transcript "$env:TEMP\oats-gemma-smoke\transcript.md" `
  -NotesMaxTokens 160 `
  -NotesCtxSize 2048
```

Example offline smoke after models are staged:

```powershell
$env:ARISO_WINDOWS_PARAKEET_BASE_URL = "http://127.0.0.1:9/offline-parakeet"
$env:ARISO_WINDOWS_DIARIZATION_SEGMENTATION_URL = "http://127.0.0.1:9/offline-segmentation.tar.bz2"
$env:ARISO_WINDOWS_DIARIZATION_EMBEDDING_URL = "http://127.0.0.1:9/offline-speaker.onnx"
$env:ARISO_WINDOWS_GEMMA_GGUF_URL = "http://127.0.0.1:9/offline-gemma.gguf"
$env:ARISO_WINDOWS_LLAMA_RUNTIME_URL = "http://127.0.0.1:9/offline-llama.zip"
powershell -ExecutionPolicy Bypass -File scripts\windows-local-smoke.ps1 `
  -Models "$env:TEMP\oats-smoke\models" `
  -Audio "$env:TEMP\oats-smoke\audio.wav" `
  -Transcript "$env:TEMP\oats-smoke\transcript.md"
```

For a longer STT datapoint, pass `-RepeatAudio` with a WAV fixture:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\windows-local-smoke.ps1 `
  -Models "$env:TEMP\oats-smoke\models" `
  -Audio "$env:TEMP\oats-smoke\audio.wav" `
  -RepeatAudio 4
```

The smoke harness reports the generated audio path, `audioSeconds`, and `realTimeFactor`; the
current Windows VM smoke completed a 64s repeated WAV with 2 participants, speakers `0,1`, and no
dedicated GPU requirement.

The spike notes downloader uses the ungated `ggml-org/gemma-3-1b-it-GGUF`
`gemma-3-1b-it-Q4_K_M.gguf` artifact and stores it as
`gemma-3-1b-it-q4_0.gguf` so the `llama.cpp` runtime path can be tested without
HF auth. The production Windows notes artifact should be the official
`google/gemma-3-1b-it-qat-q4_0-gguf` model file, mirrored to R2 with a pinned
digest after license acceptance.
