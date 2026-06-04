import Foundation
import FluidAudio
import MLXLLM
import MLXLMCommon
import MLXHuggingFace
import HuggingFace
import Tokenizers

// MARK: - Output contract (must match the Rust `TranscriptResult` deserializer)

struct OutParticipant: Codable {
    let id: Int
    let label: String
}

struct OutSegment: Codable {
    let speaker: Int
    let text: String
    let start: Double
    let end: Double
}

struct OutResult: Codable {
    let language: String
    let durationSeconds: Double
    let participants: [OutParticipant]
    let segments: [OutSegment]
}

// MARK: - IO helpers (ONLY contract JSON / progress lines on stdout; logs on stderr)

func argValue(_ name: String) -> String? {
    let a = CommandLine.arguments
    guard let i = a.firstIndex(of: name), i + 1 < a.count else { return nil }
    return a[i + 1]
}

func stderrLine(_ msg: String) {
    FileHandle.standardError.write(Data((msg + "\n").utf8))
}

func fail(_ msg: String) -> Never {
    stderrLine(msg)
    exit(1)
}

/// Write a line to stdout immediately (unbuffered) so the Rust side can stream
/// progress events as they arrive.
func stdoutLine(_ s: String) {
    FileHandle.standardOutput.write(Data((s + "\n").utf8))
}

func emitProgress(_ fraction: Double) {
    let clamped = max(0.0, min(1.0, fraction))
    stdoutLine("{\"type\":\"progress\",\"fraction\":\(clamped)}")
}

/// Run an async body to completion, then exit. `fail()` handles error exits.
///
/// Uses `dispatchMain()` rather than blocking the main thread on a semaphore:
/// swift-huggingface drives model-download progress through a `@MainActor`
/// handler, so a parked main thread would starve the MainActor and the download
/// would deadlock (TCP connected, zero bytes, forever). `dispatchMain()` parks
/// the main thread while still servicing the main queue; the async `Task` exits
/// the process when `body` completes.
func runToCompletion(_ body: @escaping @Sendable () async -> Void) -> Never {
    Task {
        await body()
        exit(0)
    }
    dispatchMain()
}

// MARK: - Token -> text reconstruction

/// Parakeet/SentencePiece tokens use U+2581 ("▁") to mark a leading space.
func reconstructText(_ tokens: [TokenTiming]) -> String {
    let joined = tokens.map { $0.token }.joined()
    return joined
        .replacingOccurrences(of: "\u{2581}", with: " ")
        .trimmingCharacters(in: .whitespacesAndNewlines)
}

/// Merge ASR token timings with diarization turns into speaker-attributed,
/// time-ordered segments. Speaker ids are remapped to contiguous 0-based
/// indices in order of first appearance; labels are "Speaker N".
func mergeSegments(asr: ASRResult, diarization: [TimedSpeakerSegment]) -> OutResult {
    let timings = asr.tokenTimings ?? []

    var speakerIndex: [String: Int] = [:]
    var order: [String] = []
    func indexFor(_ speakerId: String) -> Int {
        if let i = speakerIndex[speakerId] { return i }
        let i = order.count
        speakerIndex[speakerId] = i
        order.append(speakerId)
        return i
    }

    var segments: [OutSegment] = []

    if diarization.isEmpty || timings.isEmpty {
        // No diarization (or no token timings): a single segment for the whole transcript.
        segments.append(
            OutSegment(speaker: 0, text: asr.text, start: 0, end: asr.duration))
    } else {
        let ordered = diarization.sorted { $0.startTimeSeconds < $1.startTimeSeconds }
        for turn in ordered {
            let start = Double(turn.startTimeSeconds)
            let end = Double(turn.endTimeSeconds)
            let inTurn = timings.filter {
                let mid = ($0.startTime + $0.endTime) / 2.0
                return mid >= start && mid < end
            }
            if inTurn.isEmpty { continue }
            segments.append(
                OutSegment(
                    speaker: indexFor(turn.speakerId),
                    text: reconstructText(inTurn),
                    start: start,
                    end: end))
        }
        // If nothing matched (e.g. timing/turn misalignment), fall back to one segment.
        if segments.isEmpty {
            segments.append(
                OutSegment(
                    speaker: indexFor(ordered[0].speakerId),
                    text: asr.text,
                    start: 0,
                    end: asr.duration))
        }
    }

    let participants: [OutParticipant] =
        order.isEmpty
        ? [OutParticipant(id: 0, label: "Speaker 1")]
        : order.indices.map { OutParticipant(id: $0, label: "Speaker \($0 + 1)") }

    return OutResult(
        language: "en",
        durationSeconds: asr.duration,
        participants: participants,
        segments: segments)
}

// MARK: - Notes (LLM meeting-notes generation)

/// Load the notes LLM from its local directory. The model is downloaded
/// out-of-band by the Rust app from the project CDN (the published weights are
/// HuggingFace Xet-backed, which the Swift HF client can't fetch), so here we
/// only LOAD from disk — no network. Model + tokenizer are read from
/// `<models>/llm/gemma-3-1b-it-qat-4bit/`.
func loadNotesModel(modelsURL: URL) async throws -> ModelContainer {
    let dir = modelsURL
        .appendingPathComponent("llm")
        .appendingPathComponent("gemma-3-1b-it-qat-4bit")
    return try await LLMModelFactory.shared.loadContainer(
        from: dir,
        using: #huggingFaceTokenizerLoader())
}

/// Run the notes model on `transcript` and return the full Markdown notes.
func generateNotes(transcript: String, modelsURL: URL) async throws -> String {
    let container = try await loadNotesModel(modelsURL: modelsURL)
    // System instructions describe the format in prose — with NO copyable
    // placeholder lines — so a small model writes real content instead of
    // echoing the template (which it did when the format was a fill-in scaffold).
    let instructions = """
        You are a meeting-notes assistant. You are given a meeting transcript and you write concise meeting notes in Markdown.

        Rules:
        - Use only facts stated in the transcript. Never invent details, names, or speakers.
        - The transcript labels speakers generically (e.g. "Speaker 1", "Speaker 2"). Do not invent any speaker or person who does not appear in the transcript.
        - Output the notes only — no preamble, no closing remarks, and never repeat or restate these instructions.
        - Use these level-2 (##) sections, in this order: Summary, Key Points, Decisions, Action Items.
        - "Summary" is 2-3 sentences describing what the meeting was about. The other sections are bullet lists.
        - For each action item, state the task. Only attribute it to a speaker if that exact speaker explicitly committed to it in the transcript; otherwise give the task with no owner.
        - Omit any section that has no real content in the transcript (for example, if no decisions were made, leave out the Decisions section entirely). Never write placeholder text under a heading.
        """
    let session = ChatSession(
        container,
        instructions: instructions,
        generateParameters: GenerateParameters(maxTokens: 2048, temperature: 0.3))
    return try await session.respond(to: "Transcript:\n\(transcript)")
}

// MARK: - Entry

let arguments = CommandLine.arguments
let isDownload = arguments.count > 1 && arguments[1] == "download"

guard let modelsPath = argValue("--models") else { fail("missing --models") }
let modelsURL = URL(fileURLWithPath: modelsPath)
let asrDir = modelsURL.appendingPathComponent("asr")
let diarizerDir = modelsURL.appendingPathComponent("diarizer")

if isDownload {
    // Downloads the speech models (ASR + diarizer, CoreML) over a 0..1 bar:
    // ASR 0..0.66, diarizer 0.66..1.0. The notes LLM is NOT downloaded here —
    // the Rust app fetches it directly from the project CDN (see download_local_llm).
    runToCompletion {
        do {
            let onAsr: DownloadUtils.ProgressHandler = { p in
                emitProgress(p.fractionCompleted * 0.66)
            }
            let onDiarizer: DownloadUtils.ProgressHandler = { p in
                emitProgress(0.66 + p.fractionCompleted * 0.34)
            }
            _ = try await AsrModels.downloadAndLoad(
                to: asrDir, version: .v3, progressHandler: onAsr)
            _ = try await DiarizerModels.downloadIfNeeded(
                to: diarizerDir, progressHandler: onDiarizer)
            emitProgress(1.0)
            stdoutLine("{\"type\":\"done\"}")
        } catch {
            fail("download error: \(error)")
        }
    }
}

let isNotes = arguments.count > 1 && arguments[1] == "notes"

if isNotes {
    guard let transcriptPath = argValue("--transcript") else { fail("missing --transcript") }
    let transcript: String
    do {
        transcript = try String(contentsOf: URL(fileURLWithPath: transcriptPath), encoding: .utf8)
    } catch {
        fail("notes read error: \(error)")
    }
    runToCompletion {
        do {
            let notes = try await generateNotes(transcript: transcript, modelsURL: modelsURL)
            FileHandle.standardOutput.write(Data(notes.utf8))
        } catch {
            fail("notes error: \(error)")
        }
    }
}

guard let audioPath = argValue("--audio") else { fail("missing --audio") }
let audioURL = URL(fileURLWithPath: audioPath)

runToCompletion {
    do {
        // ASR: load models and transcribe (resampling handled internally).
        let asrModels = try await AsrModels.load(from: asrDir, version: .v3)
        let asrManager = AsrManager()
        try await asrManager.loadModels(asrModels)
        var decoderState = try TdtDecoderState()
        let asrResult = try await asrManager.transcribe(audioURL, decoderState: &decoderState)

        // Diarization: needs 16 kHz mono Float samples.
        let samples = try AudioConverter().resampleAudioFile(audioURL)
        let diarizerModels = try await DiarizerModels.downloadIfNeeded(to: diarizerDir)
        let diarizer = DiarizerManager()
        diarizer.initialize(models: diarizerModels)
        let diarization = try diarizer.performCompleteDiarization(samples, sampleRate: 16000)

        let result = mergeSegments(asr: asrResult, diarization: diarization.segments)
        let data = try JSONEncoder().encode(result)
        FileHandle.standardOutput.write(data)
    } catch {
        fail("transcribe error: \(error)")
    }
}
