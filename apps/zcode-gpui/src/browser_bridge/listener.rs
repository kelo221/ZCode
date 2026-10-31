//! Loopback listener for `zcode-browser-host` sockets. Threads here only
//! accept, check the token and move lines; `BrowserBridge` in `AppState`
//! makes every decision. The endpoint file tells the host where to connect.

use super::native::{self, ENDPOINT_VERSION, Endpoint, HostHello};
use crate::remote::line_socket::{LineSocket, POLL_INTERVAL};
use crate::remote::listener::PhoneOutbound as Outbound;
use futures::channel::mpsc::UnboundedSender;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

const MAX_SOCKETS: usize = 8;
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
static NEXT_CONN: AtomicU64 = AtomicU64::new(1);

pub(crate) enum BridgeEvent {
    Connected { conn: u64, outbound: Outbound },
    Line { conn: u64, line: String },
    Closed { conn: u64 },
}

/// Stops the accept loop and every socket thread, and removes the endpoint
/// file (if it is still ours), when dropped.
pub(crate) struct BridgeListener {
    stop: Arc<AtomicBool>,
    pub(crate) addr: SocketAddr,
    endpoint_path: PathBuf,
    token: String,
}

impl Drop for BridgeListener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if read_endpoint(&self.endpoint_path).is_some_and(|e| e.token == self.token) {
            let _ = std::fs::remove_file(&self.endpoint_path);
        }
    }
}

fn read_endpoint(path: &PathBuf) -> Option<Endpoint> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn new_token() -> Result<String, String> {
    use ring::rand::SecureRandom;
    let mut bytes = [0u8; 32];
    ring::rand::SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| "no system randomness".to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn tokens_equal(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// Another running ZCode already serves the browser: its endpoint answers.
fn live_owner(path: &PathBuf) -> bool {
    read_endpoint(path).is_some_and(|e| {
        e.pid != std::process::id()
            && TcpStream::connect_timeout(
                &SocketAddr::from(([127, 0, 0, 1], e.port)),
                Duration::from_millis(300),
            )
            .is_ok()
    })
}

pub(crate) fn start(
    endpoint_path: PathBuf,
    events: UnboundedSender<BridgeEvent>,
) -> Result<BridgeListener, String> {
    if live_owner(&endpoint_path) {
        return Err("another GCode window is already connected to the browser".into());
    }
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    let token = new_token()?;
    let endpoint = Endpoint {
        version: ENDPOINT_VERSION,
        port: addr.port(),
        token: token.clone(),
        pid: std::process::id(),
    };
    if let Some(dir) = endpoint_path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let staging = endpoint_path.with_extension("json.tmp");
    std::fs::write(
        &staging,
        serde_json::to_vec(&endpoint).map_err(|e| e.to_string())?,
    )
    .and_then(|_| std::fs::rename(&staging, &endpoint_path))
    .map_err(|e| format!("could not write {}: {e}", endpoint_path.display()))?;

    let stop = Arc::new(AtomicBool::new(false));
    let accept_stop = stop.clone();
    let accept_token = token.clone();
    let open = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        while !accept_stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((tcp, _)) => {
                    if open.load(Ordering::Relaxed) >= MAX_SOCKETS {
                        continue;
                    }
                    open.fetch_add(1, Ordering::Relaxed);
                    let (events, stop, open, token) = (
                        events.clone(),
                        accept_stop.clone(),
                        open.clone(),
                        accept_token.clone(),
                    );
                    std::thread::spawn(move || {
                        serve(tcp, &token, &events, &stop);
                        open.fetch_sub(1, Ordering::Relaxed);
                    });
                }
                Err(_) => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    });
    Ok(BridgeListener {
        stop,
        addr,
        endpoint_path,
        token,
    })
}

fn authenticate(line: &str, token: &str) -> bool {
    serde_json::from_str::<HostHello>(line).is_ok_and(|hello| {
        hello.kind == "hello"
            && tokens_equal(&hello.token, token)
            && native::origin_is_extension(&hello.origin)
    })
}

fn serve(tcp: TcpStream, token: &str, events: &UnboundedSender<BridgeEvent>, stop: &AtomicBool) {
    let _ = tcp.set_nonblocking(false);
    let _ = tcp.set_nodelay(true);
    if tcp.set_read_timeout(Some(POLL_INTERVAL)).is_err() {
        return;
    }
    let mut sock = LineSocket::new(tcp, native::MAX_FROM_BROWSER);
    let Ok(first) = sock.read_line_within(HELLO_TIMEOUT) else {
        return;
    };
    if !authenticate(&first, token) {
        let _ = sock.write_line(r#"{"op":"unavailable","reason":"unauthorized"}"#);
        return;
    }
    let conn = NEXT_CONN.fetch_add(1, Ordering::Relaxed);
    let (outbound, out_rx, queued) = Outbound::new();
    if events
        .unbounded_send(BridgeEvent::Connected { conn, outbound })
        .is_err()
    {
        return;
    }
    pump(&mut sock, conn, &out_rx, &queued, events, stop);
    let _ = events.unbounded_send(BridgeEvent::Closed { conn });
}

fn pump(
    sock: &mut LineSocket<TcpStream>,
    conn: u64,
    out_rx: &Receiver<String>,
    queued: &AtomicUsize,
    events: &UnboundedSender<BridgeEvent>,
    stop: &AtomicBool,
) {
    while !stop.load(Ordering::Relaxed) {
        loop {
            match out_rx.try_recv() {
                Ok(line) => {
                    let written = sock.write_line(&line);
                    queued.fetch_sub(line.len(), Ordering::Relaxed);
                    if written.is_err() {
                        return;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        match sock.read_lines() {
            Ok(lines) => {
                for line in lines {
                    if events
                        .unbounded_send(BridgeEvent::Line { conn, line })
                        .is_err()
                    {
                        return;
                    }
                }
            }
            Err(_) => return,
        }
    }
}
