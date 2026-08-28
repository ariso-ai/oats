//! Windows local-inference sidecar for oats.
//!
//! The Tauri host launches this executable through the shared `ariso-stt` CLI
//! contract. This entry point only parses and dispatches commands; the Windows
//! inference implementations live in the focused modules below. Model downloads
//! are owned by the Tauri host, matching the macOS sidecar boundary.

mod audio;
mod models;
mod notes;
mod transcribe;

use anyhow::{Result, anyhow, bail};
use notes::{run_completion, run_notes};
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use transcribe::transcribe;

/// Keeps the human-facing help text beside the compatibility boundary it
/// documents. The supported surface intentionally remains smaller than a
/// general inference CLI because the Tauri host is its only production caller.
fn usage() -> &'static str {
    "ariso-stt Windows local inference sidecar\n\n\
     Contract:\n\
       ariso-stt --audio <path> --models <dir> --format json\n\
       ariso-stt notes --transcript <path> --models <dir>\n\
       ariso-stt llm-complete --prompt <path> --models <dir> [generation options]\n"
}

/// Converts rich internal errors into the process contract expected by the
/// Tauri host: stdout is reserved for successful payloads, stderr for diagnosis,
/// and any failure is represented by a stable non-zero exit.
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err:#}");
            ExitCode::from(2)
        }
    }
}

/// Dispatches the shared `ariso-stt` contract without taking ownership of model
/// downloads or recording storage. Those lifecycle concerns remain in the host
/// so macOS and Windows present the same application workflow.
fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{}", usage());
        return Ok(());
    }

    match parse_args(&args)? {
        SidecarCommand::Notes { transcript, models } => run_notes(&transcript, &models),
        SidecarCommand::Complete {
            prompt,
            models,
            max_tokens,
            temperature,
            repetition_penalty,
        } => run_completion(
            &prompt,
            &models,
            max_tokens,
            temperature,
            repetition_penalty,
        ),
        SidecarCommand::Transcribe {
            audio,
            models,
            format,
        } => {
            if format != "json" {
                bail!("unsupported format {format:?}; expected json");
            }
            let output = transcribe(&audio, &models)?;
            println!("{}", serde_json::to_string(&output)?);
            Ok(())
        }
    }
}

#[derive(Debug, PartialEq)]
/// Gives the tiny CLI a typed boundary before it reaches inference code. Paths
/// remain opaque here; existence and bundle completeness are validated by the
/// model-specific modules that understand those layouts.
enum SidecarCommand {
    Notes {
        transcript: PathBuf,
        models: PathBuf,
    },
    Complete {
        prompt: PathBuf,
        models: PathBuf,
        max_tokens: u32,
        temperature: f32,
        repetition_penalty: f32,
    },
    Transcribe {
        audio: PathBuf,
        models: PathBuf,
        format: String,
    },
}

/// Preserves the argv shape already used by the macOS sidecar and Rust caller.
/// This is intentionally not a shell parser, which keeps paths as argv values
/// and avoids platform-specific quoting behavior.
fn parse_args(args: &[String]) -> Result<SidecarCommand> {
    if args.first().is_some_and(|arg| arg == "notes") {
        return Ok(SidecarCommand::Notes {
            transcript: required_path_arg(&args[1..], "--transcript")?,
            models: required_path_arg(&args[1..], "--models")?,
        });
    }

    if args.first().is_some_and(|arg| arg == "llm-complete") {
        let command_args = &args[1..];
        let max_tokens: u32 = optional_parsed_arg(command_args, "--max-tokens")?.unwrap_or(2048);
        let temperature: f32 = optional_parsed_arg(command_args, "--temperature")?.unwrap_or(0.3);
        let repetition_penalty: f32 =
            optional_parsed_arg(command_args, "--repetition-penalty")?.unwrap_or(1.15);
        if !(1..=4096).contains(&max_tokens) {
            bail!("--max-tokens must be between 1 and 4096");
        }
        if !temperature.is_finite() || !(0.0..=2.0).contains(&temperature) {
            bail!("--temperature must be between 0 and 2");
        }
        if !repetition_penalty.is_finite() || !(0.5..=2.0).contains(&repetition_penalty) {
            bail!("--repetition-penalty must be between 0.5 and 2");
        }
        return Ok(SidecarCommand::Complete {
            prompt: required_path_arg(command_args, "--prompt")?,
            models: required_path_arg(command_args, "--models")?,
            max_tokens,
            temperature,
            repetition_penalty,
        });
    }

    if args.iter().any(|arg| arg == "--audio") {
        return Ok(SidecarCommand::Transcribe {
            audio: required_path_arg(args, "--audio")?,
            models: required_path_arg(args, "--models")?,
            format: required_string_arg(args, "--format")?,
        });
    }

    bail!("{}", usage())
}

/// Marks path-valued arguments at the parsing boundary while leaving filesystem
/// policy to the command implementation. A transcript or model path may be
/// relative in tests even though production callers send absolute paths.
fn required_path_arg(args: &[String], flag: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(required_string_arg(args, flag)?))
}

/// Centralizes required-value validation so both commands fail in the same
/// shape and the host can surface one consistent sidecar error contract.
fn required_string_arg(args: &[String], flag: &str) -> Result<String> {
    let pos = args
        .iter()
        .position(|arg| arg == flag)
        .ok_or_else(|| anyhow!("missing required {flag} argument"))?;
    args.get(pos + 1)
        .filter(|value| !value.starts_with("--"))
        .cloned()
        .ok_or_else(|| anyhow!("missing value for {flag}"))
}

fn optional_parsed_arg<T>(args: &[String], flag: &str) -> Result<Option<T>>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let Some(pos) = args.iter().position(|arg| arg == flag) else {
        return Ok(None);
    };
    let raw = args
        .get(pos + 1)
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| anyhow!("missing value for {flag}"))?;
    raw.parse()
        .map(Some)
        .map_err(|error| anyhow!("invalid value for {flag}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_transcribe_contract() {
        let args = vec![
            "--audio".to_string(),
            "meeting.mp3".to_string(),
            "--models".to_string(),
            "models".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];
        assert_eq!(
            parse_args(&args).unwrap(),
            SidecarCommand::Transcribe {
                audio: PathBuf::from("meeting.mp3"),
                models: PathBuf::from("models"),
                format: "json".to_string(),
            }
        );
    }

    #[test]
    fn parses_notes_contract() {
        let args = vec![
            "notes".to_string(),
            "--transcript".to_string(),
            "transcript.md".to_string(),
            "--models".to_string(),
            "models".to_string(),
        ];
        assert_eq!(
            parse_args(&args).unwrap(),
            SidecarCommand::Notes {
                transcript: PathBuf::from("transcript.md"),
                models: PathBuf::from("models"),
            }
        );
    }

    #[test]
    fn parses_completion_contract() {
        let args = vec![
            "llm-complete".to_string(),
            "--prompt".to_string(),
            "prompt.md".to_string(),
            "--models".to_string(),
            "models".to_string(),
            "--max-tokens".to_string(),
            "1024".to_string(),
            "--temperature".to_string(),
            "0.2".to_string(),
            "--repetition-penalty".to_string(),
            "1.1".to_string(),
        ];
        assert_eq!(
            parse_args(&args).unwrap(),
            SidecarCommand::Complete {
                prompt: PathBuf::from("prompt.md"),
                models: PathBuf::from("models"),
                max_tokens: 1024,
                temperature: 0.2,
                repetition_penalty: 1.1,
            }
        );
    }

    #[test]
    fn rejects_unbounded_completion_parameters() {
        let args = vec![
            "llm-complete".to_string(),
            "--prompt".to_string(),
            "prompt.md".to_string(),
            "--models".to_string(),
            "models".to_string(),
            "--max-tokens".to_string(),
            "5000".to_string(),
        ];
        assert!(parse_args(&args).unwrap_err().to_string().contains("4096"));
    }
}
