# zcode-gpui for Android (unofficial)

The `zcode-gpui` window, built as an Android app: a LAN-only remote screen for a
desktop `zcode-gpui` instance. It runs no agent itself; every workspace is served
by the paired desktop. See
[`../zcode-gpui/REMOTE_CONTROL.md`](../zcode-gpui/REMOTE_CONTROL.md) ("LAN slice").
It is not an official ZCode product.

## Pairing

1. On the desktop `zcode-gpui`, open Settings › Phone and turn on "Allow phone
   remote control". Allow it through the firewall on private networks if asked.
2. Click "New pairing link" and scan the QR code with the phone's camera within
   5 minutes. The link opens this app and pairs it.
3. Without a camera: "Copy link", then either open it with
   `adb shell am start -a android.intent.action.VIEW -d '<link>'` or put it on the
   phone's clipboard and use Settings › Phone › "Pair from clipboard" in the app.

The pairing survives restarts of either side. Revoke it on the desktop, or use
"Forget desktop" on the phone.

## Layout

| Path | What |
|---|---|
| `src/lib.rs` | `android_main`: sets `HOME`/`TMPDIR`, then opens the shared `zcode_gpui::open_window` as a remote client; the JNI entry for pairing links |
| `gpui_android/` | GPUI platform layer for Android (GameActivity, wgpu/Vulkan, touch, IME), vendored from Zdroid, Apache-2.0 |
| `patches/gpui_platform/` | `gpui_platform` with an Android branch, so `ely-gpui-component` builds |
| `android/` | Gradle project and Kotlin shell (`dev.zcode.gpui.mobile`): Application, GameActivity, IME host view, JNI bridge |
| `scripts/deploy.sh` | Build, strip, package, install and launch |

## Build and run

Needs Rust with the `aarch64-linux-android` target, `cargo-ndk`, the Android SDK
with NDK 27.0.12077973 and JDK 17. Gradle 8.10.2 comes from the wrapper in `android/`.

```sh
rustup target add aarch64-linux-android
cargo install cargo-ndk
PHONE=<adb serial> scripts/deploy.sh
```

`deploy.sh` defaults the SDK, NDK and JDK paths to a Windows `%LOCALAPPDATA%\Android`
install; set `ANDROID_SDK`, `ANDROID_NDK_HOME` and `JAVA_HOME` elsewhere.

`android/local.properties` (not committed) must point `sdk.dir` at the SDK.
Logs: `adb logcat -s zcode_gpui`.

## Third-party code

`gpui_android/` comes from
[Dylanmurzello/zed-android-port](https://github.com/Dylanmurzello/zed-android-port)
`crates/gpui_android` at `1755a0d45865f985bbfa27f65350a840c7df52d2`, licensed
Apache-2.0 (`gpui_android/LICENSE-APACHE`). It was adapted to the zed revision
this repo pins and its JNI symbols were renamed to `dev.zcode.gpui.mobile`. The
Kotlin shell is our own; none of that project's app code is used.
