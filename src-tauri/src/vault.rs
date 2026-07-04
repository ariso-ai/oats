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
}
