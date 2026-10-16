//! The `zcode-voice` sidecar process: locating, spawning and talking NDJSON.
//!
//! Dropping a [`Sidecar`] closes its stdin (the sidecar stops speaking and
//! exits) and kills it; on Windows it also sits in a kill-on-close job so it
//! never outlives GPUI.

use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

pub(crate) const VOICE_EXE: &str = if cfg!(windows) {
    "zcode-voice.exe"
} else {
    "zcode-voice"
};

pub(crate) enum SidecarEvent {
    /// One parsed stdout event object (`{"event": ...}`).
    Event(Value),
    Exited,
}

/// Where to look, in order: the env override, beside the GPUI executable
/// (packaged layout), then `apps/zcode-voice/target/{release,debug}` under
/// any ancestor of the executable or cwd (dev builds).
pub(crate) fn candidates(
    override_path: Option<PathBuf>,
    exe_dir: Option<&Path>,
    cwd: Option<&Path>,
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = override_path.into_iter().collect();
    if let Some(dir) = exe_dir {
        out.push(dir.join(VOICE_EXE));
    }
    for start in [exe_dir, cwd].into_iter().flatten() {
        for ancestor in start.ancestors() {
            for profile in ["release", "debug"] {
                out.push(
                    ancestor
                        .join("apps/zcode-voice/target")
                        .join(profile)
                        .join(VOICE_EXE),
                );
            }
        }
    }
    out
}

pub(crate) fn locate() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let cwd = std::env::current_dir().ok();
    candidates(
        std::env::var_os("ZCODE_GPUI_VOICE_PROGRAM").map(PathBuf::from),
        exe_dir.as_deref(),
        cwd.as_deref(),
    )
    .into_iter()
    .find(|path| path.is_file())
}

pub(crate) fn model_dir() -> PathBuf {
    crate::shared::settings::resolve_user_home_dir().join(".zcode/v2/voice")
}

/// Returns the parsed event when `line` is a sidecar event; kokoro-tiny's own
/// stdout chatter is skipped.
pub(crate) fn parse_event(line: &str) -> Option<Value> {
    let value: Value = serde_json::from_str(line.trim()).ok()?;
    value.get("event")?.as_str()?;
    Some(value)
}

pub(crate) struct Sidecar {
    stdin: Option<ChildStdin>,
    child: Child,
    #[cfg(windows)]
    kill: Option<Box<dyn FnOnce() + Send>>,
}

impl Sidecar {
    pub(crate) fn spawn(
        exe: &Path,
        model_dir: &Path,
        events: futures::channel::mpsc::UnboundedSender<SidecarEvent>,
    ) -> std::io::Result<Self> {
        let mut command = Command::new(exe);
        command
            .arg("--model-dir")
            .arg(model_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn()?;
        #[cfg(windows)]
        let kill = Some(crate::backend::conn::make_kill(&child));
        let stdin = child.stdin.take();
        if let Some(stdout) = child.stdout.take() {
            std::thread::Builder::new()
                .name("zcode-voice-stdout".into())
                .spawn(move || {
                    for line in BufReader::new(stdout).lines() {
                        let Ok(line) = line else { break };
                        if let Some(event) = parse_event(&line)
                            && events.unbounded_send(SidecarEvent::Event(event)).is_err()
                        {
                            return;
                        }
                    }
                    let _ = events.unbounded_send(SidecarEvent::Exited);
                })?;
        }
        Ok(Self {
            stdin,
            child,
            #[cfg(windows)]
            kill,
        })
    }

    pub(crate) fn send(&mut self, command: &Value) -> std::io::Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| std::io::Error::other("voice sidecar stdin closed"))?;
        writeln!(stdin, "{command}")?;
        stdin.flush()
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        self.stdin.take();
        #[cfg(windows)]
        if let Some(kill) = self.kill.take() {
            kill();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_prefer_override_then_sibling_then_dev_targets() {
        let exe_dir = Path::new("/repo/apps/zcode-gpui/target/release");
        let list = candidates(Some(PathBuf::from("/custom/voice")), Some(exe_dir), None);
        assert_eq!(list[0], PathBuf::from("/custom/voice"));
        assert_eq!(list[1], exe_dir.join(VOICE_EXE));
        assert!(
            list.contains(
                &Path::new("/repo")
                    .join("apps/zcode-voice/target/release")
                    .join(VOICE_EXE)
            )
        );
    }

    #[test]
    fn parse_event_skips_non_event_lines() {
        assert!(parse_event("   Text length: 59 chars").is_none());
        assert!(parse_event(r#"{"other":1}"#).is_none());
        let event = parse_event(r#"{"event":"ready","voices":["af_sky"]}"#).unwrap();
        assert_eq!(event["voices"][0], "af_sky");
    }
}
