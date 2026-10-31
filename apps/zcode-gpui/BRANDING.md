# GCode branding

GCode is this fork's name for the GPUI client (desktop and Android) and for the
agent identity in the CLI system prompt. This file is the spec for the name, the
theme, the fonts and the logo.

## Name

- **GCode** in every user-visible string: window title, notifications, settings
  text, errors, i18n values (`packages/ui/src/i18n/locales/*.ts` →
  `i18n_cache.json`), the Android launcher label, release notes, and the model's
  identity line ("You are GCode, an interactive coding agent").
- **Unchanged internal identifiers**, so existing installs, settings and pairing keep
  working:
  - `~/.zcode` and `setting.json` keys;
  - crate and binary names (`zcode-gpui`, `zcode-backend`, …);
  - `ZCODE_*` environment variables;
  - the `zcode://`, `zcode-gpui://` and `zcode-file:` schemes;
  - the single-instance mutex and registry keys;
  - the Android `applicationId`;
  - HTTP headers and the `ZCode Protocol` name;
  - the paths used to locate the official app (`Programs\ZCode`, `ZCode.exe`, …).

## Theme

The app has one theme: [Dracula](https://draculatheme.com/contribute). There is no
light theme and no theme setting. A stored `theme` / `themePreference` value is
left in `setting.json` (the official app shares that file) but is not read. The
switch-theme command is not bound.

| Token (`ThemePalette`) | Value |
| --- | --- |
| background | `#282a36` |
| sidebar, panel, input | `#21222c` |
| window_frame | `#191a21` |
| card, menu | `#343746` |
| selected, selection, border, tag, menu_hover, accent | `#44475a` (Current Line) |
| border_hover, subtlest | `#6272a4` (Comment) |
| foreground | `#f8f8f2`; subtle = 60 % |
| inverse (text on fills) | `#282a36` |
| brand (primary buttons) | Purple `#bd93f9` |
| info, link | Cyan `#8be9fd` |
| success | Green `#50fa7b` |
| warning | Orange `#ffb86c` |
| destructive | Red `#ff5555` |
| reasoning | Purple |
| tool | Yellow `#f1fa8c` |
| user | Pink `#ff79c6` |
| diff add / del / hunk | Green / Red / Purple, translucent |

- **Code blocks:** `assets/syntax/Dracula.tmTheme` (MIT, dracula/textmate).
- **Terminal and Ely's ANSI slots:** `DRACULA_ANSI` in `src/shared/theme_palette.rs`.
- **Ely syntax colours:** follow the same palette.

## Fonts

UI text uses **Geist** and code uses **Geist Mono**. Both are SIL OFL 1.1 (see
`assets/fonts/OFL.txt`) and taken from the vercel/geist-font v1.9.0 static TTFs.
They are embedded at compile time and registered in `theme::register_fonts`, which
runs right after `ely_gpui_component::init`. Ely's `Theme.font_family` and
`mono_family` point at the same names. Android shares this code path.

## Logo

- **Source:** `assets/brand/gcode.svg`, 512×512. It is a rounded Dracula tile
  holding a purple-to-pink "G" stroke and a green block cursor.
- **In-app mark:** `assets/brand/gcode-mark.svg` is the same mark without the tile.
  It appears above the draft greeting. `draft_greeting.rs` rasterises it at
  exactly display size × window scale factor. Passing the SVG path to `img()`
  instead would rasterise at the intrinsic size and stretch the result, which
  makes the pixels visible.
- **Windows:** `assets/brand/gcode.ico` holds 16, 24, 32, 48, 64, 128 and 256 px
  frames. `build.rs` embeds it as icon resource id 1 (`assets/brand/gcode.rc`),
  which is the id gpui's window class loads. `scripts/package-gpui.mjs` also copies
  it next to the exe.
- **Android:** adaptive icon in `apps/zcode-gpui-android/android/app/src/main/res`.
  The foreground and monochrome VectorDrawables are the same paths scaled into the
  108 dp canvas. The background is `#282a36`.

To regenerate the ICO after editing the SVG, use ImageMagick 7 (librsvg delegate):

```sh
for s in 16 24 32 48 64 128 256; do
  magick -background none -density 384 assets/brand/gcode.svg -resize ${s}x${s} /tmp/i$s.png
done
magick /tmp/i{16,24,32,48,64,128,256}.png assets/brand/gcode.ico
```
