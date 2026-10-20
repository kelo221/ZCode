//! Language-server diagnostics on file-edit cards (PARITY.md "Diagnostics
//! after edits"): the agent attaches a summary to the `file_diff` display of
//! each Edit/Write; the card shows it as a chip and the quiet-mode fold line
//! adds up the latest error count per file.

use crate::shared::i18n::label;
use crate::shared::theme::{DANGER, MUTED, WARNING, ui_size};
use crate::shared::theme_colors::color as rgb;
use gpui::{AnyElement, ElementId, IntoElement, ParentElement, Styled, div, prelude::*, px};
use serde_json::Value;
use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EditDiagnostics {
    pub errors: u32,
    pub warnings: u32,
    /// The server answered (`availability: ok`).
    pub available: bool,
    /// Answered for the current contents before the deadline; otherwise the
    /// chip carries a "?".
    pub confirmed: bool,
}

pub(crate) fn edit_diagnostics(display: Option<&Value>) -> Option<EditDiagnostics> {
    let display = display?;
    if display.get("kind")?.as_str()? != "file_diff" {
        return None;
    }
    let summary = display.get("diagnostics")?;
    let count = |key: &str| {
        summary
            .get(key)
            .and_then(Value::as_u64)
            .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX))
    };
    let available = summary.get("availability")?.as_str()? == "ok";
    let confirmed = available
        && summary.get("freshness").and_then(Value::as_str) == Some("confirmed")
        && summary.get("timedOut").and_then(Value::as_bool) != Some(true);
    Some(EditDiagnostics {
        errors: count("errors"),
        warnings: count("warnings"),
        available,
        confirmed,
    })
}

/// Identity of the edited file, so a later edit of the same file replaces its
/// count in the fold summary. Windows paths compare case-insensitively.
pub(crate) fn file_key(display: Option<&Value>) -> Option<u64> {
    let path = display?.get("filePath")?.as_str()?;
    let mut hasher = DefaultHasher::new();
    path.replace('\\', "/").to_lowercase().hash(&mut hasher);
    Some(hasher.finish())
}

pub(crate) fn chip_text(diagnostics: EditDiagnostics) -> String {
    if !diagnostics.available {
        return label("Diagnostics pending", "诊断未就绪").to_owned();
    }
    let text = match (diagnostics.errors, diagnostics.warnings) {
        (0, 0) => label("No problems", "无问题").to_owned(),
        (1, _) => label("1 error", "1 个错误").to_owned(),
        (n, 0) if n > 0 => format!("{n} {}", label("errors", "个错误")),
        (0, 1) => label("1 warning", "1 个警告").to_owned(),
        (0, n) => format!("{n} {}", label("warnings", "个警告")),
        (n, _) => format!("{n} {}", label("errors", "个错误")),
    };
    if diagnostics.confirmed {
        text
    } else {
        format!("{text}?")
    }
}

pub(crate) fn diagnostics_chip(display: Option<&Value>, row_id: u64) -> Option<AnyElement> {
    let diagnostics = edit_diagnostics(display)?;
    let color = if diagnostics.errors > 0 {
        DANGER
    } else if diagnostics.warnings > 0 {
        WARNING
    } else {
        MUTED
    };
    let chip = div()
        .id(ElementId::NamedInteger("edit-diagnostics".into(), row_id))
        .flex_none()
        .text_size(px(ui_size(12.)))
        .text_color(rgb(color))
        .child(chip_text(diagnostics));
    #[cfg(test)]
    let chip = crate::app::test_support::track_children(
        div().child(chip),
        vec![format!("edit-diagnostics-{row_id}")],
    );
    Some(chip.into_any_element())
}

#[cfg(test)]
#[path = "edit_diagnostics_tests.rs"]
mod tests;
