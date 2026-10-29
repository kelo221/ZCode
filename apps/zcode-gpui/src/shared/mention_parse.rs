//! Mention markdown in user text, parsed the way Desktop does
//! (`packages/ui/src/mentions/mentionMarkdown.ts` `parseMentionMarkdown`), so a
//! composer chip and a sent bubble show `⚡ Architecture Governance` instead of
//! `[$architecture-governance](C:\\…\\SKILL.md)`. The raw text is never
//! rewritten; callers only change how it is displayed.

use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MentionKind {
    File,
    Directory,
    Skill,
    Session,
    Plugin,
    Subagent,
    Command,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Mention {
    pub(crate) kind: MentionKind,
    pub(crate) label: String,
    /// Byte range of the whole mention in the raw text.
    pub(crate) raw: Range<usize>,
}

impl Mention {
    /// What the pill shows: an icon and the label.
    pub(crate) fn display(&self) -> String {
        match self.kind {
            MentionKind::File => format!("📄 {}", self.label),
            MentionKind::Directory => format!("📁 {}", self.label),
            MentionKind::Skill => format!("⚡ {}", skill_display_label(&self.label)),
            MentionKind::Session => format!("# {}", self.label),
            MentionKind::Plugin => format!("🔌 {}", self.label),
            MentionKind::Subagent => format!("@{}", self.label),
            MentionKind::Command => format!("/{}", self.label),
        }
    }
}

/// Every mention in `text`, in order (links first-class, then bare tokens in the gaps).
pub(crate) fn parse_mentions(text: &str) -> Vec<Mention> {
    let mut out = Vec::new();
    let mut cursor = 0;
    let mut i = 0;
    while let Some(off) = text[i..].find('[') {
        let start = i + off;
        if let Some((label, dest, end)) = parse_link(text, start) {
            inline_tokens(text, cursor..start, &mut out);
            out.push(link_mention(label, dest, start..end));
            cursor = end;
            i = end;
        } else {
            i = start + 1;
        }
    }
    inline_tokens(text, cursor..text.len(), &mut out);
    out
}

/// `text` with every mention replaced by its pill text, and the pill ranges in the result.
pub(crate) fn display_with_pills(text: &str) -> (String, Vec<Range<usize>>) {
    let mut out = String::with_capacity(text.len());
    let mut pills = Vec::new();
    let mut at = 0;
    for m in parse_mentions(text) {
        out.push_str(&text[at..m.raw.start]);
        let start = out.len();
        out.push_str(&m.display());
        pills.push(start..out.len());
        at = m.raw.end;
    }
    out.push_str(&text[at..]);
    (out, pills)
}

/// The single mention covering all of `raw` (a composer chip), if it is one.
pub(crate) fn whole_mention(raw: &str) -> Option<Mention> {
    let mut found = parse_mentions(raw);
    (found.len() == 1 && found[0].raw == (0..raw.len())).then(|| found.remove(0))
}

/// `code-review` → `Code Review`; phrases that already contain spaces stay as typed.
pub(crate) fn skill_display_label(label: &str) -> String {
    let t = label.trim();
    if t.is_empty() {
        return label.into();
    }
    if t.contains(' ') && !t.contains(['-', '_']) {
        return t.into();
    }
    t.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            chars.next().map_or_else(String::new, |c| {
                c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn link_mention(label: String, dest: &str, raw: Range<usize>) -> Mention {
    let strip = |l: &str, p: char| l.strip_prefix(p).unwrap_or(l).to_string();
    let (kind, label) = if dest
        .strip_prefix("#sess_")
        .is_some_and(|id| !id.is_empty() && id.chars().all(is_token_char))
    {
        (MentionKind::Session, strip(&label, '#'))
    } else if dest.starts_with("plugin://") {
        (MentionKind::Plugin, strip(&label, '@'))
    } else if let Some(skill) = label.strip_prefix('$') {
        (MentionKind::Skill, skill.to_string())
    } else if dest.ends_with(['/', '\\']) {
        (MentionKind::Directory, strip(&label, '@'))
    } else {
        (MentionKind::File, strip(&label, '@'))
    };
    Mention { kind, label, raw }
}

/// `[label](dest)` or `[label](<dest>)` at `start` (a `[`), with `\` escapes:
/// unescaped label, raw destination, end offset.
fn parse_link(text: &str, start: usize) -> Option<(String, &str, usize)> {
    let b = text.as_bytes();
    let mut i = start + 1;
    let mut label = String::new();
    loop {
        match *b.get(i)? {
            b'\\' if i + 1 < b.len() => {
                let c = text[i + 1..].chars().next()?;
                label.push(c);
                i += 1 + c.len_utf8();
            }
            b'\\' => return None,
            b']' => break,
            _ => {
                let c = text[i..].chars().next()?;
                label.push(c);
                i += c.len_utf8();
            }
        }
    }
    i += 1;
    if b.get(i) != Some(&b'(') {
        return None;
    }
    i += 1;
    let angle = b.get(i) == Some(&b'<');
    if angle {
        i += 1;
    }
    let close = if angle { b'>' } else { b')' };
    let dest_start = i;
    loop {
        match *b.get(i)? {
            b'\\' if i + 1 < b.len() => i += 2,
            c if c == close => break,
            _ => i += 1,
        }
    }
    // Escapes skip two bytes; never land inside a multi-byte character.
    if !text.is_char_boundary(i) {
        return None;
    }
    let dest = &text[dest_start..i];
    i += 1;
    if angle {
        if b.get(i) != Some(&b')') {
            return None;
        }
        i += 1;
    }
    Some((label, dest, i))
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

/// Bare `$skill`, `/command`, `@name` and `#sess_…` tokens between whitespace.
fn inline_tokens(text: &str, seg: Range<usize>, out: &mut Vec<Mention>) {
    let s = &text[seg.clone()];
    let mut prev_ws = true;
    let mut iter = s.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        if prev_ws && matches!(c, '$' | '/' | '@' | '#') {
            let body_start = i + 1;
            let body_end = s[body_start..]
                .find(|ch: char| !is_token_char(ch))
                .map_or(s.len(), |n| body_start + n);
            let body = &s[body_start..body_end];
            let at_boundary = s[body_end..].chars().next().is_none_or(char::is_whitespace);
            let kind = match c {
                '$' => Some(MentionKind::Skill),
                '/' => Some(MentionKind::Command),
                '@' => Some(MentionKind::Subagent),
                _ if body.strip_prefix("sess_").is_some_and(|r| !r.is_empty()) => {
                    Some(MentionKind::Session)
                }
                _ => None,
            };
            if let Some(kind) = kind.filter(|_| !body.is_empty() && at_boundary) {
                out.push(Mention {
                    kind,
                    label: body.to_string(),
                    raw: seg.start + i..seg.start + body_end,
                });
                while iter.peek().is_some_and(|&(j, _)| j < body_end) {
                    iter.next();
                }
                prev_ws = false;
                continue;
            }
        }
        prev_ws = c.is_whitespace();
    }
}

#[cfg(test)]
#[path = "mention_parse_tests.rs"]
mod tests;
