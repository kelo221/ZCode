//! File references in assistant Markdown: path-like inline code and scheme-less
//! link targets become `zcode-file:` links, resolved against the workspace on click.
//!
//! Spec: PARITY.md "Transcript file references (GPUI-only)".

use std::path::{Component, Path, PathBuf};

/// Link target prefix for file references; never handed to the OS URL handler.
pub(crate) const FILE_LINK_SCHEME: &str = "zcode-file:";

/// Clicked file reference (`raw` is the link target without [`FILE_LINK_SCHEME`]);
/// the root view resolves it against the active workspace.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = zcode_gpui, no_json)]
pub(crate) struct OpenFileReference {
    pub raw: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileRef {
    pub path: String,
    pub line: Option<u32>,
}

/// Split a `:line[:col]`, `:line-line` or `#L12[-L20]` suffix off `raw`.
pub(crate) fn parse_target(raw: &str) -> FileRef {
    let raw = raw.trim();
    if let Some((path, frag)) = raw.rsplit_once("#L") {
        let digits = frag.split('-').next().unwrap_or("");
        if let Ok(line) = digits.parse::<u32>()
            && !path.is_empty()
        {
            return FileRef {
                path: path.to_string(),
                line: Some(line),
            };
        }
    }
    // `a.rs:12:3` / `a.rs:12-20` / `a.rs:12`; a drive colon (`C:\x`) is never all digits.
    let mut path = raw;
    let mut line = None;
    for _ in 0..2 {
        let Some((head, tail)) = path.rsplit_once(':') else {
            break;
        };
        let first = tail.split('-').next().unwrap_or("");
        if head.is_empty() || first.is_empty() || !first.bytes().all(|b| b.is_ascii_digit()) {
            break;
        }
        line = first.parse::<u32>().ok();
        path = head;
    }
    FileRef {
        path: path.to_string(),
        line,
    }
}

fn has_file_extension(segment: &str) -> bool {
    let Some((stem, ext)) = segment.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && (1..=10).contains(&ext.len())
        && ext.bytes().all(|b| b.is_ascii_alphanumeric())
        && ext.bytes().any(|b| b.is_ascii_alphabetic())
}

fn has_explicit_root(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with("./")
        || path.starts_with(".\\")
        || path.starts_with("../")
        || path.starts_with("~/")
        || path.starts_with('/')
        || (bytes.len() > 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}

/// Inline code that reads as a single file path (`PLAN.md`, `src/a.rs:42`,
/// `C:\x\y.ts`). Commands, flags, globs, URLs, symbols and versions are not.
pub(crate) fn code_file_ref(code: &str) -> Option<FileRef> {
    let code = code.trim();
    if code.is_empty()
        || code.len() > 400
        || code.starts_with('-')
        || code.contains("://")
        || code.contains("::")
        || code
            .chars()
            .any(|c| c.is_whitespace() || "*?<>|\"'`$(){}[],;=!@%^&+".contains(c))
    {
        return None;
    }
    let target = parse_target(code);
    let path = target.path.trim_end_matches(['/', '\\']);
    if path.len() != target.path.len() {
        // A folder (`src/`) is not something the preview can open.
        return None;
    }
    let last = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let looks_like_file = has_file_extension(last)
        || (has_explicit_root(path) && path.contains(['/', '\\']) && !last.is_empty());
    looks_like_file.then_some(target)
}

/// Markdown link target to rewrite as a file reference: no URL scheme, or `file://`.
pub(crate) fn link_file_target(dest: &str) -> Option<String> {
    let dest = dest.trim();
    if dest.is_empty() || dest.starts_with('#') {
        return None;
    }
    if let Some(rest) = dest.strip_prefix("file://") {
        // file:///C:/x → C:/x; file:///home/x → /home/x
        let rest = rest
            .strip_prefix('/')
            .filter(|r| is_drive(r))
            .unwrap_or(rest);
        return Some(rest.to_string());
    }
    let scheme_end = dest.find(':');
    let has_scheme = scheme_end.is_some_and(|i| {
        i > 1
            && dest[..i]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'.' | b'-'))
            && dest.as_bytes()[0].is_ascii_alphabetic()
            // `a.rs:12` is a path with a line, not scheme `a.rs`.
            && !dest[i + 1..].bytes().next().is_some_and(|b| b.is_ascii_digit())
    });
    (!has_scheme).then(|| dest.to_string())
}

fn is_drive(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Absolute candidate path for `path`. Relative paths stay inside `root`
/// (`..` may not leave it); `None` when nothing sensible can be built.
pub(crate) fn resolve_path(
    root: Option<&Path>,
    home: Option<&Path>,
    path: &str,
) -> Option<PathBuf> {
    let path = path.trim();
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        return home.map(|h| h.join(rest));
    }
    // `/C:/x` (a URL-style Windows path) → `C:/x`.
    let path = path
        .strip_prefix('/')
        .filter(|r| is_drive(r))
        .unwrap_or(path);
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return Some(candidate.to_path_buf());
    }
    let root = root?;
    let rel = path.trim_start_matches(['/', '\\']);
    if is_drive(rel) {
        // `C:x` (drive-relative) has no meaning here.
        return None;
    }
    let mut out = root.to_path_buf();
    let mut depth = 0usize;
    for component in Path::new(rel).components() {
        match component {
            Component::Normal(part) => {
                out.push(part);
                depth += 1;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if depth == 0 {
                    return None;
                }
                out.pop();
                depth -= 1;
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (depth > 0).then_some(out)
}

/// A bare file name (no directory part) that may live anywhere in the workspace.
pub(crate) fn is_bare_name(path: &str) -> bool {
    !path.contains(['/', '\\']) && !path.starts_with('~')
}

#[cfg(test)]
#[path = "file_ref_tests.rs"]
mod tests;
