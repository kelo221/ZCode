//! Wire pieces shared by GPUI and the `zcode-browser-host` binary (which
//! includes this file by path, so it must only depend on std and serde):
//! the browser's native messaging frames and the endpoint file the host
//! reads to find the running GPUI.

use serde::{Deserialize, Serialize};
use std::io::{ErrorKind, Read, Write};
use std::path::PathBuf;

/// The ZCode extension's id, fixed by the `key` in its manifest. The host
/// manifest's `allowed_origins` and the host's own origin check use it.
pub const EXTENSION_ID: &str = "ogdkmkfjcfnbhcankdfaeddjjkohdojh";
/// Native messaging host name registered with the browser.
pub const HOST_NAME: &str = "dev.zcode.gpui.browser";
/// Chromium refuses host-to-browser messages above 1 MiB.
pub const MAX_TO_BROWSER: usize = 1024 * 1024;
/// Chromium caps browser-to-host messages at 64 MiB.
pub const MAX_FROM_BROWSER: usize = 64 * 1024 * 1024;
pub const ENDPOINT_VERSION: u32 = 1;

/// Written by GPUI while its bridge listens; removed when it stops.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub version: u32,
    pub port: u16,
    pub token: String,
    pub pid: u32,
}

/// First line the host sends on the GPUI socket.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostHello {
    #[serde(rename = "type")]
    pub kind: String,
    pub token: String,
    pub origin: String,
}

/// The user's home, resolved the same way by GPUI and by the host (which
/// the browser starts without any ZCode environment).
fn user_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

pub fn bridge_dir() -> Option<PathBuf> {
    user_home().map(|h| h.join(".zcode").join("v2").join("gpui-browser-bridge"))
}

pub fn endpoint_path() -> Option<PathBuf> {
    bridge_dir().map(|d| d.join("endpoint.json"))
}

/// The caller origin Chromium passes as the first argument, e.g.
/// `chrome-extension://<id>/`.
pub fn origin_is_extension(origin: &str) -> bool {
    origin
        .strip_prefix("chrome-extension://")
        .map(|rest| rest.trim_end_matches('/'))
        == Some(EXTENSION_ID)
}

/// One native messaging frame: 4-byte native-endian length, then UTF-8 JSON.
/// `Ok(None)` is a clean end of input between frames.
pub fn read_frame(r: &mut impl Read, max: usize) -> std::io::Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    let mut got = 0;
    while got < 4 {
        match r.read(&mut len[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err(ErrorKind::UnexpectedEof.into()),
            Ok(n) => got += n,
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    let len = u32::from_ne_bytes(len) as usize;
    if len > max {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    Ok(Some(body))
}

pub fn write_frame(w: &mut impl Write, payload: &[u8]) -> std::io::Result<()> {
    if payload.len() > MAX_TO_BROWSER {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    w.write_all(&(payload.len() as u32).to_ne_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

/// JSON never needs raw line breaks (inside strings they must be escaped),
/// so a frame becomes one socket line by blanking any stray ones.
pub fn frame_to_line(mut frame: Vec<u8>) -> Vec<u8> {
    for b in frame.iter_mut() {
        if *b == b'\n' || *b == b'\r' {
            *b = b' ';
        }
    }
    frame
}
