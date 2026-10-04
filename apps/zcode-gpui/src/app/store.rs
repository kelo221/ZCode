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
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

pub struct AppState {
    pub workspaces: Vec<WorkspaceHandle>,
    /// Workspace key of the context used for "New chat" (last selected).
    pub active_workspace: Option<String>,
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
    /// What Enter in the composer does (send / edit message / rename).
    pub composer_intent: crate::conversation::msg_actions::ComposerIntent,
    /// Stable per-install client identity for command envelopes.
    pub(crate) client_id: String,
    pub(crate) log: VecDeque<String>,
    /// Dismissable error banners (most recent last, newest shown first).
    pub errors: VecDeque<String>,
    /// Event-pump backlog shared with the pump tasks (backpressure).
    pub(crate) inflight_events: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// Per-workspace flow latch (true = we told the CLI to pause).
    flow_saturated: HashMap<String, bool>,
    pub(crate) assembler: crate::backend::wire::FrameAssembler,
    /// Last applied `toSeq` per topic; a `(fromSeq, toSeq]` gap means the
    /// client missed frames (deltas are then unreliable until re-subscribe).
    pub(crate) last_seq: HashMap<String, u64>,
    /// Usage statistics snapshot (`v4/usage/stats`).
    pub usage_stats: Option<crate::shared::usage_stats::AppUsageSnapshot>,
    /// Connected MCP server snapshots (`mcp/list`).
    pub mcp_servers: Vec<crate::shared::mcp::McpServerSnapshot>,
    /// Plugin store overview snapshot (`plugins/overview`).
    pub plugins_overview: Option<crate::shared::plugins::PluginsOverviewResult>,
    /// Child subagent session -> (workspace_key, parent_session_id)
    pub child_owner: HashMap<String, (String, String)>,
    /// Active child subagent session being viewed (read-only)
    pub viewing_child: Option<String>,
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
            conversations: HashMap::new(),
            active: None,
            child_owner: HashMap::new(),
            viewing_child: None,
            draft: false,
            workspace_configs: HashMap::new(),
            ui_model_value: None,
            ui_mode: None,
            composer: cx.new(crate::composer::input::Composer::new),
            session_drafts: HashMap::new(),
            composer_intent: Default::default(),
            client_id: format!("client-{}", uuid::Uuid::now_v7()),
            log: VecDeque::new(),
            errors: VecDeque::new(),
            inflight_events: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            flow_saturated: HashMap::new(),
            assembler: crate::backend::wire::FrameAssembler::new(),
            last_seq: HashMap::new(),
            usage_stats: None,
            mcp_servers: Vec::new(),
            plugins_overview: None,
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
        // gpui does not guarantee entity drops at process exit, so `Drop`
        // alone can leave agents running where no kill-on-close job object
        // exists (non-Windows, or a failed job assignment).
        cx.on_app_quit(|this, _cx| {
            this.shutdown_all_workspaces();
            async {}
        })
        .detach();
        state
    }

    /// Periodic sweep that unloads agents idle for too long.
    fn start_maintenance(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_spawn(async {
                    std::thread::sleep(std::time::Duration::from_secs(30));
                })
                .await;
                let alive = this
                    .update(cx, |state, cx| {
                        state.reap_idle_agents();
                        // Resync routes whose frame assembly got stuck >30s.
                        for topic in state.assembler.sweep_timeouts() {
                            if let Some(ws_key) = state.ws_for_topic(&topic) {
                                state.resync_topic(&ws_key, &topic);
                            }
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            }
        })
        .detach();
    }

    /// Latch the per-workspace flow signal; sends only on state change.
    pub(crate) fn flow_latch(&mut self, ws_key: &str, saturated: bool) {
        let current = self.flow_saturated.get(ws_key).copied().unwrap_or(false);
        if current == saturated {
            return;
        }
        self.flow_saturated.insert(ws_key.to_string(), saturated);
        self.send_flow_flag(ws_key, saturated);
        self.push_log(format!(
            "backpressure: {} streaming for {ws_key}",
            if saturated { "paused" } else { "resumed" }
        ));
    }

    /// Workspace key that owns a topic (for targeted resyncs).
    pub(crate) fn ws_for_topic(&self, topic: &str) -> Option<String> {
        if let Some(k) = topic
            .strip_prefix("sessions-index/")
            .or_else(|| topic.strip_prefix("workspace-config/"))
        {
            return Some(k.to_string());
        }
        if let Some(ws_key) =
            crate::conversation::subagent_nav::child_topic_workspace(&self.child_owner, topic)
        {
            return Some(ws_key);
        }
        topic.strip_prefix("conversation/").and_then(|sid| {
            self.workspaces
                .iter()
                .find(|w| w.sessions.iter().any(|s| s.session_id == sid))
                .map(|w| w.key.clone())
        })
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

    /// Kill agents with no open conversation that the user hasn't touched for
    /// a while. The active workspace and any workspace with a running turn are
    /// never unloaded.
    fn reap_idle_agents(&mut self) {
        let idle_secs: u64 = std::env::var("ZCODE_GPUI_IDLE_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(600);
        let active_ws = self.active_ws_key();
        let keys: Vec<String> = self.workspaces.iter().map(|w| w.key.clone()).collect();
        for key in keys {
            let Some(ws) = self.ws(&key) else { continue };
            let spawned = ws.started || ws.inbound.is_some();
            if !spawned {
                continue;
            }
            if Some(&key) == active_ws.as_ref() {
                continue;
            }
            if !ws.pending.is_empty() {
                continue; // startup/handshake in flight
            }
            // The sessions-index phase covers sessions this client never
            // opened (e.g. automations); a waiting interaction also pins the
            // agent, since unloading it would abort the blocked turn.
            let running = ws.sessions.iter().any(|s| {
                crate::conversation::model::phase_is_active(&s.phase)
                    || self
                        .conversations
                        .get(&s.session_id)
                        .is_some_and(|c| c.phase_running() || !c.pending_interactions.is_empty())
            });
            if running
                || ws
                    .last_activity
                    .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(idle_secs))
            {
                continue;
            }
            let sids: Vec<String> = ws.sessions.iter().map(|s| s.session_id.clone()).collect();
            self.ws_mut(&key).unwrap().unload();
            for sid in sids {
                if let Some(c) = self.conversations.get_mut(&sid) {
                    c.subscribed = false;
                }
            }
            self.push_log(format!(
                "unloaded idle agent for {}",
                self.ws(&key).map(|w| w.display.clone()).unwrap_or(key)
            ));
        }
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
        let pending = self.inflight_events.clone();
        crate::backend::events::attach_pump(cx, key.to_string(), pending, events);
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

    pub(crate) fn active_ws_mut(&mut self) -> Option<&mut WorkspaceHandle> {
        let key = self.active_ws_key()?;
        self.ws_mut(&key)
    }

    pub(crate) fn active_workspace_config(
        &self,
    ) -> Option<&crate::composer::catalog::WorkspaceConfig> {
        self.workspace_configs.get(&self.active_ws_key()?)
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

    pub(crate) fn status_error(&mut self, msg: &str) {
        self.push_log(format!("status: {msg}"));
        if let Some(ws) = self.active_ws_mut() {
            ws.status = format!("error: {msg}");
        }
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        self.shutdown_all_workspaces();
    }
}
