//! App state: one agent process per workspace (like the desktop's host), V4
//! subscriptions per workspace, and mirrored conversation state. UI-side only
//! — the backend owns sessions, admission and stream semantics; the frontend
//! mirrors snapshots/deltas and issues commands.
//!
//! Connection event handling lives in backend/events.rs, outbound actions in
//! backend/commands.rs / composer/config_cmds.rs.

use crate::backend::workspace::{WorkspaceHandle, WorkspacePurpose};
use crate::conversation::model::ConversationState;
use gpui::{AppContext, Context, Entity};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

/// The route cursor of the last frame applied on a topic: subscription
/// generation, host log epoch and the applied seq. Continuity is judged
/// against this exact triple — never against a topic-only seq (2026-10-05
/// audit P0.1; canonical rule: controller.ts `isWindowHostControllerFrameGap`).
#[derive(Clone, Debug)]
pub(crate) struct RouteCursor {
    pub(crate) subscription_id: String,
    pub(crate) log_epoch: String,
    pub(crate) seq: u64,
}

pub struct AppState {
    pub workspaces: Vec<WorkspaceHandle>,
    /// Workspace key of the context used for "New chat" (last selected).
    pub active_workspace: Option<String>,
    pub(crate) navigation_generation: u64,
    pub(crate) held_confirmation:
        Option<std::sync::Arc<crate::composer::held_queue::HeldConfirmation>>,
    pub(crate) workflow_navigation_generation: u64,
    pub(crate) workflow_focus: Option<crate::backend::workflow_navigation::WorkflowFocus>,
    pub(crate) workflow_follow: Option<crate::backend::workflow_navigation::WorkflowFollow>,
    pub conversations: HashMap<String, ConversationState>,
    pub active: Option<String>,
    pub draft: bool,
    pub workspace_configs: HashMap<String, crate::composer::catalog::WorkspaceConfig>,
    /// Draft-mode selections (option value / mode id), applied via
    /// createSession.config since CAS config commands need a live session.
    pub ui_model_value: Option<String>,
    pub ui_mode: Option<String>,
    pub composer: Entity<crate::composer::input::Composer>,
    /// Per-session draft text saved when navigating between conversations.
    pub session_drafts: HashMap<String, String>,
    pub(crate) draft_submission_overrides: HashMap<String, Value>,
    pub(crate) recovered_attachments:
        HashMap<String, Vec<crate::composer::attachment::AttachmentRef>>,
    /// What Enter in the composer does (send / edit message / rename).
    pub composer_intent: crate::conversation::msg_actions::ComposerIntent,
    /// Stable per-install client identity for command envelopes (persisted
    /// once under the data dir; shared::identity owns the file lifecycle).
    pub(crate) client_id: String,
    pub(crate) log: VecDeque<String>,
    /// Dismissable error banners (most recent last, newest shown first).
    pub(crate) errors: VecDeque<String>,
    /// Per-workspace flow latch (true = we told the CLI to pause).
    pub(crate) flow_saturated: HashMap<String, bool>,
    pub(crate) assembler: crate::backend::wire::FrameAssembler,
    /// Last applied route cursor per topic (`(fromSeq, toSeq]` continuity +
    /// subscription generation). Cleared together with fragment state on
    /// reconnect/unsubscribe.
    pub(crate) route_cursors: HashMap<String, RouteCursor>,
    /// Child subagent session -> (workspace_key, parent_session_id)
    pub child_owner: HashMap<String, (String, String)>,
    /// Active child subagent session being viewed (read-only)
    pub viewing_child: Option<String>,
    /// Backend launch candidates for workspaces added at runtime (the
    /// folder picker; startup workspaces get their own copies).
    pub(crate) launch_candidates: Vec<crate::backend::launcher::BackendLaunch>,
    /// Temp files from submitted pasted images: kept until quit (the backend
    /// may still read the reference), deleted on app exit.
    pub(crate) retired_temp_files: Vec<PathBuf>,
}

impl AppState {
    pub fn new(
        workspace_paths: Vec<PathBuf>,
        candidates: Vec<crate::backend::launcher::BackendLaunch>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut state = Self {
            workspaces: workspace_paths
                .into_iter()
                .map(|p| WorkspaceHandle::new(p, candidates.clone()))
                .collect(),
            active_workspace: None,
            navigation_generation: 0,
            held_confirmation: None,
            conversations: HashMap::new(),
            workflow_navigation_generation: 0,
            workflow_focus: None,
            workflow_follow: None,
            active: None,
            child_owner: HashMap::new(),
            viewing_child: None,
            launch_candidates: candidates.clone(),
            retired_temp_files: Vec::new(),
            draft: false,
            workspace_configs: HashMap::new(),
            ui_model_value: None,
            ui_mode: None,
            composer: cx.new(crate::composer::input::Composer::new),
            session_drafts: HashMap::new(),
            draft_submission_overrides: HashMap::new(),
            recovered_attachments: HashMap::new(),
            composer_intent: Default::default(),
            client_id: crate::shared::identity::install_client_id(),
            log: VecDeque::new(),
            errors: VecDeque::new(),
            flow_saturated: HashMap::new(),
            assembler: crate::backend::wire::FrameAssembler::new(),
            route_cursors: HashMap::new(),
        };
        state.register_conversation_workspace(candidates);
        let primary = state
            .workspaces
            .iter()
            .find(|w| w.purpose == crate::backend::workspace::WorkspacePurpose::Project)
            .map(|w| w.key.clone());
        state.active_workspace =
            primary.or_else(|| state.workspaces.first().map(|w| w.key.clone()));
        if let Some(key) = state.active_workspace.clone() {
            state.spawn_workspace(&key, cx);
        }
        state.start_maintenance(cx);
        crate::backend::flow_cmds::start_tail_poll(cx);
        // Startup sweep: paste temp files left behind by a crashed session.
        crate::shared::temp_attachments::scavenge_stale();
        // gpui does not guarantee entity drops at process exit, so `Drop`
        // alone can leave agents running where no kill-on-close job object
        // exists (non-Windows, or a failed job assignment).
        cx.on_app_quit(|this, _cx| {
            this.shutdown_all_workspaces();
            // Submitted pasted images may still be referenced by the backend
            // until now; quit is the point where they can finally go.
            for path in this.retired_temp_files.drain(..) {
                crate::shared::temp_attachments::delete_owned(&path);
            }
            async {}
        })
        .detach();
        state
    }

    pub(crate) fn push_error(&mut self, line: String) {
        // Error banners echo backend/provider text, which can quote keys.
        self.errors.push_back(crate::shared::redact::scrub(&line));
        while self.errors.len() > 3 {
            self.errors.pop_front();
        }
    }

    pub fn dismiss_error(&mut self, index: usize) {
        // UI renders newest-first; map back to deque order.
        let len = self.errors.len();
        if index < len {
            self.errors.remove(len - 1 - index);
        }
    }

    /// Terminate all workspace agents cleanly.
    pub fn shutdown_all_workspaces(&mut self) {
        for w in &mut self.workspaces {
            w.shutdown();
        }
    }

    /// Open a picked folder as a project workspace (Electron parity:
    /// WorkspaceSidebar "+" → Open folder). Adds (or re-activates) the
    /// workspace, spawns its agent, and best-effort persists it to
    /// `setting.json` `recentProjects` — refused while the desktop runs (it
    /// rewrites the file whole), in which case the project still exists for
    /// this session.
    pub(crate) fn add_workspace(&mut self, raw: &Path, cx: &mut Context<Self>) {
        let canonical = crate::backend::workspace::canonical_workspace_string(raw);
        if !Path::new(&canonical).is_dir() {
            self.push_error(format!("not a directory: {canonical}"));
            return;
        }
        let key = match self.workspaces.iter().find(|w| w.key == canonical) {
            Some(w) => w.key.clone(),
            None => {
                let mut h =
                    WorkspaceHandle::new(PathBuf::from(&canonical), self.launch_candidates.clone());
                h.purpose = WorkspacePurpose::Project;
                let key = h.key.clone();
                self.workspaces.push(h);
                key
            }
        };
        self.active_workspace = Some(key.clone());
        self.spawn_workspace(&key, cx);
        crate::shared::preferences::Preferences::enqueue(
            crate::shared::preferences::PreferenceChange::RecentProject(canonical.clone()),
            cx,
        );
        self.push_log(format!("added project {canonical}"));
        cx.notify();
    }

    /// Spawn the agent for a workspace if it isn't running (lazy start).
    pub(crate) fn spawn_workspace(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(ws) = self.ws_mut(key) else {
            return;
        };
        if ws.pumping {
            return;
        }
        ws.pumping = true;
        ws.last_activity = Some(std::time::Instant::now());
        let Some(events) = ws.try_spawn() else {
            ws.pumping = false;
            ws.status = "no backend".into();
            let display = ws.display.clone();
            self.push_log(format!("no usable backend for workspace {key}"));
            self.push_error(format!(
                "no usable backend for {display} — check the installed ZCode app"
            ));
            return;
        };
        if let (Some(backlog), Some(log_backlog)) = (ws.backlog.clone(), ws.log_backlog.clone()) {
            crate::backend::events::attach_pump(
                cx,
                key.to_string(),
                ws.generation,
                backlog,
                log_backlog,
                events,
            );
        }
        cx.notify();
    }

    pub(crate) fn ws(&self, key: &str) -> Option<&WorkspaceHandle> {
        self.workspaces.iter().find(|w| w.key == key)
    }

    pub(crate) fn ws_mut(&mut self, key: &str) -> Option<&mut WorkspaceHandle> {
        self.workspaces.iter_mut().find(|w| w.key == key)
    }

    /// Filesystem path of the active workspace (git + terminal + file tree).
    pub(crate) fn active_workspace_path(&self) -> Option<std::path::PathBuf> {
        let key = self.active_ws_key()?;
        self.ws(&key).map(|w| w.path.clone())
    }

    /// Workspace of the active session if known, else the last active one.
    pub(crate) fn active_ws_key(&self) -> Option<String> {
        self.active
            .as_ref()
            .and_then(|sid| {
                self.workspaces
                    .iter()
                    .find(|w| w.sessions.iter().any(|s| &s.session_id == sid))
                    .map(|w| w.key.clone())
            })
            .or_else(|| self.active_workspace.clone())
    }

    pub(crate) fn active_sessions(&self) -> Vec<(String, String)> {
        let key = self.active_ws_key();
        self.workspaces
            .iter()
            .find(|w| key.as_ref() == Some(&w.key))
            .map(|w| {
                w.sessions
                    .iter()
                    .map(|s| {
                        let t = s.title.trim();
                        let title = if t.is_empty() { &s.session_id } else { t };
                        (s.session_id.clone(), title.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn active_conversation(&self) -> Option<&ConversationState> {
        let sid = crate::conversation::subagent_nav::active_conversation_sid(
            self.viewing_child.as_deref(),
            self.active.as_deref(),
        )?;
        self.conversations.get(sid)
    }

    pub(crate) fn push_log(&mut self, line: String) {
        let line = crate::shared::redact::scrub(&line);
        if std::env::var("ZCODE_GPUI_LOG_STDOUT").is_ok() {
            eprintln!("[zcode-gpui] {line}");
        }
        self.log.push_back(line);
        while self.log.len() > 200 {
            self.log.pop_front();
        }
    }

    pub(crate) fn register_conversation_workspace(
        &mut self,
        candidates: Vec<crate::backend::launcher::BackendLaunch>,
    ) {
        use crate::backend::workspace::{
            canonical_workspace_string, ensure_conversation_workspace_dir,
        };
        let p = ensure_conversation_workspace_dir();
        let key = canonical_workspace_string(&p);
        let mut h = WorkspaceHandle::new(p, candidates);
        h.purpose = WorkspacePurpose::Conversation;
        h.display = "Tasks".into();
        match self.workspaces.iter_mut().find(|w| w.key == key) {
            Some(ws) => *ws = h,
            None => self.workspaces.insert(0, h),
        }
    }

    pub fn conversation_workspace_key(&self) -> Option<String> {
        self.workspaces
            .iter()
            .find(|w| w.purpose == WorkspacePurpose::Conversation)
            .map(|w| w.key.clone())
    }

    pub fn is_conversation_workspace(&self, ws_key: &str) -> bool {
        self.ws(ws_key)
            .is_some_and(|w| w.purpose == WorkspacePurpose::Conversation)
    }

    pub(crate) fn status_error(&mut self, ws_key: &str, msg: &str) {
        let msg = crate::backend::status::set_workspace_error(&mut self.workspaces, ws_key, msg);
        self.push_log(format!("status: {msg}"));
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        self.shutdown_all_workspaces();
        for path in &self.retired_temp_files {
            crate::shared::temp_attachments::delete_owned(path);
        }
    }
}
