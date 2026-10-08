//! Internationalization (i18n) module for zcode-gpui.
//! Translations are extracted from packages/ui/src/i18n/locales/{en-US,zh-CN}.ts.

#![allow(dead_code)]

use std::sync::atomic::{AtomicU8, Ordering};

// Include generated translation tables:
// - `pub static EN_US_TRANSLATIONS: &[(&str, &str)]`
// - `pub static ZH_CN_TRANSLATIONS: &[(&str, &str)]`
include!(concat!(env!("OUT_DIR"), "/i18n_data.rs"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Locale {
    #[default]
    EnUs,
    ZhCn,
}

impl Locale {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EnUs => "en-US",
            Self::ZhCn => "zh-CN",
        }
    }

    pub fn from_str_lenient(s: &str) -> Self {
        let lower = s.to_lowercase();
        if lower.starts_with("zh") {
            Self::ZhCn
        } else {
            Self::EnUs
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LocalePreference {
    System,
    #[default]
    EnUs,
    ZhCn,
}

impl LocalePreference {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::EnUs => "en-US",
            Self::ZhCn => "zh-CN",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "zh-CN" | "zh" => Self::ZhCn,
            "en-US" | "en" => Self::EnUs,
            "system" => Self::System,
            // 未设置或损坏的语言偏好默认英文；只有明确选择 System 才读取系统语言。
            _ => Self::EnUs,
        }
    }

    pub fn resolve(&self) -> Locale {
        match self {
            Self::EnUs => Locale::EnUs,
            Self::ZhCn => Locale::ZhCn,
            Self::System => detect_system_locale(),
        }
    }
}

static CURRENT_LOCALE: AtomicU8 = AtomicU8::new(0); // 0 = EnUs, 1 = ZhCn

pub fn set_current_locale(locale: Locale) {
    let val = match locale {
        Locale::EnUs => 0,
        Locale::ZhCn => 1,
    };
    CURRENT_LOCALE.store(val, Ordering::Relaxed);
}

pub(crate) fn apply_locale(locale: Locale, cx: &mut gpui::App) {
    use ely_gpui_component::i18n::I18n;
    // Ely 原本默认英文；由同一次已提交偏好投影同步语言，避免中文界面混入英文组件提示。
    if !cx.has_global::<I18n>() {
        cx.set_global(
            I18n::new(locale.as_str())
                .catalog("en-US", &[])
                .catalog("zh-CN", &[]),
        );
    } else if cx.global::<I18n>().locale().tag != locale.as_str() {
        I18n::set_locale(locale.as_str(), cx);
    }
    set_current_locale(locale);
}

pub fn current_locale() -> Locale {
    match CURRENT_LOCALE.load(Ordering::Relaxed) {
        1 => Locale::ZhCn,
        _ => Locale::EnUs,
    }
}

/// Detect host system locale on Windows, macOS, or Linux.
pub fn detect_system_locale() -> Locale {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(lang) = std::env::var(key)
            && !lang.trim().is_empty()
        {
            return Locale::from_str_lenient(&lang);
        }
    }
    #[cfg(windows)]
    {
        let mut name = [0u16; 85];
        // Windows 通常没有 LANG；读取用户区域设置才能兑现 System 语言偏好。
        let length = unsafe { windows::Win32::Globalization::GetUserDefaultLocaleName(&mut name) };
        if length > 1 {
            return Locale::from_str_lenient(&String::from_utf16_lossy(
                &name[..length as usize - 1],
            ));
        }
    }
    Locale::EnUs
}

/// Look up a localized string in the specified locale.
pub fn t_locale(key: &str, locale: Locale) -> &str {
    let table = match locale {
        Locale::EnUs => EN_US_TRANSLATIONS,
        Locale::ZhCn => ZH_CN_TRANSLATIONS,
    };

    if let Ok(idx) = table.binary_search_by_key(&key, |&(k, _)| k) {
        return table[idx].1;
    }

    // Fallback to en-US if missing in zh-CN
    if locale != Locale::EnUs
        && let Ok(idx) = EN_US_TRANSLATIONS.binary_search_by_key(&key, |&(k, _)| k)
    {
        return EN_US_TRANSLATIONS[idx].1;
    }

    key
}

pub fn label<'a>(english: &'a str, chinese: &'a str) -> &'a str {
    match current_locale() {
        Locale::EnUs => english,
        Locale::ZhCn => chinese,
    }
}

/// Look up a localized string in the active locale.
pub fn t(key: &str) -> &str {
    t_locale(key, current_locale())
}

/// Look up a localized string and replace `{key}` placeholders.
pub fn t_fmt(key: &str, vars: &[(&str, &str)]) -> String {
    let raw = t(key);
    let mut result = raw.to_string();
    for &(var_key, var_val) in vars {
        let pattern = format!("{{{var_key}}}");
        result = result.replace(&pattern, var_val);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_defaults_to_english_and_system_is_explicit() {
        assert_eq!(LocalePreference::default(), LocalePreference::EnUs);
        for invalid in ["", "unknown", "zh-JP", "en-unsupported"] {
            assert_eq!(LocalePreference::parse(invalid), LocalePreference::EnUs);
        }
        assert_eq!(LocalePreference::parse("system"), LocalePreference::System);
        assert_eq!(LocalePreference::parse("zh-CN"), LocalePreference::ZhCn);
        assert_eq!(LocalePreference::parse("zh"), LocalePreference::ZhCn);
        assert_eq!(LocalePreference::parse("en-US"), LocalePreference::EnUs);
        assert_eq!(LocalePreference::parse("en"), LocalePreference::EnUs);
        assert_eq!(LocalePreference::System.resolve(), detect_system_locale());
    }

    #[test]
    fn test_translations_loaded() {
        assert!(!EN_US_TRANSLATIONS.is_empty());
        assert!(!ZH_CN_TRANSLATIONS.is_empty());
        assert!(EN_US_TRANSLATIONS.len() > 5000);
        assert!(ZH_CN_TRANSLATIONS.len() > 5000);
    }

    #[test]
    fn test_exact_lookup() {
        let en = t_locale("quickPick.command.newTask", Locale::EnUs);
        assert_eq!(en, "New task");

        let zh = t_locale("quickPick.command.newTask", Locale::ZhCn);
        assert_eq!(zh, "新任务");
    }

    #[test]
    fn test_fallback_to_key_when_unknown() {
        assert_eq!(
            t_locale("nonexistent.key.xyz", Locale::EnUs),
            "nonexistent.key.xyz"
        );
    }

    #[test]
    fn test_placeholder_formatting() {
        set_current_locale(Locale::EnUs);
        let res = t_fmt("scheduledPreview.toast.running", &[("title", "MyTask")]);
        assert_eq!(res, "Running “MyTask”…");
    }
}
