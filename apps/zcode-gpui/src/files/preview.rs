//! File preview reader and UI rendering (markdown, text, external Office/PDF launcher).

use crate::shared::theme::{ACCENT, BORDER, CARD, HOVER, MUTED, TEXT};
use crate::shared::theme_colors::color as rgb;
use crate::shared::{i18n::label, os::file_launcher, theme::ui_size};
use gpui::{AnyElement, CursorStyle, IntoElement, ParentElement, Styled, div, prelude::*, px};
use std::path::Path;

const MAX_PREVIEW_BYTES: u64 = 256 * 1024;

/// What the preview pane shows for the selected file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Preview {
    Image(crate::files::image_preview::ImagePreview),
    Markdown(String),
    Text(String),
    External { kind: String, extension: String },
    Binary,
    TooLarge(u64),
    Error(PreviewError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreviewError {
    Unavailable,
    NotRegular,
    ReadFailed,
    InvalidImage,
}

impl PreviewError {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Unavailable => label("File is unavailable", "文件不可用"),
            Self::NotRegular => label("Not a regular file", "不是普通文件"),
            Self::ReadFailed => label("File could not be read", "无法读取文件"),
            Self::InvalidImage => label(
                "Image is invalid or exceeds preview limits",
                "图片无效或超出预览限制",
            ),
        }
    }
}

pub fn pane_note(text: &str) -> AnyElement {
    div()
        .p_3()
        .text_size(px(ui_size(12.)))
        .text_color(rgb(MUTED))
        .child(text.to_string())
        .into_any_element()
}

pub fn preview_element(path: &Path, preview: &Preview) -> AnyElement {
    let path_buf = path.to_path_buf();
    let header = div()
        .px_2()
        .py_1()
        .text_size(px(ui_size(11.)))
        .text_color(rgb(MUTED))
        .border_b_1()
        .border_color(rgb(BORDER))
        .child(path.display().to_string());

    let body: AnyElement = match preview {
        Preview::Image(image) => crate::files::image_preview::element(image),
        Preview::Error(e) => pane_note(e.label()),
        Preview::Binary => pane_note(label("Binary file", "二进制文件")),
        Preview::TooLarge(size) => pane_note(&format!(
            "{} ({size} {})",
            label("File too large", "文件过大"),
            label("bytes", "字节")
        )),
        Preview::Text(text) => div()
            .p_2()
            .font_family(crate::shared::theme::MONO_FONT)
            .text_size(px(11.5))
            .child(text.clone())
            .into_any_element(),
        Preview::Markdown(text) => div()
            .p_2()
            .child(crate::shared::markdown::render_markdown(
                text, 0xF11E5, false,
            ))
            .into_any_element(),
        Preview::External { kind, extension } => {
            let p1 = path_buf.clone();
            let p2 = path_buf.clone();
            let p3 = path_buf.clone();
            div()
                .p_4()
                .flex()
                .flex_col()
                .gap_3()
                .items_start()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_size(px(13.))
                                .text_color(rgb(TEXT))
                                .child(format!("{kind} (.{extension})")),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(rgb(MUTED))
                                .child("This document type has no native renderer in GPUI. Open externally with system default or your editor."),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(
                            preview_action_btn("btn-open-default", "Open in System App", move |_, _, _| {
                                let _ = file_launcher::open_with_system_default(&p1);
                            })
                        )
                        .child(
                            preview_action_btn("btn-reveal", "Reveal in File Manager", move |_, _, _| {
                                let _ = file_launcher::reveal_in_file_manager(&p2);
                            })
                        )
                        .child(
                            preview_action_btn("btn-open-code", "Open in VS Code", move |_, _, _| {
                                let _ = file_launcher::open_in_editor(&p3);
                            })
                        ),
                )
                .into_any_element()
        }
    };

    div()
        .flex()
        .flex_col()
        .min_h_0()
        .child(header)
        .child(body)
        .into_any_element()
}

fn preview_action_btn(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .px_3()
        .py_1()
        .rounded_md()
        .bg(rgb(CARD))
        .border_1()
        .border_color(rgb(BORDER))
        .text_size(px(11.5))
        .text_color(rgb(ACCENT))
        .cursor(CursorStyle::PointingHand)
        .hover(|h| h.bg(rgb(HOVER)))
        .on_click(on_click)
        .child(label)
        .into_any_element()
}

/// Sniff file extension and contents for preview classification.
pub fn read_preview(path: &Path) -> Preview {
    use std::io::Read;
    let Ok(file) = std::fs::File::open(path) else {
        return Preview::Error(PreviewError::Unavailable);
    };
    let Ok(meta) = file.metadata() else {
        return Preview::Error(PreviewError::Unavailable);
    };
    if !meta.is_file() {
        return Preview::Error(PreviewError::NotRegular);
    }
    let format = crate::files::image_preview::format(path);
    let limit = if format.is_some() {
        crate::files::image_preview::MAX_IMAGE_BYTES
    } else {
        MAX_PREVIEW_BYTES
    };
    if meta.len() > limit {
        return Preview::TooLarge(meta.len());
    }
    let mut bytes = Vec::new();
    // 文件在 metadata 后仍可增长；从同一 handle 限长读取，不能让变化绕过预览上界。
    if file.take(limit + 1).read_to_end(&mut bytes).is_err() {
        return Preview::Error(PreviewError::ReadFailed);
    }
    if bytes.len() as u64 > limit {
        return Preview::TooLarge(bytes.len() as u64);
    }
    if let Some(format) = format {
        return crate::files::image_preview::decode(bytes, format);
    }

    if let Some(ext) = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
    {
        match ext.as_str() {
            "pdf" => {
                return Preview::External {
                    kind: "PDF Document".into(),
                    extension: ext,
                };
            }
            "docx" | "doc" => {
                return Preview::External {
                    kind: "Word Document".into(),
                    extension: ext,
                };
            }
            "xlsx" | "xls" | "csv" => {
                return Preview::External {
                    kind: "Spreadsheet".into(),
                    extension: ext,
                };
            }
            "pptx" | "ppt" => {
                return Preview::External {
                    kind: "PowerPoint Presentation".into(),
                    extension: ext,
                };
            }
            _ => {}
        }
    }

    if bytes.contains(&0) {
        return Preview::Binary;
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    match path.extension().and_then(|e| e.to_str()) {
        Some("md") | Some("markdown") => Preview::Markdown(text),
        _ => Preview::Text(text),
    }
}
