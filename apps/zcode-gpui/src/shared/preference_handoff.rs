use super::preferences::{PreferenceOwner, Preferences};
use super::settings::AppSettings;
use gpui::App;

impl Preferences {
    pub(crate) fn suspend(cx: &mut App) -> futures::channel::oneshot::Receiver<()> {
        let (tx, rx) = futures::channel::oneshot::channel();
        let owner = cx.global::<PreferenceOwner>().0.clone();
        owner.update(cx, |s, cx| {
            s.suspended = true;
            s.error = None;
            if !s.saving && s.queue.is_empty() {
                let _ = tx.send(());
            } else {
                s.drain_waiters.push(tx);
            }
            cx.notify();
        });
        rx
    }

    #[cfg(test)]
    pub(crate) fn drained(cx: &App) -> bool {
        let s = cx.global::<PreferenceOwner>().0.read(cx);
        !s.saving && s.queue.is_empty()
    }

    pub(crate) fn reload_path(cx: &App) -> std::path::PathBuf {
        cx.global::<PreferenceOwner>().0.read(cx).path.clone()
    }

    pub(crate) fn resume_from(mut disk: AppSettings, cx: &mut App) {
        let owner = cx.global::<PreferenceOwner>().0.clone();
        owner.update(cx, |s, cx| {
            // 现有 Host schema 不含原生外观字段；只保留有效显示，不回写旧整份配置。
            if disk.theme_preference.is_none() {
                disk.theme_preference = s.snapshot.theme_preference.clone();
            }
            if disk.ui_font_size.is_none() {
                disk.ui_font_size = s.snapshot.ui_font_size;
            }
            s.snapshot = disk;
            s.suspended = false;
            s.error = None;
            Preferences::apply_snapshot(&s.snapshot, cx);
            s.queue.extend(s.deferred.drain(..));
            if !s.saving {
                s.start_next(cx);
            }
            cx.notify();
        });
    }

    #[cfg(test)]
    pub(crate) fn deferred_count(&self) -> usize {
        self.deferred.len()
    }
}

pub(crate) fn reload(path: &std::path::Path) -> Result<AppSettings, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| "Settings reload failed; preferences remain suspended".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AppSettings::default()),
        Err(_) => Err("Settings reload failed; preferences remain suspended".into()),
    }
}
