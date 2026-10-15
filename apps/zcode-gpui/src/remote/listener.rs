//! Desktop TCP+TLS listener for phone sockets. Threads here only accept,
//! authenticate and move lines; every routing decision is made by the
//! gateway in `AppState`, which receives `GatewayEvent`s.

use crate::backend::backlog::MAX_RAW_LINE_BYTES;
use crate::remote::handshake::{DesktopReply, HANDSHAKE_TIMEOUT, PhoneHello};
use crate::remote::line_socket::{LineSocket, POLL_INTERVAL};
use crate::remote::pairing::PairingStore;
use futures::channel::mpsc::UnboundedSender;
use std::io::Write;
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Bytes queued for one phone before the desktop gives up on it.
pub(crate) const MAX_QUEUED_BYTES: usize = 32 * 1024 * 1024;
/// Open sockets (including ones still authenticating).
const MAX_SOCKETS: usize = 16;

static NEXT_CONN: AtomicU64 = AtomicU64::new(1);

pub(crate) enum GatewayEvent {
    Authed {
        conn: u64,
        outbound: PhoneOutbound,
        device: String,
    },
    Line {
        conn: u64,
        line: String,
    },
    Closed {
        conn: u64,
    },
}

/// Desktop → phone line queue with a byte bound. Dropping every clone closes
/// the socket.
#[derive(Clone)]
pub(crate) struct PhoneOutbound {
    tx: Sender<String>,
    queued: Arc<AtomicUsize>,
}

impl PhoneOutbound {
    pub(crate) fn new() -> (Self, Receiver<String>, Arc<AtomicUsize>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let queued = Arc::new(AtomicUsize::new(0));
        (
            Self {
                tx,
                queued: queued.clone(),
            },
            rx,
            queued,
        )
    }

    /// False when the socket is gone or too far behind; the caller closes it.
    pub(crate) fn send(&self, line: String) -> bool {
        let len = line.len();
        if self.queued.load(Ordering::Relaxed) + len > MAX_QUEUED_BYTES {
            return false;
        }
        self.queued.fetch_add(len, Ordering::Relaxed);
        self.tx.send(line).is_ok()
    }
}

/// Stops the accept loop and every socket thread when dropped.
pub(crate) struct ListenerHandle {
    stop: Arc<AtomicBool>,
    pub(crate) addr: SocketAddr,
}

impl Drop for ListenerHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub(crate) fn start(
    bind: SocketAddr,
    tls: Arc<rustls::ServerConfig>,
    store: Arc<Mutex<PairingStore>>,
    events: UnboundedSender<GatewayEvent>,
) -> std::io::Result<ListenerHandle> {
    let listener = TcpListener::bind(bind)?;
    listener.set_nonblocking(true)?;
    let addr = listener.local_addr()?;
    let stop = Arc::new(AtomicBool::new(false));
    let accept_stop = stop.clone();
    let open = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        while !accept_stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((tcp, peer)) => {
                    if open.load(Ordering::Relaxed) >= MAX_SOCKETS {
                        continue;
                    }
                    open.fetch_add(1, Ordering::Relaxed);
                    let (tls, store, events, stop, open) = (
                        tls.clone(),
                        store.clone(),
                        events.clone(),
                        accept_stop.clone(),
                        open.clone(),
                    );
                    std::thread::spawn(move || {
                        serve(tcp, peer.ip(), tls, store, events, stop);
                        open.fetch_sub(1, Ordering::Relaxed);
                    });
                }
                Err(_) => std::thread::sleep(Duration::from_millis(100)),
            }
        }
    });
    Ok(ListenerHandle { stop, addr })
}

/// Check the first line. Ok carries an optional reply (the device token on
/// pairing) and the device name; Err carries the error code for the phone.
fn authenticate(
    line: &str,
    peer: IpAddr,
    store: &Mutex<PairingStore>,
) -> Result<(Option<String>, String), &'static str> {
    let mut store = store.lock().map_err(|_| "internal")?;
    let now = Instant::now();
    if !store.allow_attempt(peer, now) {
        return Err("rate_limited");
    }
    let outcome = match serde_json::from_str::<PhoneHello>(line) {
        Ok(PhoneHello::Pair {
            pairing_token,
            device_name,
        }) => store
            .pair(&pairing_token, &device_name, now)
            .map(|device_token| {
                let name = store
                    .file
                    .device
                    .as_ref()
                    .map(|d| d.name.clone())
                    .unwrap_or_default();
                (Some(DesktopReply::Paired { device_token }.line()), name)
            })
            .ok_or("invalid_pairing_token"),
        Ok(PhoneHello::Auth { device_token }) if store.verify(&device_token) => Ok((
            None,
            store
                .file
                .device
                .as_ref()
                .map(|d| d.name.clone())
                .unwrap_or_default(),
        )),
        Ok(PhoneHello::Auth { .. }) => Err("unauthorized"),
        _ => Err("bad_handshake"),
    };
    if outcome.is_err() {
        store.record_failure(peer, now);
    }
    outcome
}

fn serve(
    tcp: TcpStream,
    peer: IpAddr,
    tls: Arc<rustls::ServerConfig>,
    store: Arc<Mutex<PairingStore>>,
    events: UnboundedSender<GatewayEvent>,
    stop: Arc<AtomicBool>,
) {
    let _ = tcp.set_nonblocking(false);
    let _ = tcp.set_nodelay(true);
    if tcp.set_read_timeout(Some(POLL_INTERVAL)).is_err() {
        return;
    }
    let Ok(server) = rustls::ServerConnection::new(tls) else {
        return;
    };
    let mut sock = LineSocket::new(rustls::StreamOwned::new(server, tcp), MAX_RAW_LINE_BYTES);
    let Ok(first) = sock.read_line_within(HANDSHAKE_TIMEOUT) else {
        return;
    };
    let device = match authenticate(&first, peer, &store) {
        Ok((reply, device)) => {
            if let Some(reply) = reply
                && sock.write_line(&reply).is_err()
            {
                return;
            }
            device
        }
        Err(code) => {
            let _ = sock.write_line(&DesktopReply::error(code));
            return;
        }
    };
    let conn = NEXT_CONN.fetch_add(1, Ordering::Relaxed);
    let (outbound, out_rx, queued) = PhoneOutbound::new();
    if events
        .unbounded_send(GatewayEvent::Authed {
            conn,
            outbound,
            device,
        })
        .is_err()
    {
        return;
    }
    pump(&mut sock, conn, &out_rx, &queued, &events, &stop);
    let _ = events.unbounded_send(GatewayEvent::Closed { conn });
    let stream = sock.get_mut();
    stream.conn.send_close_notify();
    let _ = stream.flush();
}

fn pump<S: std::io::Read + Write>(
    sock: &mut LineSocket<S>,
    conn: u64,
    out_rx: &Receiver<String>,
    queued: &AtomicUsize,
    events: &UnboundedSender<GatewayEvent>,
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
                        .unbounded_send(GatewayEvent::Line { conn, line })
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

/// The address of the interface that carries the default route. Connecting
/// a UDP socket sends nothing; it only selects the route.
pub(crate) fn lan_ip() -> Option<IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

#[cfg(test)]
#[path = "listener_tests.rs"]
mod tests;
