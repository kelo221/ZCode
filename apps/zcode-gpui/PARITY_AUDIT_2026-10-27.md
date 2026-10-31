# Parity Audit: GPUI client vs Electron client (2026-10-27)

Point-in-time audit against `main` (ba4105d2). Companion to the living domain spec
[PARITY.md](PARITY.md) — that file keeps the behavioral specs; this file is the
prioritized gap snapshot. Every "missing" claim was verified by searching the whole
`apps/zcode-gpui/src` tree; search terms are noted where the conclusion rests on
absence of evidence.

Priorities below are reconciled with PARITY.md's decided **out-of-scope list**
(computer use, remote SSH/WSL/Docker workspaces, phone relay/bots, conversation
share, whiteboard, treemapping, model-trajectory/developer-tools panes, feedback
center, resource-manager window, ARMS telemetry, multi-window) and with what is
inapplicable to this fork (auto-update, official account/Coding-Plan services,
embedded BrowserView — browser use goes through the extension bridge). Such items
are listed in the last section instead of being tracked as open gaps.

## TL;DR

GPUI is at parity or ahead on the core chat loop: send/queue/steer while busy,
attachments + clipboard images, @-mentions, model/thinking pickers, plan & permission
modes, permission prompts with free-text deny, markdown + syntax-highlighted code +
local images, cross-block text selection, edit/resend/fork/retry, file rewind, provider
management, plugin store + config, MCP status + OAuth, subagent overrides, shortcuts,
bilingual i18n, and TTS (kokoro Speak — Electron has no local playback at all).

The open gaps cluster in four areas: **session-list management**, **extension-resource
management UI**, **rich content rendering**, and **platform/OS integration**.

---

## P0 — Core UX, users hit these daily

| # | Gap | What's missing | Electron reference |
|---|-----|----------------|--------------------|
| 1 | Pin/unpin tasks | No pin state, intent, or menu entry on session rows (grep `pin/pinned/unpin` → only list-cap + pairing hits). PARITY.md confirms: "pin/archive/unread … are missing; do not invent frontend authority." | `packages/ui/src/project-sidebar/task-mutation.ts:64`, `useProjectPinIntent.ts` (protocol `setTaskPinned`) |
| 2 | Archive/unarchive + archive view | Zero hits for `archive` in the GPUI tree. | `task-mutation.ts:55-58`, `useArchiveManagement.ts`, `ArchiveGroupCard.tsx` |
| 3 | Unread markers | No unread dot / mark-(un)read on session rows. | `task-mutation.ts:66-73`, `useProjectUnreadSignals.ts` |
| 4 | In-conversation find (Ctrl+F) | `ShortcutCommandId::FindInTask` is declared in `shared/shortcuts.rs:19` but has **no handler and no UI** (grep `FindInTask` → only shortcuts.rs). | `packages/ui/src/quickpick/TaskFindDialog.tsx`, `quickpick/conversationFindSearch.js` |
| 5 | MCP server add / edit / delete / enable-disable | MCP pane is read-only listing + refresh (`app/mcp_pane.rs`); PARITY.md agrees: "configuration CRUD is not supplied". | `packages/ui/src/settings/McpServerForm.tsx`, `McpServerList.tsx:161`, `McpSettingsSection.tsx:1249,1308` |
| 6 | Composer draft persistence across restart | `session_drafts` / `draft_submission_overrides` are in-memory only (`app/store.rs:52-54`); Electron persists text + mode + model selection + mention. PARITY.md: "Drafts survive navigation in memory only." | `packages/ui/src/v4/composer/composerDraftStore.ts:1,344` |
| 7 | Sidebar inline search | `filter_session_indices` already supports a query but the sidebar always passes `""` (`sessions/sidebar.rs:208`); only Ctrl+K QuickPick exists. | `ProjectSidebarListHeader.tsx:32-57`, `useGlobalTaskList.ts` |
| 8 | General settings rows | General pane has only locale / font size / theme / memory. Missing: notifications + sound, close-to-tray, keep-awake toggle, HTTP proxy, terminal shell/font, data base dir. | `packages/ui/src/settingsPageHelpers.tsx:130+`, `DataBaseDirControl.tsx` |

## P1 — Management UI and rendering that power users notice

| # | Gap | What's missing | Electron reference |
|---|-----|----------------|--------------------|
| 10 | Skills management section | Skills are only invocable via `@skill` autocomplete; no list/enable/import/remote-sync/diagnostics UI. PARITY.md lists Skills as still requiring owner ports. | `packages/ui/src/settings/SkillsSection.tsx`, `RemoteSkillSyncDialog.tsx` |
| 11 | Custom slash-command CRUD | `backend/slash_catalog.rs` is read-only; no create/edit/delete for custom commands. PARITY.md: Commands section "still requires missing owner ports". | `packages/ui/src/settings/CommandsSection.tsx`, `CommandForm.tsx` |
| 12 | Hooks configuration UI | Only the `workspaceHookReview` trust card exists (`conversation/interactions.rs:52`). | `packages/ui/src/settings/HooksSection.tsx`, `HookForm.tsx` |
| 13 | Automations / scheduled tasks | Nothing (grep `automation` → one comment in `app/maintenance.rs:80`). PARITY.md: "Automations/off-peak stay host-owned and fail fast"; the dock keeps saved workflows "until an Automations Settings section exists". | `packages/ui/src/settings/AutomationsSection.tsx`, `OffPeakEditView.tsx` |
| 14 | Model metadata editing | Context window / vision / max tokens shown as read-only badges (`settings_provider_models.rs:161-169`). PARITY.md confirms not ported. | `ProviderModelMetadataDialog.tsx`, `ProviderModelMetadataFields.tsx` |
| 15 | Per-model reasoning-level editor | Zero `reasoning` hits in `apps/zcode-gpui/src/provider_settings/`. | `ProviderModelReasoningLevelEditor.tsx:89-181`, `packages/shared/src/model-config.ts` |
| 16 | Per-tool renderers (~40 tools) | Workflow / node-repl / MCP / skill / web-fetch and similar tools fall back to a generic key-values card (`transcript/tool_inputs.rs:184`). CUA renderers are not tracked — computer use is decided out of scope. | `packages/ui/src/ToolCallBlocks/renderers/` (`resolveRenderer.ts`, …) |
| 17 | Compaction display | `/compact` can be issued, but there is no compaction timeline/status/turn rendering (grep `compaction` → only token formatting). | `packages/ui/src/lib/contextCompactionRuntime.ts` |
| 18 | Cost display | Usage pane shows tokens only (`app/usage_pane.rs:154-243`); no cost anywhere (grep `\bcost\b` → one false positive). | `packages/ui/src/components/ai-elements/context.tsx:224-291`, `TaskUsageState.cost` (`zcodeSessionStoreTypes.ts:69-75`) |
| 19 | Memory viewer | GPUI Memory pane is a single on/off switch; Electron has the AGENTS.md / project-memory catalog viewer + editor. | `packages/ui/src/settings/MemorySettingsViewer.tsx` |
| 20 | Stream/display behavior toggles | No equivalents for show-reasoning, show-todos, tool grouping (Explore/Terminal/Changes), interaction behavior, askUserQuestion auto-resolution, model-IO retention, tool search mode. (GPUI's noise-free mode covers part of this, GPUI-only.) | `packages/ui/src/settingsPageHelpers.tsx` |
| 21 | Cross-platform OS notifications | Windows PowerShell toast only; the non-Windows branch is a stub (`shared/os/notifications.rs:40-42`). | `packages/desktop/src/main/desktopNotifications.ts` |
| 22 | Notification sounds + unread/taskbar badges | No audio playback (grep `sound/audio/chime` → zero); no dock badge / Windows overlay icon. | `packages/ui/src/lib/taskNotificationSound.ts`, `packages/desktop/src/main/unreadBadge.ts` |
| 23 | Permission-rule scope labels | "Allow always" options lack session/project/global scope labels — GPUI's option parser drops `permissionUpdates` (`conversation/interactions.rs:73`). | `packages/ui/src/PermissionDialog.tsx:109-142` |

## P2 — Nice-to-have parity

| # | Gap | What's missing | Electron reference |
|---|-----|----------------|--------------------|
| 24 | workspaceIdentity semantics | Workspaces keyed by canonical path only (`backend/workspace.rs:14`); no identity-keyed dedup/binding/persistence (grep `workspaceIdentity` → only subagent-profile payloads). | `packages/shared/src/remote-workspace-identity.ts`, `packages/ui/src/store/tabWorkspaceIdentity.ts:21-43`, `remoteWorkspaceSessionStore.ts:104-126` |
| 25 | Mermaid + LaTeX rendering | Zero hits for both. | `code-block.tsx` + `mermaid-block.tsx`, `@streamdown/math` (`message.tsx:14`) |
| 26 | Richer diff views | Unified text diff only (`shared/diff_view.rs:123`); no side-by-side or word-level diff. | `packages/ui/src/ui/diff-viewer.tsx`, `lightweight-diff-preview.tsx` |
| 27 | Custom task groups / timeline view / virtual projects | Sidebar has only Tasks + Projects. | `project-sidebar/grouped/`, `ProjectSidebarNavigation.tsx`, `contract.ts:48` |
| 28 | Manual ordering persistence | Projects/sessions sort by latest activity only (`sessions/items.rs:83-85`). | `projectOrder.ts`, `useProjectSidebarPreferences.ts` |
| 29 | Onboarding / first-run flow | Only the time-of-day draft greeting. | `packages/ui/src/onboarding/OnboardingDialog.tsx`, `WelcomeScreen.tsx` |
| 30 | System tray | Absent (grep `system_tray/tray_icon` → zero). PARITY.md: "tray, multi-window … remain partial". | `packages/desktop/src/main/desktopTray.ts` |
| 31 | Desktop deep links | Only the Android pair link (`zcode-gpui://pair`); no OAuth/workspace deep links. | `desktopOAuthDeepLink.ts:412`, `packages/desktop/src/main/index.ts:1988-2014` |
| 32 | QR pairing for phone | Pairing is one-time link/token only (grep `qr` in `remote/` → zero). | `WebRemoteControlDialog.tsx:266` |
| 33 | Data export/import + archives (settings) | No migration / archive management / auto-archive settings (grep `migration` → zero). | `MigrationSection.tsx`, `SettingsPage.tsx:2163-2174` |
| 34 | Code-preview appearance settings | No theme / line-numbers / wrap / font options. | `packages/ui/src/settingsCodePreview.tsx:81-268` |
| 35 | Usage stats per-provider tabs | Usage pane lacks provider / coding-plan breakdowns. | `UsageStatsSection.tsx` + `usage-stats/` |
| 36 | Workspace tabs | Multiple projects per window via sidebar, but no tab strip or tab persistence. | `packages/ui/src/store/tabStore.ts:30-76`, `useTabPersistence.ts` |
| 37 | Transcript loading skeletons | Panes have spinners; chat has no skeleton/shimmer. | `ai-elements/chat-loading.tsx`, `shimmer.tsx` |

## Out of scope — not tracked (decided in PARITY.md §1, or fork-inapplicable)

Decided in PARITY.md §1 (revisit only with a new requirement): conversation share
links, remote SSH/WSL/Docker workspaces + connection registry, relay-based phone
remote / desktop-continuous (the LAN-only replacement is the plan,
`REMOTE_CONTROL.md`), computer use (CUA), model-trajectory / developer-tools panes,
feedback center, resource-manager window, whiteboard, treemapping, multi-window
(single window is the current design).

Fork-inapplicable (this fork is built from the GitHub repo, not official
distribution/services — do not port):

- **Auto-update** — the official manifest updater points at official releases;
  `shared/os/updater.rs` stays dead code. Updates come from this repo's releases.
- **Coding Plan / StartPlan panels, account OAuth login** — official z.ai billing and
  account services; the fork manages its own providers with API keys.
- **ARMS crash reporting / telemetry** — the panic log file (`lib.rs:97-99`) is enough.
- **Embedded browser view (Electron BrowserView)** — nothing to port: browser use runs
  through the user's own browser via the extension bridge (`browser_bridge/`,
  `BROWSER_BRIDGE.md`), already at parity including Settings › Browser Use
  (`app/settings_browser.rs`).

## Known non-gaps (do not port)

- **Speech-to-text**: Electron's `speech-input.tsx` is never mounted anywhere — dead code.
- **TTS playback**: GPUI is *ahead* — full kokoro sidecar with settings, gating, and
  turn-ending rules (`voice/`, spec `VOICE.md`); Electron has no local playback.

## Verified at parity (highlights)

Markdown core (headings/lists/tables/code + syntect highlighting + copy button),
local markdown images in replies (`shared/markdown/images.rs`, `transcript/chat.rs:67-88`),
reasoning blocks, cross-block text selection (`shared/selectable.rs`), message
edit/resend (`editUserQuery`), retry/fork/like/dislike, queue editing + held-queue
choices, steering (modifier-enter), file drag-drop into composer, paste images,
full terminal pane, error banners, keep-awake, single-instance, permission prompts,
plan mode + permission modes, file rewind, provider management, plugin store/config,
MCP status + OAuth + elicitation, subagent model/reasoning overrides, shortcut
re-binding, i18n (system-follow, en/zh), `setting.json` + `gpui_settings.json`
persistence compatible with Electron.
