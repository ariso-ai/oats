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

// MARK: - IO helpers (ONLY contract JSON on stdout; logs on stderr)

func argValue(_ name: String) -> String? {
    let a = CommandLine.arguments
    guard let i = a.firstIndex(of: name), i + 1 < a.count else { return nil }
    return a[i + 1]
}

func boundedIntArg(_ name: String, default defaultValue: Int, range: ClosedRange<Int>) -> Int {
    guard let raw = argValue(name) else { return defaultValue }
    guard let value = Int(raw), range.contains(value) else { fail("invalid \(name)") }
    return value
}

func boundedFloatArg(
    _ name: String, default defaultValue: Float, range: ClosedRange<Float>
) -> Float {
    guard let raw = argValue(name) else { return defaultValue }
    guard let value = Float(raw), value.isFinite, range.contains(value) else {
        fail("invalid \(name)")
    }
    return value
}

func stderrLine(_ msg: String) {
    FileHandle.standardError.write(Data((msg + "\n").utf8))
}

func fail(_ msg: String) -> Never {
    stderrLine(msg)
    exit(1)
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
func generateNotes(container: ModelContainer, transcript: String) async throws -> String {
    // System instructions describe the format in prose — with NO copyable
    // placeholder lines — so a small model writes real content instead of
    // echoing the template (which it did when the format was a fill-in scaffold).
    let instructions = """
        You are a meeting-notes assistant. You are given a meeting transcript and you write concise meeting notes in Markdown.

        Rules:
        - Use only facts stated in the transcript. Never invent details, names, or speakers.
        - The transcript labels speakers generically (e.g. "Speaker 1", "Speaker 2"). Do not invent any speaker or person who does not appear in the transcript.
        - Output the notes only — no preamble, no closing remarks, and never repeat or restate these instructions.
        - Output raw Markdown directly. Never wrap the notes in a code fence (do not emit ``` or ```markdown).
        - Use these level-2 (##) sections, in this order: Summary, Key Points, Decisions, Action Items.
        - "Summary" is 2-3 sentences describing what the meeting was about. The other sections are bullet lists.
        - For each action item, state the task. Only attribute it to a speaker if that exact speaker explicitly committed to it in the transcript; otherwise give the task with no owner.
        - Omit any section that has no real content in the transcript (for example, if no decisions were made, leave out the Decisions section entirely). Never write placeholder text under a heading.
        """
    // A repetition penalty is essential here: the small notes model otherwise
    // falls into a degeneration loop, repeating a sentence until maxTokens and
    // emitting the transcript back instead of a summary.
    let session = ChatSession(
        container,
        instructions: instructions,
        generateParameters: GenerateParameters(
            maxTokens: 2048, temperature: 0.3,
            repetitionPenalty: 1.15, repetitionContextSize: 64))
    let raw = try await session.respond(to: "Transcript:\n\(transcript)")
    return stripCodeFence(raw)
}

/// A meeting-notes result: the Markdown notes plus a short generated title.
struct NotesResult: Codable {
    let title: String
    let notes: String
}

struct CompletionResult: Codable {
    let text: String
}

/// Run one bounded local-model prompt. Reducer policy and prompt construction
/// stay in the Rust host; this command is only a platform model adapter.
func generateCompletion(
    container: ModelContainer,
    prompt: String,
    maxTokens: Int,
    temperature: Float,
    repetitionPenalty: Float
) async throws -> String {
    let session = ChatSession(
        container,
        instructions: "Follow the user's prompt exactly and return only the requested output.",
        generateParameters: GenerateParameters(
            maxTokens: maxTokens,
            temperature: temperature,
            repetitionPenalty: repetitionPenalty,
            repetitionContextSize: 64))
    let raw = try await session.respond(to: prompt)
    return raw.trimmingCharacters(in: .whitespacesAndNewlines)
}

/// Generate a short, plain-text title from already-generated notes, reusing the
/// loaded model container. Returns "" if nothing usable comes back.
func generateTitle(container: ModelContainer, notes: String) async throws -> String {
    let instructions = """
        You write a short title for a meeting, given its notes.

        Rules:
        - Output ONLY the title text — no quotes, no Markdown, no preamble, no trailing punctuation.
        - Keep it short and specific: at most 6 words and 40 characters.
        - Use Title Case. Use only facts present in the notes; never invent names.
        - Do not start with "Meeting", "Notes", "Summary", or a date.
        """
    let session = ChatSession(
        container,
        instructions: instructions,
        generateParameters: GenerateParameters(
            maxTokens: 32, temperature: 0.3,
            repetitionPenalty: 1.15, repetitionContextSize: 64))
    let raw = try await session.respond(to: "Notes:\n\(notes)")
    return sanitizeTitle(raw)
}

/// Reduce a model's title output to one clean line: take the first non-empty
/// non-fence line, strip surrounding quotes / Markdown markers (both ends),
/// collapse whitespace, drop trailing sentence punctuation, strip a leading
/// "Meeting"/"Notes"/"Summary" token, and cap at 40 characters on a word
/// boundary. Returns "" when nothing usable remains.
func sanitizeTitle(_ raw: String) -> String {
    let firstLine = raw
        .replacingOccurrences(of: "\r\n", with: "\n")
        .components(separatedBy: "\n")
        .map { $0.trimmingCharacters(in: .whitespaces) }
        .first(where: { !$0.isEmpty && !$0.hasPrefix("```") }) ?? ""
    var s = firstLine
    // Strip the same marker set from BOTH ends so e.g. "**Budget Review**"
    // fully unwraps (a leading-only strip would leave the trailing "**").
    let markers = "#*->\"'`"
    while let f = s.first, markers.contains(f) { s.removeFirst() }
    s = s.trimmingCharacters(in: .whitespaces)
    while let l = s.last, markers.contains(l) { s.removeLast() }
    s = s.trimmingCharacters(in: .whitespaces)
    s = s.split(whereSeparator: { $0 == " " || $0 == "\t" }).joined(separator: " ")
    while let l = s.last, ".,;:".contains(l) { s.removeLast() }
    s = s.trimmingCharacters(in: .whitespaces)
    // Deterministically drop a leading "Meeting"/"Notes"/"Summary" token (a
    // stated title constraint the model sometimes violates), optionally
    // followed by ":"/"-"/whitespace. Loop so "Meeting Notes: X" -> "X". Strip
    // from the original-case `s` so the remainder keeps its casing.
    let bannedPrefixes = ["meeting", "notes", "summary"]
    var strippedPrefix = true
    while strippedPrefix {
        strippedPrefix = false
        for prefix in bannedPrefixes where s.lowercased().hasPrefix(prefix) {
            let rest = String(s.dropFirst(prefix.count))
            // Only strip if the token is a whole word (next char isn't a letter
            // or digit continuing it), so "Meetings Recap" is left untouched.
            if let next = rest.first, next.isLetter || next.isNumber { continue }
            var trimmed = rest
            while let f = trimmed.first, f == ":" || f == "-" || f == " " || f == "\t" {
                trimmed.removeFirst()
            }
            s = trimmed.trimmingCharacters(in: .whitespaces)
            strippedPrefix = true
            break
        }
    }
    let maxChars = 40
    if s.count > maxChars {
        let capped = String(s.prefix(maxChars))
        if let lastSpace = capped.lastIndex(of: " ") {
            s = String(capped[..<lastSpace]).trimmingCharacters(in: .whitespaces)
        } else {
            s = capped
        }
    }
    return s
}

/// Load the model once, generate notes, then a title from those notes.
func generateNotesAndTitle(transcript: String, modelsURL: URL) async throws -> NotesResult {
    let container = try await loadNotesModel(modelsURL: modelsURL)
    let notes = try await generateNotes(container: container, transcript: transcript)
    let title = (try? await generateTitle(container: container, notes: notes)) ?? ""
    return NotesResult(title: title, notes: notes)
}

/// The small notes model often wraps its whole answer in a ```markdown … ```
/// fence despite the prompt. Strip fence markers so `note.md` is raw Markdown:
/// if the content is fully wrapped, unwrap it; otherwise drop any stray fence
/// lines. Meeting notes never contain a legitimate code block, so removing all
/// ``` lines is safe here.
func stripCodeFence(_ raw: String) -> String {
    let normalized = raw.replacingOccurrences(of: "\r\n", with: "\n")
    let kept = normalized
        .components(separatedBy: "\n")
        .filter { !$0.trimmingCharacters(in: .whitespaces).hasPrefix("```") }
    return kept.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
}

// MARK: - Entry

let arguments = CommandLine.arguments

guard let modelsPath = argValue("--models") else { fail("missing --models") }
let modelsURL = URL(fileURLWithPath: modelsPath)
// The STT models (ASR + diarizer) are downloaded and integrity-verified by the
// Rust app from the project CDN (see download_local_stt) and laid out where
// FluidAudio expects them, so this sidecar only LOADS them — it never downloads.
let asrDir = modelsURL.appendingPathComponent("parakeet-tdt-0.6b-v3")
let diarizerDir = modelsURL.appendingPathComponent("speaker-diarization")

let command = arguments.count > 1 ? arguments[1] : "transcribe"
let isNotes = command == "notes"
let isCompletion = command == "llm-complete"

if isCompletion {
    guard let promptPath = argValue("--prompt") else { fail("missing --prompt") }
    let maxTokens = boundedIntArg("--max-tokens", default: 2048, range: 1...4096)
    let temperature = boundedFloatArg("--temperature", default: 0.3, range: 0...2)
    let repetitionPenalty = boundedFloatArg(
        "--repetition-penalty", default: 1.15, range: 0.5...2)
    let prompt: String
    do {
        prompt = try String(contentsOf: URL(fileURLWithPath: promptPath), encoding: .utf8)
    } catch {
        fail("completion prompt read error")
    }
    runToCompletion {
        do {
            let container = try await loadNotesModel(modelsURL: modelsURL)
            let text = try await generateCompletion(
                container: container,
                prompt: prompt,
                maxTokens: maxTokens,
                temperature: temperature,
                repetitionPenalty: repetitionPenalty)
            let data = try JSONEncoder().encode(CompletionResult(text: text))
            FileHandle.standardOutput.write(data)
        } catch {
            fail("completion error: \(error)")
        }
    }
}

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
            let result = try await generateNotesAndTitle(transcript: transcript, modelsURL: modelsURL)
            let data = try JSONEncoder().encode(result)
            FileHandle.standardOutput.write(data)
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
