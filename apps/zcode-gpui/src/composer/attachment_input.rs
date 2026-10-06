use crate::composer::{attachment::AttachmentRef, input::Composer};
use gpui::Context;
use std::path::PathBuf;

impl Composer {
    pub(crate) fn paste_image(
        &mut self,
        bytes: impl AsRef<[u8]>,
        format: gpui::ImageFormat,
        cx: &mut Context<Self>,
    ) {
        let bytes = bytes.as_ref().to_vec();
        let size = bytes.len() as u64;
        let generation = self.replacement_generation;
        let write = cx.background_executor().spawn(async move {
            crate::shared::temp_attachments::write_temp_image(&bytes, format.extension())
        });
        cx.spawn(async move |this, cx| {
            match write.await {
                Ok(path) => {
                    let attachment = AttachmentRef {
                        reference: path.to_string_lossy().into_owned(),
                        file_name: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        // 剪贴板可能提供 JPEG/BMP；必须沿用声明格式，不能仅改后缀冒充 PNG。
                        mime: format.mime_type().into(),
                        bytes: size,
                        preview_ref: None,
                    };
                    let adopted = this
                        .update(cx, |c, cx| {
                            // 异步粘贴完成后 draft 可能已替换；不能把旧输入的附件注入新绑定。
                            if c.replacement_generation != generation {
                                return false;
                            }
                            c.add_owned_attachment(attachment, cx);
                            true
                        })
                        .unwrap_or(false);
                    if !adopted {
                        cx.background_executor()
                            .spawn(
                                async move { crate::shared::temp_attachments::delete_owned(&path) },
                            )
                            .detach();
                    }
                }
                Err(error) => eprintln!(
                    "[zcode-gpui] paste temp file failed: {}",
                    crate::shared::redact::scrub(&error.to_string())
                ),
            }
        })
        .detach();
    }

    pub fn add_attachment(&mut self, att: AttachmentRef, cx: &mut Context<Self>) {
        if !self
            .attachments
            .iter()
            .any(|a| a.reference == att.reference)
        {
            self.attachments.push(att);
            cx.notify();
        }
    }

    pub(crate) fn add_owned_attachment(&mut self, att: AttachmentRef, cx: &mut Context<Self>) {
        let path = PathBuf::from(&att.reference);
        if crate::shared::temp_attachments::is_owned_path(&att.reference)
            && !self.temp_owned.contains(&path)
        {
            self.temp_owned.push(path);
        }
        self.add_attachment(att, cx);
    }

    pub fn remove_attachment(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.attachments.len() {
            return;
        }
        let removed = self.attachments.remove(index);
        let path = PathBuf::from(removed.reference);
        // 队列或拒绝恢复的路径可能沿用 paste 名称；只有 producer 显式转交的所有权允许删除。
        if let Some(index) = self.temp_owned.iter().position(|p| p == &path) {
            let path = self.temp_owned.remove(index);
            cx.background_executor()
                .spawn(async move { crate::shared::temp_attachments::delete_owned(&path) })
                .detach();
        }
        cx.notify();
    }
}

#[cfg(test)]
#[path = "attachment_input_tests.rs"]
mod tests;
