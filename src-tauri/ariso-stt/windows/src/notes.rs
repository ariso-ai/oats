//! Windows adapter for local meeting-note generation.
//!
//! This module preserves the sidecar's title-and-Markdown JSON contract while
//! using a bundled llama.cpp runtime and GGUF model. Model acquisition,
//! readiness UX, transcript persistence, and retry scheduling remain in the
//! Tauri host.

use crate::models::discover_gemma;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::{io::AsRawHandle, process::CommandExt};
#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
#[cfg(windows)]
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

const DEFAULT_MAX_TOKENS: u32 = 512;
const DEFAULT_CONTEXT_SIZE: u32 = 4096;
const TITLE_MAX_TOKENS: u32 = 32;
const CHUNK_MAX_TOKENS: u32 = 256;
const PROMPT_RESERVE_TOKENS: u32 = 768;
const CONSERVATIVE_CHARS_PER_TOKEN: usize = 3;
const MAX_SOURCE_CHUNKS: usize = 24;
const MAX_REDUCTION_PASSES: usize = 4;
const MAX_MODEL_CALLS: usize = 32;
const NOTES_RUNTIME_BUDGET: Duration = Duration::from_secs(25 * 60);
const SERVER_START_TIMEOUT: Duration = Duration::from_secs(2 * 60);
const SERVER_POLL_INTERVAL: Duration = Duration::from_millis(250);
const SERVER_HEALTH_TIMEOUT: Duration = Duration::from_secs(2);

/// Implements the notes subcommand as a narrow file-to-stdout adapter. It does
/// not update recording metadata itself; the host interprets success or failure
/// and owns the durable notes lifecycle shared with macOS.
pub(crate) fn run_notes(transcript: &Path, models: &Path) -> Result<()> {
    let gemma = discover_gemma(models)?;
    let llama_server = discover_notes_runtime()?;
    let transcript_text = fs::read_to_string(transcript)
        .with_context(|| format!("read transcript {}", transcript.display()))?;
    let mut runtime = LlamaServer::start(&llama_server, &gemma)?;
    let notes = clean_notes(&run_llama_notes(&mut runtime, &transcript_text)?);
    // Title generation is enrichment, not the durable notes operation. Match
    // macOS by returning an empty title when the second model pass fails so the
    // host still saves useful notes and simply keeps the timestamp title.
    let title = run_llama_title(&mut runtime, &notes).unwrap_or_default();
    print!("{}", serialize_notes_result(&title, &notes)?);
    Ok(())
}

/// Mirrors the macOS sidecar's stdout payload so the Tauri host can apply a
/// generated title without knowing which native model adapter produced it.
#[derive(Serialize)]
struct NotesResult<'a> {
    title: &'a str,
    notes: &'a str,
}

fn serialize_notes_result(title: &str, notes: &str) -> Result<String> {
    serde_json::to_string(&NotesResult { title, notes }).context("serialize notes result")
}

/// Isolates llama.cpp process details from the sidecar contract. Prompt-policy
/// evolution belongs here, while model selection and installation remain fixed
/// by the versioned bundle discovered below.
fn run_llama_notes(runtime: &mut impl PromptRunner, transcript: &str) -> Result<String> {
    let max_tokens = DEFAULT_MAX_TOKENS;
    let ctx_size = DEFAULT_CONTEXT_SIZE;
    let final_budget = input_char_budget(ctx_size, max_tokens)?;

    if transcript.chars().count() <= final_budget {
        return runtime.complete(&gemma_notes_prompt(transcript), max_tokens);
    }

    let summary_tokens = max_tokens.min(CHUNK_MAX_TOKENS);
    let summary_budget = input_char_budget(ctx_size, summary_tokens)?;
    let transcript_chunks = chunk_text(transcript, summary_budget);
    let total_chunks = transcript_chunks.len();
    if total_chunks > MAX_SOURCE_CHUNKS {
        bail!(
            "transcript requires {total_chunks} note chunks; local notes support at most {MAX_SOURCE_CHUNKS}"
        );
    }
    let mut summaries = Vec::with_capacity(total_chunks);

    for (index, chunk) in transcript_chunks.iter().enumerate() {
        summaries.push(runtime.complete(
            &chunk_summary_prompt(chunk, index + 1, total_chunks),
            summary_tokens,
        )?);
    }

    // Very long recordings can produce enough partial summaries to overflow the
    // final prompt too. Repeatedly condense bounded groups until one final Gemma
    // request can see all retained facts at once.
    let mut previous_size = join_summaries(&summaries).chars().count();
    for pass in 1..=MAX_REDUCTION_PASSES {
        let combined = join_summaries(&summaries);
        if combined.chars().count() <= final_budget {
            return runtime.complete(&gemma_notes_prompt(&combined), max_tokens);
        }

        let groups = pack_summaries(&summaries, summary_budget);
        let group_count = groups.len();
        let mut reduced = Vec::with_capacity(group_count);
        for (index, group) in groups.iter().enumerate() {
            reduced.push(runtime.complete(
                &summary_reduction_prompt(group, index + 1, group_count),
                summary_tokens,
            )?);
        }
        let reduced_size = join_summaries(&reduced).chars().count();
        if reduced_size >= previous_size {
            bail!(
                "notes reduction pass {pass} did not shrink its input ({previous_size} to {reduced_size} characters)"
            );
        }
        if reduced_size <= final_budget {
            let combined = join_summaries(&reduced);
            return runtime.complete(&gemma_notes_prompt(&combined), max_tokens);
        }
        previous_size = reduced_size;
        summaries = reduced;
    }

    bail!("notes reduction exceeded {MAX_REDUCTION_PASSES} passes")
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

trait PromptRunner {
    fn complete(&mut self, prompt: &str, max_tokens: u32) -> Result<String>;
}

#[derive(Serialize)]
struct CompletionRequest<'a> {
    prompt: &'a str,
    n_predict: u32,
    temperature: f32,
    repeat_penalty: f32,
    stream: bool,
}

#[derive(Deserialize)]
struct CompletionResponse {
    content: String,
}

/// Owns one model-loaded llama.cpp server for the complete notes operation.
/// It binds only to loopback, requires an ephemeral API key, and remains in the
/// sidecar's kill-on-close Job Object so the host timeout still tears down the
/// entire inference process tree.
struct LlamaServer {
    child: Child,
    _job: KillOnCloseJob,
    agent: ureq::Agent,
    completion_url: String,
    authorization: String,
    deadline: Instant,
    model_calls: usize,
}

impl LlamaServer {
    fn start(server: &Path, gemma: &Path) -> Result<Self> {
        let deadline = Instant::now()
            .checked_add(NOTES_RUNTIME_BUDGET)
            .ok_or_else(|| anyhow!("notes runtime deadline overflow"))?;
        let port = reserve_loopback_port()?;
        let api_key = generate_local_api_key()?;
        let authorization = format!("Bearer {api_key}");
        let mut command = Command::new(server);
        command
            .arg("-m")
            .arg(gemma)
            .arg("-c")
            .arg(DEFAULT_CONTEXT_SIZE.to_string())
            .arg("--host")
            .arg(Ipv4Addr::LOCALHOST.to_string())
            .arg("--port")
            .arg(port.to_string())
            .arg("--parallel")
            .arg("1")
            .arg("--no-webui")
            .arg("--no-slots")
            .arg("--api-key")
            .arg(&api_key)
            .arg("--log-disable")
            .arg("--no-warmup")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(parent) = server.parent() {
            command.current_dir(parent);
        }
        configure_background_process(&mut command);

        let mut child = command
            .spawn()
            .with_context(|| format!("spawn llama.cpp notes server {}", server.display()))?;
        let job = match KillOnCloseJob::assign(&child) {
            Ok(job) => job,
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(err).context("attach llama.cpp notes server to Windows job");
            }
        };
        let agent = ureq::AgentBuilder::new().build();
        let mut runtime = Self {
            child,
            _job: job,
            agent,
            completion_url: format!("http://127.0.0.1:{port}/completion"),
            authorization,
            deadline,
            model_calls: 0,
        };
        runtime.wait_until_ready(port)?;
        Ok(runtime)
    }

    fn wait_until_ready(&mut self, port: u16) -> Result<()> {
        let health_url = format!("http://127.0.0.1:{port}/health");
        let startup_deadline = Instant::now()
            .checked_add(SERVER_START_TIMEOUT)
            .ok_or_else(|| anyhow!("llama.cpp startup deadline overflow"))?;

        loop {
            if let Some(status) = self
                .child
                .try_wait()
                .context("check llama.cpp notes server status")?
            {
                bail!("llama.cpp notes server exited during startup with {status}");
            }
            if Instant::now() >= startup_deadline || Instant::now() >= self.deadline {
                bail!(
                    "llama.cpp notes server did not become ready within {} seconds",
                    SERVER_START_TIMEOUT.as_secs()
                );
            }

            match self
                .agent
                .get(&health_url)
                .timeout(SERVER_HEALTH_TIMEOUT)
                .call()
            {
                Ok(_) => {
                    // Confirm the responding listener belongs to our child before
                    // sending the bearer token or any transcript content.
                    #[cfg(windows)]
                    verify_server_pid(port, self.child.id())?;
                    return Ok(());
                }
                Err(ureq::Error::Status(503, _)) | Err(ureq::Error::Transport(_)) => {
                    thread::sleep(SERVER_POLL_INTERVAL);
                }
                Err(error) => bail!("llama.cpp notes server health check failed: {error}"),
            }
        }
    }

    fn remaining_time(&self) -> Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| {
                anyhow!(
                    "local notes exceeded its {}-second runtime budget",
                    NOTES_RUNTIME_BUDGET.as_secs()
                )
            })
    }
}

impl PromptRunner for LlamaServer {
    fn complete(&mut self, prompt: &str, max_tokens: u32) -> Result<String> {
        if self.model_calls >= MAX_MODEL_CALLS {
            bail!("local notes exceeded its {MAX_MODEL_CALLS}-request model budget");
        }
        if let Some(status) = self
            .child
            .try_wait()
            .context("check llama.cpp notes server status")?
        {
            bail!("llama.cpp notes server exited with {status}");
        }
        let timeout = self.remaining_time()?;
        self.model_calls += 1;
        let response = self
            .agent
            .post(&self.completion_url)
            .set("Authorization", &self.authorization)
            .timeout(timeout)
            .send_json(CompletionRequest {
                prompt,
                n_predict: max_tokens,
                temperature: 0.3,
                repeat_penalty: 1.15,
                stream: false,
            })
            .map_err(|error| anyhow!("llama.cpp notes request failed: {error}"))?;
        let payload: CompletionResponse = response
            .into_json()
            .context("parse llama.cpp notes response")?;
        let content = strip_code_fences(&payload.content);
        if content.is_empty() {
            bail!("llama.cpp Gemma notes runtime returned empty content");
        }
        Ok(content)
    }
}

impl Drop for LlamaServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn reserve_loopback_port() -> Result<u16> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .context("reserve localhost port for llama.cpp notes server")?;
    let port = listener
        .local_addr()
        .context("read llama.cpp notes server port")?
        .port();
    drop(listener);
    Ok(port)
}

/// Queries the OS TCP listener table and returns an error if the process that
/// owns `port` on 127.0.0.1 is not `expected_pid`. This closes the bind-gap
/// window: a rogue process that grabbed the port before our child could bind
/// will be detected here, before the bearer token or transcript are sent.
#[cfg(windows)]
fn verify_server_pid(port: u16, expected_pid: u32) -> Result<()> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
    };

    // AF_INET = 2 (IPv4); avoid pulling in Win32_Networking_WinSock for one constant.
    const AF_INET: u32 = 2;
    // ERROR_INSUFFICIENT_BUFFER = 122; returned by the sizing call.
    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

    // First call: obtain the required buffer size.
    let mut buf_size: u32 = 0;
    // SAFETY: null pointer is valid for the sizing call; the OS only writes buf_size.
    let rc = unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut buf_size,
            0,
            AF_INET,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    };
    if rc != ERROR_INSUFFICIENT_BUFFER {
        bail!("GetExtendedTcpTable size query returned unexpected code {rc}");
    }

    // Extra headroom for listeners that appear between the two calls.
    buf_size = buf_size.saturating_add(512);
    let mut buf = vec![0u8; buf_size as usize];

    // Second call: fill the buffer with the actual table.
    // SAFETY: buf is non-null and sized to at least buf_size bytes; the OS
    // writes a valid MIB_TCPTABLE_OWNER_PID into it on success.
    let rc = unsafe {
        GetExtendedTcpTable(
            buf.as_mut_ptr().cast(),
            &mut buf_size,
            0,
            AF_INET,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    };
    if rc != 0 {
        bail!("GetExtendedTcpTable failed with code {rc}");
    }

    // SAFETY: The OS wrote a valid MIB_TCPTABLE_OWNER_PID into buf; the
    // dwNumEntries sequential MIB_TCPROW_OWNER_PID elements follow the
    // single-element table[1] header field in the struct definition.
    let table = unsafe { &*(buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID) };
    let row_count = table.dwNumEntries as usize;
    let rows = unsafe { std::slice::from_raw_parts(table.table.as_ptr(), row_count) };

    // dwLocalPort is stored in network byte order (big-endian) as a DWORD;
    // cast to u16 then swap bytes to get the host-order port number.
    let owned = rows.iter().any(|row| {
        u16::from_be(row.dwLocalPort as u16) == port && row.dwOwningPid == expected_pid
    });
    if !owned {
        bail!(
            "port {port} is not owned by the spawned llama.cpp process (PID \
             {expected_pid}); possible port squatting between reserve and bind"
        );
    }
    Ok(())
}

fn generate_local_api_key() -> Result<String> {
    use std::fmt::Write as _;

    let mut bytes = [0_u8; 24];
    getrandom::getrandom(&mut bytes)
        .map_err(|error| anyhow!("generate llama.cpp localhost API key: {error}"))?;
    let mut key = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut key, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(key)
}

/// Uses the completed notes as compact evidence for a second, bounded model
/// pass. Keeping title generation separate prevents JSON formatting mistakes
/// from corrupting the Markdown body that the editor persists.
fn run_llama_title(runtime: &mut impl PromptRunner, notes: &str) -> Result<String> {
    let raw = runtime.complete(&gemma_title_prompt(notes), TITLE_MAX_TOKENS)?;
    Ok(sanitize_title(&raw))
}

/// Prevents the bundled console runtime from surfacing through the sidecar's
/// otherwise invisible notes pipeline without sacrificing captured diagnostics.
#[cfg(windows)]
fn configure_background_process(command: &mut Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn configure_background_process(_command: &mut Command) {}

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

fn gemma_title_prompt(notes: &str) -> String {
    let instructions = "\
You write a short title for a meeting, given its notes.\n\n\
Rules:\n\
- Output ONLY the title text: no quotes, Markdown, preamble, or trailing punctuation.\n\
- Keep it short and specific: at most 6 words and 40 characters.\n\
- Use Title Case. Use only facts present in the notes; never invent names.\n\
- Do not start with Meeting, Notes, Summary, or a date.";

    format!(
        "<bos><start_of_turn>user\n{instructions}\n\nNotes:\n{notes}<end_of_turn>\n<start_of_turn>model\n"
    )
}

/// Applies the same defensive title boundary as macOS: unwrap common model
/// formatting, reject generic leading labels, and cap user-visible filenames
/// without splitting a Unicode scalar or an ordinary word.
fn sanitize_title(raw: &str) -> String {
    let mut title = raw
        .replace("\r\n", "\n")
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("```"))
        .unwrap_or_default()
        .trim_matches(|character| "#*->\"'`".contains(character))
        .trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(|character| ".,;:".contains(character))
        .trim()
        .to_string();

    loop {
        let lowercase = title.to_lowercase();
        let mut stripped = false;
        for prefix in ["meeting", "notes", "summary"] {
            if !lowercase.starts_with(prefix) {
                continue;
            }
            let remainder = &title[prefix.len()..];
            if remainder.chars().next().is_some_and(char::is_alphanumeric) {
                continue;
            }
            title = remainder
                .trim_start_matches([':', '-', ' ', '\t'])
                .trim()
                .to_string();
            stripped = true;
            break;
        }
        if !stripped {
            break;
        }
    }

    const MAX_TITLE_CHARS: usize = 40;
    if title.chars().count() > MAX_TITLE_CHARS {
        let capped = title.chars().take(MAX_TITLE_CHARS).collect::<String>();
        title = capped
            .rfind(' ')
            .map(|index| capped[..index].trim().to_string())
            .unwrap_or(capped);
    }
    title
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
        .join("llama-server.exe");
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
    use std::collections::VecDeque;

    #[cfg(windows)]
    const CONSOLE_PROBE_ENV: &str = "ARISO_STT_TEST_CONSOLE_PROBE_CHILD";

    struct StubPromptRunner {
        responses: VecDeque<String>,
        calls: usize,
    }

    impl StubPromptRunner {
        fn new(responses: impl IntoIterator<Item = String>) -> Self {
            Self {
                responses: responses.into_iter().collect(),
                calls: 0,
            }
        }
    }

    impl PromptRunner for StubPromptRunner {
        fn complete(&mut self, _prompt: &str, _max_tokens: u32) -> Result<String> {
            self.calls += 1;
            self.responses
                .pop_front()
                .ok_or_else(|| anyhow!("stub prompt response exhausted"))
        }
    }

    /// Uses a real console-subsystem test process to verify the exact launch
    /// configuration applied to llama.cpp rather than only checking a constant.
    #[cfg(windows)]
    #[test]
    fn background_llama_child_has_no_console_window() {
        if std::env::var_os(CONSOLE_PROBE_ENV).is_some() {
            let has_console =
                unsafe { !windows_sys::Win32::System::Console::GetConsoleWindow().is_null() };
            println!("ARISO_STT_CONSOLE_WINDOW={has_console}");
            return;
        }

        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--exact")
            .arg("notes::tests::background_llama_child_has_no_console_window")
            .arg("--nocapture")
            .env(CONSOLE_PROBE_ENV, "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_background_process(&mut command);

        let output = command.output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "stdout: {stdout}\nstderr: {stderr}"
        );
        assert!(
            stdout.contains("ARISO_STT_CONSOLE_WINDOW=false"),
            "child unexpectedly acquired a console window; stdout: {stdout}\nstderr: {stderr}"
        );
    }

    #[test]
    fn discovers_packaged_llama_runtime_beside_sidecar() {
        let temp = tempfile::tempdir().unwrap();
        let sidecar = temp.path().join("ariso-stt.exe");
        let runtime = temp.path().join("llama").join("llama-server.exe");
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
    fn notes_result_serializes_the_host_title_contract() {
        let output = serialize_notes_result("Budget Planning", "## Summary\n- Approved").unwrap();
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(json["title"], "Budget Planning");
        assert_eq!(json["notes"], "## Summary\n- Approved");
    }

    #[test]
    fn generated_title_is_sanitized_like_the_macos_sidecar() {
        assert_eq!(
            sanitize_title("```text\n**Meeting Notes: Budget Planning.**\n```"),
            "Budget Planning"
        );
        assert_eq!(
            sanitize_title("Quarterly Infrastructure Migration Readiness Discussion"),
            "Quarterly Infrastructure Migration"
        );
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
    fn long_notes_reuse_one_runner_for_chunk_and_final_prompts() {
        let transcript = "word ".repeat(1_800);
        let mut runtime = StubPromptRunner::new([
            "Speaker 1 approved the launch.".to_string(),
            "## Summary\n- Launch approved".to_string(),
        ]);

        let notes = run_llama_notes(&mut runtime, &transcript).unwrap();

        assert_eq!(notes, "## Summary\n- Launch approved");
        assert_eq!(runtime.calls, 2);
    }

    #[test]
    fn long_notes_reject_a_reduction_that_does_not_shrink() {
        let transcript = "word ".repeat(1_800);
        let mut runtime = StubPromptRunner::new(["x".repeat(9_000), "y".repeat(9_000)]);

        let error = run_llama_notes(&mut runtime, &transcript).unwrap_err();

        assert!(error.to_string().contains("did not shrink"));
        assert_eq!(runtime.calls, 2);
    }

    #[test]
    fn long_notes_stop_after_the_reduction_pass_limit() {
        let transcript = "word ".repeat(1_800);
        let mut runtime = StubPromptRunner::new([
            "a".repeat(9_000),
            "b".repeat(8_900),
            "c".repeat(8_800),
            "d".repeat(8_700),
            "e".repeat(8_600),
        ]);

        let error = run_llama_notes(&mut runtime, &transcript).unwrap_err();

        assert!(error.to_string().contains("exceeded 4 passes"));
        assert_eq!(runtime.calls, 5);
    }

    #[test]
    fn oversized_transcripts_fail_before_starting_inference() {
        let summary_budget = input_char_budget(DEFAULT_CONTEXT_SIZE, CHUNK_MAX_TOKENS).unwrap();
        let transcript = "word ".repeat(summary_budget * (MAX_SOURCE_CHUNKS + 1) / 5 + 10);
        let mut runtime = StubPromptRunner::new([]);

        let error = run_llama_notes(&mut runtime, &transcript).unwrap_err();

        assert!(error.to_string().contains("local notes support at most 24"));
        assert_eq!(runtime.calls, 0);
    }

    #[test]
    fn strips_markdown_code_fences() {
        let cleaned = strip_code_fences("```markdown\n## Summary\n- Done\n```\n");
        assert_eq!(cleaned, "## Summary\n- Done");
    }

    #[test]
    fn cleans_prompt_echo_before_notes_heading() {
        let raw = "Rules:\n- Use sections.\n## Summary\n- Done";
        assert_eq!(clean_notes(raw), "## Summary\n- Done");
    }
}
