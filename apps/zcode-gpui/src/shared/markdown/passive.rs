use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

pub(super) fn events(text: &str, options: Options) -> Vec<Event<'_>> {
    // 产物预览不是外部导航入口；移除链接/图片 tag，保留标签与格式，不注册 open_url。
    Parser::new_ext(text, options)
        .filter(|event| {
            !matches!(
                event,
                Event::Start(Tag::Link { .. } | Tag::Image { .. })
                    | Event::End(TagEnd::Link | TagEnd::Image)
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::markdown::inline::{Style, render_inlines};
    #[test]
    fn passive_markdown_keeps_labels_formatting_and_html_without_url_actions() {
        let events = events(
            "[**link**](javascript:alert) ![image](https://example.invalid/pixel) <script>inert</script>",
            Options::empty(),
        );
        let mut position = 1;
        let inline = render_inlines(
            &events,
            &mut position,
            Some(&TagEnd::Paragraph),
            &Style::default(),
        );
        assert!(inline.links.is_empty());
        assert!(inline.text.contains("link"));
        assert!(inline.text.contains("image"));
        assert!(inline.text.contains("<script>"));
        assert!(
            inline
                .runs
                .iter()
                .any(|run| run.font.weight == gpui::FontWeight::BOLD)
        );
        assert!(!inline.text.contains("example.invalid"));
    }
}
