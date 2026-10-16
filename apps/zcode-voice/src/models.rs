//! Kokoro model files: location, validation and download.
//!
//! kokoro-tiny's own downloader never checks the HTTP status and treats a
//! partially written file as present, so the sidecar fetches the files itself
//! (status + length checked, `.part` then rename) before handing kokoro-tiny
//! the finished paths.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub struct ModelFile {
    pub name: &'static str,
    pub url: &'static str,
    /// Smallest plausible size; anything below is an error page or truncated.
    pub min_bytes: u64,
}

pub const MODEL: ModelFile = ModelFile {
    name: "0.onnx",
    url: "https://github.com/8b-is/kokoro-tiny/raw/main/models/0.onnx",
    min_bytes: 100_000_000,
};

pub const VOICES: ModelFile = ModelFile {
    name: "0.bin",
    url: "https://github.com/8b-is/kokoro-tiny/raw/main/models/0.bin",
    min_bytes: 1_000_000,
};

pub fn path_in(dir: &Path, file: &ModelFile) -> PathBuf {
    dir.join(file.name)
}

/// A finished file exists and is at least `min_bytes` long.
pub fn is_present(dir: &Path, file: &ModelFile) -> bool {
    fs::metadata(path_in(dir, file))
        .map(|m| m.is_file() && m.len() >= file.min_bytes)
        .unwrap_or(false)
}

/// Checks a finished download before it may replace the real file.
pub fn validate_download(
    status: u16,
    expected_len: Option<u64>,
    written: u64,
    min_bytes: u64,
) -> Result<(), String> {
    if !(200..300).contains(&status) {
        return Err(format!("HTTP status {status}"));
    }
    if let Some(expected) = expected_len
        && expected != written
    {
        return Err(format!(
            "truncated download ({written} of {expected} bytes)"
        ));
    }
    if written < min_bytes {
        return Err(format!("download too small ({written} bytes)"));
    }
    Ok(())
}

/// Downloads `file` into `dir` unless it is already present.
pub fn ensure(dir: &Path, file: &ModelFile, on_progress: &dyn Fn(u32)) -> Result<(), String> {
    if is_present(dir, file) {
        return Ok(());
    }
    fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let client = reqwest::blocking::Client::builder()
        .timeout(None)
        .user_agent("zcode-voice")
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client.get(file.url).send().map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let expected = response.content_length();
    let part = dir.join(format!("{}.part", file.name));
    let written = copy_with_progress(&mut response, &part, expected, on_progress)
        .map_err(|e| format!("write {}: {e}", part.display()))?;
    if let Err(err) = validate_download(status, expected, written, file.min_bytes) {
        let _ = fs::remove_file(&part);
        return Err(format!("{}: {err}", file.name));
    }
    fs::rename(&part, path_in(dir, file)).map_err(|e| format!("rename {}: {e}", file.name))
}

fn copy_with_progress(
    reader: &mut dyn Read,
    dest: &Path,
    expected: Option<u64>,
    on_progress: &dyn Fn(u32),
) -> io::Result<u64> {
    let mut out = File::create(dest)?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut written = 0u64;
    let mut last_percent = None;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        written += n as u64;
        if let Some(total) = expected.filter(|t| *t > 0) {
            let percent = ((written * 100) / total).min(100) as u32;
            if last_percent != Some(percent) {
                last_percent = Some(percent);
                on_progress(percent);
            }
        }
    }
    out.flush()?;
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_bad_status_truncation_and_tiny_files() {
        assert!(validate_download(404, Some(10), 10, 1).is_err());
        assert!(validate_download(200, Some(100), 50, 1).is_err());
        assert!(validate_download(200, None, 5, 10).is_err());
        assert!(validate_download(200, Some(20), 20, 10).is_ok());
        assert!(validate_download(200, None, 20, 10).is_ok());
    }

    #[test]
    fn presence_requires_min_size() {
        let dir = tempfile::tempdir().unwrap();
        let file = ModelFile {
            name: "m.bin",
            url: "",
            min_bytes: 4,
        };
        assert!(!is_present(dir.path(), &file));
        fs::write(path_in(dir.path(), &file), b"ab").unwrap();
        assert!(!is_present(dir.path(), &file));
        fs::write(path_in(dir.path(), &file), b"abcd").unwrap();
        assert!(is_present(dir.path(), &file));
    }

    #[test]
    fn copy_reports_progress_once_per_percent() {
        let dir = tempfile::tempdir().unwrap();
        let data = vec![7u8; 1000];
        let seen = std::cell::RefCell::new(Vec::new());
        let written = copy_with_progress(
            &mut data.as_slice(),
            &dir.path().join("x"),
            Some(1000),
            &|p| seen.borrow_mut().push(p),
        )
        .unwrap();
        assert_eq!(written, 1000);
        assert_eq!(seen.borrow().last(), Some(&100));
        let mut sorted = seen.borrow().clone();
        sorted.dedup();
        assert_eq!(sorted.len(), seen.borrow().len());
    }
}
