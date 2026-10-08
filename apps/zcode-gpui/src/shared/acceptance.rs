//! Debug-only disposable acceptance capability; never a public home override.
use super::i18n::Locale;
use super::isolation::IsolatedSettings;
use super::theme::ThemeMode;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

const TICKET_ENV: &str = "ZCODE_GPUI_ACCEPTANCE_TICKET";
const TOKEN_ENV: &str = "ZCODE_GPUI_ACCEPTANCE_TOKEN";
const THEME_ENV: &str = "ZCODE_GPUI_ACCEPTANCE_THEME";
const LOCALE_ENV: &str = "ZCODE_GPUI_ACCEPTANCE_LOCALE";
static VISUALS: OnceLock<VisualOverrides> = OnceLock::new();

#[derive(Clone, Copy, Default)]
struct VisualOverrides {
    theme: Option<ThemeMode>,
    locale: Option<Locale>,
}

impl VisualOverrides {
    fn apply(&self, cx: &mut gpui::App) {
        if let Some(locale) = self.locale {
            super::i18n::apply_locale(locale, cx);
        }
        if let Some(theme) = self.theme {
            super::theme::set_theme_mode(theme);
            super::theme::apply_appearance(cx.window_appearance(), cx);
        }
    }
}

pub(crate) struct AcceptanceRequest {
    ticket: Option<PathBuf>,
    token: Option<String>,
    visuals: VisualOverrides,
}

impl AcceptanceRequest {
    pub(crate) fn read(isolated: bool) -> Result<Self, String> {
        Self::from_environment(isolated, cfg!(debug_assertions), |name| {
            std::env::var_os(name)
        })
    }

    fn from_environment(
        isolated: bool,
        debug: bool,
        env: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Self, String> {
        let values = [TICKET_ENV, TOKEN_ENV, THEME_ENV, LOCALE_ENV].map(env);
        if values.iter().any(Option::is_some) && (!isolated || !debug) {
            return Err("Acceptance environment requires debug --isolated-settings".into());
        }
        let [ticket, token, theme, locale] = values;
        if ticket.is_some() != token.is_some() {
            return Err("Acceptance reattach requires both ticket and token".into());
        }
        let text = |value: Option<OsString>| -> Result<Option<String>, String> {
            value
                .map(|v| {
                    v.into_string()
                        .map_err(|_| "Invalid acceptance value".into())
                })
                .transpose()
        };
        let token = text(token)?;
        if token.as_ref().is_some_and(String::is_empty) {
            return Err("Acceptance token cannot be empty".into());
        }
        let theme = match text(theme)?.as_deref() {
            None => None,
            Some("zai-dark") => Some(ThemeMode::ZaiDark),
            Some("zai-light") => Some(ThemeMode::ZaiLight),
            _ => return Err("Acceptance theme must be zai-dark or zai-light".into()),
        };
        let locale = match text(locale)?.as_deref() {
            None => None,
            Some("en-US") => Some(Locale::EnUs),
            Some("zh-CN") => Some(Locale::ZhCn),
            _ => return Err("Acceptance locale must be en-US or zh-CN".into()),
        };
        Ok(Self {
            ticket: ticket.map(PathBuf::from),
            token,
            visuals: VisualOverrides { theme, locale },
        })
    }

    pub(crate) fn open_isolated(self) -> Result<Arc<IsolatedSettings>, String> {
        let isolated = match (&self.ticket, &self.token) {
            (None, None) => IsolatedSettings::create(),
            #[cfg(debug_assertions)]
            (Some(ticket), Some(token)) => IsolatedSettings::reattach(ticket, token),
            _ => return Err("Acceptance reattach is debug-only".into()),
        }
        .map_err(|_| {
            "Disposable acceptance root refused (invalid capability or active lock)".to_owned()
        })?;
        VISUALS
            .set(self.visuals)
            .map_err(|_| "Acceptance capability already installed")?;
        #[cfg(debug_assertions)]
        {
            use std::io::Write;
            let announcement = serde_json::json!({
                "root": isolated.root(), "ticket": isolated.ticket_path(),
            });
            println!("ZCODE_GPUI_ACCEPTANCE {announcement}");
            let _ = std::io::stdout().flush();
        }
        Ok(isolated)
    }
}

pub(crate) fn apply_runtime(cx: &mut gpui::App) {
    if cfg!(debug_assertions)
        && super::isolation::active().is_some()
        && let Some(visuals) = VISUALS.get()
    {
        // 安装/重载偏好会覆盖全局外观；只重投影 scratch 覆盖值，不修改或保存偏好快照。
        visuals.apply(cx);
    }
}

#[cfg(test)]
#[path = "acceptance_tests.rs"]
mod tests;
