# Language-server diagnostics after edits (fork, GPUI-gated)

Status: fork feature, 2026-10-20. Only sessions whose host answers
`session/requestRuntimePreferences` with `lspDiagnosticsEnabled: true` (GPUI setting
`gpuiLspDiagnostics`, default on) get it. Electron never sends the field, so nothing here runs
for Electron sessions.

## What the model gets

- After every successful `Edit` or `Write` whose file belongs to a supported language, the
  model-visible tool result ends with a `<diagnostics>` block for that file (quick mode).
- A deferred, read-only `Diagnostics` tool runs a full check for up to 20 files under one
  deadline (`wait_seconds`, default 30, max 120).
- Supported servers (used only when found on PATH): `rust-analyzer` (Rust),
  `csharp-ls` (C#), `typescript-language-server --stdio` (TypeScript/JavaScript).

## Result contract (`DiagnosticsPort.collect`)

Per file:

| Field | Meaning |
| --- | --- |
| `availability` | `ok`, `unsupported` (no server for this language), `unavailable` (server missing or crashed), `starting` (server still loading the project), `capacity` (all server slots busy until the deadline) |
| `freshness` | `confirmed`: a pull-diagnostics response for the content we synced, or a push whose `version` matches the version we sent. `best_effort`: an unversioned push (typescript-language-server does not send versions) or a push from before our sync |
| `check` | `not_requested` (quick), `complete`, `incomplete` (deadline, cancellation, or an edit/restart during the check), `not_supported` (the server has no project check) |
| `timedOut` | the deadline passed before the server answered for this file |
| `diagnostics` | errors and warnings only; hints and information are dropped |
| `knownOtherFileErrors` | errors the server currently reports for other files. "Known", never a project total, unless `check` is `complete` |

Silence is never a clean result: a file with no answer before the deadline is `timedOut` with
`best_effort` freshness.

## Modes

- **Quick** (after Edit/Write, 3 s deadline): syncs the file and collects native analysis only.
  Our rust-analyzer instance runs with `checkOnSave: false`, so edits never start or restart
  `cargo check`. Diagnostics whose `source` is `rustc` or `clippy` come from the last full check;
  they are marked stale and labelled with the time of that check. A 300 ms quiet window only
  merges notifications; it never decides that a result is correct.
- **Full** (`Diagnostics` tool): Rust starts (or joins) one `rust-analyzer/runFlycheck` per
  project root and tracks its work-done progress. Every sync, save or restart in that root bumps
  a generation counter. A check is `complete` only when its progress ends with the generation
  unchanged and no newer check started; otherwise `incomplete`. Waiters share the check; a
  cancelled waiter only detaches. C# uses pull diagnostics when the server offers them;
  TypeScript has no project check (`not_supported`).

## Documents and disk

- A document is open in the server only during a collect: `didOpen` with the current disk
  contents, collect, `didClose`. ZCode never keeps an in-memory copy that could go stale when
  Bash, git or an editor changes the file.
- Files written by ZCode's tools are also announced with `workspace/didChangeWatchedFiles`.
  rust-analyzer runs with `files.watcher: "server"`; tsserver and csharp-ls watch closed files
  themselves.
- Each `publishDiagnostics` replaces the file's previous set; an empty array clears it.

## Client (`packages/adapters/src/lsp/`)

Hand-written subset, no new dependency:

- Content-Length framing counted in UTF-8 bytes; split and coalesced chunks; header ≤ 8 KiB,
  body ≤ 32 MiB, ≤ 64 pending requests.
- `initialize` → `initialized`, full-text sync, canonical `file://` URIs (Windows drive letters
  compared case-insensitively).
- Answers `workspace/configuration` (same settings as the initialization options),
  `window/workDoneProgress/create` and `client/registerCapability`; refuses
  `workspace/applyEdit`; answers any other server request with MethodNotFound.
- 60 s startup deadline, stderr drained to the debug log, crash detection (the next request
  restarts the server), `shutdown`/`exit` then a process-tree kill.

## Lifecycle (`packages/bootstrap/src/zcode-protocol/diagnostics-manager.ts`)

- One server per (language, root). Roots: Rust = Cargo workspace root
  (`cargo locate-project --workspace`, cached per manifest); C# = nearest directory with a
  `*.sln`; TypeScript = nearest `tsconfig.json`, else `package.json`.
- At most 4 servers, counting ones still starting. Only idle servers are evicted (no waiter, not
  starting, no check running). If all slots are busy the request waits within its own deadline,
  then returns `capacity`.
- A server closes after 10 minutes without use while its workspace has no active agent
  (same busy facts as code search), and on shutdown.
- rust-analyzer gets `cargo.targetDir: true`, so its checks build in a subdirectory of the target
  dir and don't block the user's or agents' own cargo builds.

## Display and persistence

One collected result feeds the model trailer and the `file_diff` display's optional
`diagnostics` summary (`errors`, `warnings`, `availability`, `freshness`, `check`, `timedOut`).
Counts are computed before the trailer is truncated (20 lines, 4 KiB). The display is persisted
with the tool part metadata like the diff itself, so a cold reopen shows the same counts. The
CLI contract (`fileDiffToolResultDisplayPayloadSchema`) and the V4 mirror
(`packages/shared/src/zcode-protocol-v4/toolDisplay.ts`) must stay in sync.

## Limits

- Only the edited file's diagnostics are listed in full.
- C# needs the `.sln`/`.csproj` files Unity generates; diagnostics in Unity projects without
  `Library/` are unreliable.
- The first Rust edit after a cold start usually reports `starting`.
- The setting applies to new and resumed sessions; switching it off stops servers at their next
  idle close.
