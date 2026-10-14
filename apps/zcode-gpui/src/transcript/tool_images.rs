//! Images in tool results: inline base64 from the row display
//! (`node_repl_images`, `media` of CUA/MCP results; toolDisplay.ts) and image
//! files read by the Read tool. Decoding runs in the background through the
//! Files pane's decoder and is cached by source.

use crate::files::image_preview::{ImagePreview, decode, element};
use crate::files::preview::{Preview, read_preview};
use crate::shared::theme::{BORDER, MUTED, ui_size};
use crate::shared::theme_colors::color as rgb;
use base64::Engine;
use gpui::{AnyElement, Context, ElementId, ObjectFit, StyledImage, div, img, prelude::*, px};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ImageRef {
    Inline { mime: String, data: String },
    File(PathBuf),
}

impl ImageRef {
    fn key(&self) -> String {
        match self {
            Self::Inline { data, .. } => format!(
                "b64:{}:{:08x}",
                data.len(),
                crc32fast::hash(data.as_bytes())
            ),
            Self::File(p) => format!("file:{}", p.display()),
        }
    }
}

fn inline_items(list: Option<&Value>, data_key: &str, out: &mut Vec<ImageRef>) {
    for item in list.and_then(Value::as_array).into_iter().flatten() {
        let mime = item.get("mimeType").and_then(Value::as_str).unwrap_or("");
        if let Some(data) = item.get(data_key).and_then(Value::as_str)
            && mime.starts_with("image/")
        {
            out.push(ImageRef::Inline {
                mime: mime.to_string(),
                data: data.to_string(),
            });
        }
    }
}

/// Image sources of a finished tool call.
pub(crate) fn image_refs(
    tool: &str,
    status: &str,
    input: Option<&Value>,
    display: Option<&Value>,
) -> Vec<ImageRef> {
    let mut out = Vec::new();
    if let Some(d) = display {
        inline_items(d.get("images"), "base64", &mut out);
        inline_items(d.get("media"), "data", &mut out);
    }
    if tool == "Read"
        && status == "success"
        && let Some(path) = input
            .and_then(|v| v.get("file_path"))
            .and_then(Value::as_str)
            .map(PathBuf::from)
        && path.is_absolute()
        && crate::files::image_preview::format(&path).is_some()
    {
        out.push(ImageRef::File(path));
    }
    out
}

fn mime_format(mime: &str) -> Option<image::ImageFormat> {
    match mime.to_ascii_lowercase().as_str() {
        "image/png" => Some(image::ImageFormat::Png),
        "image/jpeg" | "image/jpg" => Some(image::ImageFormat::Jpeg),
        "image/gif" => Some(image::ImageFormat::Gif),
        "image/webp" => Some(image::ImageFormat::WebP),
        "image/bmp" => Some(image::ImageFormat::Bmp),
        _ => None,
    }
}

fn load(r: ImageRef) -> Result<ImagePreview, String> {
    let preview = match r {
        ImageRef::Inline { mime, data } => {
            let format =
                mime_format(&mime).ok_or_else(|| format!("Unsupported image type {mime}"))?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data.as_bytes())
                .map_err(|_| "Image data is not valid base64".to_string())?;
            decode(bytes, format)
        }
        ImageRef::File(path) => read_preview(&path),
    };
    match preview {
        Preview::Image(img) => Ok(img),
        Preview::Error(e) => Err(e.label().to_string()),
        Preview::TooLarge(_) => Err("Image is too large to preview".to_string()),
        _ => Err("Not an image".to_string()),
    }
}

#[derive(Default)]
pub(crate) struct ToolImageCache {
    images: HashMap<String, Option<Result<ImagePreview, String>>>,
    enlarged: HashSet<String>,
}

impl crate::app::root::RootView {
    fn start_tool_image(&mut self, r: ImageRef, cx: &mut Context<Self>) {
        let key = r.key();
        if self.tool_images.images.contains_key(&key) {
            return;
        }
        self.tool_images.images.insert(key.clone(), None);
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { load(r) }).await;
            this.update(cx, |v, cx| {
                v.tool_images.images.insert(key, Some(result));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn tool_images_view(
        &self,
        refs: Vec<ImageRef>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if refs.is_empty() {
            return None;
        }
        let mut row = div().w_full().min_w_0().flex().flex_wrap().gap_2();
        for r in refs {
            let key = r.key();
            match self.tool_images.images.get(&key) {
                None => {
                    // Render cannot mutate; claim and start the load on the next turn.
                    let r = r.clone();
                    cx.spawn(async move |this, cx| {
                        this.update(cx, |v, cx| v.start_tool_image(r, cx)).ok();
                    })
                    .detach();
                    row = row.child(note("Loading image…"));
                }
                Some(None) => row = row.child(note("Loading image…")),
                Some(Some(Err(e))) => row = row.child(note(e)),
                Some(Some(Ok(preview))) => {
                    let big = self.tool_images.enlarged.contains(&key);
                    let id = ElementId::Name(format!("tool-img-{key}").into());
                    row = row.child(
                        if big {
                            div()
                                .id(id)
                                .w_full()
                                .cursor_pointer()
                                .child(element(preview))
                        } else {
                            div()
                                .id(id)
                                .cursor_pointer()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(BORDER))
                                .overflow_hidden()
                                .child(
                                    img(gpui::ImageSource::Render(preview.image.clone()))
                                        .w(px(240.))
                                        .h(px(160.))
                                        .object_fit(ObjectFit::Contain),
                                )
                        }
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if !this.tool_images.enlarged.remove(&key) {
                                this.tool_images.enlarged.insert(key.clone());
                            }
                            cx.notify();
                        })),
                    );
                }
            }
        }
        Some(row.into_any_element())
    }
}

fn note(text: &str) -> gpui::Div {
    div()
        .text_size(px(ui_size(12.)))
        .text_color(rgb(MUTED))
        .child(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn display_images_and_media_are_collected() {
        let d = json!({ "kind": "node_repl_images",
            "images": [ { "base64": "AAAA", "mimeType": "image/png" } ],
            "media": [ { "mimeType": "image/jpeg", "data": "BBBB" },
                       { "mimeType": "audio/wav", "data": "CCCC" } ] });
        let refs = image_refs("NodeRepl", "success", None, Some(&d));
        assert_eq!(refs.len(), 2);
        assert_eq!(
            refs[1],
            ImageRef::Inline {
                mime: "image/jpeg".into(),
                data: "BBBB".into()
            }
        );
    }

    #[test]
    fn read_of_absolute_image_path_only_when_successful() {
        let path = if cfg!(windows) {
            "C:/tmp/shot.png"
        } else {
            "/tmp/shot.png"
        };
        let input = json!({ "file_path": path });
        assert_eq!(image_refs("Read", "success", Some(&input), None).len(), 1);
        assert!(image_refs("Read", "running", Some(&input), None).is_empty());
        let text = json!({ "file_path": "/tmp/notes.md" });
        assert!(image_refs("Read", "success", Some(&text), None).is_empty());
        let rel = json!({ "file_path": "shot.png" });
        assert!(image_refs("Read", "success", Some(&rel), None).is_empty());
    }

    #[test]
    fn invalid_inline_data_reports_an_error() {
        let r = ImageRef::Inline {
            mime: "image/png".into(),
            data: "!!".into(),
        };
        assert!(load(r).is_err());
        let r = ImageRef::Inline {
            mime: "image/tiff".into(),
            data: "AAAA".into(),
        };
        assert!(load(r).unwrap_err().contains("Unsupported"));
    }
}
