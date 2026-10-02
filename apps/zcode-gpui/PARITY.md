# GPUI Frontend Parity Roadmap

Status: draft v1 (2026-10-02). This file is a domain spec for `apps/zcode-gpui`;
per repo discipline, update this file before implementing the corresponding behavior.

## Goal and guardrails

**Goal**: evolve `zcode-gpui` from a "minimal chat client" to day-to-day parity with
the Electron frontend, while keeping three invariants:

1. **Zero backend forks**: only use wire surfaces that already exist in
   `packages/shared/src/zcode-protocol*` (V4 topics/commands + the legacy method
   registry). Upstream updates land via git pull; only a protocol version change
   requires touching the frontend.
2. **Ownership boundaries stay put**: agent process lifecycle, CommandInbox
   admission, task leases, and the remote connection registry always belong to the
   backend; the frontend only mirrors state, keeps drafts, and renders optimistic UI.
3. **Every milestone ships a usable app**: main must always compile, connect to the
   real backend, and complete a full conversation round.

**Explicitly stays in the original program** (unless marked as a spike below):
desktop global settings (writes to `~/.zcode/v2/setting.json`), login/OAuth (unless
the spike proves the CLI side can do it standalone), phone remote-control relay,
embedded browser (BrowserView-class), desktop auto-update feed.

## Current baseline (done)

- One agent process per project (processes keyed by workspaceKey, same as desktop);
  project list read from `~/.zcode/v2/setting.json`.
- Live session list (`sessions-index/<ws>`); session open/replay (snapshot + delta,
  base64 fragment reassembly with CRC-32, seq-gap diagnostics).
- Chat: `createSession`/`sendText`/`stop`, streaming assistant text, turn/user/
  assistant/reasoning/toolCall row rendering, IME input, autoscroll.
- Model catalog (harvested via legacy `session/create` + `deleteSession` cleanup),
  model/thinking-level switching (`switchModelConfig`, CAS + stale retry),
  collaboration mode switching (`switchCollaborationMode`: build/edit/plan/yolo).
- Unknown agent callbacks answered with `-32601` (keeps
  `session/requestRuntimePreferences` etc. from blocking commands).

## Key facts (wire capabilities that shape the route)

- **The legacy method registry is reachable over stdio** (`zcodeProtocolMethods`):
  `plugins/*` (full store lifecycle), `automation/*`, `offPeak/*`, `workflows/*`,
  `mcp/list`, `skills/referenceCatalog`, `usage/stats`,
  `session/{list,read,messages,fork,resume,compact,usage,subagents,events,…}`,
  `provider/testModelConnectivity`, `workspace/readPresentation` (slash-command
  catalog).
- **`interaction/*` are agent→client callbacks**: `requestPermission` and
  `requestUserInput` are exactly the permission/elicitation prompt path; currently
  they fall back to -32601, and implementing them unlocks approval UI.
- **V4 advanced commands defined but unused by us**: the queue group,
  `setFollowupMode`, `editUserQuery`, `retryTurn`, `applyFileRewind`,
  `forkAssistant`, `resolveInteraction`; plus `v4/conversation/rowsRange`
  (history paging), `v4/conversation/resync` (recovery), `v4/connection/flow`
  (backpressure).
- **Host-only (unreachable over stdio)**: settingService writes,
  ModelSelectionView, conversation share, terminal (node-pty belongs to the host),
  file/git services. For each, choose "implement locally in the frontend" or
  "keep in the original program" — decided per item below.

## Desktop UI surface inventory (checked against a screenshot, 2026-10)

Item-by-item against the desktop so nothing is missed (parenthesis = where it
lands in this plan):

- Left nav rail: New task (Ctrl+N), Search (Ctrl+K), Automations, Plugin
  Marketplace as first-class destinations (M4 quickpick/search shell; M5/M6 as
  top-level nav).
- Sidebar dual view: Group/Project grouping toggle, relative time labels
  (13h/1d), "Show more", and a flat cross-project **Tasks** list
  (M1/M4; `session/list` without a workspace filter returns all sessions).
- Task view: turn duration "Worked for 12m 8s" (M1; turnHeader
  startedAt/endedAt/activeMs), message hover actions copy/download/branch (M1),
  per-turn "N files changed +a −b" summary with **Undo** (M1;
  `turnHeader.fileChanges` + `applyFileRewind`), and the **turn plan progress
  checklist "Progress 6/6"** (M1; the V4 snapshot `plan`/`goal` regions).
- Git tools floating panel: changes summary, current branch, "Commit or push"
  (M3 git pane, floating-panel form).
- Right dock: multi-pane tabs (Inventory / Deep dive / … / Review) + a Review
  pane with an Unstaged filter, per-file +/− stats, expandable per-file diffs,
  Refresh (M3 dock container + review pane).
- Composer layout: `+` attach | mode dropdown (Full access ⌄) | model | thinking
  level (Max) | send (M1 — controls already exist, match this arrangement).

---

## M0 — Correctness and resilience (the floor) — ✅ DONE (2026-10-02)
**Goal**: long sessions, flaky environments, and backend upgrades must not break.
The base for everything after. (Plus, pulled forward: lazy spawn + idle unload +
job-object kill — see risk #5.)

- Self-healing: restart the agent per workspace on exit/crash and re-subscribe
  (snapshot recovery); conversation state survives.
- Frame-fault recovery: CRC failure / assembly timeout →
  `v4/conversation/resync {forceSnapshot:true}`; a seq gap triggers resync instead
  of only logging.
- History paging: `rowsRange` to fetch messages beyond the 60-row tail window
  (load-on-scroll-up).
- Error presentation: upgrade the status string to actionable toasts/banners with
  retry; surface backend `lastError`/`apiRetry`.
- Backpressure: `v4/connection/flow` saturated/drained.
- Test foundation: golden tests for `backend/wire.rs`/`conversation/model.rs` from real captured
  traffic; `cargo test` in CI; `cargo fmt/clippy -D warnings` alongside
  `pnpm verify:pre-push`.

**Acceptance**: kill the agent process → the app recovers within 5s and replays the
open session; a 200+ row history scrolls up and loads; offline for 30s → clear
error with retry. **Size: M — verified**: external agent kill → auto-restart
(1s backoff) → re-subscribe + catalog re-harvest observed live; `rowsRange`
probed against the real backend (200-row page + paged cursor, contiguous);
12 golden tests green, clippy/fmt clean, CI in `.github/workflows/gpui.yml`.

## M1 — Conversation core (chat parity) — ✅ DONE (2026-10-02)
**Goal**: a genuinely complete daily chat client.

- Permissions/elicitation: cards render the V4 `pendingInteractions` region
  (permission options, `fullAccessOption`, AskUserQuestion `questions` with
  single/multi-select, free text taken from the message box, Decline). Answers go
  **only** through V4 `resolveInteraction`. The legacy `interaction/request*` stdio
  requests are deliberately left unanswered: the backend races them against the V4
  answer and uses whichever arrives first (`interaction-response-race.ts`), and a stdio
  reply carries less information (Allow always, Full access and question answers
  would be lost or rejected).
- Message actions: Edit puts the message in the composer and Enter sends
  `editUserQuery`; `retryTurn`; `applyFileRewind` (file Undo, two-click confirm);
  copy to clipboard; session rename (✎, composer as the title field) and delete
  (two-click confirm, sent to the workspace that owns the session).
- Follow-up queue: V4 `queue` state mirroring, `sendQueuedNow`, `deleteQueueItem`,
  and `setAutoDrain` toggle.
- Turn metadata: duration ("Worked for 12m 8s" from turnHeader activeMs), the per-turn
  files-changed summary bar ("N files changed +a −b") with interactive **Undo** button,
  and the **plan progress checklist** ("Progress {completed}/{total}" with collapsible
  checklist view in header and transcript).
- Sidebar upgrades: local session search bar filtering sessions by title, delete
  session button with stop propagation.
- Composer 2.0: multi-line input (Shift+Enter for newline, Enter to send), action bar
  matching desktop (`+` attach, mode dropdown, model selector, thinking level, Send/Stop).
- Draft persistence: uncommitted composer drafts saved atomically per session & workspace,
  restored smoothly on navigation, and purged upon sending.
- Row rendering upgrades: collapsible reasoning toggle ("Thought for Ns"), expandable
  toolCall (streaming inputText + output preview), turnHeader with elapsed time & diff stats.

**Command rules** (`backend/session_cmds.rs`; spec: zcode-protocol-v4/command.ts):
- Every session command goes through `send_session_command`, which registers a
  `Pending::Command` so its `CommandAck` is checked.
- CAS commands (`COMMANDS_REQUIRING_BASE_REVISION`) take `baseRevision` from the
  mirrored conversation; row commands (`ROW_TARGETING_COMMANDS`) also need the
  snapshot `logEpoch`. With no snapshot/epoch the action is disabled; the client never
  sends a guessed revision or epoch.
- `stale` → resend once from `revisionAtDecision` with a new commandId.
  `rejected`/`failed`/second `stale`/JSON-RPC error → error banner plus a forced
  resync of the affected topic (conversation, or sessions-index for rename/delete).
  The backend snapshot is the only rollback path for optimistic UI updates.
- The idle reaper keeps an agent alive while any of its sessions is `running` or
  `prewarming` (sessions-index phase) or has a pending interaction.

**Acceptance**: complete "plan → approve tool → interrupt → edit & resend → queued
follow-up" entirely in the GPUI client, matching desktop behavior. **Size: L**:
24 golden unit tests (envelope shapes per command, ack decisions, interaction
parsing and answer shapes, queue parsing), `cargo fmt --check` and
`cargo clippy --all-targets -- -D warnings` clean, all source files <= 400 lines.
The 2026-10-02 review fixes above still need a live pass against a real backend.

## M2 — Rich rendering (reading parity) — ✅ DONE (2026-10-02)
**Goal**: assistant output reads like a document, not plain text.

- Streaming Markdown renderer (pulldown-cmark → gpui elements): headings,
  paragraphs, nested ordered/unordered lists, block quotes, rules, task
  lists, GFM tables (with alignment), inline bold/italic/strikethrough and
  inline code.
- Code highlighting: syntect (`base16-ocean.dark`) mapped to per-span colors
  with bold/italic; fenced blocks render as cards with a language label and
  Copy button; oversized blocks (>120k chars) degrade to plain text.
- Links: colored, underlined on hover, opened externally via `App::open_url`;
  images render as links to their target (v1).
- Diff view: tool outputs that look like unified patches render with the
  theme's diff tokens (`--color-diff-added`/`-removed` backgrounds, hunk
  strips), 2000-line cap.
- Virtualization: the transcript renders through gpui's `list` element —
  only the visible window (+800px overdraw) is built; cached row index;
  follow-bottom tracking via the list scroll handler; appends splice in
  place, history prepends reset and anchor to the old first row, session
  switches force a full reset (row ids repeat across conversations).
  Plan checklist / pending interactions / "Load earlier" sit in a pinned
  strip above the list.
- Theme engineering: diff/code/link tokens added to `shared/theme.rs` (Zai Dark as
  the canonical surface set).

**Deferred (tracked, not acceptance-blocking)**: Zai Light theme +
follow-system light/dark, rem-based font scaling, sidebar `uniform_list`,
math rendering, image rendering.
**Acceptance**: a reply with code blocks/tables/long code reads the same on both
clients; a 10k-row session scrolls at full frame rate. **Size: L — verified**:
33 unit tests green (markdown span highlighting, diff classification,
wire/model/command goldens), `cargo clippy --all-targets -- -D warnings` clean,
all source files <= 400 lines, app verified live (~59 MB at startup, no panics).

## M3 — Tool panes (Git / terminal / files) — ✅ DONE (2026-10-02)
**Goal**: the high-frequency developer motions beyond chat.

- Git surface: a **right dock container** (Review / Files / Terminal tabs,
  Ctrl+B toggle + header "Tools" pill) hosting a **Review pane** with
  Unstaged/Staged filter, per-file +/− stats with expandable per-file diffs
  (untracked files synthesize an all-added view), Refresh, Stage all, Commit
  (single-line composer reuse) and Push. **Decision taken**: shell out to the
  `git` CLI directly (a local frontend process, no backend ownership
  involved) — commands run on the background executor with CREATE_NO_WINDOW.
  Status auto-refreshes when a turn completes while the dock is open.
- Terminal: `portable-pty` (ConPTY) + `alacritty_terminal` VT parsing, one
  local PTY per workspace (PowerShell on Windows), lazily spawned when the
  tab first opens. Reader thread → shared buffer → 60ms main-thread poll;
  resize via a paint-phase size probe applied on the poll tick; key mapping
  covers arrows/home/end/page/delete, Ctrl+letters, Alt-prefix, bracketless
  paste (Ctrl+V from the OS clipboard); 256-color + truecolor cells rendered
  as merged styled runs with inverted cursor.
- File tree + workspace file preview: lazy per-directory loading (dotfiles
  skipped), text preview (256KB cap, binary sniff) and Markdown files
  rendered through the M2 renderer; local workspaces only.

**Deferred (tracked)**: terminal scrollback view (live screen only in v1),
per-file stage/unstage buttons, image previews, floating (undocked) panel
form. The Review pane's summary doubles as the "Git tools" panel.
**Acceptance**: "check the diff → run tests in the terminal → continue the
conversation" without leaving the app. **Size: L — verified**: 41 unit tests
green (porcelain/numstat parsing, badge fallback, palette + key mapping,
plus M0–M2 suites), `cargo clippy --all-targets -- -D warnings` clean, all
source files ≤ 400 lines, app verified live (~59 MB before opening the dock;
PTY and git spawn lazily).

## M4 — Session lifecycle and multi-window
**Goal**: make the GPUI client a resident primary UI.

- Multi-window (gpui multiple Windows, each bound to a different
  workspace/session), single-instance + focus-on-second-launch.
- Session search/filter (local filtering over sessions-index), export (markdown
  dump).
- System integration: taskbar notifications (task completion), tray (show/quit),
  file dialogs (rfd), open-in-editor/file manager, deep links (zcode:// protocol
  registration).
- Login spike: verify whether CLI-side OAuth/credentials can be done standalone
  (the `zcode` CLI ships an auth module; if yes, build a minimal login wizard,
  otherwise keep login in the original program).
- Packaging v1: Windows NSIS/portable zip + icon + crash logs; mac/linux build
  matrix to follow.

**Acceptance**: use it as the only UI for a full day with no mandatory trip back to
the desktop (assuming the login spike passes). **Size: L**

## M5 — Workflows / automations / usage (legacy registry dividend)
**Goal**: replicate the desktop "project tools" surface in the standalone client
(all reachable over stdio, low risk).

- Shell integration: the left nav rail becomes real — New task (Ctrl+N) and
  Search (Ctrl+K, sessions + commands + cross-project tasks) as first-class
  destinations; Automations and Plugin Marketplace promote from dialogs to
  top-level nav views.
- Workflows: `workflows/{list,get,runs}` + V4 `workflowRunDeltas` (declare the
  capability and render whole-key patches) + `startSavedWorkflow`/resume/amend;
  run timeline view (node status/concurrency, read-only first).
- Automations: `automation/{list,create,update,delete,checkTaskBinding}`;
  offPeak: `offPeak/{list,create}`. Scheduled-task CRUD forms.
- Usage: `usage/stats` charts (hand-drawn gpui bars/lines, no third-party chart
  lib).
- MCP status panel: `mcp/list` read-only + point back to the original program for
  editing (or spike the MCP config write path).
- Subagent panel: `session/subagents` + a read-only sub-session side pane.

**Acceptance**: without opening the original program, "create an automation → watch
a workflow run → check this month's usage". **Size: M**

## M6 — Plugin store (heaviest chunk of the legacy registry)
**Goal**: the full plugin lifecycle available in the standalone client (stdio
already covers every method).

- Store browsing: `plugins/overview` + `plugins/referenceCatalog{WithCategory}`;
  install/update/uninstall/enable/configure/restore-builtin
  (`plugins/{install,update,uninstall,setEnabled,configure,resetConfig,
  restoreBuiltin,validate}`).
- Marketplace sources: `plugins/marketplace/{add,remove,update}`; operation
  progress (`plugins/cancelOperation` + overview polling).
- Domain vocabulary strictly per `CONTEXT.md` (official marketplace / personal
  sources / catalog refresh / orphaned plugins, etc.).
- **Dependencies**: M2 rendering (rich store detail pages) + M1 interaction
  patterns.

**Acceptance**: add a personal source → install → enable → configure → uninstall,
interoperating with the original program (same database). **Size: M**

## M7 — Long-tail parity and distribution
**Goal**: close the tail and establish a release cadence.

- Keyboard shortcut system (keymap aligned with a desktop subset) + quickpick
  (command palette).
- Session share/whiteboard/trajectory viewer: evaluate degraded paths via the
  host-only surface (read-only or jump to the original program).
- Embedded browser spike: webview feasibility in the gpui ecosystem (Wry/WebView2
  embedding); if not viable, mark the browser pane as "jump to original program".
- Updates: app self-update (reuse the desktop manifest feed concept at
  `/api/v1/releases` or a dedicated feed).
- Docs: README/PARITY tied to the protocol version; version policy (follow the
  repo minor).

**Acceptance**: ship a 1.0 installer; biweekly iteration tracking upstream.
**Size: M**

---

## Cross-cutting workstreams (span all milestones)

| Stream | Content |
|---|---|
| Protocol conformance | Golden tests over captured wire samples; after each upstream pull, diff `zcode-protocol-v4` and run `pnpm typecheck`; show a clear UI notice on protocol mismatch |
| Governance | Every file ≤400 lines, register new modules in architecture-policy, PARITY.md first (spec-first) |
| i18n | All new UI goes through the en/zh key tables; key names aligned with the desktop en-US.ts for translation reuse |
| Performance | Run the 10k-row session + 8-project stress test every milestone; budgets in M2 |
| Security | No credentials in frontend logs; the -32601 fallback list shrinks as handlers land and is tracked |

## Risks and decision points

1. **Model catalog source** (solved but fragile): the legacy `session/create`
   harvest depends on `mapSessionSettings` behavior; if upstream changes the
   response shape we must follow → covered by a golden test.
2. **Git/terminal/files implemented locally in the frontend**: this deviates from
   the letter of "all logic lives in the backend" — but the desktop host itself is
   a local process implementation (node-pty), so localizing in the frontend adds no
   new remote semantics; record the decision here.
3. **webview**: no native equivalent for BrowserView; if the spike fails, the
   browser pane stays in the original program for good.
4. **Login/OAuth**: the desktop host owns the oauth service; standalone CLI-side
   login needs the M4 spike.
5. **Process overhead**: ~~8 resident agent processes cost real memory~~ **Addressed
   pre-M0**: lazy spawn (only the primary workspace's agent boots; others spawn on
   click) + idle unload after 10 min (`ZCODE_GPUI_IDLE_SECS` to tune; never unloads
   the active workspace or a running turn). Startup footprint went from ~1,026 MB
   (8 agents) to ~267 MB (1 agent, fresh). Children are bound to a Windows job
   object with KILL_ON_JOB_CLOSE so they die with the frontend even on crash
   (mirrors desktop processTreeOwnership).

## Suggested cadence

M0 → M1 → M2 is the "daily driver" line (after that, the GPUI client can be your
primary chat surface); M3/M4 proceed in parallel for tools + residency; M5/M6 are
low-risk quick wins from the legacy registry and can be interleaved.
