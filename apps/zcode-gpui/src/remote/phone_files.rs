//! Phone build: files the platform read for the composer. The Android entry
//! registers a picker (`set_file_picker`) and delivers what the user picked,
//! or an image the keyboard inserted, through `submit_picked_file`; the app
//! polls the queue and uploads each file to the desktop (`remote::staging`).

use std::sync::{Mutex, OnceLock};

pub(crate) enum PickerEvent {
    File {
        name: String,
        mime: String,
        bytes: Vec<u8>,
    },
    Error(String),
}

static EVENTS: Mutex<Vec<PickerEvent>> = Mutex::new(Vec::new());
static FILE_PICKER: OnceLock<fn() -> Result<(), String>> = OnceLock::new();

/// Install the platform's "pick files" launcher. Called once by the Android entry.
pub fn set_file_picker(open: fn() -> Result<(), String>) {
    let _ = FILE_PICKER.set(open);
}

/// Hand a file read on the phone to the composer.
pub fn submit_picked_file(name: String, mime: String, bytes: Vec<u8>) {
    if let Ok(mut events) = EVENTS.lock() {
        events.push(PickerEvent::File { name, mime, bytes });
    }
}

/// Report a file the platform could not read (too large, permission lost).
pub fn submit_picker_error(message: String) {
    if let Ok(mut events) = EVENTS.lock() {
        events.push(PickerEvent::Error(message));
    }
}

pub(crate) fn take_events() -> Vec<PickerEvent> {
    EVENTS
        .lock()
        .map(|mut events| std::mem::take(&mut *events))
        .unwrap_or_default()
}

/// Open the platform picker; `Err` when there is none or it failed to start.
pub(crate) fn open_file_picker() -> Result<(), String> {
    let open = FILE_PICKER.get().ok_or("No file picker on this device")?;
    open()
}
