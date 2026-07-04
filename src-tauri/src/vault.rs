use crate::storage::{format_hms, RecordingMeta};
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
}
