//! Newline-delimited lines over a blocking stream with a short read timeout,
//! so one thread can both read and write (rustls streams cannot be split).

use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::time::{Duration, Instant};

/// Socket read timeout: the upper bound on how long a queued outbound line
/// waits while the IO thread is blocked in a read.
pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(15);
const WRITE_DEADLINE: Duration = Duration::from_secs(10);

pub(crate) struct LineSocket<S> {
    stream: S,
    buf: Vec<u8>,
    /// Lines already read but not yet handed out by `read_line_within`.
    ready: VecDeque<String>,
    max_line: usize,
}

fn is_timeout(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
    )
}

impl<S: Read + Write> LineSocket<S> {
    pub(crate) fn new(stream: S, max_line: usize) -> Self {
        Self {
            stream,
            buf: Vec::new(),
            ready: VecDeque::new(),
            max_line,
        }
    }

    /// Complete lines read so far; empty when the read timed out. EOF and an
    /// unterminated line longer than `max_line` are errors.
    pub(crate) fn read_lines(&mut self) -> std::io::Result<Vec<String>> {
        if !self.ready.is_empty() {
            return Ok(self.ready.drain(..).collect());
        }
        let mut chunk = [0u8; 16 * 1024];
        match self.stream.read(&mut chunk) {
            Ok(0) => return Err(ErrorKind::UnexpectedEof.into()),
            Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
            Err(error) if is_timeout(&error) => return Ok(Vec::new()),
            Err(error) => return Err(error),
        }
        let mut lines = Vec::new();
        while let Some(pos) = self.buf.iter().position(|b| *b == b'\n') {
            let mut line: Vec<u8> = self.buf.drain(..=pos).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.len() > self.max_line {
                return Err(std::io::Error::new(ErrorKind::InvalidData, "line too long"));
            }
            if let Ok(text) = String::from_utf8(line)
                && !text.trim().is_empty()
            {
                lines.push(text);
            }
        }
        if self.buf.len() > self.max_line {
            return Err(std::io::Error::new(ErrorKind::InvalidData, "line too long"));
        }
        Ok(lines)
    }

    /// Wait up to `timeout` for the first complete line.
    pub(crate) fn read_line_within(&mut self, timeout: Duration) -> std::io::Result<String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(line) = self.ready.pop_front() {
                return Ok(line);
            }
            let lines = self.read_lines()?;
            self.ready.extend(lines);
            if let Some(line) = self.ready.pop_front() {
                return Ok(line);
            }
            if Instant::now() >= deadline {
                return Err(ErrorKind::TimedOut.into());
            }
        }
    }

    pub(crate) fn write_line(&mut self, line: &str) -> std::io::Result<()> {
        let mut bytes = Vec::with_capacity(line.len() + 1);
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
        let deadline = Instant::now() + WRITE_DEADLINE;
        let mut written = 0;
        while written < bytes.len() {
            match self.stream.write(&bytes[written..]) {
                Ok(0) => return Err(ErrorKind::WriteZero.into()),
                Ok(n) => written += n,
                Err(error) if is_timeout(&error) && Instant::now() < deadline => {}
                Err(error) => return Err(error),
            }
        }
        loop {
            match self.stream.flush() {
                Ok(()) => return Ok(()),
                Err(error) if is_timeout(&error) && Instant::now() < deadline => {}
                Err(error) => return Err(error),
            }
        }
    }

    pub(crate) fn get_mut(&mut self) -> &mut S {
        &mut self.stream
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    struct Duplex {
        input: Cursor<Vec<u8>>,
        output: Vec<u8>,
    }

    impl Read for Duplex {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.input.read(buf)
        }
    }

    impl Write for Duplex {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.output.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn socket(input: &[u8], max: usize) -> LineSocket<Duplex> {
        LineSocket::new(
            Duplex {
                input: Cursor::new(input.to_vec()),
                output: Vec::new(),
            },
            max,
        )
    }

    #[test]
    fn splits_lines_and_skips_blank_and_crlf() {
        let mut s = socket(b"{\"a\":1}\r\n\n{\"b\":2}\npartial", 64);
        assert_eq!(s.read_lines().unwrap(), vec!["{\"a\":1}", "{\"b\":2}"]);
        assert!(matches!(s.read_lines(), Err(e) if e.kind() == ErrorKind::UnexpectedEof));
    }

    #[test]
    fn overlong_lines_fail_before_a_newline_arrives() {
        let mut s = socket(&[b'x'; 100], 10);
        assert!(matches!(s.read_lines(), Err(e) if e.kind() == ErrorKind::InvalidData));
        let mut s = socket(b"0123456789abc\n", 10);
        assert!(matches!(s.read_lines(), Err(e) if e.kind() == ErrorKind::InvalidData));
    }

    #[test]
    fn read_line_within_keeps_the_rest_of_a_batch() {
        let mut s = socket(b"one\ntwo\n", 64);
        assert_eq!(s.read_line_within(Duration::from_secs(1)).unwrap(), "one");
        assert_eq!(s.read_lines().unwrap(), vec!["two"]);
    }

    #[test]
    fn write_appends_newline() {
        let mut s = socket(b"", 10);
        s.write_line("hi").unwrap();
        assert_eq!(s.get_mut().output, b"hi\n");
    }
}
