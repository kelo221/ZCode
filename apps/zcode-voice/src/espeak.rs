//! Locating eSpeak NG phoneme data for kokoro-tiny's espeak-rs.
//!
//! espeak-rs reads `PIPER_ESPEAKNG_DATA_DIRECTORY` (a directory that contains
//! `espeak-ng-data`), then the cwd, then the exe directory. Data built next to
//! the exe matches the statically linked espeak-ng version, so it wins over a
//! system install.

use std::path::{Path, PathBuf};

pub const DATA_DIR_NAME: &str = "espeak-ng-data";
pub const ENV_VAR: &str = "PIPER_ESPEAKNG_DATA_DIRECTORY";

fn has_data(dir: &Path) -> bool {
    dir.join(DATA_DIR_NAME).join("phontab").is_file()
}

/// Returns the first candidate directory containing usable `espeak-ng-data`.
pub fn find_data_root(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|dir| has_data(dir)).cloned()
}

pub fn default_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(dir) = std::env::var(ENV_VAR) {
        out.push(PathBuf::from(dir));
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        out.push(exe_dir.clone());
        out.extend(build_output_roots(&exe_dir));
    }
    for var in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Ok(base) = std::env::var(var) {
            out.push(Path::new(&base).join("eSpeak NG"));
        }
    }
    out.push(PathBuf::from("/usr/share"));
    out.push(PathBuf::from("/usr/lib/x86_64-linux-gnu"));
    out.push(PathBuf::from("/opt/homebrew/share"));
    out
}

/// `cargo build` output (`target/<profile>/build/espeak-rs-sys-*/out/share`),
/// so a development build runs without copying the data next to the exe.
fn build_output_roots(exe_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(exe_dir.join("build")) else {
        return Vec::new();
    };
    let mut roots: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("espeak-rs-sys-")
        })
        .map(|e| e.path().join("out").join("share"))
        .collect();
    roots.sort();
    roots
}

/// Points espeak-rs at the data root. Must run before the first synthesis.
pub fn configure() -> Result<PathBuf, String> {
    let root = find_data_root(&default_candidates()).ok_or_else(|| {
        "eSpeak NG data not found; put espeak-ng-data next to zcode-voice or install eSpeak NG"
            .to_string()
    })?;
    // SAFETY: main calls this before spawning any thread.
    unsafe { std::env::set_var(ENV_VAR, &root) };
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn picks_first_candidate_with_phontab() {
        let empty = tempfile::tempdir().unwrap();
        let partial = tempfile::tempdir().unwrap();
        fs::create_dir_all(partial.path().join(DATA_DIR_NAME)).unwrap();
        let full = tempfile::tempdir().unwrap();
        fs::create_dir_all(full.path().join(DATA_DIR_NAME)).unwrap();
        fs::write(full.path().join(DATA_DIR_NAME).join("phontab"), b"x").unwrap();
        let candidates = [
            empty.path().to_path_buf(),
            partial.path().to_path_buf(),
            full.path().to_path_buf(),
        ];
        assert_eq!(find_data_root(&candidates), Some(full.path().to_path_buf()));
        assert_eq!(find_data_root(&candidates[..2]), None);
    }

    #[test]
    fn finds_cargo_build_output() {
        let exe_dir = tempfile::tempdir().unwrap();
        let share = exe_dir.path().join("build/espeak-rs-sys-abc/out/share");
        fs::create_dir_all(share.join(DATA_DIR_NAME)).unwrap();
        fs::write(share.join(DATA_DIR_NAME).join("phontab"), b"x").unwrap();
        fs::create_dir_all(exe_dir.path().join("build/other-crate")).unwrap();
        let roots = build_output_roots(exe_dir.path());
        assert_eq!(find_data_root(&roots), Some(share));
    }
}
