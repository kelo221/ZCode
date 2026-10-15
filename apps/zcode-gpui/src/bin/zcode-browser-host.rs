//! Native messaging host started by Chromium for the ZCode extension. It keeps
//! no state: it checks the caller, finds the running GPUI through the endpoint
//! file, authenticates with its token and relays messages both ways until
//! either side closes (spec: BROWSER_BRIDGE.md).

#[path = "../browser_bridge/native.rs"]
#[allow(dead_code)]
mod native;

use native::{Endpoint, HostHello, MAX_FROM_BROWSER};
use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Tell the extension why there is no session, then exit; it retries later.
fn unavailable(reason: &str) -> ! {
    let message = serde_json::json!({ "op": "unavailable", "reason": reason }).to_string();
    let mut out = std::io::stdout().lock();
    let _ = native::write_frame(&mut out, message.as_bytes());
    std::process::exit(0);
}

fn connect() -> Result<TcpStream, &'static str> {
    let path = native::endpoint_path().ok_or("no_home_directory")?;
    let text = std::fs::read_to_string(path).map_err(|_| "gpui_not_running")?;
    let endpoint: Endpoint = serde_json::from_str(&text).map_err(|_| "gpui_not_running")?;
    if endpoint.version != native::ENDPOINT_VERSION {
        return Err("version_mismatch");
    }
    let addr = SocketAddr::from(([127, 0, 0, 1], endpoint.port));
    let mut stream =
        TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(|_| "gpui_not_running")?;
    let _ = stream.set_nodelay(true);
    let hello = HostHello {
        kind: "hello".into(),
        token: endpoint.token,
        origin: format!("chrome-extension://{}/", native::EXTENSION_ID),
    };
    let line = serde_json::to_string(&hello).map_err(|_| "internal")?;
    stream
        .write_all(format!("{line}\n").as_bytes())
        .map_err(|_| "gpui_not_running")?;
    Ok(stream)
}

fn main() {
    // Chromium passes the caller origin first (Windows adds --parent-window).
    let origin = std::env::args().nth(1).unwrap_or_default();
    if !native::origin_is_extension(&origin) {
        unavailable("unknown_caller");
    }
    let stream = match connect() {
        Ok(stream) => stream,
        Err(reason) => unavailable(reason),
    };
    let Ok(upstream) = stream.try_clone() else {
        unavailable("internal");
    };

    // Browser -> GPUI: frames become lines.
    std::thread::spawn(move || {
        let mut upstream = upstream;
        let mut stdin = std::io::stdin().lock();
        while let Ok(Some(frame)) = native::read_frame(&mut stdin, MAX_FROM_BROWSER) {
            let mut line = native::frame_to_line(frame);
            line.push(b'\n');
            if upstream.write_all(&line).is_err() {
                break;
            }
        }
        // The browser closed the port (or sent garbage): end the session.
        let _ = upstream.shutdown(Shutdown::Both);
        std::process::exit(0);
    });

    // GPUI -> browser: lines become frames.
    let mut stdout = std::io::stdout().lock();
    let mut reader = BufReader::new(stream);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                while matches!(line.last(), Some(b'\n' | b'\r')) {
                    line.pop();
                }
                if line.is_empty() {
                    continue;
                }
                if native::write_frame(&mut stdout, &line).is_err() {
                    break;
                }
            }
        }
    }
    std::process::exit(0);
}
