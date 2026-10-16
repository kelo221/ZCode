//! zcode-voice: text-to-speech sidecar for the unofficial ZCode GPUI client.
//!
//! Usage:
//!   zcode-voice --model-dir <dir>          NDJSON commands on stdin, events on stdout
//!   zcode-voice --model-dir <dir> --say T  speak T once and exit (manual check)
//!
//! Closing stdin stops speech and exits, so the sidecar never outlives GPUI.

mod engine;
mod espeak;
mod models;
mod protocol;
mod queue;

use engine::Shared;
use protocol::{Command, ErrorCode, Event, SpeakRequest, clean_text, emit, parse_command};
use std::io::BufRead;
use std::path::PathBuf;
use std::sync::Arc;

struct Args {
    model_dir: PathBuf,
    say: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut model_dir = None;
    let mut say = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--model-dir" => model_dir = args.next().map(PathBuf::from),
            "--say" => say = args.next(),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    let model_dir = model_dir.ok_or("--model-dir <dir> is required")?;
    Ok(Args { model_dir, say })
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("zcode-voice: {message}");
            std::process::exit(2);
        }
    };
    // Must run before any other thread exists (it sets an env var).
    let espeak = espeak::configure();
    let shared = Arc::new(Shared::default());
    let worker = {
        let shared = shared.clone();
        let model_dir = args.model_dir.clone();
        std::thread::spawn(move || engine::run(shared, model_dir, espeak))
    };

    if let Some(text) = args.say {
        shared.queue.push(SpeakRequest {
            id: "say".into(),
            text: clean_text(&text),
            voice: None,
            speed: None,
            volume: None,
        });
        shared.queue.close();
        let _ = worker.join();
        return;
    }

    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        handle_line(&shared, &line);
    }
    shared.queue.close();
    shared.stop_current();
    std::process::exit(0);
}

fn handle_line(shared: &Shared, line: &str) {
    match parse_command(line) {
        Ok(Command::Speak(mut request)) => {
            request.text = clean_text(&request.text);
            if request.text.is_empty() {
                return emit(&Event::Error {
                    id: Some(request.id),
                    code: ErrorCode::BadCommand,
                    message: "empty text".into(),
                });
            }
            if let Some(dropped) = shared.queue.push(request) {
                emit(&Event::Dropped { id: dropped.id });
            }
        }
        Ok(Command::Stop) => {
            for dropped in shared.queue.clear() {
                emit(&Event::Dropped { id: dropped.id });
            }
            shared.stop_current();
        }
        Ok(Command::Status) => {
            let state = shared
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            emit(&state.unwrap_or(Event::Loading));
        }
        Err(message) => emit(&Event::Error {
            id: None,
            code: ErrorCode::BadCommand,
            message,
        }),
    }
}
