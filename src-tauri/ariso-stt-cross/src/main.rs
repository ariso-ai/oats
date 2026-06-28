use std::env;
use std::process::ExitCode;

fn usage() -> &'static str {
    "ariso-stt Windows sidecar placeholder\n\n\
     Contract:\n\
       ariso-stt --audio <path> --models <dir> --format json\n\
       ariso-stt download --models <dir>\n\
       ariso-stt notes --transcript <path> --models <dir>\n"
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{}", usage());
        return ExitCode::SUCCESS;
    }

    if args.first().is_some_and(|arg| arg == "download") {
        eprintln!(
            "Windows local model download is not implemented in this placeholder sidecar yet"
        );
        return ExitCode::from(2);
    }

    if args.first().is_some_and(|arg| arg == "notes") {
        eprintln!(
            "Windows local notes generation is not implemented in this placeholder sidecar yet"
        );
        return ExitCode::from(2);
    }

    if args.iter().any(|arg| arg == "--audio") {
        eprintln!(
            "Windows local transcription is not implemented in this placeholder sidecar yet"
        );
        return ExitCode::from(2);
    }

    eprintln!("{}", usage());
    ExitCode::from(2)
}
