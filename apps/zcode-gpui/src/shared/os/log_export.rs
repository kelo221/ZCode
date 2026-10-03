//! Log bundle exporter (desktop & agent logs + panic log + in-memory GPUI log).
//! Gathers logs from ~/.zcode/logs and %TEMP% into ~/.zcode/exports/zcode-logs-<timestamp>.zip.
//!
//! Every file is copied through `shared::redact::scrub`: desktop and agent
//! logs are written by other processes and may contain provider keys or auth
//! headers, and the bundle is meant to be shared.

#![allow(dead_code)]

use crate::shared::redact::scrub;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

pub fn export_logs(memory_logs: &[String]) -> Result<PathBuf, std::io::Error> {
    let home = std::env::var("ZCODE_DESKTOP_HOME_DIR")
        .or_else(|_| std::env::var("USERPROFILE"))
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    let base_dir = PathBuf::from(home).join(".zcode");
    let staging_dir = stage_bundle(&base_dir, &std::env::temp_dir(), memory_logs)?;
    Ok(compress(&staging_dir).unwrap_or(staging_dir))
}

/// Copy a text log through the scrubber. Invalid UTF-8 is replaced rather
/// than copied raw, so no byte bypasses the scrubber.
fn copy_scrubbed(src: &Path, dst: &Path) {
    if let Ok(bytes) = fs::read(src) {
        let _ = fs::write(dst, scrub(&String::from_utf8_lossy(&bytes)));
    }
}

/// Quote a path as a single-quoted PowerShell literal (`'` doubles).
pub(crate) fn ps_quote(p: &Path) -> String {
    format!("'{}'", p.display().to_string().replace('\'', "''"))
}

/// Collect scrubbed copies of every log into `<base>/exports/zcode-logs-<ms>`.
pub(crate) fn stage_bundle(
    base_dir: &Path,
    temp_dir: &Path,
    memory_logs: &[String],
) -> Result<PathBuf, std::io::Error> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let staging_dir = base_dir.join("exports").join(format!("zcode-logs-{now}"));
    fs::create_dir_all(&staging_dir)?;

    // In-memory GPUI log: scrubbed at the `push_log` sink, scrubbed again in
    // case a caller bypassed it.
    fs::write(
        staging_dir.join("gpui-session.log"),
        scrub(&memory_logs.join("\n")),
    )?;

    if let Ok(entries) = fs::read_dir(base_dir.join("logs")) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file()
                && p.extension().is_some_and(|ext| ext == "log")
                && let Some(name) = p.file_name()
            {
                copy_scrubbed(&p, &staging_dir.join(name));
            }
        }
    }

    // Panic logs in the temp dir (zcode*.log).
    if let Ok(entries) = fs::read_dir(temp_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            if p.is_file() && name.starts_with("zcode") && name.ends_with(".log") {
                copy_scrubbed(&p, &staging_dir.join(&*name));
            }
        }
    }
    Ok(staging_dir)
}

/// Zip the staged folder next to it (Windows PowerShell). `-LiteralPath`
/// keeps `[`/`]` in user paths from being treated as wildcards.
fn compress(staging_dir: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let zip_file = staging_dir.with_extension("zip");
        let script = format!(
            "Compress-Archive -LiteralPath {} -DestinationPath {} -Force",
            ps_quote(staging_dir),
            ps_quote(&zip_file)
        );
        let mut cmd = std::process::Command::new("powershell");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
        cmd.creation_flags(0x08000000);
        if cmd.status().is_ok_and(|s| s.success()) {
            let _ = fs::remove_dir_all(staging_dir);
            return Some(zip_file);
        }
    }
    let _ = staging_dir;
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("zcode_gpui_logexport_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Uses scratch dirs only: never reads the developer's real ~/.zcode.
    #[test]
    fn staged_bundle_is_scrubbed() {
        let base = scratch("base");
        let temp = scratch("temp");
        fs::create_dir_all(base.join("logs")).unwrap();
        fs::write(
            base.join("logs").join("main.log"),
            "request Authorization: Bearer abcdefgh12345678 ok",
        )
        .unwrap();
        fs::write(
            temp.join("zcode-gpui-panic.log"),
            r#"{"apiKey":"k-panic-1"}"#,
        )
        .unwrap();
        fs::write(temp.join("other.log"), "not ours").unwrap();
        let mem = vec!["key sk-ABCDEFGHIJKLMNOPQRSTUV".to_string()];

        let staging = stage_bundle(&base, &temp, &mem).unwrap();
        assert!(staging.starts_with(base.join("exports")));
        assert!(!staging.join("other.log").exists());
        let mut files = 0;
        for entry in fs::read_dir(&staging).unwrap().flatten() {
            files += 1;
            let text = fs::read_to_string(entry.path()).unwrap();
            assert!(text.contains("[REDACTED]"), "{entry:?}: {text}");
            for secret in ["abcdefgh12345678", "k-panic-1", "ABCDEFGHIJKLMNOPQRSTUV"] {
                assert!(!text.contains(secret), "{secret} leaked into {entry:?}");
            }
        }
        assert_eq!(files, 3);
        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn powershell_quoting_escapes_apostrophes() {
        assert_eq!(
            ps_quote(Path::new(r"C:\Users\O'Brien\x")),
            r"'C:\Users\O''Brien\x'"
        );
    }
}
