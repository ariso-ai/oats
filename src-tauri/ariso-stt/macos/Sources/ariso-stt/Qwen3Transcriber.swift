import Foundation
import MLX
import MLXAudioSTT

/// Qwen3-ASR emits text only; the forced aligner turns each chunk's text into
/// timed words so the shared diarizer merge can attribute speakers.
///
/// Chunking happens here, not inside `Qwen3ASRModel.generate`: its `maxTokens`
/// is ONE budget shared by every internal chunk (later chunks are skipped once
/// it is spent), and the aligner accepts at most ~5 minutes per call.
///
/// Greedy decoding can fall into a phrase loop on long chunks (a 290 s chunk
/// repeated one sentence ~900 times and lost ~200 s of speech; the library's
/// only loop guard catches ≤3 distinct tokens). Two layers stop that:
/// short chunks (`chunkSeconds`) prevent it, and every chunk's result is
/// checked (`isDegenerate`) — a degenerate chunk is re-split in halves down to
/// `minSplitSeconds`, and at the floor any loop is collapsed to one occurrence
/// with a stderr warning. Looped text is never emitted.
enum Qwen3Transcriber {
    static let asrDir = "qwen3-asr-0.6b-4bit"
    static let alignerDir = "qwen3-forcedaligner-0.6b-4bit"
    static let sampleRate = 16000
    /// Target ASR chunk length; the splitter's ±5 s silence search may stretch it.
    /// Measured on a 348 s two-speaker Mandarin fixture: 290 s and 120 s chunks
    /// each hit a decode loop; 60, 90 and 150 s did not. 60 s is the most
    /// margin at no cost (wall time was the same, and so was accuracy).
    static let chunkSeconds: Float = 60
    /// Degenerate chunks are halved while each half stays at least this long.
    static let minSplitSeconds: Float = 10
    /// Token budget per second of audio. Dense Mandarin runs ~5–8 tokens/s;
    /// 15/s plus a fixed allowance only runs out in a decode loop.
    static let tokensPerSecond: Float = 15
    static let baseTokens = 64

    struct Output {
        let words: [TimedWord]
        let text: String
        let language: String
    }

    /// stdout carries only the JSON contract, but mlx-audio-swift `print`s
    /// progress (e.g. "Generated tokenizer.json at: …" on first load), so the
    /// whole Qwen3 run writes stdout to stderr.
    static func transcribe(samples: [Float], modelsURL: URL) async throws -> Output {
        fflush(stdout)
        let savedStdout = dup(STDOUT_FILENO)
        dup2(STDERR_FILENO, STDOUT_FILENO)
        defer {
            fflush(stdout)
            dup2(savedStdout, STDOUT_FILENO)
            close(savedStdout)
        }
        return try await run(samples: samples, modelsURL: modelsURL)
    }

    /// One transcribed and aligned piece of audio, offsets relative to the recording.
    struct Piece {
        let text: String
        let language: String
        let words: [TimedWord]
    }

    private static func run(samples: [Float], modelsURL: URL) async throws -> Output {
        // fromModelDirectory only — fromPretrained would download.
        let asr = try await Qwen3ASRModel.fromModelDirectory(modelsURL.appendingPathComponent(asrDir))
        let aligner = try await Qwen3ForcedAlignerModel.fromModelDirectory(
            modelsURL.appendingPathComponent(alignerDir))

        let chunks = splitAudioIntoChunks(
            MLXArray(samples), sampleRate: sampleRate, chunkDuration: chunkSeconds)
        var pieces: [Piece] = []
        for (chunk, offset) in chunks {
            pieces += transcribeChunk(
                chunk, offset: offset, asr: asr, aligner: aligner, samples: samples)
        }

        var words: [TimedWord] = []
        var texts: [String] = []
        var languages: [String] = []
        for piece in pieces {
            texts.append(piece.text)
            if piece.language.lowercased() != "none" { languages.append(piece.language) }
            var pieceWords = piece.words
            // Pieces are joined like words: a space unless either side is CJK.
            if let first = pieceWords.first, let previous = words.last,
               needsSpace(between: previous.text, and: first.text) {
                pieceWords[0] = TimedWord(text: " " + first.text, start: first.start, end: first.end)
            }
            words += pieceWords
        }
        return Output(words: words, text: joinWords(texts), language: isoCode(dominant(languages)))
    }

    /// Transcribe and align one chunk; if the result is degenerate, re-split it
    /// in halves (recursively) until pieces would drop below `minSplitSeconds`.
    private static func transcribeChunk(
        _ chunk: MLXArray, offset: Float, asr: Qwen3ASRModel,
        aligner: Qwen3ForcedAlignerModel, samples: [Float]
    ) -> [Piece] {
        defer { Memory.clearCache() }
        let seconds = Float(chunk.dim(0)) / Float(sampleRate)
        let budget = baseTokens + Int(seconds * tokensPerSecond)
        // chunkDuration above the chunk length: never re-split inside generate.
        let out = asr.generate(
            audio: chunk, maxTokens: budget, language: nil, chunkDuration: max(600, seconds + 1))
        var text = out.text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return [] }
        let language = (out.language ?? "English")
            .split(separator: ",").first.map(String.init) ?? "English"

        let looped = findLoop(in: text) != nil
        let overBudget = out.generationTokens >= budget
        var aligned: ForcedAlignResult?
        if !looped && !overBudget {
            aligned = aligner.generate(audio: chunk, text: text, language: alignerLanguage(language))
        }
        let unaligned = aligned.map { $0.items.isEmpty && !isSilent(
            samples: samples, offset: offset, count: chunk.dim(0)) } ?? false

        if looped || overBudget || unaligned {
            let reason = looped ? "repetition loop" : overBudget ? "token budget exhausted" : "no aligned words"
            if seconds / 2 >= minSplitSeconds {
                stderrLine(String(
                    format: "warning: qwen3 chunk %.1fs-%.1fs degenerate (%@); re-splitting",
                    Double(offset), Double(offset + seconds), reason))
                let halves = splitAudioIntoChunks(chunk, sampleRate: sampleRate, chunkDuration: seconds / 2)
                return halves.flatMap { half, halfOffset in
                    transcribeChunk(
                        half, offset: offset + halfOffset, asr: asr, aligner: aligner, samples: samples)
                }
            }
            // At the floor: keep what was said once, never the loop.
            text = collapseLoops(text)
            stderrLine(String(
                format: "warning: qwen3 chunk %.1fs-%.1fs still degenerate (%@) at the %.0fs floor; loops collapsed",
                Double(offset), Double(offset + seconds), reason, Double(minSplitSeconds)))
            if aligned == nil {
                aligned = aligner.generate(audio: chunk, text: text, language: alignerLanguage(language))
            }
        }

        let items = aligned?.items ?? []
        warnIfTailUncovered(
            lastWordEnd: items.last?.endTime ?? 0,
            chunkSamples: chunk.dim(0), offset: offset, samples: samples)
        let words = attachSeparators(items: items, text: text).map {
            TimedWord(text: $0.text, start: $0.start + Double(offset), end: $0.end + Double(offset))
        }
        return [Piece(text: text, language: language, words: words)]
    }

    /// A decode loop: some unit of `period` characters repeated at least
    /// `minLoopRepeats` times back to back, spanning at least `minLoopChars`
    /// characters. The span floor keeps short natural repeats ("哈哈哈",
    /// "no no no") from counting. Returns the loop's start, period and repeat
    /// count, earliest start first.
    static let minLoopRepeats = 4
    static let minLoopChars = 24
    static let maxLoopPeriod = 200

    static func findLoop(in text: String) -> (start: Int, period: Int, repeats: Int)? {
        findLoop(Array(text))
    }

    static func findLoop(_ c: [Character]) -> (start: Int, period: Int, repeats: Int)? {
        var best: (start: Int, period: Int, repeats: Int)?
        for period in 1...max(1, min(maxLoopPeriod, c.count / minLoopRepeats)) {
            // run = consecutive positions i with c[i] == c[i + period]; a run of
            // length L starting at i means c[i ..< i + L + period] has this period.
            var runStart = 0
            var run = 0
            var i = 0
            while i + period <= c.count {
                if i + period < c.count && c[i] == c[i + period] {
                    if run == 0 { runStart = i }
                    run += 1
                } else {
                    let span = run + period
                    let repeats = span / period
                    if run > 0 && repeats >= minLoopRepeats && span >= minLoopChars {
                        if best == nil || runStart < best!.start {
                            best = (runStart, period, repeats)
                        }
                        break
                    }
                    run = 0
                }
                i += 1
            }
        }
        return best
    }

    /// Replace every decode loop with a single occurrence of its unit.
    static func collapseLoops(_ text: String) -> String {
        var c = Array(text)
        while let loop = findLoop(c) {
            let keepEnd = loop.start + loop.period
            let resumeAt = loop.start + loop.repeats * loop.period
            c.removeSubrange(keepEnd..<resumeAt)
        }
        return String(c)
    }

    /// True when the chunk's audio is below the silence threshold throughout.
    static func isSilent(samples: [Float], offset: Float, count: Int) -> Bool {
        let from = min(samples.count, Int(Double(offset) * Double(sampleRate)))
        let to = min(samples.count, from + count)
        return rms(samples, from, to) <= silenceRMS
    }

    static func rms(_ samples: [Float], _ from: Int, _ to: Int) -> Float {
        guard to > from else { return 0 }
        var sumSquares: Float = 0
        for i in from..<to { sumSquares += samples[i] * samples[i] }
        return (sumSquares / Float(to - from)).squareRoot()
    }

    /// Uncovered tail longer than this (seconds) that is not silent suggests
    /// the ASR stopped early (e.g. hit its token budget) and dropped speech.
    static let uncoveredTailWarnSeconds = 30.0
    /// RMS above this (about -40 dBFS) counts as "not silent".
    static let silenceRMS: Float = 0.01

    /// Warn on stderr when the chunk's audio after its last aligned word is
    /// long and not silent — a sign of silently truncated transcription.
    static func warnIfTailUncovered(
        lastWordEnd: Double, chunkSamples: Int, offset: Float, samples: [Float]
    ) {
        let chunkSeconds = Double(chunkSamples) / Double(sampleRate)
        guard chunkSeconds - lastWordEnd > uncoveredTailWarnSeconds else { return }
        let chunkStart = Int(Double(offset) * Double(sampleRate))
        let from = min(samples.count, chunkStart + Int(lastWordEnd * Double(sampleRate)))
        let to = min(samples.count, chunkStart + chunkSamples)
        guard to > from else { return }
        let rms = rms(samples, from, to)
        guard rms > silenceRMS else { return }
        let start = Double(offset) + lastWordEnd
        let end = Double(offset) + chunkSeconds
        stderrLine(String(
            format: "warning: qwen3 chunk at %.1fs: %.1fs-%.1fs has audio (rms %.3f) but no aligned words",
            Double(offset), start, end, rms))
    }

    /// The aligner returns bare words: its tokenizer drops every character that
    /// is not a letter, digit or apostrophe, so punctuation and spacing are gone.
    /// Walk the ASR text and hand the original text between two aligned words
    /// back to them: what follows the previous word up to the first whitespace
    /// (its trailing punctuation, e.g. "，" "。" "," ".") stays with the previous
    /// word; the whitespace and anything after it (e.g. an opening quote) leads
    /// the next word. Text after the last word goes to the last word. Rendering
    /// is then plain concatenation, which keeps punctuation and needs no
    /// per-script spacing rules. If the words cannot be matched back onto the
    /// text, fall back to bare words joined by `joinWords`' spacing rule.
    static func attachSeparators(items: [ForcedAlignItem], text: String) -> [TimedWord] {
        let isKept = ForceAlignProcessor().isKeptChar
        let chars = Array(text)
        var cursor = 0
        // Each word as (start, end) indices into `chars`, including separators.
        var spans: [(start: Int, end: Int)] = []

        matching: for item in items {
            let gapStart = cursor
            var wordStart: Int?
            for needed in item.text {
                // Skip separators/punctuation until the next kept character.
                while cursor < chars.count, chars[cursor] != needed, !isKept(chars[cursor]) {
                    cursor += 1
                }
                guard cursor < chars.count, chars[cursor] == needed else { break matching }
                if wordStart == nil { wordStart = cursor }
                cursor += 1
            }
            guard let wordStart else { break matching }
            // Split the gap: trailing punctuation to the previous word, the rest here.
            var split = gapStart
            if !spans.isEmpty {
                while split < wordStart, !chars[split].isWhitespace { split += 1 }
                spans[spans.count - 1].end = split
            }
            spans.append((start: split, end: cursor))
        }

        guard spans.count == items.count, !spans.isEmpty else {
            var fallback: [TimedWord] = []
            for item in items {
                let separator = fallback.last.map { needsSpace(between: $0.text, and: item.text) ? " " : "" } ?? ""
                fallback.append(TimedWord(
                    text: separator + item.text, start: item.startTime, end: item.endTime))
            }
            return fallback
        }
        spans[spans.count - 1].end = chars.count
        return zip(items, spans).map { item, span in
            TimedWord(
                text: String(chars[span.start..<span.end]), start: item.startTime, end: item.endTime)
        }
    }

    /// The aligner splits "Chinese" into characters (keeping embedded Latin
    /// words whole) and everything else on spaces.
    static func alignerLanguage(_ language: String) -> String {
        switch language {
        case "Chinese", "Cantonese": return "Chinese"
        default: return language
        }
    }

    static func dominant(_ languages: [String]) -> String {
        let counts = Dictionary(languages.map { ($0, 1) }, uniquingKeysWith: +)
        return counts.max { a, b in a.value < b.value || (a.value == b.value && a.key > b.key) }?.key ?? "English"
    }

    static let isoCodes: [String: String] = [
        "Chinese": "zh", "English": "en", "Cantonese": "yue", "Arabic": "ar", "German": "de",
        "French": "fr", "Spanish": "es", "Portuguese": "pt", "Indonesian": "id", "Italian": "it",
        "Korean": "ko", "Russian": "ru", "Thai": "th", "Vietnamese": "vi", "Japanese": "ja",
        "Turkish": "tr", "Hindi": "hi", "Malay": "ms", "Dutch": "nl", "Swedish": "sv",
        "Danish": "da", "Finnish": "fi", "Polish": "pl", "Czech": "cs", "Filipino": "fil",
        "Persian": "fa", "Greek": "el", "Romanian": "ro", "Hungarian": "hu", "Macedonian": "mk",
    ]

    static func isoCode(_ language: String) -> String {
        isoCodes[language] ?? language.lowercased()
    }
}

/// Join words for display: a space between two Latin-script words, none where
/// either side is CJK (Chinese text has no inter-word spaces).
func joinWords(_ words: [String]) -> String {
    var out = ""
    for word in words {
        let w = word.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !w.isEmpty else { continue }
        if needsSpace(between: out, and: w) {
            out += " "
        }
        out += w
    }
    return out
}

/// True when joining `left` and `right` needs a space: both boundary
/// characters exist, neither is whitespace, and neither is CJK.
func needsSpace(between left: String, and right: String) -> Bool {
    guard let last = left.unicodeScalars.last, let first = right.unicodeScalars.first else {
        return false
    }
    if last.properties.isWhitespace || first.properties.isWhitespace { return false }
    return !isCJK(last) && !isCJK(first)
}

func isCJK(_ s: Unicode.Scalar) -> Bool {
    switch s.value {
    case 0x3000...0x303F, 0x3040...0x30FF, 0x3400...0x4DBF, 0x4E00...0x9FFF,
         0xAC00...0xD7AF, 0xF900...0xFAFF, 0xFF00...0xFFEF:
        return true
    default:
        return false
    }
}

/// Qwen3 words already carry their leading separators (see
/// `Qwen3Transcriber.attachSeparators`), so rendering is concatenation.
func renderQwen3(_ words: [TimedWord]) -> String {
    words.map { $0.text }.joined().trimmingCharacters(in: .whitespacesAndNewlines)
}
