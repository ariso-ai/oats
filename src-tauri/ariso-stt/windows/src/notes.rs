//! Windows adapter for local meeting-note generation.
//!
//! This module preserves the sidecar's Markdown-on-stdout contract while using
//! a bundled llama.cpp runtime and GGUF model. Model acquisition, readiness UX,
//! transcript persistence, and retry scheduling remain in the Tauri host.

use crate::models::discover_gemma;
use anyhow::{Context, Result, anyhow, bail};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
#[cfg(windows)]
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};

const DEFAULT_MAX_TOKENS: u32 = 512;
const DEFAULT_CONTEXT_SIZE: u32 = 4096;
const CHUNK_MAX_TOKENS: u32 = 256;
const PROMPT_RESERVE_TOKENS: u32 = 768;
const CONSERVATIVE_CHARS_PER_TOKEN: usize = 3;

/// Implements the notes subcommand as a narrow file-to-stdout adapter. It does
/// not update recording metadata itself; the host interprets success or failure
/// and owns the durable notes lifecycle shared with macOS.
pub(crate) fn run_notes(transcript: &Path, models: &Path) -> Result<()> {
    let gemma = discover_gemma(models)?;
    let llama_cli = discover_notes_runtime()?;
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
    let max_tokens = DEFAULT_MAX_TOKENS;
    let ctx_size = DEFAULT_CONTEXT_SIZE;
    let final_budget = input_char_budget(ctx_size, max_tokens)?;

    if transcript.chars().count() <= final_budget {
        return run_llama_prompt(
            llama_cli,
            gemma,
            &gemma_notes_prompt(transcript),
            prompt_dir,
            max_tokens,
            ctx_size,
        );
    }

    let summary_tokens = max_tokens.min(CHUNK_MAX_TOKENS);
    let summary_budget = input_char_budget(ctx_size, summary_tokens)?;
    let transcript_chunks = chunk_text(transcript, summary_budget);
    let total_chunks = transcript_chunks.len();
    let mut summaries = Vec::with_capacity(total_chunks);

    for (index, chunk) in transcript_chunks.iter().enumerate() {
        summaries.push(run_llama_prompt(
            llama_cli,
            gemma,
            &chunk_summary_prompt(chunk, index + 1, total_chunks),
            prompt_dir,
            summary_tokens,
            ctx_size,
        )?);
    }

    // Very long recordings can produce enough partial summaries to overflow the
    // final prompt too. Repeatedly condense bounded groups until one final Gemma
    // request can see all retained facts at once.
    loop {
        let combined = join_summaries(&summaries);
        if combined.chars().count() <= final_budget {
            return run_llama_prompt(
                llama_cli,
                gemma,
                &gemma_notes_prompt(&combined),
                prompt_dir,
                max_tokens,
                ctx_size,
            );
        }

        let groups = pack_summaries(&summaries, summary_budget);
        let group_count = groups.len();
        let mut reduced = Vec::with_capacity(group_count);
        for (index, group) in groups.iter().enumerate() {
            reduced.push(run_llama_prompt(
                llama_cli,
                gemma,
                &summary_reduction_prompt(group, index + 1, group_count),
                prompt_dir,
                summary_tokens,
                ctx_size,
            )?);
        }
        summaries = reduced;
    }
}

/// Converts the model context into a conservative source-text allowance. The
/// reserve covers Gemma turn markers, instructions, and generation output; a
/// deliberately low characters-per-token estimate keeps punctuation-heavy
/// speaker transcripts away from llama.cpp's hard context boundary.
fn input_char_budget(ctx_size: u32, output_tokens: u32) -> Result<usize> {
    let source_tokens = ctx_size
        .checked_sub(output_tokens)
        .and_then(|remaining| remaining.checked_sub(PROMPT_RESERVE_TOKENS))
        .ok_or_else(|| {
            anyhow!("notes context size {ctx_size} is too small for {output_tokens} output tokens")
        })?;
    if source_tokens == 0 {
        bail!("notes context leaves no room for transcript input");
    }
    Ok(source_tokens as usize * CONSERVATIVE_CHARS_PER_TOKEN)
}

/// Packs source material at word boundaries while measuring in Unicode scalar
/// values. This avoids slicing UTF-8 or ordinary speaker text mid-word and gives
/// every model invocation a hard, testable size ceiling.
fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let max_chars = max_chars.max(1);

    for word in text.split_whitespace() {
        if word.chars().count() > max_chars {
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            let mut remaining = word;
            while remaining.chars().count() > max_chars {
                let split = byte_index_after_chars(remaining, max_chars);
                chunks.push(remaining[..split].to_string());
                remaining = &remaining[split..];
            }
            current.push_str(remaining);
            continue;
        }

        let separator = usize::from(!current.is_empty());
        if !current.is_empty()
            && current.chars().count() + separator + word.chars().count() > max_chars
        {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }

    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn byte_index_after_chars(text: &str, char_count: usize) -> usize {
    text.char_indices()
        .nth(char_count)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

/// Packs complete partial summaries together instead of splitting their
/// evidence mid-item. A defensive chunking path handles unexpected model
/// output that exceeds its requested generation limit.
fn pack_summaries(summaries: &[String], max_chars: usize) -> Vec<String> {
    let mut groups = Vec::new();
    let mut current = String::new();

    for summary in summaries {
        for part in chunk_text(summary, max_chars) {
            let separator = if current.is_empty() { 0 } else { 2 };
            if !current.is_empty()
                && current.chars().count() + separator + part.chars().count() > max_chars
            {
                groups.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push_str("\n\n");
            }
            current.push_str(&part);
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

fn join_summaries(summaries: &[String]) -> String {
    summaries
        .iter()
        .enumerate()
        .map(|(index, summary)| format!("Partial summary {}:\n{summary}", index + 1))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn chunk_summary_prompt(chunk: &str, index: usize, total: usize) -> String {
    format!(
        "<bos><start_of_turn>user\n\
You are preserving evidence from one chunk of a longer meeting transcript. Summarize only facts stated in this chunk. Retain decisions, commitments, action items, unresolved questions, and generic speaker labels. Be concise and do not add a preamble.\n\n\
Transcript chunk {index} of {total}:\n{chunk}<end_of_turn>\n<start_of_turn>model\n"
    )
}

fn summary_reduction_prompt(group: &str, index: usize, total: usize) -> String {
    format!(
        "<bos><start_of_turn>user\n\
Consolidate these partial meeting summaries without dropping decisions, commitments, action items, unresolved questions, or speaker labels. Use only the supplied facts, remove repetition, and return concise Markdown with no preamble.\n\n\
Summary group {index} of {total}:\n{group}<end_of_turn>\n<start_of_turn>model\n"
    )
}

/// Owns one llama.cpp invocation and attaches it to a kill-on-close Windows job.
/// The Tauri host can therefore enforce its timeout by killing the sidecar;
/// Windows then tears down this descendant instead of leaving model inference
/// running invisibly after the recording has already reported a failure.
fn run_llama_prompt(
    llama_cli: &Path,
    gemma: &Path,
    prompt: &str,
    prompt_dir: &Path,
    max_tokens: u32,
    ctx_size: u32,
) -> Result<String> {
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
    let mut command = Command::new(llama_cli);
    command
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
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| format!("spawn llama.cpp notes runtime {}", llama_cli.display()))?;
    let _job = match KillOnCloseJob::assign(&child) {
        Ok(job) => job,
        Err(err) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(err).context("attach llama.cpp notes runtime to Windows job");
        }
    };
    let output = child
        .wait_with_output()
        .context("wait for llama.cpp notes runtime")?;
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

#[cfg(windows)]
struct KillOnCloseJob(HANDLE);

#[cfg(windows)]
impl KillOnCloseJob {
    fn assign(child: &Child) -> Result<Self> {
        // SAFETY: all handles are checked before use, the information buffer has
        // the exact Win32 layout and lifetime required by SetInformationJobObject,
        // and this type closes its sole owned job handle in Drop.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(std::io::Error::last_os_error()).context("create Windows job object");
            }

            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                std::ptr::addr_of!(limits).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                let error = std::io::Error::last_os_error();
                CloseHandle(job);
                return Err(error).context("configure Windows job object");
            }

            if AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) == 0 {
                let error = std::io::Error::last_os_error();
                CloseHandle(job);
                return Err(error).context("assign llama.cpp to Windows job object");
            }
            Ok(Self(job))
        }
    }
}

#[cfg(windows)]
impl Drop for KillOnCloseJob {
    fn drop(&mut self) {
        // SAFETY: the constructor returns only after taking ownership of a valid
        // job handle, and Drop runs exactly once for that handle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[cfg(not(windows))]
struct KillOnCloseJob;

#[cfg(not(windows))]
impl KillOnCloseJob {
    fn assign(_child: &Child) -> Result<Self> {
        Ok(Self)
    }
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

/// Finds the llama.cpp executable shipped beside the sidecar as a signed Tauri
/// resource. Model downloads contain data only and cannot introduce executable
/// code after the app has been installed.
pub(crate) fn discover_notes_runtime() -> Result<PathBuf> {
    let sidecar = std::env::current_exe().context("resolve ariso-stt executable")?;
    discover_notes_runtime_from(&sidecar)
}

fn discover_notes_runtime_from(sidecar: &Path) -> Result<PathBuf> {
    let runtime = sidecar
        .parent()
        .ok_or_else(|| anyhow!("ariso-stt executable has no parent directory"))?
        .join("llama")
        .join("llama-cli.exe");
    if runtime.is_file() {
        Ok(runtime)
    } else {
        bail!(
            "packaged llama.cpp notes runtime not found at {}",
            runtime.display()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_packaged_llama_runtime_beside_sidecar() {
        let temp = tempfile::tempdir().unwrap();
        let sidecar = temp.path().join("ariso-stt.exe");
        let runtime = temp.path().join("llama").join("llama-cli.exe");
        fs::create_dir_all(runtime.parent().unwrap()).unwrap();
        fs::write(&sidecar, b"sidecar").unwrap();
        fs::write(&runtime, b"runtime").unwrap();
        assert_eq!(discover_notes_runtime_from(&sidecar).unwrap(), runtime);
    }

    #[test]
    fn notes_prompt_uses_gemma_turn_markers() {
        let prompt = gemma_notes_prompt("Speaker 1: Ship it.");
        assert!(prompt.starts_with("<bos><start_of_turn>user\n"));
        assert!(prompt.contains("Transcript:\nSpeaker 1: Ship it."));
        assert!(prompt.ends_with("<start_of_turn>model\n"));
    }

    #[test]
    fn transcript_chunks_are_unicode_safe_and_bounded() {
        let transcript = "Speaker 1: Résumé approved.\nSpeaker 2: Ship the café update tomorrow.";
        let chunks = chunk_text(transcript, 24);

        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 24));
        assert_eq!(
            chunks.join(" ").split_whitespace().collect::<Vec<_>>(),
            transcript.split_whitespace().collect::<Vec<_>>()
        );
    }

    #[test]
    fn notes_budget_reserves_prompt_and_generation_space() {
        assert_eq!(input_char_budget(4096, 512).unwrap(), 8448);
        assert!(input_char_budget(1024, 512).is_err());
    }

    #[test]
    fn partial_summaries_are_packed_without_exceeding_the_limit() {
        let summaries = vec!["alpha beta".to_string(), "gamma delta".to_string()];
        let groups = pack_summaries(&summaries, 12);

        assert_eq!(groups, vec!["alpha beta", "gamma delta"]);
        assert!(groups.iter().all(|group| group.chars().count() <= 12));
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
