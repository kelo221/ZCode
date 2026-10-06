# Local Files image preview

Status: reachable bounded local preview, automated gates passed on 2026-10-06; native visual acceptance remains open. Local Files is an existing background filesystem
adapter, not a new Host file-service owner or remote-workspace implementation.

## Behavior and ownership

The Files dock previews PNG, JPEG, GIF, WebP, BMP and ICO in place, rather than treating
all images as external-only. Decode happens on the existing background executor. Encoded
input is capped at 20 MiB, dimensions at 8192 per side and decoded output at 16 million
pixels / 64 MiB rendered BGRA. Decoder dimensions and output-byte limits are checked
before decoding, with a 128 MiB best-effort decoder scratch limit (the image library does
not promise a hard process-allocation ceiling). Only one static frame is shown; GIF/animated formats do not start
an animation or eagerly decode all frames. Invalid, unsupported, oversize and unavailable
images show fixed localized feedback, never raw decoder/filesystem errors or partial images.
SVG/remote images and office/PDF rendering are not added. Previewing sends no backend
command, launches no app, writes no file and does not adopt composer attachments.

`FilesState` remains the sole local tree/selection/preview owner. Its workspace key is
separate from the filesystem root; switching between two identities with the same path
still resets the tree. Its request generation must advance on each selection and workspace
reset. A separate reset generation scopes directory listings without invalidating an
in-flight listing merely because a file was selected. Directory completion checks that
reset generation, workspace key and root against the current owner before adopting entries. Completion captures workspace key,
root, selected path and generation; A→B→A selections or workspace A→B→A resets discard
old work even if the selected path matches again. File callbacks capture their rendered
workspace/root and generation before selection, preventing an old tree row from navigating
another workspace. File readers use a bounded read on an opened regular-file handle, not
metadata-then-unbounded-read; a file growing during IO cannot bypass the size cap. Existing
text preview remains capped at 256 KiB. No durable preview/image cache or extra path owner
is introduced; GPUI receives one decoded render image owned by the current selection.

```text
Files tree current workspace → captured row → current owner/generation check
                            → FilesState selection generation → background bounded read
                            → format/dimension/allocation validation → static BGRA frame
                            → same workspace/root/path/generation → fit-contained preview
other selection/workspace/reset → discard late completion, no image/selection adoption
```

The image fits inside the preview panel without cropping or enlarging layout; dimensions
and static-frame behavior are visible. Controls and metadata use semantic UI typography,
light/dark tokens and English/Chinese labels; text/code bodies retain independent typography.
Current parent chat drafts, attachments and backend subscriptions are unaffected.

## Acceptance

- Valid PNG/JPEG/GIF/WebP/BMP/ICO are classified and decoded within bounds; a binary or
  malformed payload cannot reach the render image.
- Input growth, encoded-byte, dimension and decoded-pixel caps fail before unbounded decode.
- One-frame GIF behavior is explicit and tested; EXIF orientation is respected if supported
  by the already-resolved decoder API.
- Measured Files row clicks show the image and preserve the composer; selecting another
  file replaces it. Late A→B→A and workspace-reset completions are rejected deterministically.
- Real local disposable fixture IO and GPUI pointer/render-atlas tests are separate evidence from
  native visual acceptance. Full file-service parity, remote paths, image zoom/rotation,
  animation and native cross-platform execution remain open.

## Recorded evidence (2026-10-06)

Seven added Files regressions pass, including actual background directory/image IO from
measured row clicks, image/text/image replacement, render-atlas membership, two-frame GIF
first-frame-only behavior and same-path identity/reset late-result rejection. Public GPUI
test APIs do not expose image-sprite bounds; atlas membership is not native visual approval.
The decoder applies reported orientation, but an EXIF fixture/native orientation check has
not been run. Strict all-target Clippy, formatting, 425 unit tests, one PTY smoke, eight
reverse-RPC tests and the Rust 400-line integration gate pass. Bun typecheck/lint and
architecture checks pass (71 existing lint warnings, zero errors); root format check still
reports 2851 baseline files. No full-repository formatting was applied. The direct image
adapter reuses locked image 0.25.10; inherited dependency changes were preserved.
