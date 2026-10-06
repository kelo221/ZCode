# Local terminal selection and copy

Status: reachable bounded interaction with automated acceptance on 2026-10-06; native OS clipboard and glyph/layout acceptance remain open. The reachable React bottom terminal uses
xterm selection and consumes Ctrl/Cmd+C only when selection exists. GPUI adds that local
interaction without copying another application's terminal implementation or creating a
second PTY/output/scrollback owner.

## Owner and rules

`TermPane.term` (alacritty) owns text, scrollback and selection. Pointer-drag bookkeeping
and the measured grid bounds belong to `TermPane`; they are not a second text selection.
Left press focuses the terminal and begins a simple cell selection, drag updates its end,
and release inside or outside stops dragging. A click with no drag clears the old
selection. Selection uses alacritty's wide-character/wrapped-line rules and current display
offset, including scrollback. New output and resizing keep alacritty's selection behavior.
Grid rows and column geometry must correspond to the actual pointer-to-cell mapping;
unstyled empty cells and wide glyph spacers must not shift selection highlights. Rendering,
resize and hit-testing use the same monospace advance and line height. Native metrics use
GPUI font advance/ascent/descent rather than a hard-coded assumed glyph width; test windows
use an explicit deterministic metrics override because NoopTextSystem returns synthetic
successful font measurements. Measurement uses the actual grid's prepaint bounds, not an
overlaid canvas that can intercept pointer input. Adjacent plain single-column cells with
identical styling are batched into positioned runs; wide/combining cells keep their own
explicit column span, avoiding one layout element per ordinary character.

Ctrl+C or Cmd+C with a nonempty terminal selection copies alacritty's plain text to the
existing GPUI clipboard adapter and does not write to the PTY. Ctrl+C without selection
continues to send ETX (`0x03`), preserving interruption. Copy does not clear the selection,
change chat drafts, add an output log or automatically adopt output. Ctrl/Cmd+V retains the
existing single paste path; bracketed-paste, OSC52, context-menu actions, terminal tabs,
persistent sessions and cross-workspace scrollback are outside this slice.

Term teardown resets drag/bounds/selection. Rendered handlers are scoped to the current
workspace and exact PTY writer instance so a stale old grid cannot select/copy/send into a
replacement terminal. Copy only works for the active workspace owner. No selected text,
clipboard contents or raw PTY output is added to diagnostics.

```text
current grid bounds + workspace/spawn → left press/drag → alacritty selection
                                     → renderable selection → selected cell styling
                                     → Ctrl/Cmd+C → selection_to_string → clipboard
no selection + Ctrl+C → existing key mapping → existing PTY writer → ETX
workspace/spawn replacement → teardown → retire drag, bounds and old handlers
```

## Acceptance

- Pure geometry tests map measured viewport positions, cell side and display offset,
  clamp to valid cells and reject unusable bounds.
- Grid tests cover selected styling, ANSI/wide/combining characters, wrapped lines and
  history copy using the authoritative terminal selection, not a copied text buffer.
- Measured pointer drags and actual key dispatch verify clipboard readback; with no
  selection the same key sends ETX to a disposable recording writer. Chat draft is intact.
- Release outside, stale workspace/spawn events and teardown cannot carry a drag or
  selection into a replacement shell.
- Existing computed-output PTY smoke remains a separate native-process check. Test-platform
  clipboard readback is not native OS clipboard or glyph/layout acceptance; native
  cross-platform terminal interactions remain unverified.

## Recorded evidence (2026-10-06)

Four added terminal regressions pass. The pointer fixture measures the real interactive
grid, selects output, asserts a nonzero painted selection-background quad, and checks
Ctrl+C/Cmd+C clipboard readback without PTY writes. A plain click clears selection and
Ctrl+C writes ETX to the recording writer. Release-outside and owner/spawn retirement are
covered; model tests cover ANSI/wide/combining/wrapped text and history. An explicit
Noop-backed metrics override and custom-metric tests avoid assuming synthetic font values
are native measurements. Native glyph shaping, real OS clipboard and drag autoscroll
are not validated. Full 429 unit tests, PTY/reverse-RPC/source-limit integrations, strict
Clippy and Rust formatting pass. Required Bun repository checks and isolated real CLI
smoke pass; lint retains 71 baseline warnings, and root formatting retains 2851 baseline
files. The CLI smoke is protocol/runtime evidence, not a terminal-selection native E2E.
