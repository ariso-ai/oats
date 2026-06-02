import Foundation
import FluidAudio

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
func runToCompletion(_ body: @escaping @Sendable () async -> Void) -> Never {
    let sem = DispatchSemaphore(value: 0)
    Task {
        await body()
        sem.signal()
    }
    sem.wait()
    exit(0)
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

// MARK: - Entry

let arguments = CommandLine.arguments
let isDownload = arguments.count > 1 && arguments[1] == "download"

guard let modelsPath = argValue("--models") else { fail("missing --models") }
let modelsURL = URL(fileURLWithPath: modelsPath)
let asrDir = modelsURL.appendingPathComponent("asr")
let diarizerDir = modelsURL.appendingPathComponent("diarizer")

if isDownload {
    runToCompletion {
        do {
            let onProgress: DownloadUtils.ProgressHandler = { p in
                emitProgress(p.fractionCompleted)
            }
            _ = try await AsrModels.downloadAndLoad(
                to: asrDir, version: .v3, progressHandler: onProgress)
            _ = try await DiarizerModels.downloadIfNeeded(
                to: diarizerDir, progressHandler: onProgress)
            emitProgress(1.0)
            stdoutLine("{\"type\":\"done\"}")
        } catch {
            fail("download error: \(error)")
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
