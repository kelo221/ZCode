//! One agent process per workspace key — the same model the desktop uses
//! (`packages/services/src/zcode-agent/zcodeAgentProcessManager.ts` keys
//! processes by `workspaceIdentity?.trim() || workspacePath`).

use crate::backend::launcher::{BackendLaunch, ConnEvent, spawn_connection};
use crate::conversation::model::SessionEntry;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

pub(crate) use crate::backend::pending::Pending;

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
    /// Stale retry only: the ack's `revisionAtDecision`, used as
    /// `baseRevision` instead of the mirror (which stays snapshot-owned).
    pub retry_base: Option<u64>,
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
            retry_base: None,
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

/// Subscription identity captured from a subscribe ack (subscribeAckSchema:
/// `subscriptionId` + `logEpoch`). Both halves are the live route generation
/// `classify_frame` matches against; an epoch change is a replacement.
#[derive(Clone, Debug)]
pub(crate) struct RouteSubscription {
    pub(crate) id: String,
    pub(crate) log_epoch: String,
}

/// Purpose classification for a workspace: projects belong to a directory or
/// repository, while conversations (tasks) use the app-managed default workspace.
/// Desktop parity: packages/shared/src/workspacePurpose.ts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspacePurpose {
    Project,
    Conversation,
}

pub struct WorkspaceHandle {
    pub key: String,
    pub path: PathBuf,
    pub display: String,
    pub purpose: WorkspacePurpose,
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
    /// topic → subscription identity, captured from subscribe acks (resync
    /// needs the id; route validation compares generations by it). This
    /// registry is the authoritative topic-owner source (review finding 2).
    pub(crate) subscriptions: HashMap<String, RouteSubscription>,
    /// Protocol (stdout) backlog accounting for the current connection
    /// (backpressure + hard byte bound). None while not spawned.
    pub(crate) backlog: Option<crate::backend::backlog::EventBacklog>,
    /// Independent lossy stderr backlog for the current connection.
    pub(crate) log_backlog: Option<crate::backend::backlog::EventBacklog>,
    /// Consecutive crashes of the agent (reset when storage reaches ready).
    pub(crate) restart_attempts: u32,
    pub(crate) generation: u64,
    pub(crate) image_uploads: crate::backend::attachment_upload::ImageUploads,
    pub(crate) slash_catalogs: HashMap<
        Option<String>,
        crate::backend::inspection::QueryState<Vec<crate::composer::slash::SlashCommand>>,
    >,
    pub(crate) reference_catalogs:
        HashMap<Option<String>, crate::composer::references::ReferenceCatalog>,
    pub(crate) subagent_directories:
        HashMap<String, crate::conversation::subagent_directory::SubagentDirectory>,
    pub(crate) inspection: crate::backend::inspection::WorkspaceInspection,
    pub(crate) plugin_source_draft: crate::app::plugin_source_input::PluginSourceDraft,
    pub(crate) saved_workflows: HashMap<
        String,
        crate::backend::inspection::QueryState<crate::shared::saved_workflows::SavedWorkflowList>,
    >,
    pub(crate) saved_workflow_form: crate::app::saved_workflow_form::SavedWorkflowForm,
    pub(crate) workflow_definitions: HashMap<
        String,
        crate::backend::inspection::QueryState<
            crate::shared::workflow_definition::WorkflowDefinition,
        >,
    >,
    pub(crate) workflow_histories: HashMap<
        String,
        crate::backend::inspection::QueryState<crate::shared::workflow_history::WorkflowHistory>,
    >,
    pub(crate) workflow_artifacts: crate::backend::workflow_artifacts::RunArtifactQueries,
    pub(crate) workflow_settings:
        HashMap<String, crate::backend::workflow_settings::WorkflowSettingsDraft>,
    pub(crate) workflow_management:
        Option<crate::backend::workflow_management::WorkflowManagementDraft>,
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
            purpose: WorkspacePurpose::Project,
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
            backlog: None,
            log_backlog: None,
            restart_attempts: 0,
            generation: 0,
            image_uploads: HashMap::new(),
            slash_catalogs: HashMap::new(),
            reference_catalogs: HashMap::new(),
            subagent_directories: HashMap::new(),
            inspection: Default::default(),
            plugin_source_draft: Default::default(),
            saved_workflows: HashMap::new(),
            saved_workflow_form: Default::default(),
            workflow_settings: HashMap::new(),
            workflow_definitions: HashMap::new(),
            workflow_histories: HashMap::new(),
            workflow_artifacts: HashMap::new(),
            workflow_management: None,
        }
    }

    /// Cleanly terminate the backend process / job object.
    pub fn shutdown(&mut self) {
        self.invalidate_connection();
        if let Some(kill) = self.kill.take() {
            kill();
        }
        self.started = false;
        self.inbound = None;
    }

    /// Spawn (or fall back to) the next backend candidate. Returns the event
    /// receiver for the store's pump task when a process came up; the
    /// connection's backlog counter is stored on the handle.
    pub fn try_spawn(&mut self) -> Option<futures::channel::mpsc::UnboundedReceiver<ConnEvent>> {
        let launch = self.candidates.get(self.candidate_idx)?;
        let used = self.candidate_idx;
        self.candidate_idx += 1;
        match spawn_connection(launch, &self.path) {
            Ok(conn) => {
                let description = launch.describe.clone();
                self.invalidate_connection();
                self.connection_id = uuid::Uuid::now_v7().to_string();
                self.status = format!("starting ({description})");
                self.conn_desc = description;
                self.working_candidate = used;
                self.inbound = Some(conn.inbound);
                self.kill = Some(conn.kill);
                self.backlog = Some(conn.backlog);
                self.log_backlog = Some(conn.log_backlog);
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

    pub(crate) fn invalidate_connection(&mut self) {
        self.image_uploads.clear();
        self.pending.retain(|_, pending| {
            !matches!(
                pending,
                Pending::AttachmentUpload { .. } | Pending::AttachmentAbort
            )
        });
        self.slash_catalogs.clear();
        self.pending
            .retain(|_, pending| !matches!(pending, Pending::SlashCatalog(_)));
        self.reference_catalogs.clear();
        self.subagent_directories.clear();
        self.reset_workflow_queries();
        self.reset_plugin_queries();
        // 工作区 key 不变不代表进程未更换；旧泵和延迟重启必须失效。
        self.generation = self
            .generation
            .checked_add(1)
            .expect("connection generation exhausted");
    }

    pub fn unload(&mut self) {
        self.invalidate_connection();
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

    pub(crate) fn send_line(&mut self, line: String) -> bool {
        // 通道关闭发生在连接检查之后；必须回传入队结果，避免清空未发送草稿。
        if self
            .inbound
            .as_ref()
            .is_some_and(|tx| tx.send(line).is_ok())
        {
            return true;
        }
        self.status = "write failed".into();
        false
    }

    pub(crate) fn send_pending_line(&mut self, id: u64, line: String) -> bool {
        let sent = self.send_line(line);
        if !sent {
            // 读请求和插件请求也要撤销未入队关联，否则轮询会被永久 in-flight 阻塞。
            self.pending.remove(&id);
        }
        sent
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

/// Nearest existing ancestor directory of `path` (used as the spawn cwd): a
/// deleted or unmounted workspace must not prevent the backend from starting;
/// workspace identity still travels on the protocol (review finding 8).
pub(crate) fn nearest_existing_dir(path: &Path) -> PathBuf {
    let mut current = Some(path.to_path_buf());
    while let Some(dir) = current {
        if dir.is_dir() {
            return dir;
        }
        current = dir.parent().map(Path::to_path_buf);
    }
    std::env::temp_dir()
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;

impl Drop for WorkspaceHandle {
    fn drop(&mut self) {
        self.shutdown();
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

#[cfg(test)]
pub use super::workspace_paths::conversation_workspace_dir;
pub use super::workspace_paths::{discover_workspaces, ensure_conversation_workspace_dir};
