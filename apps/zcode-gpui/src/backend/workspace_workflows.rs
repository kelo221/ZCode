use crate::backend::workspace::{Pending, WorkspaceHandle};

impl WorkspaceHandle {
    pub(crate) fn reset_workflow_queries(&mut self) {
        self.saved_workflows.clear();
        self.workflow_definitions.clear();
        self.workflow_histories.clear();
        self.workflow_artifacts.clear();
        self.interrupt_workflow_settings();
        if let Some(form) = &mut self.workflow_management {
            form.needs_reload = true;
            form.reload_requested = false;
            form.confirmation = None;
            form.error = Some("Connection interrupted; reload definition before another write; pending mutation outcome is unknown".into());
        }
        if self.pending.values().any(|p| {
            matches!(
                p,
                Pending::SavedWorkflowCreate(_) | Pending::SavedWorkflowStart { .. }
            )
        }) {
            self.saved_workflow_form.error = Some("Connection interrupted; saved workflow launch outcome is unknown; check tasks before retrying".into());
        }
        self.pending.retain(|_, p| {
            !matches!(
                p,
                Pending::SavedWorkflowList(_)
                    | Pending::SavedWorkflowCreate(_)
                    | Pending::SavedWorkflowStart { .. }
                    | Pending::SavedWorkflowCleanup
                    | Pending::WorkflowSettings { .. }
                    | Pending::WorkflowDefinition { .. }
                    | Pending::WorkflowHistory { .. }
                    | Pending::WorkflowArtifacts(_)
                    | Pending::WorkflowArtifactContent(_)
                    | Pending::WorkflowPreflight(_)
                    | Pending::WorkflowMutation(_)
            )
        });
    }
}
