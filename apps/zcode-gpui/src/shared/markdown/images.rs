//! Local image files embedded in Markdown (`![alt](path)`), shown as
//! thumbnails under assistant replies. Only absolute paths and `file://` URLs
//! are taken; remote URLs are never fetched on the model's behalf.
//!
//! Spec: PARITY.md "Images".

use pulldown_cmark::{Event, Options, Parser, Tag};
use std::path::PathBuf;

/// Distinct local image files referenced by Markdown image syntax, in order.
pub(crate) fn image_targets(text: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for event in Parser::new_ext(text, Options::all()) {
        if let Event::Start(Tag::Image { dest_url, .. }) = event
            && let Some(path) = local_path(&dest_url)
            && path.is_absolute()
            && crate::files::image_preview::format(&path).is_some()
            && !out.contains(&path)
        {
            out.push(path);
        }
    }
    out
}

fn local_path(dest: &str) -> Option<PathBuf> {
    let dest = dest.trim();
    // `C:/x.png` would parse as a URL with scheme `c`, so only `file:` goes
    // through the URL parser.
    if dest.len() > 5 && dest[..5].eq_ignore_ascii_case("file:") {
        return url::Url::parse(dest).ok()?.to_file_path().ok();
    }
    if dest.contains("://") {
        return None;
    }
    Some(PathBuf::from(dest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn windows_paths_and_file_urls() {
        let md = "a ![shot](C:/tmp/shot.png) b ![x](file:///C:/My%20Pics/x.JPG) \
                  ![again](C:/tmp/shot.png)";
        assert_eq!(
            image_targets(md),
            vec![
                PathBuf::from("C:/tmp/shot.png"),
                PathBuf::from(r"C:\My Pics\x.JPG")
            ]
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_paths_and_file_urls() {
        let md = "![a](/tmp/a.png) ![b](file:///tmp/My%20Pics/b.webp)";
        assert_eq!(
            image_targets(md),
            vec![
                PathBuf::from("/tmp/a.png"),
                PathBuf::from("/tmp/My Pics/b.webp")
            ]
        );
    }

    #[test]
    fn skips_relative_remote_non_image_links_and_code() {
        let abs = if cfg!(windows) { "C:/tmp" } else { "/tmp" };
        let md = format!(
            "![r](shot.png) ![w](https://example.com/a.png) ![d]({abs}/notes.md) \
             [link]({abs}/link.png)\n\n```\n![c]({abs}/code.png)\n```\n\n`![i]({abs}/i.png)`"
        );
        assert!(image_targets(&md).is_empty());
    }
}
