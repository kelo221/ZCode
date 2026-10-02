# zcode-gpui

Native Rust/GPUI frontend: **All project sessions + Chat + Model / Thinking Level / Collaboration Mode selection**. Heavy settings, plugin marketplace, terminal, and previews remain in the original Electron application — both share the same backend daemon and the same persisted data.

## Running

```bash
cd apps/zcode-gpui
cargo run --release -- --workspace <absolute_path_to_default_project_directory>
```

## Features

- **All Projects Visible (Lazy Spawning)**: Discovers user project directories from `~/.zcode/v2/setting.json` (the exact same data source as the desktop app). Only the active project spawns an agent backend process upon launch (other projects remain idle until clicked); idle agent processes automatically unload after 10 minutes (`ZCODE_GPUI_IDLE_SECS` configurable; active running sessions never unload). Child processes are bound to a Windows Job Object to terminate immediately when the frontend closes or crashes. RAM consumption: ~267 MB for a single project (compared to ~1 GB previously with all-project eager spawning).
- **Model + Thinking Level Selection**: Title bar pop-up menus. The model catalog is harvested from the legacy `session/create` settings snapshot (the standalone CLI workspace-config topic does not carry the full catalog, which is a host-side data source), followed immediately by `deleteSession` cleanup of probe sessions. Switching uses `switchModelConfig` (CAS command with `baseRevision`, automatically retrying once with `revisionAtDecision` on stale collisions); thinking level switches independently using the model's available `thoughtLevels`.
- **Collaboration Mode Selection** (above composer): Ask before changes (`build`) / Edit automatically (`edit`) / Plan mode (`plan`) / Full access (`yolo`, server-side full autonomy). Switched via `switchCollaborationMode` within active sessions, or initialized into `createSession.config` during drafts.
- **Interactive Permission & Elicitation UI**: Renders the V4 `pendingInteractions` region as inline cards (Allow once, Allow always, Deny, Full access, deny-with-feedback, AskUserQuestion questions with single/multi-select, free-text answers, Decline) and answers only through V4 `resolveInteraction`. The legacy `interaction/request*` stdio requests are intentionally left unanswered; the backend cancels them when the V4 answer arrives.
- **Turn Metadata & File Undo**: Displays elapsed turn execution time ("Worked for 12m 8s"), per-turn file change summary bar ("N files changed +a −b"), and an interactive **Undo** button invoking `applyFileRewind`.
- **Turn Plan Progress Checklist**: Live header progress badge ("Progress 6/6") and collapsible checklist container mirroring the V4 `plan` and `goal` regions.
- **Follow-up Queue Panel**: Displays queued turns with "Send now" buttons, delete buttons, and a Pause/Resume auto-drain toggle (`sendQueuedNow`, `deleteQueueItem`, `setAutoDrain`).
- **Composer 2.0 with Draft Persistence**: Multi-line editing with Shift+Enter for newlines and Enter to send. Uncommitted drafts are saved atomically per session and workspace, and restored automatically when navigating between conversations.
- **Rich Rendering**: Assistant replies render as Markdown — headings, lists, quotes, GFM tables, task lists, inline bold/italic/strikethrough/code — with syntect syntax-highlighted code blocks (language label + Copy button) and clickable external links. Tool outputs carrying unified patches render as color-coded diffs. The transcript is virtualized (only the visible window is built), so 10k-row sessions scroll at full frame rate.
- **Tool Dock (Ctrl+B)**: right-side dock with three panes — **Review** (git status with unstaged/staged filters, per-file +/− stats, expandable diffs, Stage all / Commit / Push against the local `git` CLI; auto-refreshes when a turn completes), **Files** (lazy file tree with text/markdown preview), and **Terminal** (a real local shell per workspace via ConPTY + VT parsing, color-rendered in the UI). All three spawn lazily and cost nothing until opened.
- **Local Session Search**: Filter sessions in the sidebar in real time by title.

## Architecture: Wire Protocol Alignment

```
┌────────────────────┐  Per-workspace spawn + stdio (NDJSON)  ┌──────────────────────────┐
│  zcode-gpui (Rust) │ ─────────────────────────────────────► │  zcode-cli app-server    │
│  GPUI Window/State │ ◄───────────────────────────────────── │  (Node/bun/Electron-Node)│
└────────────────────┘    V4 Wire: subscriptions/frames/CAS   └──────────────────────────┘
           Backend owns session state, streams, and admission queue.
           Frontend mirrors snapshots/deltas and dispatches CommandEnvelopes.
```

- **Protocol**: `packages/shared/src/zcode-protocol-v4/` (wire version 3). Each workspace subscribes to `sessions-index/<ws>`, `workspace-config/<ws>`, and on-demand `conversation/<sid>`; commands dispatch over `v4/command` (CAS commands include `baseRevision`); frames include base64 fragmentation + CRC-32 reassembly and sequence gap detection.
- **Ownership Boundaries**: Agent process lifecycles, CommandInbox admission, task leases, and remote connection registries belong strictly to the backend/host; this frontend exclusively manages drafts, optimistic UI, selector state, and window rendering.
- **Zero Backend Forks**: Upstream updates from `zai-org/ZCode` can be pulled directly via git without breaking frontend compatibility.

## Source Layout (Vertical Slices)

Each folder under `src/` owns one feature end to end — its state, effects and views:

| Slice | Owns |
| --- | --- |
| `app/` | Root window view (`root.rs`), header/composer card (`parts.rs`), right dock (`dock.rs`), `AppState` (`store.rs`) |
| `backend/` | Agent process launch, V4 wire codec, event routing, response correlation, outbound commands |
| `conversation/` | Mirrored conversation model, rows, turn metadata/plan, queue, interactions, user actions |
| `transcript/` | Virtualized chat list, row rendering, scroll behavior (follow-bottom, middle-click autoscroll) |
| `composer/` | Message input, mode/model/thinking pickers, config catalog |
| `sessions/` | Project + session sidebar |
| `review/` | Git status, diffs, stage/commit/push |
| `files/` | File tree + preview |
| `terminal/` | PTY lifecycle, grid conversion, terminal drawer |
| `shared/` | Theme tokens and markdown/diff renderers (depends on no feature slice) |

Slices extend the two central types (`RootView`, `AppState`) with their own `impl` blocks, so every slice depends on `app/`. Tests live next to their module as `*_tests.rs`. Keep every file at or below 400 lines (`find src -name '*.rs' | xargs wc -l`).

## Backend Candidates (Auto-resolved in order)

1. `ZCODE_GPUI_AGENT_PROGRAM` (+ optional `ZCODE_GPUI_AGENT_ARGS`, defaults to `app-server --stdio`) — explicit override.
2. **Installed ZCode Desktop Runtime** (`ELECTRON_RUN_AS_NODE=1 ZCode.exe resources/glm/zcode.cjs app-server --stdio`) — default production path, updating in lockstep with the desktop app.
3. **bun + repository source code** (experimental fallback: bun 1.4.x mis-resolves `@zcode/shared/*` subpaths in this repo and dies before startup, in which case the launcher falls back automatically).

## Troubleshooting

- Red `no backend` badge on workspace: The directory does not exist or all agent backend spawn candidates failed.
- `model_request_failed` on send: Missing model credentials or configuration for this workspace; log in or select your provider in the primary application.
- `ZCODE_GPUI_LOG_STDOUT=1`: Emits internal trace logs to stderr.
- Built with Rust 1.98+ (2024 edition) and `gpui = "0.2.2"` (Windows DirectWrite backend).
