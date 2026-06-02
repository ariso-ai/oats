import Foundation

func arg(_ name: String) -> String? {
    let a = CommandLine.arguments
    guard let i = a.firstIndex(of: name), i + 1 < a.count else { return nil }
    return a[i + 1]
}

func fail(_ msg: String) -> Never {
    FileHandle.standardError.write(Data((msg + "\n").utf8))
    exit(1)
}

let args = CommandLine.arguments
let isDownload = args.count > 1 && args[1] == "download"

guard let models = arg("--models") else { fail("missing --models") }

if isDownload {
    // Placeholder: the real implementation downloads FluidAudio models into `models`.
    print("{\"type\":\"progress\",\"fraction\":1.0}")
    print("{\"type\":\"done\"}")
    exit(0)
}

guard let audio = arg("--audio") else { fail("missing --audio") }
_ = audio
_ = models

// Placeholder transcript contract (replaced by real inference in the next task).
let stub = """
{"language":"en","durationSeconds":0.0,"participants":[{"id":0,"label":"Speaker 1"}],"segments":[{"speaker":0,"text":"(transcription placeholder)","start":0.0,"end":0.0}]}
"""
print(stub)
