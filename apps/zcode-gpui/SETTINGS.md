# Settings and safety slice

Status: first Settings/safety slice implemented and automated gates verified,
2026-10-06. Native visual acceptance and full executable end-to-end evidence remain
open. This spec extends PARITY.md without expanding its host/platform exclusions.

## Product behavior

- The sidebar gear, command-center Settings action, and platform Settings shortcut
  open one per-window destination. General is the initial section. Reopening reuses
  the page. Back/Escape restore workspace focus without changing workspace/session,
  draft, transcript state, dock, or terminal. Session streaming and lifecycle work
  continue while Settings is displayed.
- General exposes System/English/Chinese language. Appearance exposes System/dark/light
  theme and UI font size (12–20 px). Theme preference and effective OS appearance are
  distinct. UI typography scales without scaling icons, geometry, code, or terminal.
- Shortcuts show only commands implemented in this client. Overrides use the shared
  command IDs and binding syntax; unsupported IDs are preserved but not advertised.
  Invalid/conflicting overrides fail validation. Empty overrides disable a command;
  reset removes the override. One dispatch path prevents duplicate execution.
- Settings-file opening is an explicitly named advanced action, not Settings navigation.
  Host-owned or unavailable controls are not shown as functioning toggles.
- Sidebar Search and command-center shortcuts use the same focus transition. Closing
  the overlay restores Settings focus or composer focus, including pointer dismissal.
  Sidebar workspace/session/new-task and Add Project actions return to the workspace;
  cancelling the folder picker leaves the current workspace and draft unchanged.
- A focused shortcut recorder consumes recording/clear/Escape events before application
  navigation. After recording or cancellation, Settings retains a usable focus target.

## Confirmed remaining Settings owner ports (2026-10-06)

Skills enable/delete, Commands CRUD/toggle and Subagents profile/model-override management
remain unavailable: current React uses its service-owned writes, while the CLI stdio
surface has no matching public management command. Composer reference catalogs and
runtime child navigation are not configuration owners. Hooks has a real
`workspace/hooks/trustGrant` write, but Settings still lacks a CLI-owned canonical hook
snapshot read supplying current `bundleDigest`/`hookDeclarationDigest`. Trust must not
be exposed using invented digests or a second frontend config parser. These are explicit
integration blockers, not completed parity or permission to edit configuration directly.

## Owners and event order

```text
gear / shortcut / command → RootView navigation → Settings view draft
Settings mutation / recent project → process preference owner
    → validated serialized background read–modify–write
    → committed snapshot → effective theme / locale / typography / shortcuts
failure → unchanged committed snapshot + visible error

session command → backend admission → ACK + authoritative stream
    explicit rejected / stale / failed → original-context draft recovery
    accepted / duplicate / noop → no replay; malformed → visible uncertain outcome
    create ACK → select only when original navigation generation is still current
connection spawn → generation → pump / flow / delayed restart
    → apply only if generation is still current
```

Application preferences use one controller and one disk mutation path. Unknown JSON
fields round-trip. Desktop-running checks remain before writing and before replacement.
A corrupt/unreadable file is never overwritten. Temporary writes are unique and cleaned
on failure. IO runs off the UI thread. A failed write cannot appear as a committed
preference. Navigation is not backend state; pending preference mutations are not an
accepted task queue.

Bootstrap settings remain under the resolved user home `.zcode/v2/setting.json`.
Application data uses `persisted dataBaseDir > ZCODE_DATA_BASE_DIR > default home`,
then appends `.zcode`, matching services/paths.ts. Discovery, install identity and child
backend receive the same resolved base. No data migration/deletion is implicit.

## Safety fixes and acceptance

| Case                     | Setup/action                                                  | Assertions                                                                    |
| ------------------------ | ------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Send enqueue failure     | Connected handle whose stdin receiver closes before send      | Returns failure; no orphan pending entry; composer text/attachments restored  |
| Background RPC error     | Workspace B fails while A is active; sentinel secret in error | Only B status changes; banners/status/logs contain no sentinel                |
| Custom roots             | Persisted root, env root, conflicting home variables          | Bootstrap location stays separate; identity/workspace/backend share data root |
| Old connection work      | Unload/respawn then deliver old events and delayed restart    | No state changes, flow signal, or restart on replacement generation           |
| Settings entries         | Gear, command, configured shortcut; repeat                    | One destination; General initial; no duplicate dispatch                       |
| Back during running turn | Draft, dock and terminal exist; open/close Settings           | Same context/draft; stream continues; composer regains focus                  |
| Preference commit        | Change locale/theme/font; reopen/restart                      | Committed value applied and persisted; System follows effective OS theme      |
| Write refusal            | Desktop starts, corrupt JSON, unwritable path                 | Old snapshot/file preserved; visible failure; no leftover temporary file      |
| Serialization            | Overlapping preference and recent-project mutations           | Both survive in order; unrelated/unknown fields preserved                     |
| Shortcut override        | Valid, invalid, duplicate, empty and reset                    | Correct live dispatch; validation error; disabled/reset semantics             |
| Platform PTY             | Platform shell prints computed sentinel                       | Actual output verified, not input echo; child cleaned up                      |

Existing settings/transport tests are extended; new pure owner/generation tests and
native interaction scenarios are required. Test windows isolate preference files and
skip native geometry persistence, keep-awake, and notification side effects. GUI/real-backend
execution is reported separately from unit coverage. No historical test count establishes
current success.

## Recorded validation (2026-10-06, Windows checkout)

- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`: passed.
  Cargo reports the ReFS incremental-cache hard-link fallback, not a source lint.
- `cargo test`: 429 unit tests, 1 PTY smoke, 8 reverse-RPC integration tests,
  and 1 source-limit integration test passed. All Rust source/test files remain ≤400 lines.
- Eight asset-capable GPUI Settings tests cover keyboard/QuickPick/action entry,
  measured-bound gear/section/Back pointer events, recorder focus and live shortcut
  dispatch, serialized preferences/refusal, new-owner hydration, and a real event-pump
  frame carrying running conversation/permission state while Settings remains open.
  Four additional Memory regressions cover pointer persistence/re-hydration, pending
  versus committed origin-only runtime replies, refused/corrupt writes and typed defaults.
  MCP adds strict authorization/status projection, unsafe URL refusal, refresh/reconnect
  receipt isolation, generic secret-safe errors and a measured Open/Refresh pointer test.
  Seven delivery tests additionally cover one-shot keyboard/pointer routing, platform
  modifiers, IME suppression, first-input/local-action compatibility, projection replacement,
  attachment ownership on refused/failed enqueue and rejected admission with newer drafts.
  Six held-queue tests cover measured Clear/Keep/Cancel, reviewed-ID payloads, new-goal
  objective routing without modifying /goal resume or /compact, current
  owner/config/draft/queue validation, stale ACK re-confirmation and no optimistic removal.
  Workflow successor navigation adds projected identity/ordering/owner tests and measured
  Open successor/All runs/Open run controls; the existing Apply test now checks integrated
  ACK-then-stream following without replacing the parent draft.
  Eight slash-catalog regressions cover strict empty/invalid decoding, draft/session ownership,
  no fallback, reconnect/late replies, attachment-preserving selection, stale Refresh,
  raw firstInput/sendText recovery and measured `/init` selection followed by keyboard Send.
  Seven `/plan` regressions additionally cover local-only preparation, model/mode selector
  overlays, nonrecursive task routing, normalized firstInput/config, attachment/chip/IME
  refusal, modified delivery, held confirmation and failed/rejected recovery. The same
  interaction fixture exercises pointer task submission and bare-command keyboard handling.
  A further measured menu/keyboard test verifies draft thinking-level edits use that override.
  Nine image-upload regressions cover metadata/chunk/count/staged-byte bounds, strict IDs/progress/refs,
  generic errors, failed enqueue, explicit Retry/Cancel, stale owner/replacement/reconnect,
  clipboard keyboard and pointer controls, and format-correct owned new-draft temp paths.
  Twelve workflow-artifact regressions cover strict safe-field/version decoding, correct
  parent/run queries, query/pending capacity, empty/unavailable/error states, failed enqueue,
  stale selection/parent/connection/run refusal and measured Open/Refresh/All runs controls.
  They also cover exact-version bounded Markdown reads, strict complete base64/MIME/UTF-8,
  passive links, unique content-selection tokens, View/Retry/Close, and late-byte retirement.
  Seven Files regressions cover six raster formats, one-frame animated GIF handling,
  strict input/dimension/pixel bounds, live localized safe errors, bounded fixture IO,
  measured image/text/image selection with actual background completion and render-atlas
  upload, and same-path identity/workspace-reset stale tree/preview refusal.
  Four terminal regressions add shared cell-metric/viewport geometry, wrapped/wide/combining
  and scrollback copy, selected ANSI cell styling, measured drag/keyboard clipboard readback,
  Ctrl+C ETX without selection, release-outside and workspace/spawn retirement.
  These use a no-op text system, not native glyph/render acceptance.
- The isolated CLI backend smoke passed with a loopback mock API-key provider, actual
  assistant frames in `desktop-continuous` and `web-remote-replayable`, and a new
  `recovery` snapshot at `completedSuccess`. It is not a full GPUI executable smoke.
  It also verifies saved launch completion/history identity, directly published markdown
  deliverable metadata, complete authorized Markdown bytes, cross-parent read refusal
  and unknown-run empty lists, definition inspection,
  metadata replacement (including `default:null`), global-to-project move, destination
  refusal and deletion of disposable harness-created fixtures. Plugin smoke also describes
  a disposable local source and verifies typed scoped configuration, secret readback
  omission/preservation, explicit clear and scope-specific reset against scratch files.
  It verifies idle startNow admission identity, a deterministically held mock response
  with actual busy queue admission, missing/stale held disposition refusal and Keep/Clear
  results from fresh strict snapshots. It accepts enabled Memory replies for both reverse
  request scopes and verifies the current status-only MCP query schema on an empty isolated configuration. No real OAuth
  browser or memory tool is exercised. The smoke also validates real draft/session slash
  catalogs, custom-command discovery and disable-noninteractive filtering, and observes
  the CLI's `/init` expansion (including additional notes) in the loopback request. The
  mock returns text only; no instruction-file write or real model tool is exercised.
  A queued disposable input also retains `mode:edit` and `planEnabled:true` in a fresh
  strict CLI snapshot. This proves frozen admission intent, not plan execution; that
  scratch queue item is cleared without running its task. A real scratch image transaction
  verifies bounded begin/chunk/commit, identical chunk/commit retries, abort refusal and
  artifact resolution into a vision-capable loopback model request. The PNG fixture has
  valid chunk checksums; request bodies are checked in memory, never logged. The standalone
  harness passes strict TypeScript checking and targeted lint.
- Literal `pnpm typecheck` / `pnpm lint` are unavailable (`pnpm` not found). Verified
  `bun run typecheck` passed; `bun run lint` passed with 71 warnings and 0 errors.
  Freshness and Bun architecture checks passed with 0 violations. GPUI is unmanaged
  by that TypeScript architecture policy, so Rust ownership was reviewed separately.
- Targeted formatting passed for README, PARITY, SETTINGS and the smoke script.
  Repository-wide `bun run fmt:check` still fails on 2,851 other files; no unrelated
  whole-repository formatting was applied. Git whitespace checking passes with CRLF
  recognized as a line ending. Cargo lock changes are the GPUI test-support dependency
  graph (including proptest/backtrace), plus a direct declaration of already-resolved
  url 2.5.8 for strict OAuth URL parsing and image 0.25.10 for bounded static preview,
  not a new production framework pin.

Not recorded: native platform screenshot/visual acceptance, actual executable restart,
full GPUI-to-real-backend rendered conversation, native folder-picker cancellation,
and macOS/Linux test execution. Those remain evidence gaps, not passing scenarios.

## Workspace-backed inspection sections

Plugins, MCP Servers and Usage become Settings destinations by reusing their existing
panes and existing stdio queries, not by introducing preference-file writes. Plugins
Settings enters Manage Installed; the marketplace remains the separate workspace dock.
MCP is inspection only until the existing owner-management operations are wired. Usage
retains its backend-supported 7d/30d/all ranges. Each section shows initial loading,
empty success, error and explicit refresh; unavailable/disconnected workspaces say so.

`WorkspaceHandle` is the sole owner of connection-local query results, loading and
sanitized errors. Usage caches are keyed by range; its selected range is workspace
view state and late responses for an older range cannot select it. Views read only
the active handle. Identical pending reads are suppressed per resource. Connection
invalidation clears results. A response captures its originating handle through the
pending registry; plugin mutations refresh that same handle, never whichever workspace
became active. Error diagnostics inside a successful plugin RPC are failures, not
completion evidence. No credentials or raw diagnostics are persisted as preferences.

```text
Settings/dock entry → existing scoped query → workspace pending registry
                   → originating query result/cache → active-handle pane only
plugin mutation → backend result + diagnostics → originating overview refresh
```

Tests cover A/B late responses, range switching, duplicate requests, reconnect clearing,
errors, successful empty results and measured-pointer section navigation/Back. The
native/backend evidence boundary remains the same as the first slice.

## Memory runtime preference

Memory is a Settings section with one application preference `memoryEnabled`, default
false. The process-global serialized PreferenceOwner persists it to the existing
bootstrap `setting.json`, sharing corruption/coexistence/atomic-write safeguards. The
UI uses the committed value, saving/error state and an Ely switch; failed writes never
change runtime replies. Unknown settings survive. This is not a new CLI write method,
a frontend memory store, or an autoload/session-specific preference.

Every `session/requestRuntimePreferences` received on an originating stdio connection
reads the current committed owner and includes that boolean in its strict response.
False disables memory in CLI runtime materialization; true leaves CLI `features.memory`
and `memory.use` policy intact, not a promise that memory is operational without backend
configuration. Materialization freezes preferences at create/resume; a changed setting
applies to newly materialized or resumed runtimes, not the already active runtime.
Existing search/AskUserQuestion response values are unchanged in this bounded slice.

```text
Memory switch → serialized preference owner → background guarded persistence
              → committed snapshot → next reverse runtime-preference request
              → originating stdio connection → CLI create/resume materialization
failed persistence → unchanged snapshot + runtime response, visible error
```

Acceptance: Memory section/switch is pointer reachable; setting round-trips and reloads;
missing/false remains disabled; true is returned only after a successful commit. Tests
exercise both reverse replies through the actual event handler, refusal keeping false,
unknown-key preservation and parent draft/focus. No active-session restart is fabricated.
The disposable CLI smoke validates current request/result schemas and accepts a true
Memory reply for real runtime materialization and user execution. This protocol evidence
is separate from GPUI pointer/owner tests and is not memory-tool execution evidence.
Native visual/executable restart and actual memory-tool execution remain evidence gaps.

## Pending MCP OAuth authorization

MCP Settings and the workspace MCP pane expose Open authorization only when the current
CLI status projection contains an OAuth authorization-code URL. `mcp/list` always uses
explicit `mode:status`; it neither supplies `mcpServers` nor reconnects/reconfigures servers.
CLI MCP runtime owns the flow. The originating connection-local query is the only owner
of status/authorization facts; the frontend owns only this user-initiated open action.

The typed projection validates the current result schema. Authorization accepts only
absolute HTTP/HTTPS URLs with a host, no userinfo and no raw control characters. Unsafe
or malformed results fail closed with generic feedback. URLs are short-lived sensitive
values: never displayed verbatim, serialized to preferences, or included in Debug/logs.
MCP RPC/server errors use generic feedback because arbitrary URL tokens may be echoed.
Open captures workspace, connection generation, server and authorization receipt; the
click revalidates the current ready origin, active workspace and nonloading/error-free
query. Refresh, replacement authorization and reconnect invalidate old callbacks. Native
URL opening uses the app's secret-safe OS adapter off the UI thread, not a shell command
or browser backend. The pinned GPUI opener can log a full URL on launch failure, so it is
not used for sensitive authorization in production. The adapter reuses ShellExecuteW on
Windows and directly invokes open/xdg-open elsewhere with null child IO; errors are generic.
Query invalidation also retires pending MCP/Usage reads so late replies cannot recreate
old connection facts. A user click commits dispatch; changing workspace afterward does
not cancel native opening, but late launch errors cannot affect a replacement origin.

```text
MCP Settings/dock → explicit status query → origin pending registry
                 → strict status/authorization projection → user Open authorization
                 → current owner/generation/receipt check → native external URL opener
refresh/reconnect/owner switch → old receipt cannot open a URL
```

Acceptance: measured pointer action, absent/unsafe URL has no action, exact status-only
request, origin-isolated late reply, reconnect invalidation, stale callback refusal,
refresh removing authorization and secret-free errors/debug output. Tests must intercept
the platform opener; no real OAuth URL or browser launch is used. Opening a page does not
prove OAuth completion. Native browser launch, real authorization, and MCP configuration
CRUD remain separate evidence/port gaps; no fake credential/configuration store is added.

## Remaining audited backlog

Plugins Manage Installed, MCP inspection and workspace/range-scoped Usage are now
reachable Settings sections alongside committed Memory runtime preferences.
Provider/Subagents/Skills/Commands/Hooks/Browser Use still require missing owner ports
or further bounded work. Current provider/updateAccountConfig is an account overlay,
not an API-key provider settings write; connectivity testing does not persist credentials.
Skills/referenceCatalog and workspace/readPresentation are read-only presentation ports,
not Settings CRUD. Hooks trustGrant requires owner-supplied bundle/declaration digests
without a current GPUI Settings snapshot adapter. MCP mode:status and its optional
mcpServers field must not be repurposed as a frontend configuration owner. Scoped live skill/plugin
catalogs, queue Edit/reorder, goal Pause/Resume, Personal Source controls, saved workflow
launch/definition/history, run settings and confirmed saved metadata/delete/move controls
are reachable; their presence does not establish complete lifecycle parity. Draft-safe
canonical plugin Example Prompt prefill is implemented. Non-path attachment upload,
workflow artifact detail remains audited. Plugin Details and scoped typed Configure/Clear/Reset
use existing CLI ports; remote-sync cancellation is excluded, not offered as local RPC cancellation. Task
index, auth and scheduling remain host-owned. Drafts survive navigation in memory, not
restart. Native visual/restart/cross-platform evidence, activation and distribution
remain incomplete. See COMPOSER_PARITY.md and WORKFLOWS.md for the later slice contracts.
