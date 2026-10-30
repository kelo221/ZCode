//! New-task draft model choice: per-draft `ui_*` snapshots and the remembered
//! last used model (PARITY.md "New-task draft model and thinking level").

use crate::app::store::AppState;
use crate::shared::settings::LastComposerModel;
use gpui::{App, Context};
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct DraftModels {
    /// `ui_*` selections of new-task drafts the user navigated away from, by draft key.
    pub configs: HashMap<String, DraftUiConfig>,
    /// Last accepted model choice (mirrors `gpuiLastComposerModel`); seeds new drafts.
    pub last: Option<LastComposerModel>,
}

impl DraftModels {
    pub(crate) fn load(cx: &App) -> Self {
        let last = cx
            .try_global::<crate::shared::preferences::PreferenceOwner>()
            .and_then(|owner| owner.0.read(cx).snapshot.gpui_last_composer_model.clone());
        Self {
            configs: HashMap::new(),
            last,
        }
    }
}

/// The `ui_*` selections of one new-task draft, kept while it is not shown.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DraftUiConfig {
    pub model_value: Option<String>,
    pub mode: Option<String>,
    pub plan: bool,
    pub thought: Option<String>,
}

impl AppState {
    /// The remembered last model, when the active workspace's catalog still offers it.
    pub(super) fn last_used_option(&self) -> Option<crate::composer::catalog::ModelOption> {
        let last = self.draft_models.last.as_ref()?;
        let key = self.active_ws_key()?;
        self.workspace_configs
            .get(&key)?
            .models
            .iter()
            .find(|m| m.provider == last.provider && m.model == last.model)
            .cloned()
    }

    /// Draft model: the user's pick, else the last used model.
    pub(super) fn draft_model_option(&self) -> Option<crate::composer::catalog::ModelOption> {
        match &self.ui_model_value {
            Some(value) => self.find_model_option(value),
            None => self.last_used_option(),
        }
    }

    /// Remember the model of an accepted message so new drafts start from it.
    pub(crate) fn remember_composer_model(
        &mut self,
        choice: LastComposerModel,
        cx: &mut Context<Self>,
    ) {
        if choice.provider.is_empty()
            || choice.model.is_empty()
            || self.draft_models.last.as_ref() == Some(&choice)
        {
            return;
        }
        self.draft_models.last = Some(choice.clone());
        if cx.has_global::<crate::shared::preferences::PreferenceOwner>() {
            crate::shared::preferences::Preferences::enqueue(
                crate::shared::preferences::PreferenceChange::LastComposerModel(choice),
                cx,
            );
        }
    }

    /// The model choice the next send would use, for [`Self::remember_composer_model`].
    pub(crate) fn current_model_choice(&self) -> LastComposerModel {
        let effective = self.effective_config();
        LastComposerModel {
            provider: effective.provider,
            model: effective.model,
            thought: effective.thought,
        }
    }
}
