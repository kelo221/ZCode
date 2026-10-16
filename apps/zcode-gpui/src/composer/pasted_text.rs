//! Long clipboard text pasted as a `text/plain` attachment instead of
//! inline text (Desktop `shouldCreateClipboardTextAttachment`,
//! `packages/ui/src/lib/chatAttachments.ts`).

use crate::composer::attachment::AttachmentRef;
use crate::composer::input::Composer;
use gpui::Context;

/// `LONG_PASTE_TEXT_ATTACHMENT_CHAR_THRESHOLD`, measured like JS `length`.
pub(crate) const LONG_PASTE_UTF16_THRESHOLD: usize = 15 * 1024;

pub(crate) const PASTED_TEXT_MIME: &str = "text/plain";

pub(crate) fn is_long_paste(text: &str) -> bool {
    // UTF-8 bytes are never fewer than UTF-16 units, so short text skips the count.
    text.len() >= LONG_PASTE_UTF16_THRESHOLD
        && text
            .encode_utf16()
            .nth(LONG_PASTE_UTF16_THRESHOLD - 1)
            .is_some()
}

/// `countClipboardTextLines`: `\n`, `\r\n` and a lone `\r` each end a line.
pub(crate) fn count_lines(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let mut lines = 1;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\n' => lines += 1,
            '\r' => {
                lines += 1;
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
            }
            _ => {}
        }
    }
    lines
}

/// `pasted-text-YYYYMMDD-HHMMSS.txt` in local time.
pub(crate) fn file_name_at(ms: u64, offset_secs: i64) -> String {
    let (y, m, d, hh, mm, ss) = crate::shared::local_time::civil_time(ms, offset_secs);
    format!("pasted-text-{y:04}{m:02}{d:02}-{hh:02}{mm:02}{ss:02}.txt")
}

pub(crate) fn file_name_now() -> String {
    file_name_at(
        crate::shared::local_time::now_ms(),
        crate::shared::local_time::local_offset_secs(),
    )
}

impl Composer {
    /// Attach pasted text and remember its line count for the chip.
    pub(crate) fn add_pasted_text(
        &mut self,
        attachment: AttachmentRef,
        lines: usize,
        cx: &mut Context<Self>,
    ) {
        self.pasted_lines
            .insert(attachment.reference.clone(), lines);
        self.add_owned_attachment(attachment, cx);
    }

    /// Line count when `attachment` came from a long paste.
    pub(crate) fn pasted_text_lines(&self, attachment: &AttachmentRef) -> Option<usize> {
        self.pasted_lines.get(&attachment.reference).copied()
    }

    /// New-task draft (no session to upload to): write an owned temp file,
    /// like a draft clipboard image.
    pub(crate) fn paste_text_file(&mut self, text: String, cx: &mut Context<Self>) {
        let generation = self.replacement_generation;
        let lines = count_lines(&text);
        let file_name = file_name_now();
        let write = cx.background_executor().spawn(async move {
            crate::shared::temp_attachments::write_temp_text(&text).map(|p| (p, text.len()))
        });
        cx.spawn(async move |this, cx| match write.await {
            Ok((path, bytes)) => {
                let attachment = AttachmentRef {
                    reference: path.to_string_lossy().into_owned(),
                    file_name,
                    mime: PASTED_TEXT_MIME.into(),
                    bytes: bytes as u64,
                    preview_ref: None,
                };
                let adopted = this
                    .update(cx, |c, cx| {
                        // 与图片粘贴相同：写盘期间草稿已被替换时，不能把附件注入新草稿。
                        if c.replacement_generation != generation {
                            return false;
                        }
                        c.add_pasted_text(attachment, lines, cx);
                        true
                    })
                    .unwrap_or(false);
                if !adopted {
                    cx.background_executor()
                        .spawn(async move { crate::shared::temp_attachments::delete_owned(&path) })
                        .detach();
                }
            }
            Err(error) => eprintln!(
                "[zcode-gpui] pasted text temp file failed: {}",
                crate::shared::redact::scrub(&error.to_string())
            ),
        })
        .detach();
    }
}

#[cfg(test)]
#[path = "pasted_text_tests.rs"]
mod tests;
