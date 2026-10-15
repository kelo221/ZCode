use super::listener::{self, BridgeEvent};
use super::native::{self, Endpoint};
use futures::channel::mpsc::UnboundedReceiver;
use std::io::{BufRead, BufReader, Cursor, ErrorKind, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

fn temp_endpoint(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("zcode-bridge-test-{name}-{}", uuid::Uuid::now_v7()));
    dir.join("endpoint.json")
}

fn next_event(rx: &mut UnboundedReceiver<BridgeEvent>) -> BridgeEvent {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(event) = rx.try_recv() {
            return event;
        }
        assert!(Instant::now() < deadline, "no bridge event");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn host_socket(endpoint: &Endpoint, token: &str) -> TcpStream {
    let mut tcp = TcpStream::connect(("127.0.0.1", endpoint.port)).unwrap();
    let hello = serde_json::json!({
        "type": "hello",
        "token": token,
        "origin": format!("chrome-extension://{}/", native::EXTENSION_ID),
    });
    tcp.write_all(format!("{hello}\n").as_bytes()).unwrap();
    tcp
}

#[test]
fn frames_round_trip_and_reject_oversized_or_truncated_input() {
    let mut out = Vec::new();
    native::write_frame(&mut out, br#"{"op":"hello"}"#).unwrap();
    assert_eq!(&out[..4], &14u32.to_ne_bytes());
    let mut input = Cursor::new(out);
    let frame = native::read_frame(&mut input, 64).unwrap().unwrap();
    assert_eq!(frame, br#"{"op":"hello"}"#);
    assert!(native::read_frame(&mut input, 64).unwrap().is_none());

    let mut big = Cursor::new(100u32.to_ne_bytes().to_vec());
    let err = native::read_frame(&mut big, 10).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidData);
    let mut cut = Cursor::new(vec![1u8, 0]);
    assert_eq!(
        native::read_frame(&mut cut, 10).unwrap_err().kind(),
        ErrorKind::UnexpectedEof
    );
    let huge = vec![b'x'; native::MAX_TO_BROWSER + 1];
    assert!(native::write_frame(&mut Vec::new(), &huge).is_err());
    assert_eq!(
        native::frame_to_line(b"{\"a\":\n1}\r".to_vec()),
        b"{\"a\": 1} ".to_vec()
    );
}

#[test]
fn only_the_zcode_extension_origin_is_accepted() {
    let id = native::EXTENSION_ID;
    assert!(native::origin_is_extension(&format!(
        "chrome-extension://{id}/"
    )));
    assert!(native::origin_is_extension(&format!(
        "chrome-extension://{id}"
    )));
    assert!(!native::origin_is_extension(
        "chrome-extension://abcdefghijklmnopabcdefghijklmnop/"
    ));
    assert!(!native::origin_is_extension(&format!("https://{id}/")));
    assert!(!native::origin_is_extension(""));
}

#[test]
fn the_listener_authenticates_hosts_and_moves_lines_both_ways() {
    let path = temp_endpoint("auth");
    let (tx, mut rx) = futures::channel::mpsc::unbounded();
    let handle = listener::start(path.clone(), tx).unwrap();
    let endpoint: Endpoint =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(endpoint.port, handle.addr.port());
    assert_eq!(endpoint.token.len(), 64);

    // A wrong token is refused and never becomes a connection.
    let bad = host_socket(&endpoint, &"0".repeat(64));
    let mut refusal = String::new();
    BufReader::new(bad).read_line(&mut refusal).unwrap();
    assert!(refusal.contains("unauthorized"));

    let good = host_socket(&endpoint, &endpoint.token);
    let BridgeEvent::Connected { conn, outbound } = next_event(&mut rx) else {
        panic!("expected a connection");
    };
    let mut writer = good.try_clone().unwrap();
    writer.write_all(b"{\"op\":\"hello\"}\n").unwrap();
    match next_event(&mut rx) {
        BridgeEvent::Line { conn: c, line } => {
            assert_eq!(c, conn);
            assert_eq!(line, "{\"op\":\"hello\"}");
        }
        _ => panic!("expected a line"),
    }
    assert!(outbound.send("{\"op\":\"welcome\"}".into()));
    let mut reader = BufReader::new(good);
    let mut welcome = String::new();
    reader.read_line(&mut welcome).unwrap();
    assert_eq!(welcome.trim(), "{\"op\":\"welcome\"}");

    drop(writer);
    drop(reader);
    assert!(matches!(next_event(&mut rx), BridgeEvent::Closed { conn: c } if c == conn));
    drop(handle);
    assert!(
        !path.exists(),
        "the endpoint file goes away with the listener"
    );
}

#[test]
fn a_live_endpoint_owned_by_another_process_is_not_taken_over() {
    let path = temp_endpoint("owner");
    let other = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let foreign = Endpoint {
        version: native::ENDPOINT_VERSION,
        port: other.local_addr().unwrap().port(),
        token: "t".into(),
        pid: std::process::id() + 1,
    };
    std::fs::write(&path, serde_json::to_vec(&foreign).unwrap()).unwrap();
    let (tx, _rx) = futures::channel::mpsc::unbounded();
    assert!(listener::start(path.clone(), tx).is_err());

    // Once that owner is gone, its stale file is replaced.
    drop(other);
    let (tx, _rx) = futures::channel::mpsc::unbounded();
    let handle = listener::start(path.clone(), tx).unwrap();
    let endpoint: Endpoint =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(endpoint.pid, std::process::id());
    drop(handle);
}
