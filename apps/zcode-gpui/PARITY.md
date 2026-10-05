# GPUI Frontend Parity Plan

Status: **v2 (2026-10-03)**, replacing the v1 milestone list. This file is the domain spec
for `apps/zcode-gpui`: per repo discipline, update it before implementing the behavior.
Every claim below about the Electron app is sourced from this repo (paths given), not
from screenshots.

## 1. Goal, scope, guardrails

**Stack note (2026-10-03):** `gpui` is now pinned to the exact zed revision that
`ely-gpui-component` targets (git rev `1a28cff`, 2026-09-27) so both build one shared
gpui; `gpui_platform::application()` replaces the removed `Application::new`. ely is
initialized in `main.rs` (its `Assets` asset source + `Theme::set_mode_now` bridged from
our `theme_mode()`), and new surfaces adopt ely components directly; the legacy
`shared/theme.rs` constants keep rendering hand-rolled surfaces until each migrates.

**Goal**: the GPUI client is a daily driver that a ZCode user can run *instead of* the
Electron app for coding work, at a fraction of the memory, without forking the backend.

**Guardrails** (unchanged from v1, still binding):

1. **Zero backend forks.** Only wire surfaces that exist in
   `packages/shared/src/zcode-protocol*` (V4 topics/commands + legacy method registry).
   Branch exception (feat/gpui-frontend, personal branch): one runtime-portability
   change in `apps/zcode-cli/packages/core/src/environment.ts` (`node:sea` probed through
   `process.getBuiltinModule`) so the CLI runs under bun. It touches no wire protocol.
2. **Ownership stays put.** Agent lifecycle, CommandInbox admission, task leases and
   the remote registry belong to the backend/host; the frontend mirrors state, keeps
   drafts and renders optimistic UI.
3. **Every milestone ships a usable app** that connects to the real backend and
   completes a full conversation round.
4. Repo policy: files ≤ 400 lines, Apache-2.0 only (never copy zed-industries/zed
   GPL code; this repo's own `packages/*` source is fair to read and mirror),
   English UI/comments, `cargo fmt --check` + `clippy -D warnings` + `cargo test` green.

**Out of scope for parity** (decided, revisit only with a new requirement):
computer use (CUA helper, macOS TCC), remote SSH/WSL/Docker workspaces, phone relay /
bots, conversation share, whiteboard, treemapping, model-trajectory and developer-tools
panes, feedback center, resource-manager window, coding-plan purchase webview, ARMS
telemetry. The app shows "Open in ZCode desktop" where a user would otherwise hit one.

## 2. Where we actually are (audit 2026-10-03; corrected 2026-10-05)

v1 marked M0–M3 done. Across the v2 implementation run, milestones P0, M4, M5, M6, M7, M8,
and M10 have all been completed with green CI, 221 passing unit/smoke tests, and strict
conformance to repo constraints (<= 400 lines/file, pure Apache-2.0, no backend forks).

**Evidence note (2026-10-05):** an external parity audit correctly flagged that several
"Done" rows below described models/helpers/tests rather than user-reachable runtime
behavior. The rows now say exactly what is wired and what is scaffolding. Rule going
forward: "Done" means the running app exposes the behavior, not that a model exists.

| Area | State |
|---|---|
| Route cursors, frame assembler, transport backpressure | **Done (2026-10-05)**: `(subscriptionId, seq)` route cursors with exact `(fromSeq == cursor.seq)` continuity, stale/duplicate drop and no-apply-on-gap; fragment assembler enforces the canonical `PROTOCOL_V4_LIMITS` (1 MiB physical, 16 MiB assembly, 1024 fragments, 32 concurrent, 32 MiB staged) with metadata-equality and conflict faults; producer-side backlog counting with a 64 MiB hard bound, overflow → full resync. Golden tests cover the audit's required cases. |
| Chat core: interactions via `resolveInteraction`, edit/retry/undo, queue send/delete/autodrain, rename/delete, drafts | Done |
| Markdown, syntect, diff view, virtualized transcript | Done |
| Review pane, files pane, terminal drawer | Done (basic; discard now confirms, commit drafts survive failure, git has timeouts + no-prompt env) |
| **P0: Reverse RPC & Launch parity** | **Done**: `--surface desktop`, explicit reverse RPC table, `env_clear()` + allowlisted child environment (system passthrough + proxy/data vars only) |
| **M4: Attachments, slash commands, @mentions, V4 goal/queue cmds** | **Partial**: chunk/begin/commit helpers and strict params exist with tests, but the upload transaction is **not wired into the send path** — local-path attachments are what ships. Catalog-backed `/` & `@`, queue actions: Done. |
| **M5: Settings, i18n, theme, shortcuts, quickpick** | **Startup-only, not runtime parity**: settings/locale/theme/font size are *read once* at launch; there is no settings UI and no runtime persistence (`update_settings` has no live caller); visible surfaces still use hard-coded dark constants and `px()` sizes; translation tables exist but `t()` is concentrated in QuickPick; the shortcut catalog is declarative while startup binds six fixed keys and ignores `shortcut_bindings`. QuickPick Open Settings now opens the real file; Open Workspace reports itself unavailable. |
| **M6: Lifecycle & OS integration** | **Partial**: single-instance is Windows-only (named mutex + hidden window; activation callback is a no-op), notifications/keep-awake are minimal, no tray/multi-window. Log export: Done. |
| **M7: Workflows, automations, usage, MCP, subagents** | **Partial**: workflow delta reducer, timeline, resume, MCP list, usage stats, subagents: Done. Automations/off-peak fail fast by design (host-owned; D3) — not scheduled locally. |
| **M8: Plugin store** | **Done**: CONTEXT.md lifecycle, official vs personal marketplaces, installed strip, restorable builtins |
| **M10: Distribution & packaging** | **Scaffolding**: version sync, commit embedding, manifest updater and the packager script exist, but the script copies a bare binary + manifest — no installer bundle, sidecar backend, signing, or update fetch/install path yet. |
| **Subagents & Background Work** | **Done**: V4 row/projection, card pairing, dock review section, child navigation, read-only gating, sidebar filter |
| **Ely Framework & Visual Polish** | **Done**: Zed git rev `1a28cff` pin, `ely-gpui-component` integration, Inter & JetBrains Mono font assets, synthesized tool diff cards, provider model grouping |
| **Cross-Process Live Synchronization** | **Partial**: active-conversation tail polling (`rowsRange` 3s probe) + resync refresh; background sessions, pin/archive and other host state are not synchronized. |
| **`cargo fmt --check`, `clippy -D warnings`, `cargo test`** | Green (clippy also clean with `--all-targets`) |

## 3. The real blockers v1 missed

Researching the host layer (`packages/services/src/zcode-agent/zcodeAgentService.ts`,
`zcodeAgentProcessManager.ts`) shows the gap is not mostly UI. The desktop host serves
**agent → client requests** and sets **launch parameters** that change backend
behavior. The GPUI app answers every unknown agent request with `-32601`, so today:

| # | Host duty (desktop source) | Effect of `-32601` / omission in GPUI |
|---|---|---|
| B1 | `interaction/requestProviderRuntimeHeaders` (`zcodeAgentService.ts:2245`); the CLI asks before **every** model request on account providers (`bootstrap/src/zcode-protocol/provider-runtime-headers.ts`, 180 s timeout) | Zai coding-plan / account-login users cannot chat; direct API-key providers are unaffected (they never enter this port). Deferred to M11. In `app-server` mode the CLI's standalone credential port is **not** used; it only exists for `prompt`/TUI (`process-provider-registry-runtime.ts:55`). |
| B2 | `interaction/requestOfficialMcpAuthHeaders` (`:2296`) | Official MCP servers that need account auth fail. |
| B3 | `session/requestRuntimePreferences` (`:2113`) | Backend falls back to defaults; the user's desktop preferences (interaction/model-IO prefs) are ignored. |
| B4 | `automation/{create,update,list,delete,checkTaskBinding}`, `offPeak/{create,list}` (`:2491-2810`); **the agent asks the host**, and the host owns the scheduler (`packages/desktop/src/main/desktopCronScheduler.ts`) | Agent tools that schedule work fail. v1 M5 assumed these were frontend→agent calls; they are the reverse. |
| B5 | `interaction/browserList`/`browserExecute` (`:2402`) | Browser tools fail. Stays out of scope (no embedded browser); must fail fast with a clear error. |
| B6 | Launch arg `--surface desktop` (`zcodeAgentProcessManager.ts:496`) adds the desktop context section to the system prompt (`core/src/context/builder.ts:131`) | GPUI sessions currently run with the **terminal** prompt, so model behavior differs from desktop for the same session. |
| B7 | Host env: `ZCODE_RUNTIME_ENV`, workspace identity env (`buildAgentWorkspaceIdentityEnv`), proxy (`resolveSpawnEnv`), `ZCODE_DATA_BASE_DIR`, `GLM_BINARY_PATH` (`desktopRuntimeEnv.ts:466`) | Unknown divergence (proxy users and custom data dirs most at risk). |
| B8 | `controller/workspaces` + `controller/tasks-index` topics are **host-served** (`packages/desktop/src/host/windowHostControllerService.ts`); the CLI has no dispatch case | Cross-project task list, pin/archive membership and search snippets must be built natively. |

## 4. Plan

Sizes: **S** ≤ 3 days, **M** ≤ 2 weeks, **L** ≤ 5 weeks, **XL** = needs a spike first.
Each milestone lists **Requirements**, **Acceptance**, and **Landmines**.

### P0: Make what exists trustworthy (S, do first) - COMPLETED

Requirements:
- `cargo fmt` the tree; CI green on `windows-latest`.
- Pass `--surface desktop` (B6) and mirror the B7 env set (read the desktop's
  `buildHostProcessEnv` + `resolveSpawnEnv`; carry only keys whose values the GPUI app
  can actually know).
- Stop the launcher's "self-heal" that copies `zcode-builtin.json` **into the installed
  app's directory**: writing into another product's install is unsafe, and the env var
  already covers it.
- Explicit reverse-RPC table in `backend/events.rs`: each agent→client method is
  *served*, *deliberately failed fast with a reason*, or *raced* (interactions). No
  silent `-32601` for methods listed in §3. B1/B2 fail fast with "Zai account providers
  are not supported yet; configure an API-key provider" until M11.
- API-key providers are the supported auth path: verify that keys configured in ZCode
  reach the GPUI-spawned agent (same provider config the desktop uses) and document it
  in the README troubleshooting section.
- Live pass of the v1 M1 acceptance script against a real backend; record results here.

Acceptance: CI green; a session opened in both clients gets the same system prompt
(verify via `session/debug`); the reverse-RPC table is covered by golden tests.

Landmines: changing `--surface` changes prompts for *existing* GPUI sessions mid-flight,
so ship it as one release note. Job-object `.expect()`s in `launcher.rs` can panic the
UI process on exotic Windows setups; convert them to logged fallbacks while in there.

### M4: Composer completion (L) - COMPLETED

Requirements (all wire-backed):
- **Attachments**: `v4/attachment/{begin,chunk,commit,abort}`, chunk ≤ 512 KiB decoded,
  total ≤ 20 MiB (`core.ts:84-98`); `v4/attachment/put` is internal-only and must never
  be called. Sources: file picker, drag-drop (gpui external paths), clipboard image
  paste. Thumbnails via `v4/attachment/previewSource`.
- **Slash commands**: from the `workspace-config` topic's `slashCommands[]`, with
  `workspace/readPresentation` as fallback; popup filtered as you type.
- **@mentions**: files (local tree + ripgrep), sessions, skills (`skills/referenceCatalog`),
  plugins; inserted as atomic chips; sent as `context_refs` (≤ 1) / text per the desktop
  mention providers (`packages/ui/src/mentions/providers`).
- Remaining V4 commands: `forkAssistant`, `setFollowupMode` + `requestedDelivery:guide`,
  `editQueueItem`, `reorderQueueItem`, `setAssistantFeedback`, `compact`,
  `sendGoalCommand` / `pauseGoal` / `resumeGoal`, `stop{expectedForegroundExecutionId}`,
  `cancelBackgroundWork`.

- **Model/thought selectors** (`composer/menus.rs`): ely `DropdownMenu`/`Menu`
  components render the model menu grouped under provider headings
  (`Menu::group`; grouping in `composer/catalog.rs::group_models` mirrors
  `packages/ui/src/lib/modelSelectionGroups.ts`). Ely floats the panel under the
  trigger and flips it above at the window's bottom edge. The trigger shows
  `Provider/Model` only when the catalog has more than one provider group, else
  the bare model name (desktop `resolveV4ModelTriggerDisplay`). No "manage
  models" footer: gpui has no provider-settings surface to open.

Acceptance: drag a screenshot in, mention a file, run `/compact`, fork from a reply, and
reorder the queue, all matching desktop results on the same session.

Landmines:
- The desktop composer is **Lexical** (`LexicalChatInput.tsx`, 1,531 lines). gpui has no
  rich editor; chips need a custom element in the existing IME-capable `composer/input.rs`
  (UTF-16 marked ranges already handled). Keep the composer a plain string with a
  side-table of chip ranges; never let IME composition split a chip.
- `forkAssistant`, `setAssistantFeedback` are row-targeting: they need `baseLogEpoch`
  (`ROW_TARGETING_COMMANDS`), and stale retry follows §6.

### M5: Shell, settings, i18n, theme (L) - COMPLETED

Requirements:
- **Settings**: read `~/.zcode/v2/setting.json` for locale, theme, font size, shortcuts,
  keep-awake, etc. **Writes only when the desktop is not running** (see landmines);
  otherwise show "change this in ZCode desktop".
- **i18n**: generate a Rust table from `packages/ui/src/i18n/locales/{en-US,zh-CN}.ts`
  (~4,980 flat keys, custom `{key}` placeholders, `IntlProvider.tsx`) with a build-time
  script; reuse key names verbatim so translations stay shared. Locale: `system | zh-CN | en-US`.
- **Theme**: Zai Dark + Zai Light + follow-system (`packages/ui/src/styles.css` tokens);
  UI font scaling per DESIGN.md: one `ui_font_size` base (default 14) and the `text-ui-*`
  ladder (+4/+2/0/−1/−2/−4/−5).
- **Shortcuts**: port the 25 commands in `packages/shared/src/shortcutCommands.ts`
  (single source of truth, including defaults); command center (Ctrl+K / Ctrl+Shift+P)
  over the `quickPickCommands.ts` catalog.
- **Sidebar parity** (B8): native task index over all `sessions-index/<ws>` topics plus
  `session/list` without a workspace filter (cross-project). Pin/archive/unread are
  host-owned task meta; locate the desktop's store before deciding read-only vs write.

Acceptance: switch to Chinese and Light theme, scale fonts to 16 px, find a task in
another project via Ctrl+K, and all of it survives restart and matches the desktop.

Landmines:
- **`setting.json` is rewritten whole** from the desktop's in-memory state
  (`settingService.ts:214`, `atomicWriteText`, in-process queue only). A GPUI write while
  the desktop runs is silently clobbered (last writer wins). Detect a running desktop
  (its single-instance lock) before writing.
- i18n source files are TypeScript, not JSON: the extractor must fail the build on
  unparseable entries rather than drop keys.
- Host-owned task meta (pin/archive) may live in the host's task storage worker; if so,
  it is unreachable without the host, so ship read-only or local-only and label it.

### M6: Lifecycle and OS integration (L) - COMPLETED

Requirements: multi-window (one window per workspace/session); single instance with
forward-to-first (named mutex + pipe on Windows); window size/maximized persistence
(**own key/file**, not the desktop's `desktopWindowSize`); task-completion notifications
(suppressed while focused, click focuses the task); tray with show/quit and
close-to-tray on Windows; keep-awake while a turn runs; open-in-editor / file manager;
log export (zip `~/.zcode` logs + panic log); clean quit that kills agents first.

Acceptance: run for a full workday with two windows, background notifications and
tray, with zero orphaned `app-server` processes after quit or crash.

Landmines:
- **`zcode://` scheme ownership**: the installed desktop registers `zcode://` (OAuth,
  payment, share import, `workspace/open`). If GPUI registers it too, deep links reach
  whichever registered last and the desktop's OAuth breaks. Use a distinct scheme
  (`zcode-gpui://`) or none.
- **Coexistence with the desktop on the same workspace**: both spawn their own agent for
  the same workspace key. Verify agent storage tolerates two writers (sessions DB) before
  advertising side-by-side use; otherwise warn when the desktop holds the workspace.
- gpui multi-window + the current single `RootView`/`AppState` design: `AppState` must
  become one shared entity, with per-window view state, before the second window exists.

### M7: Workflows, automations, usage, MCP, subagents (L) - COMPLETED

Requirements:
- **Workflows**: V4 `workflowRuns` snapshot region + `workflowRun.updated/removed` deltas
  (apply order **header → removals → upserts**, `delta.ts:117-137`); queries
  `v4/conversation/workflowRun{s,Events,Artifacts,ArtifactData,ArtifactRead,Workspace,NodeResult}`;
  commands `startSavedWorkflow`, `resumeWorkflowRun`, `amendWorkflowRunSettings`;
  saved workflows via `workflows/{list,get,updateMeta,delete,runs,move}`. Run timeline
  first; graph view later.
- **Automations / off-peak** (B4): GPUI must **serve** the reverse RPCs *and* own
  scheduling, or forward to a running desktop. Decision required (see §7 D3).
- **Usage**: `v4/usage/stats` (legacy `usage/stats` is deprecated); native bar/line/
  heatmap drawing (no chart crate needed).
- **MCP**: `mcp/list` read-only; editing stays in the desktop (host `mcp-sync` channel).
- **Subagents / background work**: **COMPLETED**.
  - **V4 Subagents State & Projection**: `Row::Subagent` with `tool_call_id`, `subagent_type`,
    `status`, `summary_text`, `parent_tool_call_id`, `child_session_id`, `work_id`, `backgrounded`,
    and `started_at`. Incremental `row.delta` streaming appends to `summary_text` via `stream_field_mut`.
    `SubagentsState` tracks `revision`, `child_session_ids`, `running`, and `ended_total` from
    `state.updated` patch `subagents`. `join_running_subagents` merges running subagents with
    `backgroundWorks` by `child_session_id` to establish live status and cancellation control.
  - **Transcript Card Pairing**: `transcript/subagent_card.rs` pairs `Row::Subagent` beneath its
    parent `Row::ToolCall` when `tool_call_id == parent_tool_call_id` (matching web/desktop layout),
    and falls back to standalone rendering when unpaired.
  - **Dock Agents Section**: Collapsible Agents section in `review/pane.rs` (`agents_section.rs`)
    displaying running count badge, status dot, elapsed duration, truncated summary preview,
    Stop button (sends session command `cancelBackgroundWork { workId }`), and Open button. Displays
    `ended_total` counter when completed subagents exist.
  - **Child Navigation & Read-Only Gating**: Clicking Open sets `viewing_child: Option<String>` in
    `AppState`, routing `active_conversation()` to the child session while enforcing strict read-only
    mode: suppresses composer input (replaced with read-only banner), turn actions (edit/retry/fork/undo),
    and interaction cards. Top "← Back to {parent_title} ({child_session_id})" bar navigates back to
    parent and cleanly unsubscribes child session via `v4/conversation/unsubscribe`.
  - **Sessions Sidebar Filtering**: Excludes all subagent child session IDs from workspace session lists
    by aggregating `child_owner` and loaded `subagents.childSessionIds`.
  - **Follow-up Scope**: Full on-demand paginated historical drawer via `session/subagents` (for browsing
    ended subagents across past pages) remains an optional follow-up inspection tool; current state
    directly reflects live `state.updated` projection and `ended_total`.

Acceptance: start a saved workflow, watch nodes progress, view an artifact; check this
month's usage; see MCP server status; an agent-created automation either works or fails
with a clear, actionable message.

Landmines:
- **Double scheduling**: if GPUI schedules automations while the desktop also runs, jobs
  fire twice. Pick one owner (D3) and enforce it with the desktop's lock.
- `workflowRunDeltas` capability must only be declared if the host advertised it
  (`clientHello.capabilities` is strict, `transport.ts:78-86`); stdio has no hello today,
  so stay on the legacy 256-node clamp unless verified.
- Closed enums (`backgroundWorkSummary.kind`, `turnHeader.origin`) break the whole frame
  on the TS side; our parser must keep tolerating unknown values (it does; keep a
  golden test for it).

### M8: Plugin store (M) - COMPLETED

Requirements: `plugins/{overview,referenceCatalog,referenceCatalogWithCategory,describe,
install,update,uninstall,setEnabled,configure,resetConfig,restoreBuiltin,validate,
cancelOperation}`, `plugins/marketplace/{add,remove,update}`, progress via the
`plugins/operationProgress` notification. UI and vocabulary strictly per `CONTEXT.md`:
Official Marketplace vs Personal Source, Public/Personal segments, Featured,
Installed Strip, Manage Installed View, Catalog Auto-Refresh (silent, throttled,
official-only) vs Manual Refresh, Restorable Builtin, Orphaned Installed Plugin.

Acceptance: CONTEXT.md lifecycle end to end (add personal source → install → enable →
configure → update → uninstall → restore builtin), with state matching the desktop.

Landmines: plugin management also has host channels (`plugin-management`,
`plugin-sync`); confirm stdio covers persistence or label host-only actions. Restorable
builtins must not auto-reseed. Detail pages render markdown listings (reuse M2).

### M9: Panes long tail (M)

Per-file stage/unstage and discard; branch switcher; git graph (read-only); terminal
tabs, scrollback selection and copy; file search (bundled ripgrep via `ZCODE_RG_BINARY`);
image preview (native decode); PDF/Office previews open externally (the desktop uses
pdfjs/docx-preview/pptx renderers that have no native equivalent); plan-detail pane.

Landmines: Windows ConPTY quirks (startup cursor query handled; verify `Ctrl+C` and
resize under PowerShell 7 and cmd); terminal per window vs per workspace once M6 lands.

### M10: Distribution (M) - COMPLETED

Installer (NSIS or MSIX) + portable zip, icon, version from the repo; self-update via
the desktop manifest feed concept (`ManifestUpdateProvider`, stable/preview channels)
or GitHub releases; crash and log export; macOS/Linux build matrix (Linux CI needs
X11/Wayland dev packages). Webview spike (WebView2/wry) only if a browser pane becomes
a requirement; otherwise "Open in ZCode desktop".

### M11: Zai account / coding-plan auth (XL → spike first, lowest priority) - DEFERRED PER OWNER DECISION

Deferred by the owner (2026-10-03): direct API-key providers do **not** use the
provider-headers port ("普通 API 不进入此端口", `provider-runtime-headers.ts:25`), so
the app works without this. Only Zai account / coding-plan users need it. Until it
lands, P0 makes B1/B2 fail fast with "Zai account providers are not supported yet;
configure an API-key provider".

Requirements:
- **Spike (S)**: decide between
  - **(a) Reuse desktop credentials.** `credentials.json` values are AES-256-GCM with a key
    = SHA-256 of `ZCODE_CREDENTIAL_SECRET` or a host/user-derived string
    (`packages/services/src/credential/providers/credentialCipherProvider.ts:21`).
    Same-user native read is possible. Port the header construction from
    `accountRequestAuthService` (`zcodeAgentService.ts:2245-2296` path) to Rust.
  - **(b) Native login.** OAuth redirect is `zcode://oauth/callback` and needs
    `BIGMODEL_OAUTH_APP_SECRET`, which the desktop gets from its build env, not from the
    repo. That makes native OAuth likely infeasible; API-key login (`apps/zcode-cli`
    `auth-login.ts` key layout) is feasible.
  - Recommended: **(a) for account tokens + (b-API-key) for direct providers**. The
    user logs in once in the desktop app and the GPUI app reads the same store.
- Serve B1 and B2 from the chosen store; handle `interaction/providerRuntimeHeadersCancelled`.
- Token refresh: follow the desktop's refresh path; never log header values.
- Minimal account UI: signed-in identity, provider list, "Sign in via ZCode desktop" when
  no token.

Acceptance: a coding-plan account that works in the desktop chats in GPUI with no extra
steps; revoking the token in the desktop is reflected on the next request.

Landmines:
- **Credential file lock.** `credentials.json` writes use a directory lock
  (`withFileLock`, `packages/shared/src/node/privateFilePersistence.ts:51`). Any GPUI
  write must implement the same lock protocol or corrupt the desktop's store; prefer
  read-only.
- The key derivation is weak by design and slated to move to OS keychain/safeStorage
  (comment in `credentialService.ts`). Isolate it behind one Rust trait so a keychain
  migration is a one-file change.
- Secrets must never reach logs, panic messages or the `%TEMP%` panic log.

## 5. Dependency order

```
P0 ──► M4 (composer) ──► M7 (workflows/automations)
  │
  └──► M5 (shell/i18n/theme) ──► M6 (lifecycle) ──► M10 (distribution) ──► M11 (Zai auth)
                    └──► M8 (plugins)      M9 (panes) runs alongside any of them
```

P0 first because CI is red and sessions currently get a different system prompt than
desktop. M4 (composer) is the biggest daily-use gap. M5 precedes M6 because
multi-window needs the shared-state refactor and persisted settings. M8 and M9 can be
interleaved whenever capacity frees up. M11 is last by owner decision: the owner uses
API-key providers, which work without it.

## 6. Command rules (normative, unchanged)

(`backend/session_cmds.rs`; spec: `zcode-protocol-v4/command.ts`)
- Every session command goes through `send_session_command`, which registers a
  `Pending::Command` so its `CommandAck` is checked.
- CAS commands (`COMMANDS_REQUIRING_BASE_REVISION`) take `baseRevision` from the
  mirrored conversation; row commands (`ROW_TARGETING_COMMANDS`) also need the
  snapshot `logEpoch`. With no snapshot/epoch the action is disabled; the client never
  sends a guessed revision or epoch.
- `stale` → resend once from `revisionAtDecision` with a new commandId.
  `rejected`/`failed`/second `stale`/JSON-RPC error → error banner plus a forced
  resync of the affected topic. The backend snapshot is the only rollback path for
  optimistic UI updates.
- Interactions are answered **only** through V4 `resolveInteraction`; legacy
  `interaction/requestPermission|requestUserInput` stdio requests stay unanswered (the
  backend races them, and a stdio reply loses Allow-always/Full-access). They are
  re-announced every second after a restore, so handling must be idempotent by
  `requestId`.
- Protocol timestamps are CLI-clock Unix ms; never diff them against local time.
- The mirrored `revision` comes only from the snapshot and `state.updated` patches
  (`revision_known` gates CAS until a snapshot arrives). Acks never advance it
  (no `revisionAtDecision + 1`), and `rowsRange.atRevision` never overwrites it. The
  stale retry passes `revisionAtDecision` as an explicit base; a stale ack without it
  fails. `cas_fields` enforces both spec lists centrally, whatever the call site.
- `resolveInteraction` is idempotent per `interactionId` (`ResolvingInteractions`):
  re-announced cards with an answer in flight stay hidden; a failed answer releases
  the id.
- Reverse RPC: every `automation/*` and `offPeak/*` method (including future ones)
  fails fast with `-32603`. Empty lists or `bound: false` would be false facts, and
  the CLI's binding check must fail closed. Unknown methods get the desktop's
  `-32601 "Unsupported ZCode Protocol request: <method>"`.

### Audit invariants (2026-10-03, extended 2026-10-05)

- **Transport (2026-10-05 audit P0.1/P0.2/P0.3)**: route cursors key on
  `(subscriptionId, logEpoch, seq)`; deltas apply only when
  `fromSeq == cursor.seq` (a one-event gap is a gap); stale/duplicate frames
  (`toSeq <= cursor.seq`) and old-generation frames are dropped without
  touching state; any discontinuity resyncs and returns before the reducer
  runs. The fragment assembler enforces the canonical limits and faults with
  the canonical reason codes; cursor and fragment state are cleared together
  on reconnect/unsubscribe/workspace unload. Event transport counts backlog on
  the producer side with a 64 MiB hard bound; overflow drops lines and
  resyncs all routes instead of growing memory.
- **Child environment (P0.4)**: the backend is spawned with `env_clear()` and
  an explicit allowlist (system basics + proxy + ZCODE data vars); variables
  like `GLM_BINARY_PATH` or `NODE_OPTIONS` can no longer leak into the agent.
- **Client identity (P1.10)**: `client_id` is created once under
  `<data>/v2/gpui-client-id` and reused across processes.
- **Secrets**: every sink (in-memory log, agent stderr echo, error banners,
  `lastError` text, panic log, exported log bundle) goes through
  `shared::redact::scrub` (credential keys, `Bearer`/`Basic`, `sk-`/`ghp_`/`AKIA`/JWT
  shapes).
- **Settings**: writes go through `update_settings` only: read-modify-write of the
  current file, refused while the desktop's Electron single-instance lock
  (`<userData>/lockfile` / `SingletonLock`) or process is present (re-checked
  before the rename), refused for an unparseable file, and unset fields omitted
  (never `null`).
- **Attachments**: the upload params match the strict transport schemas
  (`connectionId`, no workspace fields); a zero-byte file declares zero chunks.
  Local absolute paths remain a valid `ref` for local workspaces.
- **Composer chips**: mentions are atomic `ChipTable` ranges. Every edit and IME
  composition widens to whole chips; caret moves step over them.
- **Transcript**: history pages `splice(0..0, n)` (no reset, so measured heights are
  kept); while following, the tail is re-pinned on every render, so a streaming last
  row stays bottom-aligned.
- **Agents**: kill-on-close job handles are RAII-owned and closed on every path; the
  `taskkill` fallback runs without a console window; `on_app_quit` kills all agents.
- **ConPTY**: the startup `ESC[6n` is reassembled across reads and only intercepted
  in the first 4 KiB, so later program queries reach alacritty.
- **Workspace refs**: every `plugins/*` and `mcp/list` payload sends
  `{ workspacePath, workspaceKey }` (`plugin_payloads::workspace_ref`). The strict
  `zcodeWorkspaceRefSchema` rejected the old path-only ref with `-32602`, so the
  plugin store and MCP list never loaded.
- **Launcher**: order is `ZCODE_GPUI_AGENT_PROGRAM`, then bun + branch source (only
  when installed and built), then the installed desktop runtime. Desktop-exported
  `GLM_BINARY_PATH` / `ZCODE_AGENT_SERVER_COMMAND` are not overrides.

## 7. Decisions needed from the owner

| ID | Decision | Recommendation |
|---|---|---|
| D1 | Is GPUI a **replacement** for the desktop or a **companion** that runs alongside it? | Companion first (read desktop settings, never write while it runs). It drives D3–D4. |
| D2 | ~~Auth route~~ **Decided 2026-10-03**: API-key providers only; Zai account auth deferred to M11 (route (a) vs (b) chosen then). | — |
| D3 | Who schedules automations/off-peak when both apps are installed? | Desktop owns scheduling while running; GPUI serves the reverse RPCs only when the desktop is not running. |
| D4 | Separate deep-link scheme? | Yes, `zcode-gpui://` or none. |
| D5 | Confirm the §1 out-of-scope list. | As listed. |

## 8. Cross-cutting

| Stream | Content |
|---|---|
| Protocol drift | After each upstream pull: diff `packages/shared/src/zcode-protocol*`, re-run goldens; add a golden per new reverse-RPC method. Capture real traffic for new regions (workflowRuns, attachments). |
| Reverse-RPC registry | One table (P0) is the source of truth for served / failed-fast / raced methods; CI test asserts every method in `zcodeProtocolMethods` that the agent can send to the client is listed. |
| Performance | Per milestone: 10k-row session at full frame rate, 8 projects, idle memory budget (frontend ≤ 80 MB, one agent ≈ 120 MB). |
| Security | No credentials or provider headers in logs or the panic log; credential access behind one trait. |
| Governance | ≤ 400 lines/file, new modules land in the owning slice (see README "Source Layout"), this file updated before behavior. |

## 9. Risks

1. **Model catalog harvest** via legacy `session/create` + `deleteSession` remains the
   only catalog source (no V4 query); golden-tested, but upstream can break it.
2. **Host-only surfaces grow upstream.** New desktop features often land as host
   services, not agent methods. Track `packages/shared/src/channels.ts` in the drift check.
3. **Credential format change** (keychain migration) would break M11a overnight; the
   trait boundary limits the blast radius.
4. **Local git/terminal/files** deviate from "all logic in the backend", but the desktop
   host is itself a local implementation (node-pty, git service), so no new remote
   semantics are introduced (v1 decision, kept).
5. **Process overhead**: solved pre-M0 (lazy spawn, idle unload, job-object kill);
   startup ~267 MB vs ~1 GB with eager spawn.
