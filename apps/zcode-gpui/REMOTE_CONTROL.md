# Phone remote control: native Android client over the local network

Status: **in progress**. Build spikes 1 and 2 passed on 2026-10-14 (see "Spike results"). The
unconnected app runs on a phone (see "Android app status").
This file is the spec for the feature. Update it before changing behaviour.

## Product rules

- The phone runs **the same GPUI app** (an Android build of `zcode-gpui`) as a remote
  screen for a desktop `zcode-gpui` instance on the same network. It never runs an agent,
  CLI or Host itself (AGENTS.md: the phone reuses the existing host and runtime).
- **Local network only** for the first version. No external relay, no Z.AI account, and
  no official phone web page. Off-network use means the user's own tunnel (e.g. Tailscale).
- Off by default on the desktop. Pairing uses a QR code shown by the desktop. One phone
  is paired at a time; the desktop can revoke it.
- **Phone scope:**
  - list and open sessions, with a live transcript;
  - send, steer and stop;
  - answer permission, question and plan prompts;
  - attach images.

  Files, git review, terminal, plugins, MCP and subagent management stay desktop-only.

## Architecture and owners

```text
Android phone: zcode-gpui (remote-client build)         Desktop: zcode-gpui (local-backend build)
 UI, frame assembler, commands (unchanged)               ┌─────────────────────────────────┐
 RemoteConnection ──TLS, pinned cert fingerprint────▶   │ LAN gateway (off by default)     │
   one socket per open workspace,                        │  pairing / device-token auth     │
   NDJSON V4 lines, same as the CLI stdio wire           │  per-phone remote scope          │
                                                         └──────────────┬──────────────────┘
                                                                        │ existing stdio
                                                         CLI app-server (one per workspace)
```

| State | Owner |
|---|---|
| Accepted input order | CLI `CommandInbox`. Desktop and phone are peers; neither UI orders the other's input. |
| CLI process, initialize result, reverse RPC answers | Desktop `zcode-gpui` (as today). The phone never receives reverse RPC. |
| Phone subscriptions (`subscriptionId` ownership, held early frames) | Desktop remote scope, per phone connection. Dropped on disconnect. |
| `connectionId`, `clientMode` | Set by the desktop scope (`gpui-lan-<device>-<uuid>`, `web-remote-replayable`). Never taken from the phone. |
| Pairing secret | Phone: device token in app-private storage. Desktop: SHA-256 of it only. |

Event order:

```text
phone hello/initialize ─▶ scope answers from the desktop's cached initialize result (CLI not re-initialised)
phone subscribe ─▶ scope: set identity and mode, remap request id, hold that topic's early frames ─▶ CLI
CLI frame(subId) ─▶ held (ack pending) | phone socket (phone-owned subId) | desktop assembler (own)
CLI ack{subscriptionId} ─▶ scope records ownership and releases held frames in order
phone command / resolveInteraction ─▶ CLI CommandInbox
socket drop / revoke / workspace closed ─▶ unsubscribe everything the phone owned, drop held frames
phone reconnects ─▶ subscribes again ─▶ snapshot (resume-from-seq is a later optimisation)
```

## Interfaces

- **Transport.** TCP + TLS (rustls, self-signed desktop certificate). The phone accepts
  only the certificate fingerprint from the QR code. Lines are capped by `MAX_RAW_LINE_BYTES`.
- **Socket handshake.** The first line is `{"type":"pair","pairingToken","deviceName"}` or
  `{"type":"auth","deviceToken"}`. The desktop replies with the workspace list. Then
  `{"type":"open","workspaceKey"}` switches the socket to raw V4 lines for that workspace.
- **QR / link.** `zcode-gpui://pair?host=…&port=…&fp=<sha256>&t=<one-time token, 5 min>`.
  Scanning with the phone's camera app opens the link in the app.
- **Method allowlist (desktop scope):**
  - **Allowed:**
    - `v4/conversation/subscribe`, `unsubscribe`, `resync`;
    - `v4/command` for chat commands: send, steer, stop, `resolveInteraction`, mode/model/thought, session list/open/rename;
    - flow control;
    - attachment upload and read.
  - **Everything else** gets an immediate JSON-RPC error. This includes the model-catalog
    probe that creates and deletes a session; the desktop serves the catalog from its cache.
- **Client seam.** `backend/conn.rs::Connection` (event receiver, `inbound` sender, `kill`) is
  produced either by `spawn_connection` (local) or by `remote_client::connect` (TLS). Code
  above it is unchanged. `clientMode` comes from the connection type instead of being
  hardcoded in `backend/commands.rs` and `flow_cmds.rs`.

## Phases

0. **Spikes:**
   - (1) Android build on our pinned gpui: **done**.
   - (2) Soft keyboard: **done**.
   - (3) Wire test: a second `connectionId` with `web-remote-replayable` on one CLI, against a fake CLI with two subscribers.
1. **Library split.**
   - **Done:** the crate is a library (`src/lib.rs`) with a thin desktop `main.rs`, plus
     `load_startup(workspace)` and `open_window(cx, startup)` entry points shared by both builds.
   - **Still to do:** gate desktop-only modules behind a `local-backend` feature (process
     launch, services_rpc, terminal, git review, files, folder picker, OS integrations), add a
     `remote-client` feature, and check it with
     `cargo check --no-default-features --features remote-client`.
2. **Desktop remote scope** (`backend/remote_scope/`, ≤400 lines per file):
   - handshake, id remap, identity/mode injection, routing by `subscriptionId` before reassembly (1024 frames / 32 MiB held), allowlist, cleanup;
   - a phone request for a workspace that isn't loaded starts the desktop's own backend (`ensure_spawned`).
   - Tests: port `zcodeAgentConnectionScope.test.ts`, plus both stream semantics on one session.
3. **Desktop listener and pairing:**
   - the listener is bound to a chosen LAN interface;
   - pairing tokens and device tokens as specified above, with repeated auth failures rate-limited;
   - a Settings › Phone page (toggle, QR, revoke) and a header status chip;
   - tokens and links never logged.
4. **Android app** (`apps/zcode-gpui-android`):
   - **Done:** cdylib, platform layer, Kotlin shell, Gradle and the deploy script.
   - **Still to do:** the `zcode-gpui://pair` intent filter, the pinned-fingerprint TLS client and reconnect.
5. **Phone UI:**
   - phone layout (see "Phone layout" below);
   - hide desktop-only panes;
   - make hover-only controls visible on touch;
   - turn the sidebar into a drawer;
   - lift the composer above the keyboard;
   - show "reconnecting" / "desktop offline" banners.
6. **Hardening:**
   - desktop and phone driving one session at once;
   - screen-off and Wi-Fi loss mid-stream;
   - a CLI restart;
   - revocation, a wrong certificate fingerprint, oversized lines.

## Phone layout

Ported from `packages/ui/src/WebRemoteControlMobileShell.tsx`. It applies when the window is at
most 767 logical px wide (`max-width: 767px`), on any platform, so desktop users can try it by
narrowing the window.

- **Two full-screen pages** instead of the sidebar beside the chat:
  - **Home:** the sidebar at full width (new task, search, tasks, projects, settings).
  - **Chat:** a 44 px header with a back arrow and "Task chat", then the full-width workspace panel.
- **Owner:** `RootView.mobile_page` (UI-local, not persisted). It starts on Home.
- **Moves to Chat** only on an explicit user action: selecting a task row, "New task" (row, "+"
  or shortcut), or picking a session or new task in the command centre. Background session
  changes never switch pages.
- **Back** (the header arrow, or Android's back key, which arrives as key `"back"`) closes the
  topmost layer: first the command centre, then Settings, then Chat goes to Home. On Home it
  does nothing yet.
- Above 767 px the layout is unchanged; `mobile_page` is ignored, not reset.

## Spike results (2026-10-14, Pixel 7a, Android 17 / API 37, Mali-G710)

The spike workspace lives outside the repo (`%LOCALAPPDATA%\Android\spike`).

- **Platform layer.** `crates/gpui_android` from Zdroid
  (github.com/Dylanmurzello/zed-android-port @ `1755a0d4`, crate licence Apache-2.0) builds
  against our pinned zed `1a28cff` **without patching gpui**. The adaptations:
  - copy gpui's `queue.rs`, which upstream exports only for windows/linux/wasm;
  - drop five extra `PlatformWindow` hooks that exist only in Zdroid's patched gpui;
  - implement the newer trait items (`visibility`, `on_visibility_change`, `on_system_sleep`, `prevent_idle_sleep`, the `restart`/`on_quit` signatures);
  - follow `gpui_wgpu` API drift (`WgpuSurfaceConfig`, `gpu_specs`, no `set_clear_color` / `load_fonts_dir`).
- **Other build fixes:**
  - `ely-gpui-component` depends on `gpui_platform`, which needs a `target_os = "android"` branch (patched copy).
  - ely's `rust-embed` assets need the `debug-embed` feature in debug builds.
  - `android-activity` 0.6.1 requires `androidx.games:games-activity:4.4.0`.
- **Rendering.** wgpu selects Mali-G710 on Vulkan and passes its configuration test. ely
  fonts, components, SVG icons and theme render correctly. Vsync is about 16.7 ms with
  about 0.05 ms per paint, at a logical viewport of 411×914 at 2.625×.
- **Touch.** Taps arrive as `MouseDown`/`MouseUp` with correct logical coordinates, and
  ely `Button::on_click` fires.
- **Soft keyboard.** A clean-room Kotlin IME host (1×1 `ImeHostView` with an
  `InputConnection` forwarding to the `gpui_android` JNI bridge, plus a surrounding-text
  mirror) works with Gboard. Commit, backspace (`KEYCODE_DEL`), auto-capitalisation and
  suggestions all work in ely's `TextInput`, which uses the same `EntityInputHandler` path
  as the composer.
- **Known gaps:**
  - CJK text renders as boxes: the system Noto CJK fonts must be registered via `add_fonts` or a `gpui_wgpu` API.
  - The window does not resize for the keyboard under edge-to-edge, so the keyboard inset must be passed to the UI (fixed, see below).
  - The app repaints every vsync even when idle (battery).
  - The JNI symbols are fixed to `com.zdroid.NativeBridge`; rename them to our own package (done: `dev.zcode.gpui.mobile`).
  - Zdroid's Kotlin app code has no declared licence (GPL repository), so it is **not** reused.

## Android app status (2026-10-14)

`apps/zcode-gpui-android` runs the real `zcode-gpui` window on the Pixel 7a. It is not
connected to anything yet, so the workspace shows "no backend".

- **Startup.** `android_main` sets `HOME` and `TMPDIR` to the app's private directories and calls
  `zcode_gpui::load_startup` and `open_window`, the same calls the desktop binary makes.
  Settings and data live under the app's private storage.
- **System bars and keyboard.** GameActivity draws edge-to-edge. `MainActivity` opts back out
  (`setDecorFitsSystemWindows(true)` with `adjustResize`, `targetSdk 34`), so Android sizes the
  surface between the status bar and the navigation bar or keyboard. gpui gets an ordinary
  resize. Targeting SDK 35+ removes this opt-out, and the UI would then have to pad for
  insets itself.
- **Keyboard re-show.** A tap that leaves a text input focused while the keyboard is hidden
  shows it again, as native Android text fields do (`gpui_android` `window.rs`).
- **Phone layout** (see "Phone layout"): task list and chat as two pages, with back via the
  header arrow or the Android back key. The first back press closes the keyboard, as usual on
  Android.
- **Verified on the phone:**
  - Home, chat and Settings pages fit the screen.
  - The composer is above the keyboard.
  - Gboard text is committed to the composer.
  - Back returns from chat to home.
- **Known gaps** (phone UI phase):
  - A tapped row keeps its hover highlight, because touch leaves gpui's hover state set.
  - Short labels wrap mid-word in Settings' narrow form column ("Languag e").
  - Error banners clip their text at the left edge.
  - Gboard auto-capitalisation in the composer is wrong ("hI"), which suggests the composer's
    surrounding text isn't reaching the IME mirror.
  - Hover-only controls have no touch equivalent.
  - CJK fonts are not registered yet.
  - The app repaints every vsync even when idle.
