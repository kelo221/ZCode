use crate::app::store::AppState;
use crate::backend::workspace::{CommandCtx, Pending};
use gpui::Context;
use serde_json::json;

impl AppState {
    pub(crate) fn cancellation_pending(&self, workspace: &str, sid: &str, work_id: &str) -> bool {
        self.ws(workspace).is_some_and(|ws| ws.pending.values().any(|pending| {
            matches!(pending, Pending::Command(ctx) if ctx.sid == sid && ctx.ctype == "cancelBackgroundWork"
                && ctx.payload.get("workId").and_then(serde_json::Value::as_str) == Some(work_id))
        }))
    }

    pub(crate) fn cancel_background_work_for(
        &mut self,
        workspace: &str,
        sid: &str,
        work_id: &str,
        cx: &mut Context<Self>,
    ) {
        if self.active_ws_key().as_deref() != Some(workspace)
            || self.active.as_deref() != Some(sid)
            || self.is_read_only_view()
            || work_id.trim().is_empty()
            || self.cancellation_pending(workspace, sid, work_id)
        {
            return;
        }
        // 多次点击不得重复 admission；pending 请求是唯一在途状态，最终状态仍由 stream 决定。
        self.send_session_command(
            workspace,
            CommandCtx::new(sid, "cancelBackgroundWork", json!({"workId": work_id})),
        );
        cx.notify();
    }
}
