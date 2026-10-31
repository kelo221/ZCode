#![cfg(target_os = "android")]
//! Android entry for zcode-gpui: the same window as the desktop app, hosted
//! by GameActivity through the vendored `gpui_android` platform layer. It is
//! a remote client: every workspace is served by the paired desktop.

use android_activity::AndroidApp;
use jni::objects::{JByteArray, JClass, JObject, JString};
use jni::{JNIEnv, JavaVM};
use std::path::PathBuf;
use std::sync::OnceLock;

/// The process's JavaVM and MainActivity, both stable for the app's lifetime.
static JAVA: OnceLock<(usize, usize)> = OnceLock::new();

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
    let _ = JAVA.set((app.vm_as_ptr() as usize, app.activity_as_ptr() as usize));
    zcode_gpui::remote::phone_files::set_file_picker(open_file_picker);
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

/// The composer's "+": `MainActivity.launchOpenDocument()`. A Java exception
/// must be cleared here, or the next JNI call on this thread aborts the app.
fn open_file_picker() -> Result<(), String> {
    let &(vm, activity) = JAVA.get().ok_or("Android activity not ready")?;
    let vm = unsafe { JavaVM::from_raw(vm as *mut _) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { JObject::from_raw(activity as _) };
    let result = env.call_method(&activity, "launchOpenDocument", "()V", &[]);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    result
        .map(|_| ())
        .map_err(|e| format!("Could not open the file picker: {e}"))
}

/// A file read by `PickedFiles` (picker or keyboard image).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_zcode_gpui_mobile_NativeBridge_nativeAttachFile(
    mut env: JNIEnv,
    _class: JClass,
    name: JString,
    mime: JString,
    bytes: JByteArray,
) {
    let name: String = env.get_string(&name).map(Into::into).unwrap_or_default();
    let mime: String = env.get_string(&mime).map(Into::into).unwrap_or_default();
    match env.convert_byte_array(&bytes) {
        Ok(bytes) => {
            log::info!("zcode_gpui: attaching {} bytes ({mime})", bytes.len());
            zcode_gpui::remote::phone_files::submit_picked_file(name, mime, bytes);
        }
        Err(error) => zcode_gpui::remote::phone_files::submit_picker_error(format!(
            "Could not read {name}: {error}"
        )),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_zcode_gpui_mobile_NativeBridge_nativeAttachError(
    mut env: JNIEnv,
    _class: JClass,
    message: JString,
) {
    if let Ok(message) = env.get_string(&message) {
        zcode_gpui::remote::phone_files::submit_picker_error(message.into());
    }
}
