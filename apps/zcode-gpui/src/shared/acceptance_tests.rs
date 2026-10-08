use super::*;

#[test]
fn nonisolated_acceptance_environment_is_rejected_before_launch() {
    for key in [TICKET_ENV, TOKEN_ENV, THEME_ENV, LOCALE_ENV] {
        let env = |name: &str| (name == key).then(|| std::ffi::OsString::from("synthetic"));
        assert!(AcceptanceRequest::from_environment(false, true, env).is_err());
    }
}

#[test]
fn release_build_rejects_all_harness_capabilities_and_overrides() {
    for key in [TICKET_ENV, TOKEN_ENV, THEME_ENV, LOCALE_ENV] {
        assert!(
            AcceptanceRequest::from_environment(true, false, |name| {
                (name == key).then(|| std::ffi::OsString::from("synthetic"))
            })
            .is_err()
        );
    }
    assert!(AcceptanceRequest::from_environment(true, false, |_| None).is_ok());
}

#[test]
fn override_values_are_exact_and_ticket_requires_matching_token_pair() {
    for (theme, locale) in [("zai-dark", "en-US"), ("zai-light", "zh-CN")] {
        let request = AcceptanceRequest::from_environment(true, true, |name| match name {
            THEME_ENV => Some(theme.into()),
            LOCALE_ENV => Some(locale.into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(request.visuals.theme.unwrap().as_str(), theme);
        assert_eq!(request.visuals.locale.unwrap().as_str(), locale);
    }
    for (key, value) in [
        (THEME_ENV, "system"),
        (THEME_ENV, "dark"),
        (LOCALE_ENV, "zh"),
        (LOCALE_ENV, ""),
        (TICKET_ENV, "scratch"),
        (TOKEN_ENV, "token"),
    ] {
        assert!(
            AcceptanceRequest::from_environment(true, true, |name| {
                (name == key).then(|| value.into())
            })
            .is_err()
        );
    }
}

#[test]
fn projection_reapplies_after_preferences_without_mutating_snapshot() {
    use crate::shared::preferences::{PreferenceOwner, Preferences};
    use crate::shared::settings::AppSettings;
    let mut app = gpui::TestApp::with_text_system_and_assets(
        std::sync::Arc::new(gpui::NoopTextSystem::new()),
        std::sync::Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        let settings = AppSettings {
            theme_preference: Some("zai-dark".into()),
            locale_preference: Some("en-US".into()),
            ..Default::default()
        };
        Preferences::install_at(
            settings.clone(),
            "unused-scratch-settings.json".into(),
            std::sync::Arc::new(|| false),
            cx,
        );
        let visuals = VisualOverrides {
            theme: Some(ThemeMode::ZaiLight),
            locale: Some(Locale::ZhCn),
        };
        visuals.apply(cx);
        assert_eq!(crate::shared::theme::theme_mode(), ThemeMode::ZaiLight);
        assert_eq!(crate::shared::i18n::current_locale(), Locale::ZhCn);
        assert_eq!(
            cx.global::<ely_gpui_component::i18n::I18n>().locale().tag,
            "zh-CN"
        );
        Preferences::apply_runtime(cx);
        visuals.apply(cx);
        assert_eq!(crate::shared::theme::theme_mode(), ThemeMode::ZaiLight);
        assert_eq!(crate::shared::i18n::current_locale(), Locale::ZhCn);
        assert_eq!(
            cx.global::<ely_gpui_component::i18n::I18n>().locale().tag,
            "zh-CN"
        );
        let snapshot = &cx.global::<PreferenceOwner>().0.read(cx).snapshot;
        assert_eq!(snapshot.theme_preference, settings.theme_preference);
        assert_eq!(snapshot.locale_preference, settings.locale_preference);
        Preferences::apply_runtime(cx);
    });
}
