# zcode-gpui

Native Rust/GPUI frontend for the existing ZCode CLI app-server. Chat, project/session navigation, model and mode selection, basic review/files/terminal panes, and a first Settings slice are implemented. This is **not complete Electron parity**: provider/capability configuration, parts of plugin/workflow management, host-owned task metadata, and native lifecycle/distribution remain incomplete. See [PARITY.md](PARITY.md) and [SETTINGS.md](SETTINGS.md).

## Running

```sh
cargo run --release --manifest-path apps/zcode-gpui/Cargo.toml -- --workspace <absolute_project_path>
```

GPUI and `gpui_platform` are git-pinned to Zed revision `1a28cff4b409169bac058bca40dfbfeb7621d19b`; Ely components are pinned to `2f8b2f687cd1e9b98a7cd29d4e882d09406fc547`. The crate uses Rust edition 2024. Dependency versions and platform features are authoritative in `Cargo.toml` and `Cargo.lock`.

## Implemented surfaces

- **Projects and sessions:** Reads recent projects from the bootstrap settings file. Backends spawn lazily; idle, non-running workspaces unload after 10 minutes by default (`ZCODE_GPUI_IDLE_SECS`). Windows Job Objects and quit cleanup own child-process termination. Sidebar lists load progressively; search filters loaded session titles, not a complete host task index.
- **Chat:** Markdown, highlighted code, diffs, a virtualized transcript, model/thinking/mode selection, turn metadata, plans, inline permissions/questions, and supported edit/retry/undo/session actions.
- **Composer and queue:** Multi-line input, local-path attachments, live scoped skill/plugin references and CLI-owned slash catalogs (including `/init` and custom commands), local `/plan` draft/submission routing, aggregate background activity and read-only child-agent chats, queue Edit/reorder/send-now/delete/auto-drain, and availability-gated goal Pause/Resume. Ctrl+Enter (Cmd+Enter on macOS) and running modifier-click on Send choose one-shot delivery without changing follow-up preference. Paused plain-input and new-goal queues offer Clear/Keep/Cancel with reviewed-ID and stale-callback guards; the CLI alone changes accepted queue state. Drafts live in memory; they are **not saved across process restarts**. Existing-chat clipboard images use bounded begin/chunk/commit uploads with Retry/Cancel and blocked submission until ready; new-chat image pastes retain format-correct owned temporary paths. General new-chat non-path upload remains incomplete.
- **Safe submission:** Failed enqueue preserves text, attachments, and temporary-file ownership. Explicit rejection restores the originating draft without overwriting newer text or an Edit/Rename operation. An uncertain acknowledgement is reported without automatic resend. A delayed create acknowledgement cannot steal newer navigation.
- **Tool panes:** Basic Git review with stage/commit/push and diffs, a lazy file tree with text/Markdown and bounded static PNG/JPEG/GIF/WebP/BMP/ICO previews, and a local PTY terminal with scrollback drag selection and Ctrl/Cmd+C copy (Ctrl+C without selection still interrupts). Usage, MCP status listing/Open authorization, plugins, workflows, and subagent views also exist; OAuth opening is owner/connection/query guarded and uses a secret-safe native adapter, not MCP configuration CRUD. The presence of a pane does not imply every management action is implemented.
- **Plugins and workflows:** Workspace-scoped install/update/enable/uninstall/restore/source management uses checked results and shared pending guards. Personal Source plugin cards expose draft-safe Example Prompt prefill with canonical enabled references; an existing target draft blocks prefill rather than being overwritten. Saved project/global workflows have typed arguments, Run in new chat, definition inspection, saved-run history/Open chat, eligible-run model/concurrency settings, metadata editing, and confirmed Delete/global-to-project Move. Writes preflight the inspected definition and preserve newer edits; existing move destinations are never overwritten. Definitions and scheduling stay backend-owned; unknown outcomes require Reload, not automatic replay or deletion. Read-only plugin Details/Refresh and typed user/workspace configuration are reachable, with masked secret drafts, explicit per-option Clear and confirmed scope-correct Reset. Scoped writes preflight current CLI values, preserve unspecified/newer edits and refresh only their origin. Projected Open run/Open successor and All runs are reachable; a focused amendment follows its exact returned run/tool-call pair only after the authoritative stream contains it. Focused run details show journal-owned artifact metadata with explicit Refresh and distinct loading/empty/unavailable/error states. Markdown deliverables offer bounded latest-version View/Retry/Close through the authorized CLI read, with passive links/images and strict complete-response validation. Store paths are never opened. Other artifact formats, version navigation, cross-project run detail and native lifecycle evidence remain incomplete.
- **Settings:** The sidebar gear, command center, and `CmdOrCtrl+,` open the same per-window destination. General has System/English/Chinese language; Appearance has System/light/dark theme and 12–20 px interface font size; Shortcuts supports the commands listed below. Memory persists `memoryEnabled` (default off) and supplies its committed value to CLI runtime-preference requests; it permits configured memory for newly materialized/resumed runtimes, not an active-runtime reset. Back/Escape preserve the workspace, session, and composer. Streams continue while Settings is open. Localization outside Settings and selected primary surfaces is still partial.

### Settings persistence and paths

One process-global preference controller serializes background read–modify–write mutations, including recent-project updates. It preserves unknown JSON fields and publishes runtime changes only after a successful disk write. Corrupt/unreadable settings and a running Electron desktop cause refusal rather than overwrite; errors are visible. Raw “Open settings file” is an advanced editor action, not a safe coexistence writer.

- Bootstrap file: `<settings-home>/.zcode/v2/setting.json`.
- Settings home: nonblank `ZCODE_DESKTOP_HOME_DIR`, otherwise `HOME`, then `USERPROFILE`.
- Application data base: persisted `dataBaseDir`, then `ZCODE_DATA_BASE_DIR`, then default home. The data root appends `.zcode`.
- `ZCODE_DESKTOP_HOME_DIR` changes the bootstrap location, not the default data base.
- No automatic migration or deletion of existing user data is performed.

Theme preference System remains distinct from the effective OS appearance. Ely controls and legacy semantic colors/interface typography update together. Code and terminal body sizes remain independent. Preference persistence/re-hydration is tested with a new application owner; an actual native executable restart has not been recorded.

### Supported application shortcuts

`CmdOrCtrl` means Command on macOS and Control elsewhere. Overrides apply live after successful persistence. Empty bindings disable a command; Reset restores its defaults. Unsupported command IDs in the file are retained but not offered as working controls.

| Action         | Default                            |
| -------------- | ---------------------------------- |
| Settings       | `CmdOrCtrl+,`                      |
| Command center | `CmdOrCtrl+k`, `CmdOrCtrl+Shift+p` |
| Switch theme   | `CmdOrCtrl+Shift+l`                |
| Terminal       | `CmdOrCtrl+j`                      |
| Tool dock      | `CmdOrCtrl+Alt+b`                  |
| New task       | `CmdOrCtrl+n`                      |
| Open workspace | `CmdOrCtrl+o`                      |

## Architecture and protocol

```text
GPUI window / draft / preference projection
    ↔ per-workspace process adapter, stdio NDJSON
    ↔ existing CLI app-server
        owns session state, CommandInbox admission, revisions, and streams
```

The frontend uses the V4 command/topic contract in `packages/shared/src/zcode-protocol-v4/` and physical wire version 3. It validates subscription/epoch/sequence continuity and bounded fragment assembly. Process-generation checks independently reject stale pumps, flow updates, and delayed restarts. A successful enqueue or admission acknowledgement is not a completed model turn.

Desktop live delivery (`desktop-continuous`) and recovery delivery (`web-remote-replayable`) retain distinct semantics against the same backend owner. GPUI does not create a second scheduler, task metadata owner, remote registry, or backend fork. Account-provider headers, official MCP authentication, browser execution, and host scheduling remain unavailable or explicitly failed fast. Direct API-key providers are the supported model-auth path.

### Source layout

| Slice                            | Responsibility                                                                            |
| -------------------------------- | ----------------------------------------------------------------------------------------- |
| `app/`                           | Root view/state, Settings/navigation, dock and supporting panes                           |
| `backend/`                       | Process adapters, wire assembly, routing, request/ACK correlation and submission recovery |
| `conversation/`                  | Mirrored state, interactions, queue, plans and message actions                            |
| `transcript/`                    | Virtualized list and row rendering                                                        |
| `composer/`                      | Input, attachment tray, mentions and model/mode menus                                     |
| `sessions/`                      | Project/session sidebar                                                                   |
| `review/`, `files/`, `terminal/` | Local tools and views                                                                     |
| `shared/`                        | Preferences, path resolution, theme/i18n, platform adapters and render utilities          |

Slices extend `RootView` and `AppState`. Rust source/test files are capped at 400 lines by `tests/source_limits.rs`. The root TypeScript architecture policy marks this app unmanaged; its successful check is not a Rust ownership audit.

## Backend resolution

1. `ZCODE_GPUI_AGENT_PROGRAM` with optional `ZCODE_GPUI_AGENT_ARGS`. A JSON array of strings is the canonical args format; legacy whitespace splitting is retained. Defaults include `app-server --stdio --surface desktop`.
2. Bun plus this branch's CLI source, offered only when its workspace dependencies are installed and built. Bun is found through `BUN_INSTALL`, `PATH`, then `~/.bun/bin`.
3. Installed ZCode Desktop runtime with `ELECTRON_RUN_AS_NODE=1`.

Desktop-exported `GLM_BINARY_PATH` and `ZCODE_AGENT_SERVER_COMMAND` are deliberately not launcher overrides. Children use `env_clear()` and an explicit allowlist, including resolved roots, proxies, supported provider/ripgrep overrides, and custom CA variables.

### Bun source setup

The branch's root `package.json` currently declares `bun@1.4.2`. Source mode uses the existing runtime portability change that probes `node:sea` instead of statically importing it.

```sh
bun install --frozen-lockfile --ignore-scripts
# Add --backend=copyfile where ReFS hard links are unavailable.
for p in contracts dynamic-workflow adapters shared-types core dynamic-workflow-runtime i18n telemetry bootstrap tui; do
  (cd apps/zcode-cli/packages/$p && bun run --bun build)
done
```

Installer-only bundled plugins may be absent in source mode. The CLI single-file build script still relies on `tsx`; source mode does not require that bundle. No installer/sidecar distribution is supplied by this Settings slice.

## Validation

```sh
cargo fmt --manifest-path apps/zcode-gpui/Cargo.toml --check
cargo clippy --manifest-path apps/zcode-gpui/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path apps/zcode-gpui/Cargo.toml
bun apps/zcode-gpui/scripts/backend_smoke.ts
```

The backend smoke needs built CLI dependencies. It creates isolated temporary roots and a loopback mock API-key provider, checks CLI-owned draft/session slash catalogs, disabled custom filtering and actual `/init` prompt expansion, chunked image upload/idempotence/abort and artifact resolution into a vision-capable model request, one-shot idle startNow admission, deterministic busy queue admission (including frozen plan/mode intent) and held-queue missing/stale refusal plus Keep/Clear results, strict runtime-preference requests in both scopes with Memory enabled, actual assistant frames in both delivery modes, a new completed recovery snapshot, source lifecycle and plugin Describe, scoped typed config persistence, secret omission/preservation/explicit clear, both reset semantics, saved-run completion/history, published markdown-artifact metadata/content, cross-parent content-read refusal and unknown-run empty results, metadata replacement, Move/refusal and Delete of its own disposable fixtures, then cleans up. It does not use real credentials or production session data. It is a CLI/protocol smoke, **not a full GPUI-executable-to-rendered-transcript test**.

GPUI test windows exercise Settings keyboard/QuickPick entry, measured-bound pointer clicks, shortcut recording, draft/focus preservation, preference failures/ordering, and streamed state while Settings is open. Native platform rendering/visual acceptance and an actual executable restart remain unverified. Current results and remaining gaps are recorded in `SETTINGS.md`.

## Troubleshooting

- `no backend`: Check that the directory exists and a launch candidate has built dependencies.
- `model_request_failed`: Configure/select an API-key provider through the existing primary application. Native provider configuration is not yet implemented.
- Preference refusal while Electron is running: Change settings there, or close it before using GPUI's preference writer.
- `ZCODE_GPUI_LOG_STDOUT=1`: Emits frontend diagnostic logs to stderr. Diagnostics must remain redacted.
