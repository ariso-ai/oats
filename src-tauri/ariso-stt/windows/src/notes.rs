//! Windows adapter for local meeting-note generation.
//!
//! This module preserves the sidecar's Markdown-on-stdout contract while using
//! a bundled llama.cpp runtime and GGUF model. Model acquisition, readiness UX,
//! transcript persistence, and retry scheduling remain in the Tauri host.

use crate::models::{GEMMA_GGUF, GEMMA_MODEL_DIR, NOTES_MODEL_VERSION, model_dir};
use anyhow::{Context, Result, anyhow, bail};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Implements the notes subcommand as a narrow file-to-stdout adapter. It does
/// not update recording metadata itself; the host interprets success or failure
/// and owns the durable notes lifecycle shared with macOS.
pub(crate) fn run_notes(transcript: &Path, models: &Path) -> Result<()> {
    let gemma = discover_gemma(models)?;
    let llama_cli = discover_notes_runtime(models)?;
    let prompt_dir = transcript
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let transcript_text = fs::read_to_string(transcript)
        .with_context(|| format!("read transcript {}", transcript.display()))?;
    let notes = run_llama_notes(&llama_cli, &gemma, &transcript_text, prompt_dir)?;
    print!("{}", clean_notes(&notes));
    Ok(())
}

/// Isolates llama.cpp process details from the sidecar contract. Prompt-policy
/// evolution belongs here, while model selection and installation remain fixed
/// by the versioned bundle discovered below.
fn run_llama_notes(
    llama_cli: &Path,
    gemma: &Path,
    transcript: &str,
    prompt_dir: &Path,
) -> Result<String> {
    let prompt = gemma_notes_prompt(transcript);
    let mut prompt_file = tempfile::Builder::new()
        .prefix(".oats-notes-prompt-")
        .suffix(".txt")
        .tempfile_in(prompt_dir)
        .with_context(|| format!("create notes prompt in {}", prompt_dir.display()))?;
    prompt_file
        .write_all(prompt.as_bytes())
        .context("write notes prompt")?;
    prompt_file.flush().context("flush notes prompt")?;
    let prompt_path = prompt_file.into_temp_path();
    let max_tokens = env_positive("ARISO_NOTES_MAX_TOKENS", 512);
    let ctx_size = env_positive("ARISO_NOTES_CTX_SIZE", 4096);
    let output = Command::new(llama_cli)
        .arg("-m")
        .arg(gemma)
        .arg("-f")
        .arg(&prompt_path)
        .arg("-n")
        .arg(max_tokens.to_string())
        .arg("-c")
        .arg(ctx_size.to_string())
        .arg("--temp")
        .arg("0.3")
        .arg("--repeat-penalty")
        .arg("1.15")
        .arg("--no-display-prompt")
        .arg("-no-cnv")
        .arg("--single-turn")
        .arg("--simple-io")
        .arg("--log-disable")
        .arg("--no-warmup")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("spawn llama.cpp notes runtime {}", llama_cli.display()))?;
    if !output.status.success() {
        bail!(
            "llama.cpp Gemma notes runtime failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let notes = clean_llama_output(&String::from_utf8_lossy(&output.stdout));
    if notes.is_empty() {
        bail!("llama.cpp Gemma notes runtime returned empty notes");
    }
    Ok(notes)
}

/// Allows smoke tests and diagnostics to constrain expensive generation without
/// turning runtime tuning into a persisted product setting. Invalid overrides
/// intentionally fall back to the supported default.
fn env_positive(name: &str, default: u32) -> u32 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

/// Encodes both the product's notes schema and Gemma's turn protocol in one
/// prompt boundary. The model is asked for content, while this function owns the
/// section contract consumed by the existing notes UI.
fn gemma_notes_prompt(transcript: &str) -> String {
    let instructions = "\
You are a meeting-notes assistant. You are given a meeting transcript and you write concise meeting notes in Markdown.\n\n\
Rules:\n\
- Use only facts stated in the transcript. Never invent details, names, or speakers.\n\
- The transcript labels speakers generically (e.g. \"Speaker 1\", \"Speaker 2\"). Do not invent any speaker or person who does not appear in the transcript.\n\
- Output the notes only, with no preamble, no closing remarks, and never repeat or restate these instructions.\n\
- Output raw Markdown directly. Never wrap the notes in a code fence.\n\
- Use these level-2 (##) sections, in this order: Summary, Key Points, Decisions, Action Items.\n\
- \"Summary\" is 2-3 sentences describing what the meeting was about. The other sections are bullet lists.\n\
- For each action item, state the task. Only attribute it to a speaker if that exact speaker explicitly committed to it in the transcript; otherwise give the task with no owner.\n\
- Omit any section that has no real content in the transcript. Never write placeholder text under a heading.";

    format!(
        "<bos><start_of_turn>user\n{instructions}\n\nTranscript:\n{transcript}<end_of_turn>\n<start_of_turn>model\n"
    )
}

/// Removes a common model-formatting escape without attempting to parse or
/// rewrite Markdown. User-visible headings and bullets must otherwise survive
/// unchanged for the notes editor.
fn strip_code_fences(raw: &str) -> String {
    raw.replace("\r\n", "\n")
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Separates llama-cli transport chatter from generated content so runtime
/// upgrades do not leak prompts, timing output, or shutdown text into saved
/// notes. Content normalization is delegated to `clean_notes`.
fn clean_llama_output(raw: &str) -> String {
    let normalized = raw.replace("\r\n", "\n");
    let mut seen_prompt = false;
    let mut kept = Vec::new();

    for line in normalized.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("> ") {
            seen_prompt = true;
            kept.clear();
            continue;
        }
        if !seen_prompt {
            continue;
        }
        if trimmed.starts_with("[ Prompt:") || trimmed == "Exiting..." {
            break;
        }
        if !trimmed.is_empty() || !kept.is_empty() {
            kept.push(line);
        }
    }

    let cleaned = clean_notes(&kept.join("\n"));
    if cleaned.is_empty() {
        clean_notes(raw)
    } else {
        cleaned
    }
}

/// Anchors output at the first supported notes section when the model adds a
/// preamble. If no known heading exists, preserving the cleaned response gives
/// the host something diagnosable instead of silently discarding generation.
fn clean_notes(raw: &str) -> String {
    let without_fences = strip_code_fences(raw);
    for heading in [
        "## Summary",
        "## Key Points",
        "## Decisions",
        "## Action Items",
    ] {
        if let Some(index) = without_fences.find(heading) {
            return without_fences[index..].trim().to_string();
        }
    }
    without_fences
}

/// Resolves only the host-installed GGUF artifact and never reaches the network.
/// This keeps Local mode auditable: downloads happen through explicit Settings
/// actions, and inference is filesystem-only once readiness is reported.
pub(crate) fn discover_gemma(models: &Path) -> Result<PathBuf> {
    let model = model_dir(models, GEMMA_MODEL_DIR, NOTES_MODEL_VERSION).join(GEMMA_GGUF);
    model
        .is_file()
        .then_some(model)
        .ok_or_else(|| anyhow!("Gemma GGUF model not found under {}", models.display()))
}

/// Couples the native llama.cpp executable to the same versioned notes bundle as
/// the GGUF and DLLs. The sidecar therefore never depends on a machine-global
/// runtime whose ABI could differ from the packaged model stack.
pub(crate) fn discover_notes_runtime(models: &Path) -> Result<PathBuf> {
    let runtime = model_dir(models, GEMMA_MODEL_DIR, NOTES_MODEL_VERSION).join("llama-cli.exe");
    runtime.is_file().then_some(runtime).ok_or_else(|| {
        anyhow!(
            "llama.cpp notes runtime not found under {}",
            models.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{GEMMA_MODEL_DIR, model_dir};

    #[test]
    fn discovers_gemma_qat_gguf_and_llama_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let model = model_dir(temp.path(), GEMMA_MODEL_DIR, NOTES_MODEL_VERSION);
        fs::create_dir_all(&model).unwrap();
        fs::write(model.join(GEMMA_GGUF), b"gguf").unwrap();
        fs::write(model.join("llama-cli.exe"), b"runtime").unwrap();

        assert_eq!(discover_gemma(temp.path()).unwrap(), model.join(GEMMA_GGUF));
        assert_eq!(
            discover_notes_runtime(temp.path()).unwrap(),
            model.join("llama-cli.exe")
        );
    }

    #[test]
    fn notes_prompt_uses_gemma_turn_markers() {
        let prompt = gemma_notes_prompt("Speaker 1: Ship it.");
        assert!(prompt.starts_with("<bos><start_of_turn>user\n"));
        assert!(prompt.contains("Transcript:\nSpeaker 1: Ship it."));
        assert!(prompt.ends_with("<start_of_turn>model\n"));
    }

    #[test]
    fn strips_markdown_code_fences() {
        let cleaned = strip_code_fences("```markdown\n## Summary\n- Done\n```\n");
        assert_eq!(cleaned, "## Summary\n- Done");
    }

    #[test]
    fn cleans_llama_cli_banner_and_perf_footer() {
        let raw = "Loading model...\n\n> <prompt>\n## Summary\n- Done\n\n[ Prompt: 20.0 t/s | Generation: 5.0 t/s ]\n\nExiting...\n";
        assert_eq!(clean_llama_output(raw), "## Summary\n- Done");
    }

    #[test]
    fn cleans_prompt_echo_before_notes_heading() {
        let raw = "Rules:\n- Use sections.\n## Summary\n- Done";
        assert_eq!(clean_notes(raw), "## Summary\n- Done");
    }
}
