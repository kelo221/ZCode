//! Phone attachments: files the platform picker or keyboard read, uploaded
//! to the desktop's staging endpoint (`remote::staging`).

use crate::app::store::AppState;
use crate::backend::attachment_upload::UploadKind;
use crate::composer::attachment;
use gpui::Context;
use std::sync::Arc;

/// An absolute desktop path from `gcode/stage/commit`. The phone cannot
/// check it with `Path::is_absolute` because the desktop may be Windows.
pub(crate) fn valid_staged_path(reference: &str) -> bool {
    let bytes = reference.as_bytes();
    let windows = bytes.len() > 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    reference == reference.trim()
        && !reference.chars().any(char::is_control)
        && (windows || (reference.len() > 1 && reference.starts_with('/')))
}

impl AppState {
    /// A file the phone's picker or keyboard read (`remote::phone_files`).
    /// Images go through the clipboard-image path so unsupported formats are
    /// converted the same way; everything else uploads as is.
    pub(crate) fn start_file_upload(
        &mut self,
        file_name: String,
        mime: String,
        bytes: Vec<u8>,
        cx: &mut Context<Self>,
    ) {
        let replacement = self.composer.read(cx).replacement_generation;
        if let Some(format) = gpui::ImageFormat::from_mime_type(&mime) {
            self.start_image_upload(bytes, format, replacement, cx);
            return;
        }
        if self.is_read_only_view()
            || self.composer_intent != crate::conversation::msg_actions::ComposerIntent::Send
        {
            self.push_error("Attachments are unavailable while editing or renaming".into());
            cx.notify();
            return;
        }
        if bytes.len() > attachment::ATTACHMENT_MAX_BYTES {
            self.push_error(format!("{file_name} is larger than 20 MiB"));
            cx.notify();
            return;
        }
        let Some(Some(receipt)) =
            self.paste_target(replacement, "Connect this chat before attaching files", cx)
        else {
            return;
        };
        let kind = UploadKind::File { file_name, mime };
        self.insert_image_upload(receipt, Arc::new(bytes), kind, cx);
    }
}
