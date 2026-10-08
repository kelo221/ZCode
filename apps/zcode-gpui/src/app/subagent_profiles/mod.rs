//! Pure native Subagents Settings projection contract (2026-10-06).
//!
//! Existing ISubagentsService owns accepted profiles and canonical persistence.
//! Main owns workspace context, async callbacks, loading/errors and draft lifetime.
//! This module only decodes public DTOs, derives rows and builds mutation arguments.
//!
//! ```text
//! service reply -> main's current-context gate -> immutable scoped rows
//! UI input -> main-owned FormDraft -> validated public payload -> service owner
//! ```
//!
//! No IO, Markdown parsing, persistence, runtime admission, widgets or mutation replay.
//! AgentsListResult requires all three lists and capability. Settings drops runtime
//! plugin aliases and uses pluginAgents once; userAgents is decoded, not a second
//! additive list. Identity is scope + workspace identity/path + service id.
//! Supported SubAgentConfig fields survive untouched edits; unknown fields not
//! exposed by the public backend contract have no persistence guarantee here.
//! Missing/empty/wildcard tools mean All; explicitly empty Custom is a form error.
//! Validation mirrors backend name rules, including case-sensitive reserved names.
//! Model candidates/reasoning come only from the canonical facade's nested config;
//! decoding discards provider secrets. Active model changes complete from the new
//! model's last published reasoning value, never carrying old reasoning. Clearing
//! an override omits its entire selection. Existing selections are not auto-healed.
//! Acceptance is synthetic serde/row/form/model unit tests; transport, native UI
//! and canonical scratch persistence acceptance remain separately owned by main.

mod form;
mod models;
mod projection;
mod types;

pub(crate) use form::*;
pub(crate) use models::*;
pub(crate) use projection::*;
pub(crate) use types::*;

#[cfg(test)]
mod form_tests;
#[cfg(test)]
mod models_tests;
#[cfg(test)]
mod projection_tests;
