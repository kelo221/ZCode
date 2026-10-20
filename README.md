# ZCode fork: native GPUI client and agent improvements

> **Unofficial.** This is a personal fork of [zai-org/ZCode](https://github.com/zai-org/ZCode). It is not an official ZCode product and is not supported by the ZCode team.

This README only covers what the fork adds. For ZCode itself (the Electron desktop app, the Web client, the CLI distribution, packaging, configuration and the project notice), see the [upstream repository](https://github.com/zai-org/ZCode) and its README.

The fork tracks upstream releases (currently v3.16.1) and keeps its changes on `main`. The Electron desktop app and the Web client are left as upstream ships them.

## What the fork adds

### A native desktop client: `apps/zcode-gpui`

A Rust frontend built with GPUI (Zed's UI framework) that talks to the unchanged ZCode CLI app-server over stdio. It does not need Electron, so it uses about 300 MB less memory than the Electron desktop app and less CPU. See [apps/zcode-gpui/README.md](apps/zcode-gpui/README.md) for setup, features and current gaps compared with the desktop app.

Highlights beyond a plain port:

- **Noise-free transcript.** A composer toggle (on by default) folds runs of tool calls into one line and keeps agent text, todo updates, questions, plans and thinking visible. Thinking is shown open in a muted style.
- **Browser Use in your own browser.** The agent's browser tool drives tabs in Helium or Google Chrome through a ZCode extension ([apps/zcode-browser-extension](apps/zcode-browser-extension/README.md)) and a small native host. See [BROWSER_BRIDGE.md](apps/zcode-gpui/BROWSER_BRIDGE.md).
- **Voice.** Optionally, the main agent gets a `Speak` tool and says a short sentence on your PC when it needs you or finishes. Speech is generated locally by [apps/zcode-voice](apps/zcode-voice/README.md) (Kokoro). See [VOICE.md](apps/zcode-gpui/VOICE.md).
- **Phone remote control.** An Android build of the same window ([apps/zcode-gpui-android](apps/zcode-gpui-android/README.md)) pairs with the desktop over the local network by QR code. See [REMOTE_CONTROL.md](apps/zcode-gpui/REMOTE_CONTROL.md).
- **System prompt override.** Settings › System prompt replaces the main agent's system prompt for every workspace.
- **Agent branches review.** The Review pane lists branches produced by isolated subagents (see below) and shows their diff against the commit they started from, read-only.
- **Idle unload that respects background work.** Inactive workspaces unload their agent after 10 minutes, but not while background subagents, shells or workflows are still running.

### Agent and CLI changes: `apps/zcode-cli`

GPUI turns these on through optional runtime preferences. The Electron app never sends them, so its behavior is unchanged.

- **Built-in code search.** `FindFiles` (fuzzy path search) and `SearchCode` (plain or regex content search) use an in-process fff index per workspace. The index closes about a minute after the workspace goes idle. Setting: Agent capabilities › Code search.
- **Diagnostics after edits.** After Edit, Write or ApplyPatch, the agent receives errors and warnings for that file from its language server (rust-analyzer, csharp-ls or typescript-language-server, whichever is on `PATH`). A `Diagnostics` tool runs a full project check. Results state whether they are confirmed, best effort or stale; silence is never reported as a clean file. Spec: [lsp-diagnostics.md](apps/zcode-cli/docs/design/v2/tool/lsp-diagnostics.md).
- **Worktree isolation for subagents.** `Agent` accepts `isolation: "worktree"`, or an agent file can set `isolation: worktree`. The subagent then works in its own git worktree under `.zcode/worktrees/` on branch `zcode/<agentId>`, starting from HEAD (your uncommitted changes are not included). When it finishes, its changes are committed to that branch, or the worktree is removed if nothing changed, and the parent agent is told how to merge it. Spec: [07-subagent.md](apps/zcode-cli/docs/design/v2/tool/07-subagent.md).
- **Subagent IDs survive restarts.** `SendMessage` can resume a subagent from its saved metadata after the CLI restarts, a foreground subagent fails, or the conversation is compacted. A `ListAgents` tool lists a session's subagents.
- **Telemetry is off.** The CLI's OpenTelemetry export is hard-disabled and cannot be turned back on by environment variables. GPUI sends no telemetry of its own.

### Tooling

- **Bun instead of pnpm and Node.** The root `package.json` declares `bun@1.4.3`. `bun run typecheck:fast` is an advisory type check with Bun's TypeScript 7 checker; `bun run typecheck` (tsc) stays the gate.
- **Compiled backend (prototype).** `apps/zcode-gpui/scripts/build_backend.ts` compiles the CLI app-server into a single executable with `bun build --compile`.
- **`packages/browser-commands`.** Browser commands moved out of the Electron package so the extension and the desktop app share them.

## Download (Windows)

Windows x64 builds are on the [Releases page](https://github.com/kelo221/ZCode/releases). Unzip anywhere and run `zcode-gpui.exe`. The zip includes the compiled CLI backend, the browser extension and native host, and the voice program, so no checkout, Node or Bun is needed. The executables are not code-signed.

Releases are built by [gpui-release.yml](.github/workflows/gpui-release.yml). Pushing a tag such as `gpui-v3.16.1-1` (upstream version, then the fork's build number) publishes one; running the workflow by hand only produces a downloadable artifact.

## Building from source

Requirements: Git, [Bun](https://bun.sh/) 1.4.3 and a Rust toolchain (edition 2024).

```sh
bun install --frozen-lockfile --ignore-scripts
for p in contracts dynamic-workflow adapters shared-types core dynamic-workflow-runtime i18n telemetry bootstrap tui; do
  (cd apps/zcode-cli/packages/$p && bun run --bun build)
done
bun run start:gpui    # opens the repository itself as the workspace
```

To open another project, run `cargo run --release --manifest-path apps/zcode-gpui/Cargo.toml -- --workspace <absolute_project_path>`. `bun run package:gpui` stages a runnable copy in `dist/zcode-gpui/`. The optional browser extension, native host and voice program have their own build steps in [apps/zcode-gpui/README.md](apps/zcode-gpui/README.md).

Models are reached through API-key providers. GPUI cannot configure providers yet; set one up through the upstream desktop app or the CLI.

## Documentation

| Topic                                           | Where                                                      |
| ----------------------------------------------- | ---------------------------------------------------------- |
| GPUI client setup, features and troubleshooting | [apps/zcode-gpui/README.md](apps/zcode-gpui/README.md)     |
| What GPUI covers compared with the desktop app  | [apps/zcode-gpui/PARITY.md](apps/zcode-gpui/PARITY.md)     |
| GPUI settings                                   | [apps/zcode-gpui/SETTINGS.md](apps/zcode-gpui/SETTINGS.md) |
| Everything else about ZCode                     | [zai-org/ZCode](https://github.com/zai-org/ZCode)          |

## Staying in sync with upstream

Upstream is the `upstream` remote and is merged into `main` on each release:

```sh
git fetch upstream
git merge upstream/main
```

Upstream edits to `README.md` and `README.en.md` will conflict with this file. Keep this README and keep `README.en.md` deleted.

## License

Apache-2.0, like upstream; see [LICENSE](LICENSE) and [NOTICE.md](NOTICE.md). The optional voice program `apps/zcode-voice` is a separate GPL-3.0-or-later executable because it statically links eSpeak NG. GPUI only starts it and talks to it over stdin/stdout.
