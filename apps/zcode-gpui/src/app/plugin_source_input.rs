use crate::app::{root::RootView, store::AppState};
use crate::composer::input::Composer;
use gpui::{AppContext, Context, Entity};
use std::path::PathBuf;

#[derive(Default)]
pub(crate) struct PluginSourceDraft {
    pub input: Option<Entity<Composer>>,
    pub picking: bool,
    pub error: Option<String>,
}

impl AppState {
    pub(crate) fn source_input(&mut self, cx: &mut Context<Self>) -> Option<Entity<Composer>> {
        let key = self.active_ws_key()?;
        if let Some(input) = self.ws(&key)?.plugin_source_draft.input.clone() {
            return Some(input);
        }
        let input = cx.new(|cx| Composer::new_single_line("Source URL or local path", cx));
        self.ws_mut(&key)?.plugin_source_draft.input = Some(input.clone());
        Some(input)
    }

    pub(crate) fn settle_plugin_source_pick(
        &mut self,
        key: &str,
        generation: u64,
        input: &Entity<Composer>,
        original: &str,
        result: Result<Option<PathBuf>, String>,
        cx: &mut Context<Self>,
    ) {
        let bound = self.active_ws_key().as_deref() == Some(key)
            && self.navigation_generation == generation;
        let Some(ws) = self.ws_mut(key) else { return };
        ws.plugin_source_draft.picking = false;
        // 模态框期间导航或编辑后不能覆盖新 draft；entity 同一性也防止已卸载 workspace 复用。
        if !bound
            || ws.plugin_source_draft.input.as_ref() != Some(input)
            || input.read(cx).text() != original
        {
            cx.notify();
            return;
        }
        match result {
            Ok(Some(path)) => input.update(cx, |c, cx| {
                c.set_text(&path.to_string_lossy());
                cx.notify();
            }),
            Ok(None) => {}
            Err(error) => ws.plugin_source_draft.error = Some(crate::shared::redact::scrub(&error)),
        }
        cx.notify();
    }
}

impl RootView {
    pub(crate) fn pick_plugin_source(&mut self, key: &str, cx: &mut Context<Self>) {
        let capture = self.state.update(cx, |s, cx| {
            if s.active_ws_key().as_deref() != Some(key) {
                return None;
            }
            let generation = s.navigation_generation;
            let input = s.source_input(cx)?;
            let ws = s.ws_mut(key)?;
            if ws.plugin_source_draft.picking {
                return None;
            }
            ws.plugin_source_draft.picking = true;
            ws.plugin_source_draft.error = None;
            Some((generation, input.clone(), input.read(cx).text().to_owned()))
        });
        let Some((generation, input, original)) = capture else {
            return;
        };
        let key = key.to_owned();
        let state = self.state.downgrade();
        let receiver = crate::shared::os::folder_picker::pick_directory();
        cx.spawn(async move |_, cx| {
            let result = receiver
                .await
                .unwrap_or_else(|_| Err("Folder picker closed unexpectedly".into()));
            let _ = state.update(cx, |s, cx| {
                s.settle_plugin_source_pick(&key, generation, &input, &original, result, cx)
            });
        })
        .detach();
        cx.notify();
    }
}
