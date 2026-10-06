//! Tests for EventBacklog admission and the bounded line reader
//! (review finding 6).

use super::*;
use std::io::Cursor;

#[test]
fn oversized_unterminated_line_stays_bounded() {
    // A hostile 64 KiB unterminated blob plus a following valid line: the
    // reader must not grow past max+1, must report Overlong, then resume.
    let mut data = vec![b'x'; 64];
    data.extend_from_slice(b"\nok\n");
    let mut reader = Cursor::new(data);
    assert_eq!(read_bounded_line(&mut reader, 8), BoundedLine::Overlong);
    assert_eq!(
        read_bounded_line(&mut reader, 8),
        BoundedLine::Line(b"ok".to_vec())
    );
    assert_eq!(read_bounded_line(&mut reader, 8), BoundedLine::Eof);
}

#[test]
fn concurrent_producers_never_exceed_the_byte_bound() {
    let cap = 1000usize;
    let backlog = EventBacklog::new(cap);
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let b = backlog.clone();
            std::thread::spawn(move || {
                let mut admitted = 0usize;
                for _ in 0..200 {
                    if b.reserve(80) {
                        admitted += 80;
                    }
                }
                admitted
            })
        })
        .collect();
    let total: usize = threads.into_iter().map(|t| t.join().unwrap()).sum();
    assert!(total <= cap, "reserved {total} over cap {cap}");
    assert!(backlog.bytes() <= cap);
    assert!(
        backlog.take_overflow(),
        "at least one producer must overflow"
    );
}

#[test]
fn stderr_overflow_does_not_latch_the_protocol_queue() {
    let proto = EventBacklog::new(64);
    let logs = EventBacklog::new(16);
    assert!(logs.reserve(8));
    assert!(!logs.reserve(16), "log queue overflows independently");
    assert!(logs.take_overflow());
    assert!(!proto.take_overflow(), "protocol queue stays clean");
    assert!(proto.reserve(32));
}
