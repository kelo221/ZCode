# Implementation Plan: GPUI Frontend Parity with Electron (Left-Leaning Chatbox, Tasks Section, and Missing UI)

## Problem Analysis & Findings

From deep investigation of both the Electron desktop source (`packages/ui`, `packages/services`) and the GPUI frontend (`apps/zcode-gpui`), verified against the user's screenshots:

1. **Chatbox Alignment ("not left leaning as in original electron version"):**
   - **Electron (`conversationLayout.ts:5-9` & `ConversationTimeline.tsx:1926-1930`)**:
     - Uses responsive container queries: `@min-[1280px]/conversation:w-[calc(100%_-_24rem)] @min-[1280px]/conversation:max-w-6xl` (24rem = 384px reserved for the right status panel).
     - Applies an explicit panel-avoidance left translate: `@min-[1280px]/conversation:-translate-x-42` (-168px).
     - Net effect: The composer and transcript column start only ~24px from the left sidebar, with 360–384px of space on the right for the floating Git tools / status panel. This gives the signature **left-leaning** appearance.
   - **GPUI (`app/root.rs:348` & `transcript/list.rs:220`)**:
     - Hard-centers both transcript and composer with `.mx_auto()` within the entire window width (`CONTENT_WIDTH = 800`).
     - On wide windows, the composer has 250px+ of empty space to the left, and when the right dock opens (340px), the dock floats right on top of the transcript and composer.
   - **Composer Bottom Bar Ordering**:
     - In Electron (Image 2): Far left is the `+` attachment button, followed by `Full access ▾` (mode menu with shield), spacer, Model selector, Thinking selector (`On ▾` / `Off ▾`), and the round Send button (`↑`).
     - In GPUI: The mode menu was first, followed by a paperclip `📎` button, and the thinking label said `Enabled ▾`.

2. **Missing "Tasks" Section for Project-less Chats:**
   - **Electron (`WorkspaceSidebar.tsx:1580-1635`, `useConversationWorkspaceActions.ts`, `paths.ts:48`)**:
     - Sidebar partitions tabs into two purpose sections: **"Tasks"** (`WorkspacePurpose = "conversation"`) and **"Projects"** (`WorkspacePurpose = "project"`).
     - "Tasks" section has a `+` button (`MessageCirclePlus`) with tooltip "New task".
     - Clicking `+` or "+ New task" resolves the dedicated conversation directory: `~/.zcode/workspace/default` (via `getConversationWorkspaceDir()`), ensures the folder exists, and starts a draft there without any project association.
     - Sessions in `~/.zcode/workspace/default` are rendered under "Tasks". When empty, it displays "No tasks yet".
     - Real projects appear under "Projects", each with its own folder row and `+` button.
   - **GPUI (`sessions/sidebar.rs`, `backend/workspace.rs`, `commands.rs`)**:
     - Only has a static "Projects" header listing all workspaces.
     - Clicking "+ New task" or project `+` only starts a draft in whatever project is currently active.
     - No way to create a task without a project, and no "Tasks" section in the sidebar.

3. **Other Missing UI Elements & Gaps:**
   - **Sidebar Search button**: Electron has a prominent "Search" button with `Ctrl+K` hint right below "New task" that opens the Command Center / quickpick. GPUI has quickpick implemented, but no sidebar button to trigger it.
   - **Header Context Breadcrumb**: Electron displays the project name or "Tasks" context path before the session title (`[Project Name] › [Session Title]`). GPUI only shows the raw session title.
   - **Thinking Selector Display**: Formatted as `On ▾` / `Off ▾` when enabled/disabled, matching Electron.
   - **Quickpick Toggle Command**: Fix mislabeled command in `quickpick.rs`.

---

## Proposed Changes

### 1. Left-Leaning Chatbox & Transcript Layout
- **File**: `apps/zcode-gpui/src/app/root.rs`
  - Adjust the conversation container and composer wrapper (`root.rs:345-366`):
    - Replace the symmetric `mx_auto()` centering with the left-leaning layout matching Electron:
      - Left padding/margin: `pl_6` / left-aligned with `max_w(px(1000.))` and right reservation `pr(px(if self.dock_open { DOCK_WIDTH + 16. } else { 340. }))`.
      - When window width is constrained, smoothly adapt without overflowing.
  - Symmetrically align `transcript/list.rs:220` with the composer card width so message bubbles and the composer input box share the exact same column boundaries.
- **File**: `apps/zcode-gpui/src/app/parts.rs`
  - Reorder composer bottom controls:
    1. Far left: `attach_btn` using `+` icon (`I_ADD`).
    2. Next: `self.mode_menu(cx)` (`Full access ▾` with shield icon).
    3. `div().flex_1()` spacer.
    4. `self.composer_selectors(cx)` (Model selector + Thinking selector).
    5. Stop / Send button.
- **File**: `apps/zcode-gpui/src/composer/tray.rs`
  - Change `render_attach_button`: replace the emoji paperclip `📎` with the clean `I_ADD` (`+`) icon, styled consistently with Electron.
- **File**: `apps/zcode-gpui/src/composer/config_cmds.rs`
  - Update `thought_display()`: format `"enabled"` as `"On"`, and `"disabled"` / empty as `"Off"` (or level name if set to low/medium/high), matching Electron Image 2.

### 2. Dedicated "Tasks" Section in Sidebar & Project-less Chats
- **File**: `apps/zcode-gpui/src/backend/workspace.rs`
  - Add `WorkspacePurpose` enum: `Project`, `Conversation`.
  - Add `purpose: WorkspacePurpose` to `WorkspaceHandle`.
  - Add helper function `conversation_workspace_dir() -> PathBuf`: resolves `~/.zcode/workspace/default` (honoring `ZCODE_DATA_BASE_DIR`, `ZCODE_DESKTOP_HOME_DIR`, `USERPROFILE`, or `HOME`).
  - Auto-create the directory via `std::fs::create_dir_all`.
  - In `discover_workspaces()`: filter out `conversation_workspace_dir()` so it does not get treated as an ordinary project called "default".
- **File**: `apps/zcode-gpui/src/app/store.rs`
  - In `AppState::new`: ensure the conversation workspace handle is always registered (with `WorkspacePurpose::Conversation`).
  - Add helper methods to query the conversation workspace key.
- **File**: `apps/zcode-gpui/src/backend/commands.rs`
  - Add `new_conversation_chat(cx)`: ensures the conversation workspace agent is spawned, sets `active_workspace = Some(conv_key)`, sets `active = None`, `draft = true`.
  - Update `new_chat(cx)`: creates a new task in the conversation workspace (or active workspace if explicitly in a project).
- **File**: `apps/zcode-gpui/src/sessions/sidebar.rs`
  - Partition workspaces into:
    - **Tasks**: Conversation workspace sessions.
    - **Projects**: Project workspaces and their sessions.
  - Render "Tasks" section header with a `+` button (`icon(I_ADD, 12., MUTED)`) with click handler calling `new_conversation_chat(cx)`.
  - Under "Tasks": render session rows belonging to the conversation workspace; if empty, render a muted "No tasks yet" label.
  - Render "Projects" section below, retaining existing project folder rows and per-project `+` buttons.
  - Keep `sidebar.rs` strictly under 400 lines (extracting modular rendering helpers if needed).

### 3. Additional Missing UI Parity Items
- **Sidebar Search Button** (`sessions/sidebar.rs`):
  - Add a "Search" button directly below "New task" with a magnifying glass icon `I_SEARCH` (or `icon(I_SEARCH, 13., MUTED)`) and a `Ctrl+K` shortcut hint. Clicking it opens the Quickpick modal (`self.quickpick_open = true`).
- **Header Context Breadcrumb** (`app/parts.rs`):
  - In `main_header`, prefix the session title with the context tag: e.g. `Tasks ›` for conversation tasks, or `[Project Name] ›` for project tasks.
- **Quickpick Fix** (`app/quickpick.rs`):
  - Fix `ToggleSidebar` action / description.

---

## Verification & Architecture Gates

1. **Compilation & Unit Tests**:
   - Run `cargo test` to ensure all existing 198 unit tests + 1 pty smoke test continue passing, plus add new unit tests for conversation workspace resolution, purpose filtering, and thought display.
2. **Clippy & Formatting**:
   - Run `cargo clippy --all-targets -- -D warnings` (100% clean, zero warnings).
   - Run `cargo fmt --check`.
3. **Line Cap Invariant (<= 400 lines)**:
   - Check line counts of all touched files (`sidebar.rs`, `workspace.rs`, `commands.rs`, `store.rs`, `root.rs`, `parts.rs`, `config_cmds.rs`, `tray.rs`) to ensure every file is strictly `<= 400` lines.
4. **Architecture Check**:
   - Run `bun scripts/architecture/architecture-check.mjs check --changed` to verify 0 policy violations.
5. **Visual Verification**:
   - Launch test preview build to verify the left-leaning chatbox alignment, the Tasks section, and the composer layout on screen.