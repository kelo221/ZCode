# Plugin detail and configuration boundary

Status: local Details and typed scoped Configure/Clear/Reset implemented, 2026-10-06.
Pointer, safety and disposable CLI persistence evidence is recorded below; native
visual/lifecycle acceptance remains open. Existing source,
installation, enablement and draft-safe Example Prompt behavior is specified in
COMPOSER_PARITY.md. CONTEXT.md vocabulary and PARITY.md exclusions remain binding.

## Read-only marketplace detail

Available cards and installed-strip entries open one workspace-local selected detail.
The selected receipt captures stable plugin ID, name and marketplace and revalidates
membership in the current overview. A marketplace candidate uses existing
`plugins/describe {workspace, pluginName, marketplace}`; no row path is read directly.
Orphaned installed plugins retain existing management but cannot invent a removed
marketplace candidate. Describe never installs, configures, submits, or navigates chat.

WorkspaceInspection owns the selected identity and connection-local description cache,
keyed by marketplace/name with at most 32 entries. One pending describe per key prevents
duplicates. Failure retains previous content, shows sanitized diagnostics and Retry;
Close returns to the same store segment and does not erase pending correlation. Known
plugin mutations invalidate description caches and outstanding old description reads;
selected identity stays local and requires an explicit Refresh if content was invalidated. Late
responses settle only the origin cache, never select or reopen detail. Reconnect clears
query facts and selection, leaving conversation/source drafts untouched.

Strict parsing validates component kind (agent/command/skill/hook/mcp), item names and
optional descriptions, metadata and diagnostics. Only supported fields are displayed;
errors inside a result are failure feedback, not successful lifecycle evidence. Paths,
credentials and raw backend error payloads are neither copied into settings nor logged.
Metadata URLs are displayed as text, not automatically fetched or opened. Installed
Strip and card callbacks capture workspace/plugin membership; stale callbacks cannot
open another workspace's detail. Existing mutation controls remain owner-correct.

```text
card / installed strip → captured workspace + plugin → overview membership
                      → selected detail → scoped describe query
                      → strict origin cache → detail components + sanitized feedback
Close / changed navigation → no request replay or late detail re-opening
```

## Scoped configuration and reset

Manage Installed exposes Configure for a captured installed plugin, including orphaned
installed plugins. One workspace-local form owns its selected plugin, explicit user or
workspace scope, opening baseline, unsent typed field drafts and generation token.
`plugins/list {workspace, configScope}` supplies the authoritative scoped projection;
`plugins/configure` and `plugins/resetConfig` remain the sole persistence ports. No file
writes or raw JSON credential editor are introduced. Parent chat input is never reused.

Only manifest-declared keys are editable. String/directory/file fields use text inputs,
number fields require finite numeric values, and boolean fields use switches. Required
metadata is displayed, matching React; it is not a new admission rule. Effective values
come from configuredOptions, declaration defaults, then blank/false. Source labels show
user/workspace/default. Persisted sensitive values are intentionally absent from reads;
sensitive fields never hydrate values/defaults and local drafts use masked Ely input.
Blank secret input means unchanged; explicit Clear is separate. Only edited keys are
sent, so saving never copies inherited values into workspace overrides. Clear keys and
options are disjoint. Clear is offered only for an override owned by the selected scope;
it discards the old field entity so stale edit callbacks cannot undo Clear. Inherited
values cannot be cleared from another scope. Successful writes clear only submitted, still-matching drafts;
newer edits survive, and submitted secret strings are not retained in pending receipts.

Save requires a fresh scoped-list preflight matching the opening projection. External
changes block writes until explicit Reload; Reload adopts the comparison baseline but
preserves unsent fields. This is not compare-and-swap: the CLI has no revision port,
so a concurrent write after preflight remains a protocol limitation. One workspace
mutation excludes other plugin writes/Example Prompt and repeat submissions. Responses
validate pluginId and diagnostics before success; configuration errors use generic
feedback because an arbitrary secret may be echoed without a recognizable label. Raw
configuration diagnostics are never put in banners or logs. Malformed/error/uncertain outcomes
require Reload, retain drafts and never replay writes. Replies refresh only the captured
origin workspace and invalidate old scoped-list/describe reads, not the active workspace.
Close does not cancel an already dispatched write or reopen the form on late settlement.
Unknown/retired request errors are ignored before status/log storage. Editing during a
reset preflight revokes that confirmation, so an old decision cannot remove configuration
under a newer edit; ordinary Save never submits newly edited values without another click.
Reconnect retains unsent fields, invalidates pending operations and requires Reload.

Workspace Reset explicitly says Restore inherited enablement and preserves all option
and secret overrides. User Reset explicitly says Remove user configuration, removes
options/secrets and enablement overrides, and requires a separate destructive confirmation.
Both resets use preflight and explicit Confirm/Cancel. Reset scope explanation and
confirmation are grouped before the potentially long option list, so decision controls
remain reachable without scrolling past every field. The form is a nonshrinking content
child of the bounded scroll viewport. Scope changes discard the old
form only on explicit navigation and cannot reuse late responses from another generation.

```text
Configure + captured owner → scoped CLI list → form baseline + unsent typed edits
Save / confirmed Reset → exact-baseline scoped list → existing CLI persistence port
                       → strict receipt → origin invalidation + fresh scoped projection
external change / uncertain reply / reconnect → keep edits → explicit Reload, no replay
```

React cancellation currently belongs to remote plugin-sync operation IDs. Remote sync
is excluded from GPUI parity; no fake Stop is offered for local mutations with only RPC
request IDs. Disposable CLI config persistence is verified; native visual acceptance
and full plugin lifecycle remain separate evidence requirements.

## Acceptance

- Pointer card/strip detail, Close and Retry are reachable; normal chat drafts unchanged.
- Duplicate query, A/B late response, changed selection, removed source and reconnect
  cannot change detail ownership or mutate the composer.
- Empty describe, malformed components, diagnostic errors and RPC failures are distinct.
- Isolated CLI smoke describes only a disposable local source fixture without installing
  plugins, fetching remote assets or changing production data. A separate inline plugin
  fixture proves user/workspace projection isolation, secret omission and persistence,
  edited-key preservation, explicit clearing, workspace enablement-only reset and user
  footprint removal by reading only harness-created scratch configuration files.

Recorded automated evidence: the complete Rust gate passes 364 unit tests, PTY smoke,
8 reverse-RPC tests and the ≤400-line source gate, strict all-targets Clippy and Rust
formatting. Details and masked Configure/Save/Reload/scoped Clear/Reset Confirm/Cancel have measured
pointer fixtures using NoopTextSystem, not native glyph/render acceptance. Repository
and standalone harness typecheck/lint results are in SETTINGS.md.
