use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

struct TestListener {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl EventListener for TestListener {
    fn send_event(&self, event: Event) {
        if let Event::PtyWrite(text) = event
            && let Ok(mut w) = self.writer.lock()
        {
            let _ = w.write_all(text.as_bytes());
            let _ = w.flush();
        }
    }
}

struct ChildGuard(Box<dyn portable_pty::Child + Send + Sync>);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Dims {
    cols: usize,
    rows: usize,
}
impl alacritty_terminal::grid::Dimensions for Dims {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

#[test]
fn pty_roundtrip_smoke() {
    let pty = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");

    let cmd = CommandBuilder::new(if cfg!(windows) { "cmd.exe" } else { "/bin/sh" });
    let mut reader = pty.master.try_clone_reader().expect("reader");
    let writer = Arc::new(Mutex::new(pty.master.take_writer().expect("writer")));

    let listener = TestListener {
        writer: writer.clone(),
    };
    let mut term = Term::new(Config::default(), &Dims { cols: 80, rows: 24 }, listener);
    let mut processor: Processor = Processor::default();

    let child = ChildGuard(pty.slave.spawn_command(cmd).expect("spawn platform shell"));
    let input: &[u8] = if cfg!(windows) {
        b"set /a 12340+5\r\n"
    } else {
        b"printf '%s\\n' $((12340+5))\n"
    };
    writer.lock().unwrap().write_all(input).unwrap();
    writer.lock().unwrap().flush().unwrap();

    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let _ = tx.send(buf[..n].to_vec());
                }
            }
        }
    });

    let start = std::time::Instant::now();
    let mut output = String::new();

    while start.elapsed() < std::time::Duration::from_secs(6) {
        std::thread::sleep(std::time::Duration::from_millis(60));
        while let Ok(bytes) = rx.try_recv() {
            processor.advance(&mut term, &bytes);
            output.push_str(&String::from_utf8_lossy(&bytes));
        }

        if output.contains("12345") {
            break;
        }
    }

    drop(child);
    assert!(
        output.contains("12345"),
        "PTY did not produce computed output; received:\n{output}"
    );
}
