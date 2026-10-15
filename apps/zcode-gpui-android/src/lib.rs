#![cfg(target_os = "android")]
//! Android entry for zcode-gpui: the same window as the desktop app, hosted
//! by GameActivity through the vendored `gpui_android` platform layer. It is
//! a remote client: every workspace is served by the paired desktop.

use android_activity::AndroidApp;
use jni::JNIEnv;
use jni::objects::{JClass, JString};
use std::path::PathBuf;

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("zcode_gpui"),
    );
    let home = app
        .internal_data_path()
        .unwrap_or_else(|| PathBuf::from("/data/local/tmp"));
    let cache = home
        .parent()
        .map(|p| p.join("cache"))
        .unwrap_or_else(|| home.clone());
    // Android sets neither; settings, data paths and temp files derive from them.
    // SAFETY: runs before gpui starts any thread that could read the environment.
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("TMPDIR", &cache);
    }
    log::info!("zcode_gpui: android_main home={}", home.display());

    // Phones have no hardware keyboard: show the IME when a text input gains focus.
    gpui_android::set_on_screen_keyboard_enabled(true);
    let startup = zcode_gpui::load_startup(home.join("workspace")).remote_client();
    gpui_android::run(app, zcode_gpui::AppAssets, move |cx| {
        zcode_gpui::open_window(cx, startup);
    });
}

/// `MainActivity` received a `zcode-gpui://pair?...` link (QR scan or
/// `adb shell am start -d`). The app polls for it once the window is up.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_zcode_gpui_mobile_NativeBridge_nativeSubmitPairLink(
    mut env: JNIEnv,
    _class: JClass,
    link: JString,
) {
    if let Ok(link) = env.get_string(&link) {
        // The link carries a pairing secret: never log it.
        log::info!("zcode_gpui: pairing link received");
        zcode_gpui::remote::submit_pair_link(link.into());
    }
}
