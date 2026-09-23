import Foundation
import MLX
import MLXAudioSTT

/// Qwen3-ASR emits text only; the forced aligner turns each chunk's text into
/// timed words so the shared diarizer merge can attribute speakers.
///
/// Chunking happens here, not inside `Qwen3ASRModel.generate`: its `maxTokens`
/// is ONE budget shared by every internal chunk (later chunks are skipped once
/// it is spent), and the aligner accepts at most ~5 minutes per call. 290 s
/// targets plus the splitter's ±5 s silence search keep every chunk under 300 s.
enum Qwen3Transcriber {
    static let asrDir = "qwen3-asr-0.6b-4bit"
    static let alignerDir = "qwen3-forcedaligner-0.6b-4bit"
    static let sampleRate = 16000
    static let chunkSeconds: Float = 290

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

    private static func run(samples: [Float], modelsURL: URL) async throws -> Output {
        // fromModelDirectory only — fromPretrained would download.
        let asr = try await Qwen3ASRModel.fromModelDirectory(modelsURL.appendingPathComponent(asrDir))
        let aligner = try await Qwen3ForcedAlignerModel.fromModelDirectory(
            modelsURL.appendingPathComponent(alignerDir))

        let chunks = splitAudioIntoChunks(
            MLXArray(samples), sampleRate: sampleRate, chunkDuration: chunkSeconds)
        var words: [TimedWord] = []
        var texts: [String] = []
        var languages: [String] = []

        for (chunk, offset) in chunks {
            defer { Memory.clearCache() }
            // chunkDuration above the chunk length: never re-split inside generate.
            let out = asr.generate(audio: chunk, maxTokens: 8192, language: nil, chunkDuration: 600)
            let text = out.text.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !text.isEmpty else { continue }
            let language = (out.language ?? "English")
                .split(separator: ",").first.map(String.init) ?? "English"
            texts.append(text)
            if language.lowercased() != "none" { languages.append(language) }

            let aligned = aligner.generate(audio: chunk, text: text, language: alignerLanguage(language))
            warnIfTailUncovered(
                lastWordEnd: aligned.items.last?.endTime ?? 0,
                chunkSamples: chunk.dim(0), offset: offset, samples: samples)
            var chunkWords = attachSeparators(items: aligned.items, text: text)
            // Chunks are joined like words: a space unless either side is CJK.
            if let first = chunkWords.first, let previous = words.last,
               needsSpace(between: previous.text, and: first.text) {
                chunkWords[0] = TimedWord(text: " " + first.text, start: first.start, end: first.end)
            }
            for word in chunkWords {
                words.append(TimedWord(
                    text: word.text,
                    start: word.start + Double(offset),
                    end: word.end + Double(offset)))
            }
        }

        return Output(words: words, text: joinWords(texts), language: isoCode(dominant(languages)))
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
        var sumSquares: Float = 0
        for i in from..<to { sumSquares += samples[i] * samples[i] }
        let rms = (sumSquares / Float(to - from)).squareRoot()
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
