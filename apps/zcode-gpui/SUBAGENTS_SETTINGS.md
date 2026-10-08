# Subagents Settings contract

Status, 2026-10-06: opt-in local management through the unchanged Services Host, preference
handoff, outcome review and reference-style native composition are implemented. Actual native
visual/action-matrix acceptance is not yet claimed. SETTINGS_ACCEPTANCE.md distinguishes
current gates from the earlier rejected inventory and historical isolated evidence.

## Scope and hard boundary

TypeScript backend, CLI, shared protocols, provider repositories and backend assembly
remain unchanged. The rejected configuration worker and backend edits stay withdrawn.
SERVICES_RPC.md defines disposable acceptance plus explicit opt-in local management.
Activation discloses full-Host memory, conditional migrations and unsupported concurrent
writers before touching real storage. No real profile copy, Desktop port injection or
Rust configuration writer is authorized. Tests still use owned disposable storage only.

The existing CLI app-server NDJSON `session/subagents` query lists child chats, not
profile configuration. Profile management uses only the existing ISubagentsService.
Configured Models inspection is a separate read-only workspace projection; it is not
provider CRUD or persistent profile selection.

## Owners and event order

RootView owns section, scope, search and form drafts. Shared AppState owns the isolated
capability, Services connection/generation, scoped query projections, one mutation lane
and bounded receipts. The unchanged service owns canonical profile persistence. The
existing CLI owns conversations, admission and materialized runtime configuration.
Isolated preferences remain read-only. Local management suspends/drains the existing
serialized native preference writer before Host startup and observes process exit plus
strict background reload before resuming. Recent-project intents defer in the same owner;
no second writer is added. SERVICES_RPC.md specifies the handoff and appearance semantics.

```text
isolated launch → fresh capability → read-only preferences + scratch workspace
RootView scope/search → AppState query → unchanged full Host → service inventory
captured form origin + baseline + draft
  → fresh inventory → background scope/path/model validation → one mutation lane
  → canonical service mutation → committed receipt → originating-scope refresh
CLI create/materialization → same disposable roots → runtime profile snapshot
```

Profile keys include scope and workspace identity/path in addition to service ID. User
is the initial scope. Only registered local workspaces may be selected; disposable mode
additionally requires owned-root validation. Form
callbacks carry form identity; leaving the section or Settings clears the local form,
not an admitted mutation. Parent workspace/session/draft and running work remain owned
by their existing paths. Stale inventory serials and connection generations are ignored.

Update/delete paths are rebuilt from freshly validated rows, never arbitrary form text.
Validation runs off the UI thread. Cancellation is advisory and cannot establish rollback.
A failed write is conservatively uncertain; no create/delete replay occurs. Reload-and-review retains bounded intent/baseline, observes old Host exit and compares fresh
canonical inventory to the intended supported state. A unique match is labelled observed,
not original RPC success; explicit acknowledgement unlocks new commands. Partial/conflict
results stay blocked and no crash receipt is invented. Persistence success followed by refresh failure keeps
the committed receipt and reports the originating query error separately. Old-serial
refresh errors remain ignored. Native preflight is not a transaction or cross-process lock.

## Inventory and actions

- Use canonical `list` with `settingsUserOnly` or `allRuntimeScopes`. Use `pluginAgents`
  for Settings plugin groups; runtime aliases must not duplicate rows.
- Search and counts cover custom profiles, plugin groups and built-ins.
- Editable custom user/workspace profiles support create/edit/rename/confirmed delete.
  Only editable user-scope profiles support enablement.
- Built-in and plugin definitions are read-only. Their structured model/reasoning overrides
  use the existing canonical methods; clearing omits the whole modelSelection. Stable
  plugin override identity excludes installed version. Plugin source bytes remain untouched.
- Preserve every field represented by SubAgentConfig, including untouched tools, skills,
  disallowedTools, permissionMode, maxTurns, background and MCP declarations. **Unknown
  YAML fields/comments are not exposed by the service and are lost on edit.** The real
  test characterizes that limitation; no unknown-metadata preservation claim is made.
- Missing/empty/wildcard tools mean All under current runtime semantics. An empty custom
  tools form is invalid rather than presented as no tools.
- Model selections require canonical provider/model IDs and published reasoning levels.
  Changing model completes from the new model's last published level, not old reasoning.
  Historical values are not silently repaired during hydration. Forms and inline controls
  display an unavailable model/reasoning selection explicitly, retain it until deliberate
  replacement or clearing, and explain that it cannot be submitted as a current model.
  Safe facade projections
  discard access keys, URLs, headers and option maps.

## Visual acceptance: redesign not yet verified

The reference requires one clear heading, a compact scope/count/search toolbar, rounded
grouped lists, dense horizontal profile rows, agent/color icons, quiet model/tool badges,
aligned trailing switches/delete actions and inline inheritance/override controls.

The earlier rejected implementation had stacked full-width controls, vertically expanded
text rows, duplicated source/scope labels, textual metadata and weak hierarchy. These were
presentation defects, not transport failures. Ely usage and passing interactions do not
establish visual parity. The implemented redesign uses one Settings heading, compact scope/count/right-search toolbar,
custom-group Refresh/New, shared rounded lists/separators, 36px Ely bot/avatar/color identity,
name/model/tool badges, two-line description and aligned trailing switch/delete. Editable
rows activate Edit without nested control propagation. Plugin/built-in selectors commit
inline through the existing mutation lane. Narrow layouts wrap deliberately. Forms keep
Save/Cancel/validation and disclose metadata rewrite. No homemade component skin or backend
changes; full native theme/locale screenshots and interactions are required.

## Existing-interface assessment

Desktop transfers a private Electron MessagePort to its window-scoped utility-process
Host. It has no supported external native endpoint or port handoff. Source anchors:
`packages/desktop/src/main/desktopHostProcess.ts`,
`packages/desktop/src/main/desktopWindowLifecycle.ts`,
`packages/desktop/src/host/index.ts` and `packages/shared/src/channels.ts`.
Neither phone transport nor a server WebSocket is attachment to that Desktop owner.

The existing `packages/server/src/entry-stdio.ts` and `stdioServices.ts` expose Subagents
and model metadata through the full local service collection. Provider runtime starts
eagerly; ISettingService and background services are also registered. There is no supported
Settings-only/lazy service-selection flag. Startup writes and migrations are conditional,
not every initialization an unconditional preference write. Subagents list can migrate
Markdown; avoiding mutation RPCs does not make hydration read-only. Constructing this
Host does not itself spawn a workspace CLI app-server.

Approved local activation accepts disclosed existing-Host behavior; execution tests remain
isolated and never automatically start real management. A development bundle and installed
Desktop Node-compatible runtime prove compatibility, not supported packaged distribution.
The earlier rollback checks are historical integrity evidence, not current parity coverage.

## Disposable executable-restart harness

Debug acceptance creates a fresh OS-temp `zcode-gpui-settings-<UUID>` root with a
root-identity marker, unlogged random capability ticket/token and a held root file lock.
A harness reattach requires `--isolated-settings`, matching ticket/token, validated temp
parent/root UUID, canonical marker identity and bounded traversal rejecting links/reparse
escapes. It never accepts an arbitrary HOME or real profile root. Only debug isolated
processes honor `ZCODE_GPUI_ACCEPTANCE_THEME`/`ZCODE_GPUI_ACCEPTANCE_LOCALE`; display
overrides are non-persisting and reapplied after preference projection reload. Harness
secrets are not included in child environment or logs. The held scratch-root lock is the
sole isolated single-instance guard; isolated launch skips generic HOME-based IPC so it
cannot read, replace or remove a real `gpui_instance.port` file.

```text
fresh isolated process → owned capability/lock → actual GUI actions → quit
  → verify captured PID + creation-time identities and all owned children gone
  → stopped-root fixture/readback → validated same-ticket reattach → fresh service list
```

Native window activation requires the interactive input desktop and exact owned HWND/PID
creation-time checks. A bounded activation acknowledgement is not visual acceptance; any
pointer fallback must hit the verified owned caption, never a foreign/secure desktop surface.
No fixture edits or second executable overlap a live owned Host. Invalid capability,
link escape, conflicting lock and non-isolated override are refusal cases. Screenshots
use populated synthetic custom/plugin/built-in groups and model metadata; no real
profiles, credentials or provider requests are used.

## Acceptance boundaries

Real unchanged-Host tests cover supported metadata, unknown-YAML limitations, CRUD/rename,
user disablement, same names in user/two workspaces, both built-in overrides/clearing and
plugin version-stable override/clearing with immutable source bytes, including Host restarts.
Native TestApp interactions and actual Windows pointer/typing/create evidence are separate.
A same-CLI-server loopback harness checks schema name/description snapshots: held A uses
v1, newly materialized B uses committed v2, and correlated next A turn retains v1. It does
not invoke a child Agent or prove profile prompt/model/reasoning execution.

Current native captures cover populated dark/English, light/Chinese narrow, New/Save and
supported-field readback after actual same-executable restart. Real scratch service tests
characterize corrupt settings/state, Windows held-file refusal and partial rename; they do
not prove the native recovery UI. Open cases include full reference visual parity, native
rename/delete/inline overrides, the full keyboard/scroll/theme/locale/font matrix, end-to-end
uncertain-write reconciliation and non-Windows lifecycle. These remain gaps, not passes.

## Opt-in scope and remaining release limitations

Local management can use existing full-Host behavior after explicit activation; it is not
migration-free, lossless arbitrary YAML editing or concurrent-safe production parity.
Activation fails for detected Desktop/conflicting writers or missing runtime. Native
preference admission/drain and observable process exit prevent in-process competing writers.
Detection cannot provide cross-process atomic ownership. Existing native CLI connections
also retain their exact owned process IDs for detection exclusion; this is native adapter
bookkeeping, not a CLI protocol or business-state change. Only currently owned IDs/tree
members are excluded, never every ZCode executable by name. Corrupt/partial effects remain
truthful, uncertain and unreplayed; unsupported metadata warning accompanies existing edit.
Standalone packaging, non-Windows acceptance and low-memory budget remain separate work.
No backend changes, automatic real-data migration before consent or native file-writing
fallback is authorized. Completed tests must be reported separately from these limitations.
