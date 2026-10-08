# Native Services RPC contract

Updated 2026-10-06 before the approved real-management implementation. Backend, CLI,
protocols, repositories and assembly remain frozen. The existing full stdio Services
Host is reused; no worker or native configuration writer is added. Earlier isolated
results are historical evidence, not acceptance of this extension.

## Modes and owners

Disposable mode retains a fresh owned root established before discovery/launch. Local
management is an explicit process-local opt-in in ordinary Settings. Declining launches
nothing. Consent explains full-Host memory, conditional startup/list migrations, unknown
metadata loss on edit, and unsupported simultaneous Desktop/configuration writers.
Runtime/bundle and Desktop checks precede any real Host start. Tests never activate real roots.

RootView owns scope/search/forms. Shared AppState owns capability, connection generation,
scoped projections, one mutation lane, receipts, closing and reconciliation. The canonical
service owns profile/state persistence. CLI continues owning admission and runtime snapshots.
No private Electron port, remote attachment, second CLI or new backend service is introduced.

The existing serialized Rust preference owner suspends admission before management and
signals when previously admitted writes have drained. New recent-project intents remain
bounded and deferred; other preference controls report suspension. Isolated preferences
remain read-only. No Rust canonical preference write overlaps the owned Host.

```text
explicit activation → suspend preference admission → actual queue drained
  → compatible launch/Desktop check → one full Host → canonical management
leave/idle → stop new admission → finish admitted work → owned process exit observed
  → strict background preference reload/rebase → resume existing serialized writer
```

Returning to workspace is immediate; form closure never cancels an admitted mutation.
Host shutdown cannot touch existing CLI work. Another Settings section starts handoff back.
The last effective native appearance is retained while managing. If backend normalization
omits native theme/font fields, reload retains those effective values in memory, not by
restoring a stale full disk snapshot. Arbitrary settings preservation is not promised.
ISettingService get/update cannot replace all native preferences: its schema lacks native
appearance fields. Exit/reload failure keeps preferences suspended with an actionable error.

Desktop detection is best-effort refusal, not a lock. Exclude only exact currently owned
native CLI/Host process IDs and their observed descendants, not all Node-mode/desktop names. Detecting another GUI
blocks new commits and triggers quiescent retirement; simultaneous use remains unsupported.

## Existing transport and launch

Existing JSON hello/ack, 13-byte binary SocketProtocol, Channel array arguments, bounded
codec/backlogs/correlation, Undefined void success, redacted failures and background IO
remain. UI RPC deadlines are 30 seconds; timeout/cancel is not rollback. No mutation replay.
Expose actual reaper/tree-exit completion; logical connection death is not process death.
Restart cannot create a second owned Host before the old one is observed exited.

Normal launch uses resolved native roots/allowlisted environment and registered local cwd,
not scratch sanitization. Service-reported user root and fresh row paths are authoritative;
workspace keys preserve identity/path semantics. No hard-coded alternate profile parser.
The existing runtime/bundle is discovered/validated; nothing is installed. Missing assets
produce a useful error. Checkout/installed runtime support is not standalone distribution.

## Recovery and acceptance

Fresh-row preflight validates origin, capability, source/scope/ID/path/baseline and models.
Committed receipts survive current-serial refresh failure; old-serial replies are ignored.
Keep bounded uncertain intent/baseline. Reload-and-review observes old Host exit then reads
fresh inventory without replay. Unique supported-state matches are labelled observed, not
original RPC success; acknowledgement permits subsequent explicit commands. Partial/conflict
results remain blocked. Crash recovery cannot invent a durable receipt.

Before enabling, tests cover suspension/drain, deferred recent projects, actual exit before
resume/reconnect, own Host not mistaken for GUI, conflict refusal, missing runtime, stale
connection work, CRUD/overrides, lost reply and post-commit refresh failure. Native populated
screenshots/actions, actual same-scratch executable restart and theme/locale matrix are
separate acceptance layers. Scratch characterization records conditional ISettingService
normalization of native/unmodeled fields, corrupt agent state, Windows held-file refusal and
partial rename. Failure injection uses stopped-root fixtures or owned test file handles;
canonical mutation is never retried, and fresh readback/restart distinguishes partial effects
from success. Files stay <=400 lines. Real user data is never used in tests.

Opt-in management accepts disclosed existing service behavior; it does not claim migration-free,
lossless, concurrent-safe, low-memory or standalone packaged production parity.
