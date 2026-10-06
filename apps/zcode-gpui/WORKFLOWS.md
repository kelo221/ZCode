# Saved workflow actions

Status: saved browsing/typed launch, definition inspection, run history, run-settings,
metadata editing, confirmed Delete/global-to-project Move and projection-backed successor
navigation, focused-run artifact metadata and bounded passive Markdown preview implemented,
2026-10-06. Other artifact content, cross-project run detail and native evidence remain incomplete. These bounded actions use the existing Workflows dock. The backend owns
saved files, run scheduling, validation, compilation, admission and run state; GPUI
never creates another scheduler or reads/writes saved workflow files directly.

## Owners and contract

- `WorkspaceHandle` owns connection-local saved-list query caches, separately keyed by
  project/global scope. Existing `workflows/list` carries workspace path and key. Strict
  result decoding rejects malformed entries/scope; invalid definition rows show their
  sanitized reason. Duplicate reads are suppressed per scope; reset clears query facts.
- A workspace-local frontend form owns selected scope/definition and unsent argument
  inputs. Selection initializes fields from declared types, defaults and required flags.
  Input stays separate from the conversation composer, preferences and backend config.
  Connection reset does not clear these unsent fields, but a new list must re-establish
  definition membership before launch. It is in-memory, not restart-durable.
- Empty optional/defaulted arguments are omitted. Required values without defaults are
  validated locally; string/finite number/boolean/JSON parsing follows the shared
  declaration contract. Unknown keys and changed/missing definitions fail closed. The
  runtime is final authority for defaults, type checks, compile failures and capability.
- Launch callbacks capture workspace and definition scope/name; dispatch records the
  current navigation generation. The selected argument declarations are fingerprinted;
  refreshed declarations must still match, otherwise explicit re-selection is required.
  Argument-default presence is preserved, including JSON `default:null`; optional
  declaration fields with invalid null values fail strict decoding.
  Launch errors are correlated to that form generation, so a later selection is not
  annotated by an old request. Connection interruption retains inputs and reports an
  uncertain launch without automatic retry or deletion.
  One launch per workspace is pending through creation and start. No optimistic run or
  session navigation is written. Enqueue failure, rejection and unsupported capability
  are visible and retain form values and the original conversation draft.

```text
Workflows dock → scoped workflows/list → backend saved store → validated query cache
select definition → local typed argument form → captured Launch
                  → v4 createSession(sessionId:null, payload:{workspaceId})
                  → accepted createSession result.sessionId only
                  → v4 startSavedWorkflow(new sessionId, payload:{name,scope,args?})
                  → accepted startSavedWorkflow result(runId,toolCallId)
                  → same navigation generation: save old draft + open new session
                  → changed navigation: keep current view; new run remains backend-owned
explicit start rejection/failure → best-effort deleteSession(new empty target) + error
unknown/malformed ACK → no resend, no destructive cleanup; report uncertain outcome
```

Create carries no firstInput or config. The target uses runtime defaults, not the current
composer's draft overrides. Start is non-CAS. Only an accepted result with correct type
and nonblank run/tool-call IDs establishes success. Accepted but malformed results are
uncertain, not rejection; deleting a potentially running session would be unsafe.
Explicit rejected/failed/stale start ACK or transport RPC rejection cleans only the
newly created target, never the original chat. Cleanup ACK is checked and cannot be
reported as success if rejected. Missing capability uses the existing stable failed ACK,
not a frontend simulation. Connection-generation/pending gates reject stale process
results; interrupted launch is uncertain and must not automatically retry.

## Run settings

Configure uses existing `amendWorkflowRunSettings`, with captured workspace/parent/run
identity and no CAS revision. Pending/running/errored and nonsuperseded stopped runs are
eligible; completed, superseded or unknown runs are not. The workspace-local form owns
only unsent model/concurrency values and their opening baseline. Blank model means
session model; blank concurrency means no run-specific limit. Inputs are validated against
the current model catalog and positive integer constraints. Values at/above the known
ceiling normalize to no run-specific limit. Only changed fields are emitted; omission
means unchanged, null means reset, and a value means an explicit override. Unchanged
forms cannot dispatch. One pending settings command per run disables repeats.

```text
mirrored eligible run → local form baseline → changed fields + captured owner
                     → amendWorkflowRunSettings → checked ACK
                     → authoritative successor/run stream (no local run replacement)
```

Rejected/failed outcomes retain inputs and sanitized feedback. Accepted or uncertain
outcomes block another Apply until an authoritative projection confirms the submitted
settings, or the user explicitly reloads the current mirrored settings. ACKs never advance
an authoritative baseline. The pending receipt captures the submitted effective values;
confirmation rebases only the comparison baseline and never erases newer input edits.
An accepted result must have the expected type and nonblank run/tool-call IDs; no timer
or guessed successor is used. Changes to the mirrored settings while a form is open fail
closed with a reload action rather than overwriting another controller's changes.
Connection reset removes pending settings requests, retains drafts and reports uncertain
outcomes; Apply remains disabled until a fresh conversation projection arrives. Reload
explicitly discards the local form edits and captures that fresh projection. Invalid and
unchanged forms visibly disable Apply. Form caches are limited to 32 entries per workspace,
and projected removal/completion/supersession releases their entries. If the run becomes
completed/superseded the form disappears and dispatch revalidation fails closed. Native
visual evidence is still separate from interaction fixtures.

## Projected run and successor navigation

The Workflows dock may focus one projected run, with an All runs return control.
AppState owns this local navigation only; the existing conversation projection remains
the sole run/actor/node owner. Selection captures workspace/session/run/tool-call identity,
connection generation, task navigation generation and the local workflow-selection
generation. Every manual run/All runs selection invalidates older same-parent callbacks.
A successor link is actionable only
when supersededBy resolves to a distinct run in that same parent projection with a
nonblank toolCallId. A stale owner, missing target, changed identity or replaced connection
cannot follow the link or change the composer. Focusing preserves parent draft/focus.

When Apply is sent from a focused source, the navigation owner retains one transient
follow receipt. A valid accepted amend result may supply a different run/tool-call pair;
that pair is a navigation hint, not an authoritative run. Follow occurs only after the
exact pair appears in the current source parent projection and the original navigation,
connection and focused source still match. An in-place result, rejection, unknown ACK,
manual All runs/other-run navigation or changed workspace/session discards the hint.
No timer, optimistic run, extra subscription or guessed successor is introduced. The
source settings form lifecycle remains independent, so source supersession may remove
its form without losing a valid pending navigation hint.

```text
projected source supersededBy + projected target → captured link → current owner check
                                               → local focused target
focused Configure/Apply → local follow receipt → checked amend ACK IDs only
                        → matching successor stream → local focused target
manual/context/connection change → no late navigation or draft replacement
```

Acceptance covers pointer successor/All runs, missing/self/blank target refusal, exact
run/tool-call matching, stream-before-ACK and ACK-before-stream ordering, in-place amend,
late owner/connection navigation and untouched parent draft. Native run-detail/artifact
visual and actual executable lifecycle evidence remain separate.

## Focused run artifacts

Opening a projected run shows a read-only deliverable metadata section, using the existing
`v4/conversation/workflowRunArtifacts` query with the parent session and run ID. This is
not saved-definition `workflows/runs` history or the engine's model-facing return value.
The CLI journal remains the artifact owner; GPUI retains only kind, ID/title, latest
version, content type, item count and primary-deliverable designation. Store URIs, source
paths and dashboard specs are never retained or passed to the local opener. The metadata
cache contains no bytes; the optional bounded Markdown selection described below is transient.

`WorkspaceHandle` owns one connection-local query map keyed by parent-session/run pair,
capped at 32 entries. Detail entry fetches once; explicit Refresh fetches again after
settlement. No polling, render-revision refetch, conversation-epoch cursor, extra
subscription or automatic retry is added. A receipt captures exact projected run/tool
identity, workspace/process/navigation/workflow-selection generations and query revision.
Changed selection, parent, process or superseded request discards a late result, even when
the user returns to the same run. Re-entry retires the earlier query and starts a new read.
Connection invalidation clears pending reads and query facts, not the parent draft.

```text
Open run → local focus owner → captured parent/run receipt → existing read-only CLI query
         → journal-owned metadata → strict decode → safe metadata query projection
Refresh → same current owner + new query revision → new read
All runs / other parent / reconnect → late result discarded; no navigation or draft write
```

Loading, successful empty metadata, unsupported capability and ordinary failure remain
distinct. Errors are classified by method-not-found RPC code or the serialized
`V4CapabilityUnsupportedError` class on this exact query, never by raw message text.
The current generic CLI RPC serializer does not preserve its capability reason code;
no invented data discriminator or backend fork is added. Failed refresh retains the origin's previous metadata with visible generic feedback.
A malformed result fails as a whole, without partial fabricated rows. Strict decoding uses
the shared closed artifact-kind vocabulary, bounded strings and 1–16 version contract;
unknown fields/null optionals, duplicate IDs or inconsistent version history fail closed.
The frontend additionally refuses more than 256 metadata rows as a local resource bound.
Only safe metadata survives decoding. Completed, stopped and errored projected runs can
all be inspected; viewing never starts/resumes/publishes a run or opens a filesystem path.

Acceptance requires correct parent/run request, duplicate-read suppression, measured
Open run/Refresh/All runs interaction, empty/unsupported/error separation, malformed and
secret-safe failure handling, late selection/parent/reconnect refusal, and real-CLI
published-artifact metadata evidence. The bounded Markdown adapter is described below;
other content/version viewing, dashboard data rendering,
workspace reveal, cross-project detail and native visual/full executable E2E remain open.
React's current run pane has removed its event log; the retained journal-events helper
is not a reason to add a technical log as a parity feature.

## Bounded Markdown artifact preview

A successful current metadata list offers View Markdown only for `kind:markdown` with
exact `contentType:text/markdown`. This first content adapter reads the listed latest
version through `v4/conversation/workflowRunArtifactRead`, offset 0 and limit 256 KiB,
matching the CLI Markdown publish cap. It never invents older version membership, reads
source/store paths, executes a document, follows a link or fetches external images.

The existing workspace-owned run-artifact query entry owns one optional content selection
and query state. Only the currently focused run retains content; changing run focus or
All runs clears prior preview bytes and pending reads. A selection receipt adds a unique
selection token, artifact ID/version and content-query revision
to the focused parent/run receipt. View replaces the selection; Retry repeats only an
explicit current failed read; Close retires pending correlation and content bytes. Metadata
Refresh clears the content selection before reading a new list. Run/parent/process/selection
change prevents any old response or UI callback from adopting bytes. Re-entry/reconnect
clears content with the same metadata owner; no second cache or durable content store exists.

```text
current metadata + View Markdown → origin/id/version receipt → authorized CLI journal read
                                → one complete bounded response → strict base64/UTF-8 decode
                                → same receipt → passive Markdown preview
Retry → current failed selection only → new query revision
Close / metadata Refresh / changed focus → discard selection; late read ignored
```

The whole response must be strict: exact MIME, canonical base64, decoded bytes ≤256 KiB,
`totalBytes` equals decoded length and `nextOffset:null`. Invalid UTF-8, larger/partial
results or any changed metadata fail closed with generic feedback; no partial text or
fallback renderer is shown. Loading/failed/successful empty text are distinct. The shared
Markdown renderer gains an opt-in passive path that removes link/image destination events;
formatting/labels remain, but no URL callback is registered and raw HTML stays inert text.
Existing chat/file Markdown behavior is unchanged. Preview UI labels use semantic UI sizes;
code blocks retain their existing independent code typography. The preview is scrollable.

Acceptance covers measured View/Retry/Close, exact authorized parameters, size/MIME/progress/
base64/UTF-8 validation, empty text, stale selection/Refresh/parent/reconnect, passive links,
and real-CLI content read plus cross-parent authorization refusal. Other file/media/HTML/
office previews, dashboard data and version navigation remain incomplete; this adapter
is not a full artifact-viewer or native visual acceptance claim.

## Definition inspection

The selected saved definition exposes an explicit Show definition action and Refresh
definition through existing `workflows/get`. The backend resolves names and scope;
GPUI never reads a path supplied by a row. Query results are workspace/connection-local,
keyed by scope and name and capped at 32 entries. A late reply can only settle its
originating query and cannot change the selected form or navigate. Reset drops these
query facts, not unsent argument inputs. A strict success result must match the requested
name/scope and contain valid metadata and script; `{ok:false}` and RPC/malformed errors
are sanitized query failures. Empty scripts are valid; failed refresh retains the previous
read-only definition with visible error and an explicit retry. The script uses independent
code typography and is not executed by viewing it. Listing and inspection are not proof
that a run was compiled or completed. Metadata edits, delete, move and run history are
separate action slices; project-to-global promotion is not the global-to-project move RPC.

## Saved run history

The selected saved definition exposes Show history/Refresh history using existing
`workflows/runs` with scope, name and limit 50. Backend journal history is authoritative;
query caches are workspace/connection-local, keyed by scope/name, capped at 32 entries,
and cleared on reset. Missing journal introspection may return an empty page: the UI says
no history was returned, never proves it was never run. Strict decoding checks status,
stop reason, finite timestamps/tokens, optional fields and at most eight artifact summaries.
Truncation is explicit, not inferred from length. Error refresh retains previous history
and sanitized feedback. Viewing history does not start/resume a run or read artifact bytes.
Open chat requires parentSessionId/toolCallId, current selected history membership and a
matching local workspace cwd (or project scope with omitted cwd). Cross-project global
history is shown but cannot be opened through an invented path-only workspace owner.
Callbacks capture scope/name/run and revalidate before normal session navigation, which
preserves the original draft. Run detail/artifact navigation remains a separate action.

## Saved definition mutations

Metadata edit, Delete and global-to-project Move use existing workspace-level RPCs.
One workspace-local management form owns description/whenToUse/argument-declarations
JSON, a loaded definition baseline, unsent edits, confirmation and sanitized feedback.
The backend remains the file owner. Open requires a successfully inspected definition;
metadata fields are strictly validated, only dirty values can Save. Delete and Move have
an explicit Confirm/Cancel step showing name/scope and destination; Cancel writes nothing.
Move is offered only for global definitions in a known local project workspace, never as
project-to-global promotion. Existing destination refusal is displayed; no overwrite retry.

A captured receipt includes workspace, scope/name, form identity, opening definition and
requested mutation. One pending management operation per workspace excludes launch and
other saved mutations. Field edits update validation and disabled controls without an
unrelated click. Before a mutation, `workflows/get` re-reads that exact definition;
changed metadata/script/path fails closed and requires Reload rather than overwriting an
external edit. Reload explicitly adopts a newly inspected comparison baseline, preserving
local field edits, including edits made while the read is pending. This check is not
backend CAS: concurrent edits after preflight remain a
current protocol limitation. No invented lock, scheduler or local file IO is introduced.
Metadata/save/delete/move send names, never row-supplied filesystem paths. Success requires
strict `{ok:true}` plus nonblank expected paths; `{ok:false}`, malformed or RPC errors
retain inputs and show errors only on the originating form. New edits/navigation during
pending requests are not overwritten. Known successful mutation invalidates origin list/
definition/history queries and refreshes the affected scopes; selection is cleared only
for a matching successfully deleted/moved form. An uncertain response does not repeat a
write or optimistically delete a row. Reconnect retains edits and uncertainty feedback,
but a new inspected definition is required before another mutation. Actual delete/move
smoke operates only on disposable fixtures created by the harness, never user files.

```text
inspected definition → local form → Save / Confirm Delete / Confirm Move
                    → captured receipt → fresh workflows/get → unchanged baseline
                    → existing mutation RPC → validated result
                    → origin query invalidation/refresh (no optimistic file state)
Cancel / stale baseline / error → retain draft, no mutation retry
```

## Acceptance

- Pointer scope/definition selection, argument editing and Launch are reachable without
  changing the conversation composer. Project/global lists are independent and refresh
  is explicit after failure; no render-time filesystem IO.
- Required/defaulted/string/number/boolean/JSON values have deterministic validation.
- Creation must precede start; neither receives input/config/CAS. Stale workspace or
  definition controls cannot dispatch. Pending repeated clicks cannot create two targets.
- Accepted launch opens the new target only if navigation generation still matches,
  saving the previous parent draft. Failure keeps that parent active; only explicit
  failed start cleans the created empty target. Unknown outcomes never auto-resend/delete.
- Background workspace replies, connection reset and changed navigation cannot steal
  focus, replace draft inputs, or populate another query cache.
- Existing run Resume eligibility remains authoritative. Definition/history queries,
  draft-preserving Open chat and checked Apply/Reload are pointer/test-covered actions.
  Metadata Save/Reload, Delete Confirm/Cancel and global-to-project Move have scoped
  pointer controls, stale-definition and uncertainty tests, and disposable CLI lifecycle
  smoke. Focused-run artifact metadata/Refresh has query and pointer regressions plus
  disposable real-CLI publication/read/parent-authorization evidence. Bounded passive
  Markdown View/Retry/Close is reachable; other content/version and cross-project detail
  remain incomplete; a helper alone does not establish a completed action. Native visual/restart/cross-platform and
  executable-to-backend evidence remain required separately from pointer fixtures.
