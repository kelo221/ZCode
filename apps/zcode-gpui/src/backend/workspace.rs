//! One agent process per workspace key — the same model the desktop uses
//! (`packages/services/src/zcode-agent/zcodeAgentProcessManager.ts` keys
//! processes by `workspaceIdentity?.trim() || workspacePath`).

use crate::backend::launcher::{BackendLaunch, ConnEvent, spawn_connection};
use crate::conversation::model::SessionEntry;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

pub(crate) enum Pending {
    SubscribeIndex,
    SubscribeConfig,
    /// Legacy `session/create` used purely to harvest the model catalog —
    /// the V4 workspace-config topic carries no model options on standalone
    /// CLI (they are host-provided). The session it creates is deleted right
    /// after (CleanupCatalog).
    ModelCatalog,
    CleanupCatalog,
    SubscribeConversation(String),
    CreateSession,
    SendText,
    Stop,
    /// Session-scoped command whose ack is checked (see backend/session_cmds.rs).
    Command(CommandCtx),
    /// `v4/conversation/resync` recovery (ack ignored; frames re-deliver).
    Resync,
    /// `v4/conversation/rowsRange` history page for a session.
    FetchRows(String),
}

/// Everything needed to resend a session command on `stale` or to roll back
/// its optimistic UI update when it is rejected.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CommandCtx {
    pub sid: String,
    pub ctype: String,
    pub payload: Value,
    /// Envelope `baseRevision` is required (COMMANDS_REQUIRING_BASE_REVISION).
    pub cas: bool,
    /// Envelope `baseLogEpoch` for ROW_TARGETING_COMMANDS.
    pub log_epoch: Option<String>,
    pub retried: bool,
}

impl CommandCtx {
    pub fn new(sid: &str, ctype: &str, payload: Value) -> Self {
        Self {
            sid: sid.to_string(),
            ctype: ctype.to_string(),
            payload,
            cas: false,
            log_epoch: None,
            retried: false,
        }
    }

    pub fn cas(mut self) -> Self {
        self.cas = true;
        self
    }

    pub fn row(mut self, log_epoch: String) -> Self {
        self.cas = true;
        self.log_epoch = Some(log_epoch);
        self
    }
}

pub struct WorkspaceHandle {
    pub key: String,
    pub path: PathBuf,
    pub display: String,
    pub conn_desc: String,
    pub status: String,
    pub sessions: Vec<SessionEntry>,
    pub(crate) inbound: Option<Sender<String>>,
    candidates: Vec<BackendLaunch>,
    candidate_idx: usize,
    /// Index of the candidate that last worked, so respawn skips dead ones.
    working_candidate: usize,
    pub(crate) started: bool,
    pub(crate) next_id: u64,
    pub(crate) pending: HashMap<u64, Pending>,
    /// True while a pump task is attached to this workspace's connection.
    pub(crate) pumping: bool,
    /// Force-kill handle for the current agent (job object on Windows).
    pub(crate) kill: Option<Box<dyn FnOnce() + Send>>,
    /// Last user interaction with this workspace (for idle unload).
    pub(crate) last_activity: Option<std::time::Instant>,
    /// Session the user opened while the agent was still starting; subscribed
    /// once storage is ready.
    pub(crate) desired_conversation: Option<String>,
    /// Stable connection id for this workspace's agent link (resync/flow
    /// reference it; re-subscribe replaces by (connectionId, topic)).
    pub(crate) connection_id: String,
    /// topic → subscriptionId, captured from subscribe acks (resync needs it).
    pub(crate) subscriptions: HashMap<String, String>,
    /// Consecutive crashes of the agent (reset when storage reaches ready).
    pub(crate) restart_attempts: u32,
}

impl WorkspaceHandle {
    pub fn new(path: PathBuf, candidates: Vec<BackendLaunch>) -> Self {
        let key = canonical_workspace_string(&path);
        let display = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| key.clone());
        Self {
            key,
            path,
            display,
            conn_desc: String::new(),
            status: "idle — click to connect".into(),
            sessions: Vec::new(),
            inbound: None,
            candidates,
            candidate_idx: 0,
            working_candidate: 0,
            started: false,
            next_id: 1,
            pending: HashMap::new(),
            pumping: false,
            kill: None,
            last_activity: None,
            desired_conversation: None,
            connection_id: uuid::Uuid::now_v7().to_string(),
            subscriptions: HashMap::new(),
            restart_attempts: 0,
        }
    }

    /// Spawn (or fall back to) the next backend candidate. Returns the event
    /// receiver for the store's pump task when a process came up.
    pub fn try_spawn(&mut self) -> Option<futures::channel::mpsc::UnboundedReceiver<ConnEvent>> {
        let launch = self.candidates.get(self.candidate_idx)?;
        let used = self.candidate_idx;
        self.candidate_idx += 1;
        match spawn_connection(launch, &self.path) {
            Ok(conn) => {
                self.conn_desc = launch.describe.clone();
                self.status = format!("starting ({})", launch.describe);
                self.working_candidate = used;
                self.inbound = Some(conn.inbound);
                self.kill = Some(conn.kill);
                Some(conn.events)
            }
            Err(e) => {
                self.status = "spawn failed".into();
                eprintln!(
                    "[zcode-gpui] spawn failed for {}: {} ({e})",
                    self.display, launch.describe
                );
                self.try_spawn()
            }
        }
    }

    /// Tear down this workspace's agent (lazy memory management): kill the
    /// child tree, reset subscription state, keep the cached session list.
    /// Respawn picks the last-working candidate first.
    /// Point candidate resolution at the last candidate that worked, so a
    /// respawn skips known-dead ones.
    pub fn prepare_respawn(&mut self) {
        self.candidate_idx = self.working_candidate;
    }

    pub fn unload(&mut self) {
        if let Some(kill) = self.kill.take() {
            kill();
        }
        self.inbound = None;
        self.started = false;
        self.pumping = false;
        self.pending.clear();
        self.candidate_idx = self.working_candidate;
        self.status = "idle — click to connect".into();
    }

    pub(crate) fn send_line(&mut self, line: String) {
        if let Some(inbound) = self.inbound.as_ref()
            && inbound.send(line).is_err()
        {
            self.status = "write failed".into();
        }
    }

    pub(crate) fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn sort_sessions(&mut self) {
        self.sessions
            .sort_by_key(|s| std::cmp::Reverse(s.last_activity_at));
    }
}

/// Absolute path without the `\\?\` verbatim prefix Windows canonicalize adds.
/// The backend keys workspaces by the plain path string, so the prefix must go.
pub fn canonical_workspace_string(path: &Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let s = canonical.to_string_lossy().into_owned();
    s.strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .or_else(|| s.strip_prefix(r"\\?\").map(str::to_string))
        .unwrap_or(s)
}

/// The user's known projects: the desktop persists them in
/// `~/.zcode/v2/setting.json` (`recentProjects` + `lastWorkspaceSession`,
/// see packages/ui/src/hooks/useTabPersistence.ts). There is no RPC for this,
/// so we read the same file. Remote workspaces (ssh/wsl/docker) are skipped —
/// they need connection infrastructure the minimal client doesn't have.
pub fn discover_workspaces(primary: &Path, max: usize) -> Vec<PathBuf> {
    let mut ordered: Vec<PathBuf> = vec![primary.to_path_buf()];
    let Ok(raw) = std::fs::read_to_string(settings_path()) else {
        return ordered;
    };
    let Ok(v) = serde_json::from_str::<Value>(&raw) else {
        return ordered;
    };
    let mut push = |p: &str| {
        let path = PathBuf::from(p);
        if path.is_dir()
            && !ordered
                .iter()
                .any(|w| canonical_workspace_string(w) == canonical_workspace_string(&path))
        {
            ordered.push(path);
        }
    };
    if let Some(entries) = v.get("lastWorkspaceSession").and_then(Value::as_array) {
        for e in entries {
            if e.get("kind").and_then(Value::as_str) != Some("local") {
                continue;
            }
            if let Some(p) = e.get("workspacePath").and_then(Value::as_str) {
                push(p);
            }
        }
    }
    if let Some(recents) = v.get("recentProjects").and_then(Value::as_array) {
        for r in recents {
            if let Some(p) = r.as_str() {
                push(p);
            }
        }
    }
    ordered.truncate(max);
    ordered
}

fn settings_path() -> PathBuf {
    let home = std::env::var("ZCODE_DESKTOP_HOME_DIR")
        .or_else(|_| std::env::var("USERPROFILE"))
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    PathBuf::from(home)
        .join(".zcode")
        .join("v2")
        .join("setting.json")
}
