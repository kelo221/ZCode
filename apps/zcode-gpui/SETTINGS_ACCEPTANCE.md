# Native-only Settings acceptance record

Date: 2026-10-06. Environment: Windows checkout, `feat/gpui-frontend`.

**Opt-in local Subagents management and the reference-style redesign are implemented.
Actual native visual/action-matrix acceptance remains incomplete.** The previous inventory
was rejected; its screenshots and test counts below remain historical, not redesign evidence.
The rejected configuration worker/backend edits remain withdrawn. Current changed source
is under `apps/zcode-gpui/`; `packages/`, `apps/zcode-cli/` and architecture policy are frozen.
No toolchain installation, real-profile copy, production Services startup, commit or push
was performed by this slice.

Contracts: [Settings](SETTINGS.md), [Subagents](SUBAGENTS_SETTINGS.md),
[Services transport](SERVICES_RPC.md), [Parity boundaries](PARITY.md).

## Selected-language and Settings-shell correction (2026-10-06)

Settings now replaces the workspace sidebar rather than appearing next to Projects/Tasks.
Its single navigation owns Back to workspace; no New task, workspace search or runtime
footer is rendered while Settings is open. This is a render-only destination switch:
workspace/session, draft, sidebar expansion/limits, dock, terminal, streams and permissions
remain in their existing owners. Compact layout keeps a bounded Settings navigation region.

English is the default for unset/invalid language preferences; explicit Chinese/English/System
and legacy locale remain supported. The selector uses one selected language for its names.
The committed Preferences owner projects native and Ely built-in localization together.
Retained search inputs update placeholder copy on a language change without losing entity,
query or focus. Default synthetic provider/custom/plugin display content is now English-only;
real user/plugin content is not translated or rewritten.

- Final full native suite: **527 unit tests passed, zero failed, 11 ignored**; separately
  one PTY, eight reverse-RPC and one source-limit integration tests passed. The ten ignored
  real-Host cases are not rerun for this locale/render-only correction; their earlier explicit
  results below remain separate evidence. Strict all-target Clippy, Cargo formatting and
  executable build passed. Rust/Python 400-line, syntax and frozen-source checks passed.
- Language interaction tests click the real Ely selector in TestApp, then select Chinese,
  System and English by keyboard. They verify save/readback/reinstalled owner, single-language
  choice copy, native/Ely agreement, retained search placeholder updates and unchanged synthetic
  user content. Read-only/suspended controls and Desktop-refused persistence remain unchanged.
- Shell pointer tests prove workspace sidebar absence at 1280px and 700px, and its restoration
  by Back with the same workspace/session, draft, expansion, row limits, dock and terminal.
  Existing streaming/permission and Settings navigation tests pass with the exclusive shell.
- Literal `pnpm typecheck` and `pnpm lint` remain unavailable (exit 127). Installed Bun root
  equivalents passed; lint reports **71 warnings, zero errors**. Freshness and architecture
  passed with zero violations. GPUI context generation remains unavailable because it is
  unmanaged by the TypeScript policy; that policy was not changed.
- Three Python tests pass for monolingual compact/long synthetic inventories and explicit
  locale-override omission. The harness defaults to saved preference and accepts
  `--locale preference`; isolated preference controls remain read-only.

Actual native captures were directly inspected on an interactive Windows desktop:
`%TEMP%/gpui-native-acceptance-u7avta0k/settings-1.png` shows final default-English General
with Settings-only navigation; `models-shell.png` actually shows the Subagents section
(the capture name came from an imprecise click target), also without Projects/Tasks;
`workspace-return.png` shows actual Back restoring the workspace sidebar.
`%TEMP%/gpui-native-acceptance-ozjm5aje/settings-3.png` shows final saved-Chinese General
with the same exclusive shell and Chinese labels. The Chinese fixture was seeded only
while the harness-owned root was stopped and locked; these captures omit locale overrides.
Earlier `settings-1.png`/`settings-2.png` in that directory precede the shell correction.
All captured owned process identities, including the scratch Services Host, exited.

The actual selector save is proven by TestApp interaction, not by making isolated native
controls writable. These General/Back captures do not establish full Subagents reference
visual parity, all native controls, or the complete locale/theme/font acceptance matrix.
Logs: `%TEMP%/gpui-selected-language-shell-tests.log`,
`gpui-selected-language-shell-clippy.log`, `gpui-selected-language-shell-build.log`, and
`gpui-selected-language-shell-root-lint.log`. Initial red regressions proved System default,
workspace sidebar presence, and stale English search placeholder; the final suite passes.

## Implemented opt-in extension: verification in progress

Opt-in local management, preference admission suspension/drain, observed Host tree exit,
fresh reload, mode-aware preflight and uncertain-outcome review are implemented. Dense grouped
Ely rows, compact toolbar and inline model controls replace the rejected stacked inventory.
No Host has been automatically opened against real configuration. Current gate results will
be recorded separately below; older evidence does not prove the new visual composition.

## Current opt-in extension verification (2026-10-06)

- Full native tests: **522 unit tests passed, zero failed, 11 ignored by default**. The
  ignored set is ten real-Host cases plus one synthetic owned-tree leaf fixture. Separately,
  one PTY, eight reverse-RPC and one source-limit integration tests passed.
- All ten `real_unchanged_host` cases explicitly passed against the unchanged bundle and
  installed compatible runtime. Fixture writes/restart now wait for strict root/tree exit,
  not only disappearance of a root PID.
- Strict all-target Clippy passed with `-D warnings`. Cargo formatting, Rust/script source
  cap and frozen backend/dependency diff checks passed. Build output's ReFS hard-link fallback
  is infrastructure, not a suppressed Clippy diagnostic.
- Literal `pnpm typecheck`/`pnpm lint` failed to launch because pnpm is unavailable. Installed
  Bun equivalents passed: root typecheck; lint **71 warnings, zero errors**; architecture
  zero violations/baseline/new. Freshness passed. GPUI is unmanaged by the TypeScript
  architecture policy; a policy pass is not a Rust owner audit.
- Standalone new TypeScript acceptance scripts passed strict dedicated checking with installed
  Node type definitions, focused lint with zero warnings/errors and focused formatting.
  An initial command incorrectly requested unavailable Bun type declarations and failed;
  no type package was installed. Python syntax and safe capability/fixture self-checks passed.
- The correlated same-CLI-server runtime smoke passed again with five loopback requests,
  immutable existing-A schema and fresh-B schema, and owned process/scratch cleanup.

### Handoff and failure evidence

Preference tests cover actual admitted-write drain, bounded deferred recent projects, no
new native preference admission during management, corrupt reload remaining suspended,
and observed owned exit before deferred persistence. A fresh-disk deferred patch retains
effective native appearance in memory without adding omitted native fields or replacing
unknown/fresh disk state from a stale whole snapshot. Activation Cancel starts no Host.

Deterministic process snapshots prove exact own-runtime/descendant exclusion while retaining
unowned Desktop detection. The test no longer assumes the real machine has no other Desktop.
Strict Services tests exercise actual synthetic descendant cleanup, early setup/startup
failures with retained observers, observation failure remaining unsafe and buffered reply drain.
A TestApp lifecycle fixture originally allowed its blocking background wait to time out before
releasing the peer; it now releases the peer within the same update turn and passes without
using timeout as exit evidence.

Fresh-row preflight, scoped identities, stale callback guards, complete inline selection and
one mutation lane have unit/interaction coverage. A real service create with its client reply
deliberately discarded is found uniquely after strict Host restart and classified as observed;
that test does not simulate a network sever precisely after commit or prove GUI recovery.

### Unchanged-Host characterization

- With migration flags already initialized, native theme/font and a synthetic unknown field
  stayed on disk, while `ISettingService.get` omitted them from its projection.
- With migration flags missing, the observed `get`/normalization removed those fields and
  initialized the flags. Restart preserved that normalized disk state. Hydration is not
  unconditionally read-only, and arbitrary settings preservation is not claimed.
- Corrupt settings produced one backup retaining original bytes; canonical settings remained
  absent. Corrupt agent state stayed corrupt on disk while lists returned enabled defaults.
- An owned Windows held state file caused one mutation RPC to return a real error. Fresh
  list/restart showed unchanged state bytes and enablement. The unchanged backend's internal
  atomic-rename retries remain; no extra mutation RPC was sent.
- A held old Markdown file caused rename failure after new-file creation and disabled-ID
  migration. Old and new profiles both survived restart (old enabled, new disabled). The
  recovery classifier rejected this partial result; no mutation replay or cleanup was invented.

These are Windows scratch/service observations, not arbitrary permissions, UI reconciliation,
non-Windows or real-root acceptance. Unknown YAML loss on supported service edits remains
explicitly disclosed.

### Current native GUI acceptance

The redesigned executable has valid, directly inspected native captures at 1280×900 dark/English
and 860×700 light/Chinese. The populated dark inventory shows one compact toolbar, rounded
custom/plugin groups, horizontal icon/badge rows and aligned switch/delete/inline-selector
controls. The narrow light/Chinese capture shows shrinkable rows and the compact navigation
scroll region; it is a single position, not complete narrow/scroll acceptance. Relevant files:
`populated-dark.png`, `form-visible.png`, `native-saved.png`, `restart-edit.png`, and
`light-zh-narrow.png` in the current probe evidence directory.

Actual New/form typing/Save created `native-actual-restart`. The canonical scratch Markdown
contains the entered name, description, yellow color, injectAgentsMd and prompt. The same
GPUI executable was relaunched against the validated capability; refreshed inventory and
Edit displayed those supported values. Current stopped-root readback still matches. This is
actual creation/persistence/restart evidence, not only a Host restart or synthetic TestApp.

The first populated fixture used `home/.zcode/v2/agents` incorrectly. Only three verified
synthetic fixture files were moved to the service's canonical `home/.zcode/agents` after
owned-process exit and under the root lock. Future fixture generation now uses the correct
root. No Rust production profile writer was added.

Foreground availability was intermittent: an initial attempt found `Screen-saver`; later
attempts were refused when unrelated windows took focus. One task-switcher-occluded capture
(`form-current.png`) is explicitly rejected and not evidence. Guarded captures require
interactive desktop and owned foreground both before and after acquisition, but OS overlays
still require visual inspection. No secure-desktop or global keyboard workaround was used.

Further native rename/delete/enablement/inline-override, keyboard, scrolling, uncertainty UI
and full theme/locale/font matrix remain incomplete because repeated foreground interference
prevented reliable actions/captures. No false success is inferred from those attempts. All
captured owned process identities were confirmed gone at generation 15; no unrelated process
was killed. The inspected subset demonstrates the rebuilt composition, not full reference
visual parity or complete native end-to-end acceptance.

Current logs: `%TEMP%/gpui-final-current-tests.log`, `gpui-final-current-host.log`,
`gpui-final-current-clippy.log`, `gpui-current-runtime.log`, and `gpui-current-script-types.log`.
Disposable probe evidence is `%TEMP%/gpui-native-acceptance-uqqpaf5k`; older captures below
remain historical and do not prove the redesign.

## Earlier automated gates

- Freshness passed through installed Bun; branch matches its origin and is within the
  configured main-branch freshness threshold. Literal `node` is unavailable on PATH.
- `cargo fmt --manifest-path apps/zcode-gpui/Cargo.toml --check`: passed.
- `cargo test --manifest-path apps/zcode-gpui/Cargo.toml -- --test-threads=1`: **494 unit
  tests passed, zero failed, four real-Host tests ignored by default**. Separately, one
  PTY, eight reverse-RPC and one source-limit integration tests passed.
- `cargo clippy --manifest-path apps/zcode-gpui/Cargo.toml --all-targets -- -D warnings`:
  passed. ReFS incremental-cache hard-link fallback notices are infrastructure warnings,
  not suppressed source Clippy diagnostics.
- The four ignored `real_unchanged_host` tests were explicitly run against the existing
  bundle/installed compatible runtime: **four passed**, zero failed.
- Literal `pnpm typecheck` and `pnpm lint` failed to launch because pnpm is unavailable.
  Installed Bun equivalents passed: root typecheck; root lint with **71 existing warnings,
  zero errors**; architecture with zero violations, zero baseline and zero new violations.
- New runtime scripts passed dedicated strict standalone TypeScript checking, focused lint
  with zero warnings/errors, and focused formatting. Root typecheck alone would not cover
  these standalone files. TypeScript architecture leaves GPUI unmanaged; its pass is not
  a Rust ownership audit.
- Rust source/test cap passed; a separate sweep of Rust plus four new acceptance scripts
  found no files above 400 lines. Frozen-source diff guard was empty.

Logs are local acceptance artifacts under `%TEMP%`:
`gpui-final-tests.log`, `gpui-final-clippy.log`, `gpui-final-host-tests.log`,
`zcode-subagents-runtime-acceptance-Kji1Sk.log`, and
`zcode-subagents-runtime-lint-oUTqAT.log`. They are not portable distribution artifacts.

### Intermediate failures, not hidden passes

The first final Cargo command could not replace the running native executable on Windows
(`Access is denied`) and never reached tests or Clippy. On the subsequent run, one
refresh-failure regression used a pre-commit query serial. The implementation correctly
ignored that stale reply. The test now asserts the post-commit serial and committed receipt
separately; final full tests passed. No backend behavior was weakened for either failure.

## Pure/native test evidence

Transport tests exercise reference framing/codec bytes, fragmentation, bounds, Undefined
versus JSON null, void success, correlation, cancellation/deadlines, EOF and owned-process
cleanup. Profile tests cover canonical DTOs, safe model metadata, permissions, grouping,
complete structured selections, form validation and preservation of supported fields.
Preflight rebuilds update/delete paths from fresh service rows and checks scope, baseline,
capabilities and model selection. Receipts retain the latest result within the 64-item bound.

GPUI TestApp pointer/keyboard fixtures cover direct Settings navigation, per-window state,
Back/focus and parent draft preservation, compact navigation/search, read-only configured
models, complete shortcut vectors, New/form validation, multiline input, captured mutation
origin, Back while pending, failed-query render suppression and stale form refusal. These
use NoopTextSystem; they prove state/interactions, not native glyph or accepted composition.
Some mutation tests directly exercise settlement functions; they are not actual RPC IO.

## Real unchanged Services Host

All roots are freshly created scratch storage. Synthetic provider metadata is installed
before startup; management thereafter uses canonical Channel RPC. No real keys/profiles
are copied and metadata-only tests make no API calls.

Four tests verify:

1. Supported fields, CRUD/edit/rename/delete, user disablement and Host restart readback.
2. Both built-ins' full model/reasoning overrides, clearing and restart readback.
3. Same profile name across user and two workspaces without scope leakage.
4. Plugin defaults, version-stable override identity, clearing/restart and unchanged source
   bytes; runtime alias projection is distinct from Settings plugin rows.

**Preservation limitation confirmed:** hydration retains an existing unknown YAML field
and comment, but service-driven edit rewrites supported metadata and discards those unknown
bytes. Native SubAgentConfig preservation is not unknown-Markdown preservation. This is a
documented existing-Host limitation accepted by opt-in disclosure, not a passing lossless-edit
scenario or a capability supplied by the native frontend.

## Same CLI app-server runtime snapshots

The new [runtime smoke](scripts/subagents_runtime_smoke.ts) and
[transport helper](scripts/subagents_runtime_transport.ts) use existing public contracts,
one unchanged Services Host, one CLI app-server and a loopback provider in disposable roots.
Final formatted/split harness passed twice, with five model requests per run:

```text
Host RPC creates/persists v1
  → session A model request exposes v1 and remains held/running
Host RPC edits/persists v2 while A remains running
  → new session B on the same CLI server exposes v2 and completes
release A → A completes → next admitted A user turn exposes v1, not v2
```

Unique synthetic user markers correlate A/B/next-A requests; array position is not used to
infer next-turn ownership. Two tool-less requests were observed. An earlier v2 inference
mistook interleaved B work for A and was withdrawn before the final correlated assertions.
Both runtime-preference scopes were observed. Owned Host/CLI exited, mock connections
closed and scratch was removed by the harness.

Evidence is **model-facing Agent schema name/description snapshot behavior only**. No child
Agent invocation, profile systemPrompt/model/reasoning execution or next-A completion is
asserted. This is not native GUI, remote replay, hot reload or executable restart evidence.

## Actual Windows native interaction

An isolated debug native executable opened the Subagents section directly. Actual pointer
messages and text input reached New, empty-name validation, name/description/prompt fields
and Save. Refreshed inventory displayed `native-ui-agent` with the synthetic description.
The exact scratch Markdown file was subsequently read: name/description, yellow color,
injectAgentsMd and the synthetic prompt persisted. These are not screenshot-only effects.

Evidence directory: `%TEMP%/gpui-native-acceptance-wh3j34uz`; valid captures include
`settings.png`, `form.png`, `invalid.png`, `typed-name.png`, `typed-description.png`,
`typed-form.png`, and `saved.png`. Earlier occluded/focus-refused probes do not count.
The owned native probe, original CLI and original Host PIDs were checked absent at final
verification; no `zcode-gpui.exe` process remained. No unrelated process was killed.

Only English/dark and a subset of native form interactions were observed. The full native
light/dark, English/Chinese, narrow scrolling, focus/keyboard, delete and override matrix
is not verified. Host restarts are verified, but actual GPUI executable persistence restart
is not; fresh isolated launches intentionally allocate different roots.

## Memory and lifecycle measurements

Earlier isolated reference-Host measurement on this checkout:

- Cold list/model startup: 436 ms.
- Active RSS: 191.66 MiB; private memory: 148.44 MiB.
- Idle after two seconds: 193.58 MiB RSS; 150.05 MiB private.
- Child count: zero. A Settings Host does not itself create a workspace CLI server.

Actual native debug-process snapshot:

| Process                   | RSS MiB | Private MiB |
| ------------------------- | ------: | ----------: |
| GPUI                      |   64.64 |      122.18 |
| Existing conversation CLI |  255.15 |      791.05 |
| Separate Services Host    |  190.71 |      145.76 |

Total RSS: **510.50 MiB**. A separate earlier isolated native/CLI-only snapshot was about
326.41 MiB, but these are not matched production benchmarks. Working set/private memory
vary over time; they must not be conflated or sold as a low-memory budget pass.

The implementation retires the separate Host after 60 idle seconds only with no startup,
reads, validation, transport calls or mutation. One native probe observed Host retirement
while CLI remained, then a later Save started a Host again. This is partial native lifecycle
evidence, not complete idle/reconnect/race acceptance. Windows owned-process cleanup is
exercised; non-Windows lifecycle remains unverified.

The full Host eagerly initializes broader services and can conditionally write identity,
provider/settings/state files or migrate Markdown. These measurements do not establish a
narrow service set, a no-write hydration guarantee or acceptable production coexistence.

## Earlier remaining gates (superseded by opt-in extension)

- **Rejected appearance:** compact toolbar, grouped horizontal rows, icons/color indicators,
  badges, hierarchy and aligned trailing controls do not match the Desktop reference.
  No visual redesign was made after the user's criticism.
- Native theme/locale/keyboard/scrolling matrix and actual executable persistence restart.
- Full connection-generation/overlapping-read/mutation reconnect scenarios.
- Corrupt/unwritable state, partial rename/delete failures, metadata preservation and usable
  uncertain-write reconciliation. Writes remain blocked after an uncertain outcome.
- Migration-free real hydration and cross-process single preference/configuration ownership.
- Lower-memory production budget and supported packaged runtime/bundle.
- macOS/Linux and full GPUI-to-real-backend rendered conversation evidence.

The earlier isolated-only restriction is superseded by explicit opt-in local management.
Disposable transport, persistence and runtime-schema evidence still does not waive native
visual acceptance, packaging, concurrency or low-memory release limitations.
