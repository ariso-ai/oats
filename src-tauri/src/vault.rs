use crate::storage::{format_hms, RecordingMeta};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Resolve the vault root `<ariso_root>/vault`.
pub fn vault_root() -> Result<PathBuf, String> {
    Ok(crate::storage::ariso_root()?.join("vault"))
}

/// Where audio attachments live inside the vault.
pub fn attachments_dir(root: &Path) -> PathBuf {
    root.join("Attachments")
}

/// Minimal Obsidian config so the folder opens cleanly as a vault and routes
/// pasted attachments into `Attachments/`.
const OBSIDIAN_APP_JSON: &str = "{\n  \"attachmentFolderPath\": \"Attachments\"\n}\n";

/// Create the vault dir, its `Attachments/` folder, and a minimal `.obsidian/`
/// on first use. Idempotent: never overwrites an existing `app.json`.
pub fn ensure_vault() -> Result<PathBuf, String> {
    let root = vault_root()?;
    std::fs::create_dir_all(attachments_dir(&root))
        .map_err(|e| format!("create vault attachments dir: {e}"))?;
    let obsidian = root.join(".obsidian");
    std::fs::create_dir_all(&obsidian).map_err(|e| format!("create .obsidian dir: {e}"))?;
    let app_json = obsidian.join("app.json");
    if !app_json.exists() {
        std::fs::write(&app_json, OBSIDIAN_APP_JSON)
            .map_err(|e| format!("write app.json: {e}"))?;
    }
    Ok(root)
}

/// The vault-relative markdown note path for a basename.
pub fn note_path(root: &Path, basename: &str) -> PathBuf {
    root.join(format!("{basename}.md"))
}

/// Strip characters that could escape the vault or break a filename. Keeps
/// spaces (Obsidian allows them). Collapses runs of whitespace.
fn sanitize_component(s: &str) -> String {
    // Filter individual reserved/control chars FIRST, then remove `..`. If we
    // stripped `..` first, a reserved char between two lone dots (e.g. `.:.`)
    // would reform `..` after the char filter. Loop the removal to a fixed
    // point so a single pass can't leave a reformed `..` behind.
    let filtered: String = s
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') && !c.is_control())
        .collect();
    let mut cleaned = filtered;
    while cleaned.contains("..") {
        cleaned = cleaned.replace("..", "");
    }
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Derive a human note basename: `YYYY-MM-DD <title>`. Falls back to the first
/// 10 chars of `created_at` if it is not RFC3339, and to `id` if the sanitized
/// title is empty.
pub fn note_basename(created_at: &str, title: &str, id: &str) -> String {
    let date = match chrono::DateTime::parse_from_rfc3339(created_at) {
        Ok(dt) => dt.format("%Y-%m-%d").to_string(),
        Err(_) => created_at.chars().take(10).collect(),
    };
    let mut name = sanitize_component(title);
    if name.is_empty() {
        name = sanitize_component(id);
    }
    format!("{date} {name}")
}

/// Return `base`, or `base (N)` for the smallest N ≥ 2 such that neither the
/// note (`<base>.md`) nor the audio (`Attachments/<base>.mp3`) already exists.
pub fn unique_basename(root: &Path, base: &str) -> String {
    let taken = |b: &str| {
        note_path(root, b).exists() || attachments_dir(root).join(format!("{b}.mp3")).exists()
    };
    if !taken(base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base} ({n})");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Render a vault note: YAML front-matter, the audio embed, then the notes body.
pub fn render_note(meta: &RecordingMeta, audio_file: &str, notes_md: &str) -> String {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let participants = meta
        .participants
        .iter()
        .map(|p| format!("\"{}\"", esc(&p.label)))
        .collect::<Vec<_>>()
        .join(", ");
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("oats_id: {}\n", meta.id));
    out.push_str(&format!("title: \"{}\"\n", esc(&meta.title)));
    out.push_str(&format!("date: \"{}\"\n", esc(&meta.created_at)));
    out.push_str(&format!("duration: \"{}\"\n", format_hms(meta.duration_seconds as f64)));
    out.push_str(&format!("participants: [{participants}]\n"));
    out.push_str("---\n");
    out.push_str(&format!("![[Attachments/{audio_file}]]\n\n"));
    out.push_str(notes_md);
    out
}

/// Inverse of `render_note` for in-app rendering: drop the front-matter block
/// and the leading `![[...]]` embed line, returning just the notes body.
/// Content without a leading `---` front-matter fence is returned unchanged (a
/// user may have freely rewritten the note).
pub fn note_body(contents: &str) -> String {
    let Some(after_open) = contents.strip_prefix("---\n") else {
        return contents.to_string();
    };
    let Some((_frontmatter, after_fence)) = after_open.split_once("\n---\n") else {
        return contents.to_string();
    };
    // `after_fence` is `![[...]]\n\n<body>` as emitted by render_note. Strip one
    // leading embed line, then exactly one separator newline — never more, so a
    // body that itself begins with `\n` is preserved.
    let body = match after_fence.split_once('\n') {
        Some((first, tail)) if first.trim_start().starts_with("![[") => {
            tail.strip_prefix('\n').unwrap_or(tail)
        }
        _ => after_fence,
    };
    body.to_string()
}

/// Reject an attachment filename that could escape `Attachments/`.
fn validate_audio_file(audio_file: &str) -> Result<(), String> {
    if audio_file.is_empty()
        || audio_file.contains('/')
        || audio_file.contains('\\')
        || audio_file.contains("..")
    {
        return Err(format!("invalid audio filename: {audio_file}"));
    }
    Ok(())
}

/// Path to an audio attachment in the vault.
pub fn audio_path(root: &Path, audio_file: &str) -> PathBuf {
    attachments_dir(root).join(audio_file)
}

/// Atomically write an audio attachment, creating `Attachments/` if needed.
pub fn write_audio(audio_file: &str, bytes: &[u8]) -> Result<(), String> {
    validate_audio_file(audio_file)?;
    let root = ensure_vault()?;
    crate::storage::write_atomic(&audio_path(&root, audio_file), bytes)
}

/// Read an audio attachment's bytes.
pub fn read_audio(audio_file: &str) -> Result<Vec<u8>, String> {
    validate_audio_file(audio_file)?;
    let root = vault_root()?;
    std::fs::read(audio_path(&root, audio_file)).map_err(|e| format!("read vault audio: {e}"))
}

/// Extract `oats_id` from a note's front-matter head, if present. Reads only the
/// lines inside the leading `---` fence.
fn parse_oats_id(contents: &str) -> Option<String> {
    let after_open = contents.strip_prefix("---\n")?;
    let (frontmatter, _body) = after_open.split_once("\n---\n")?;
    for line in frontmatter.lines() {
        if let Some(rest) = line.strip_prefix("oats_id:") {
            let id = rest.trim();
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
    }
    None
}

/// Map every top-level `.md` note in the vault by its `oats_id`. Files without a
/// valid `oats_id` (or unreadable) are skipped. Missing vault → empty map.
pub fn scan_vault() -> Result<HashMap<String, PathBuf>, String> {
    let root = vault_root()?;
    let mut map = HashMap::new();
    if !root.exists() {
        return Ok(map);
    }
    for entry in std::fs::read_dir(&root).map_err(|e| format!("read vault dir: {e}"))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else { continue };
        if let Some(id) = parse_oats_id(&contents) {
            map.insert(id, path);
        }
    }
    Ok(map)
}

/// Path of the note whose front-matter carries `oats_id`, if any.
pub fn find_note(oats_id: &str) -> Result<Option<PathBuf>, String> {
    Ok(scan_vault()?.remove(oats_id))
}

/// The notes body (front-matter and embed stripped) for a recording, if a note
/// exists in the vault.
pub fn read_note(oats_id: &str) -> Result<Option<String>, String> {
    match find_note(oats_id)? {
        Some(path) => {
            let contents =
                std::fs::read_to_string(&path).map_err(|e| format!("read vault note: {e}"))?;
            Ok(Some(note_body(&contents)))
        }
        None => Ok(None),
    }
}

/// Atomically write a recording's vault note (front-matter + embed + body).
pub fn write_note(
    basename: &str,
    meta: &RecordingMeta,
    audio_file: &str,
    notes_md: &str,
) -> Result<(), String> {
    let root = ensure_vault()?;
    let contents = render_note(meta, audio_file, notes_md);
    crate::storage::write_atomic(&note_path(&root, basename), contents.as_bytes())
}

/// Remove a recording's vault note (located by `oats_id`) and its audio
/// attachment. Missing files are not an error (idempotent cascade).
pub fn delete_recording_artifacts(oats_id: &str, audio_file: Option<&str>) -> Result<(), String> {
    if let Some(path) = find_note(oats_id)? {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("delete vault note: {e}")),
        }
    }
    if let Some(audio_file) = audio_file {
        validate_audio_file(audio_file)?;
        let root = vault_root()?;
        match std::fs::remove_file(audio_path(&root, audio_file)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("delete vault attachment: {e}")),
        }
    }
    Ok(())
}

/// Rewrite a note's front-matter `title:` line and its `![[Attachments/...]]`
/// embed to a new title/attachment, preserving the body and every other line
/// (including any front-matter the user added in Obsidian). Content without a
/// leading `---` fence only has its embed replaced.
pub fn retitle_note_contents(
    contents: &str,
    old_audio_file: &str,
    new_audio_file: &str,
    new_title: &str,
) -> String {
    let with_embed = contents.replace(
        &format!("![[Attachments/{old_audio_file}]]"),
        &format!("![[Attachments/{new_audio_file}]]"),
    );
    // Match render_note's escaping so the title round-trips.
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let new_title_line = format!("title: \"{}\"", esc(new_title));
    if let Some(after_open) = with_embed.strip_prefix("---\n") {
        if let Some((frontmatter, rest)) = after_open.split_once("\n---\n") {
            let new_fm = frontmatter
                .lines()
                .map(|line| {
                    if line.starts_with("title:") {
                        new_title_line.clone()
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            return format!("---\n{new_fm}\n---\n{rest}");
        }
    }
    with_embed
}

/// Propagate an in-app rename to the vault: rename the audio attachment and the
/// note file to a basename derived from `new_title`, updating the note's
/// front-matter `title:` and embed while preserving its body. Returns the new
/// attachment filename for the caller to store in `meta.audio_file`. A no-op
/// (returns `old_audio_file` unchanged) when the new title yields the same
/// basename. The note is located by `oats_id`, so it is found and renamed even
/// if the user had renamed or moved it in Obsidian.
pub fn rename_recording_artifacts(
    oats_id: &str,
    created_at: &str,
    old_audio_file: &str,
    new_title: &str,
) -> Result<String, String> {
    validate_audio_file(old_audio_file)?;
    let root = vault_root()?;
    let old_basename = old_audio_file.strip_suffix(".mp3").unwrap_or(old_audio_file);
    let desired = note_basename(created_at, new_title, oats_id);
    if desired == old_basename {
        return Ok(old_audio_file.to_string());
    }
    let new_basename = unique_basename(&root, &desired);
    let new_audio_file = format!("{new_basename}.mp3");

    // Rename the attachment (best-effort: skip if the old file is already gone).
    let old_audio = audio_path(&root, old_audio_file);
    if old_audio.is_file() {
        std::fs::rename(&old_audio, audio_path(&root, &new_audio_file))
            .map_err(|e| format!("rename attachment: {e}"))?;
    }

    // Rename + retitle the note if one exists (found by oats_id).
    if let Some(old_note_path) = find_note(oats_id)? {
        let contents = std::fs::read_to_string(&old_note_path)
            .map_err(|e| format!("read note for rename: {e}"))?;
        let updated = retitle_note_contents(&contents, old_audio_file, &new_audio_file, new_title);
        let new_note_path = note_path(&root, &new_basename);
        crate::storage::write_atomic(&new_note_path, updated.as_bytes())?;
        if new_note_path != old_note_path {
            match std::fs::remove_file(&old_note_path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("remove old note: {e}")),
            }
        }
    }
    Ok(new_audio_file)
}

#[cfg(test)]
mod tests {
    use super::*;

    // SAFETY (all set_var/remove_var below): tests run with `--test-threads=1`,
    // so there is no concurrent env mutation while these calls execute.

    #[test]
    fn attachments_dir_is_under_root() {
        let root = Path::new("/tmp/v");
        assert_eq!(attachments_dir(root), Path::new("/tmp/v/Attachments"));
    }

    #[test]
    fn ensure_vault_creates_structure_idempotently() {
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("ARISO_ROOT", tmp.path());
        }
        let root = ensure_vault().unwrap();
        assert!(root.is_dir());
        assert!(attachments_dir(&root).is_dir());
        assert!(root.join(".obsidian/app.json").is_file());
        // Overwrite app.json with a distinguishable sentinel, then prove the
        // second call is idempotent AND does not clobber it. Rewriting the fixed
        // OBSIDIAN_APP_JSON constant would be indistinguishable from not writing,
        // so a sentinel is needed to actually exercise the `!exists()` guard.
        let app_json = root.join(".obsidian/app.json");
        std::fs::write(&app_json, b"SENTINEL").unwrap();
        ensure_vault().unwrap();
        assert_eq!(std::fs::read_to_string(&app_json).unwrap(), "SENTINEL");
        unsafe {
            std::env::remove_var("ARISO_ROOT");
        }
    }

    #[test]
    fn note_basename_prefixes_date_and_sanitizes_title() {
        assert_eq!(
            note_basename("2026-06-02T14:30:05Z", "Team Standup", "2026-06-02T14-30-05Z"),
            "2026-06-02 Team Standup"
        );
    }

    #[test]
    fn note_basename_strips_path_and_reserved_chars() {
        assert_eq!(
            note_basename("2026-06-02T14:30:05Z", "a/b:c\\d..e", "id"),
            "2026-06-02 abcde"
        );
    }

    #[test]
    fn note_basename_empty_title_falls_back_to_id() {
        assert_eq!(note_basename("2026-06-02T14:30:05Z", "  ///  ", "myid"), "2026-06-02 myid");
    }

    #[test]
    fn note_basename_no_dotdot_reforms_after_char_strip() {
        let b = note_basename("2026-06-02T14:30:05Z", ".:.", "id");
        assert!(!b.contains(".."), "sanitized output must not contain ..: {b}");
        assert_eq!(b, "2026-06-02 id"); // title empties out, falls back to id
    }

    #[test]
    fn note_basename_bad_date_falls_back_to_ten_char_slice() {
        // Unparseable created_at → first 10 chars used as the date prefix.
        assert_eq!(note_basename("2026-06-02xxx", "T", "id"), "2026-06-02 T");
    }

    fn meta_for_note() -> crate::storage::RecordingMeta {
        crate::storage::RecordingMeta {
            id: "2026-06-02T14-30-05Z".into(),
            title: "Team \"Standup\"".into(),
            created_at: "2026-06-02T14:30:05Z".into(),
            duration_seconds: 2533,
            status: crate::storage::RecordingStatus::Done,
            language: Some("en".into()),
            participants: vec![
                crate::storage::Participant { id: 0, label: "Speaker 1".into() },
                crate::storage::Participant { id: 1, label: "Speaker 2".into() },
            ],
            model_version: None, error: None, notes_error: None, last_clip_end_at: None,
            audio_file: None, notes_written: None,
        }
    }

    #[test]
    fn render_note_has_frontmatter_embed_and_body() {
        let md = render_note(&meta_for_note(), "2026-06-02 Team Standup.mp3", "# Notes\n- point");
        assert!(md.starts_with("---\n"));
        assert!(md.contains("oats_id: 2026-06-02T14-30-05Z\n"));
        assert!(md.contains("duration: \"00:42:13\"\n"));
        assert!(md.contains("participants: [\"Speaker 1\", \"Speaker 2\"]\n"));
        assert!(md.contains("![[Attachments/2026-06-02 Team Standup.mp3]]\n"));
        assert!(md.contains("# Notes\n- point"));
    }

    #[test]
    fn note_body_strips_frontmatter_and_embed() {
        let md = render_note(&meta_for_note(), "a.mp3", "Body line 1\nBody line 2");
        assert_eq!(note_body(&md), "Body line 1\nBody line 2");
    }

    #[test]
    fn note_body_tolerates_missing_frontmatter() {
        // A user-mangled note with no front-matter returns its content unchanged.
        assert_eq!(note_body("just text"), "just text");
    }

    #[test]
    fn note_body_leaves_embed_alone_without_frontmatter() {
        // No `---` fence: the embed/trim logic must NOT run, so a first line that
        // looks like an embed is preserved verbatim.
        assert_eq!(note_body("![[X]]\nBody"), "![[X]]\nBody");
    }

    #[test]
    fn note_body_roundtrips_body_with_leading_blank_line() {
        // A body that itself begins with `\n` must survive the roundtrip — the
        // separator strip removes exactly one newline, not all of them.
        let body = "\nLeading blank line body";
        let md = render_note(&meta_for_note(), "a.mp3", body);
        assert_eq!(note_body(&md), body);
    }

    #[test]
    fn write_and_read_audio_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("ARISO_ROOT", tmp.path());
        }
        write_audio("2026-06-02 Standup.mp3", b"mp3bytes").unwrap();
        assert_eq!(read_audio("2026-06-02 Standup.mp3").unwrap(), b"mp3bytes");
        unsafe {
            std::env::remove_var("ARISO_ROOT");
        }
    }

    #[test]
    fn audio_file_rejects_traversal() {
        assert!(write_audio("../evil.mp3", b"x").is_err());
        assert!(write_audio("a/b.mp3", b"x").is_err());
    }

    #[test]
    fn scan_indexes_by_oats_id_and_skips_junk() {
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("ARISO_ROOT", tmp.path());
        }
        let root = ensure_vault().unwrap();
        std::fs::write(root.join("A.md"), "---\noats_id: id-a\n---\n![[x]]\n\nbody a").unwrap();
        std::fs::write(root.join("B.md"), "---\noats_id: id-b\n---\n\nbody b").unwrap();
        std::fs::write(root.join("junk.md"), "no frontmatter here").unwrap();
        std::fs::write(root.join("notes.txt"), "not markdown").unwrap();

        let map = scan_vault().unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("id-a").unwrap(), &root.join("A.md"));
        assert!(find_note("id-b").unwrap().is_some());
        assert!(find_note("missing").unwrap().is_none());
        assert_eq!(read_note("id-a").unwrap().as_deref(), Some("body a"));
        assert_eq!(read_note("missing").unwrap(), None);
        unsafe {
            std::env::remove_var("ARISO_ROOT");
        }
    }

    #[test]
    fn scan_empty_when_no_vault() {
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("ARISO_ROOT", tmp.path());
        }
        assert!(scan_vault().unwrap().is_empty());
        unsafe {
            std::env::remove_var("ARISO_ROOT");
        }
    }

    #[test]
    fn write_note_persists_renderable_note() {
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("ARISO_ROOT", tmp.path());
        }
        let mut meta = meta_for_note();
        meta.id = "id-w".into();
        write_note("2026-06-02 Team Standup", &meta, "2026-06-02 Team Standup.mp3", "the body")
            .unwrap();
        assert_eq!(read_note("id-w").unwrap().as_deref(), Some("the body"));
        unsafe {
            std::env::remove_var("ARISO_ROOT");
        }
    }

    #[test]
    fn delete_removes_note_and_attachment() {
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("ARISO_ROOT", tmp.path());
        }
        let mut meta = meta_for_note();
        meta.id = "id-d".into();
        write_audio("clip.mp3", b"a").unwrap();
        write_note("Note D", &meta, "clip.mp3", "b").unwrap();
        assert!(find_note("id-d").unwrap().is_some());

        delete_recording_artifacts("id-d", Some("clip.mp3")).unwrap();
        assert!(find_note("id-d").unwrap().is_none());
        assert!(!audio_path(&vault_root().unwrap(), "clip.mp3").exists());
        // Idempotent: deleting again is fine.
        delete_recording_artifacts("id-d", Some("clip.mp3")).unwrap();
        unsafe {
            std::env::remove_var("ARISO_ROOT");
        }
    }

    #[test]
    fn unique_basename_suffixes_on_collision() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(attachments_dir(root)).unwrap();
        assert_eq!(unique_basename(root, "Note"), "Note");
        std::fs::write(note_path(root, "Note"), b"x").unwrap();
        assert_eq!(unique_basename(root, "Note"), "Note (2)");
        std::fs::write(attachments_dir(root).join("Note (2).mp3"), b"x").unwrap();
        // A collision on EITHER the .md or the .mp3 bumps the counter.
        assert_eq!(unique_basename(root, "Note"), "Note (3)");

        // Symmetric OR-branch: a bare `<base>.mp3` with NO matching `.md`
        // still counts as taken and bumps the base.
        std::fs::write(attachments_dir(root).join("Audio.mp3"), b"x").unwrap();
        assert_eq!(unique_basename(root, "Audio"), "Audio (2)");
    }

    #[test]
    fn retitle_note_contents_updates_title_and_embed_preserving_body() {
        let meta = meta_for_note();
        let note = render_note(&meta, "2026-06-02 Old.mp3", "Body line 1\nBody line 2");
        // Simulate a user-added front-matter key.
        let note = note.replace("participants:", "tags: [meeting]\nparticipants:");

        let out =
            retitle_note_contents(&note, "2026-06-02 Old.mp3", "2026-06-02 New.mp3", "New Title");

        assert!(out.contains("title: \"New Title\"\n"), "title updated: {out}");
        assert!(out.contains("![[Attachments/2026-06-02 New.mp3]]"), "embed updated");
        assert!(!out.contains("2026-06-02 Old.mp3"), "old name gone");
        assert!(out.contains("Body line 1\nBody line 2"), "body preserved");
        assert!(out.contains("tags: [meeting]"), "user front-matter preserved");
        assert!(out.contains("oats_id: 2026-06-02T14-30-05Z"), "oats_id untouched");
    }

    #[test]
    fn rename_recording_artifacts_renames_note_and_attachment() {
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("ARISO_ROOT", tmp.path());
        }
        let mut meta = meta_for_note();
        meta.id = "id-r".into();
        write_audio("2026-06-02 Old.mp3", b"aud").unwrap();
        write_note("2026-06-02 Old", &meta, "2026-06-02 Old.mp3", "the body").unwrap();

        let new_audio =
            rename_recording_artifacts(&meta.id, &meta.created_at, "2026-06-02 Old.mp3", "Q2 Sync")
                .unwrap();
        let root = vault_root().unwrap();

        assert_eq!(new_audio, "2026-06-02 Q2 Sync.mp3");
        assert!(audio_path(&root, "2026-06-02 Q2 Sync.mp3").is_file(), "attachment renamed");
        assert!(!audio_path(&root, "2026-06-02 Old.mp3").exists(), "old attachment gone");
        assert!(note_path(&root, "2026-06-02 Q2 Sync").is_file(), "note renamed");
        assert!(!note_path(&root, "2026-06-02 Old").exists(), "old note gone");
        // Still found by oats_id, body preserved, embed + title updated.
        assert_eq!(read_note(&meta.id).unwrap().as_deref(), Some("the body"));
        let raw = std::fs::read_to_string(note_path(&root, "2026-06-02 Q2 Sync")).unwrap();
        assert!(raw.contains("title: \"Q2 Sync\""));
        assert!(raw.contains("![[Attachments/2026-06-02 Q2 Sync.mp3]]"));
        unsafe {
            std::env::remove_var("ARISO_ROOT");
        }
    }

    #[test]
    fn rename_recording_artifacts_noop_when_basename_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("ARISO_ROOT", tmp.path());
        }
        let mut meta = meta_for_note();
        meta.id = "id-n".into();
        write_audio("2026-06-02 Same.mp3", b"a").unwrap();
        // A new title that sanitizes to the same basename → no-op, no error.
        let out =
            rename_recording_artifacts(&meta.id, &meta.created_at, "2026-06-02 Same.mp3", "Same")
                .unwrap();
        assert_eq!(out, "2026-06-02 Same.mp3");
        assert!(audio_path(&vault_root().unwrap(), "2026-06-02 Same.mp3").is_file());
        unsafe {
            std::env::remove_var("ARISO_ROOT");
        }
    }
}
