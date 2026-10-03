# Third-party notices for zcode-gpui

This file covers the Rust dependencies of the native GPUI frontend
(`apps/zcode-gpui`). It complements the repository-wide
`THIRD-PARTY-NOTICES.md`, which covers the Node/Electron side.

Dependencies are used through their public APIs; no source code is copied
from the Zed application repository (which is GPL-3.0 — its app sources are
intentionally **not** a dependency or a code source for this frontend). `gpui`
is consumed as the Apache-2.0-published crate from the Zed repository, pinned
to the revision that `ely-gpui-component` targets so both build one shared
gpui; the `gpui` **crate** is Apache-2.0, unlike the GPL-3.0 Zed app sources.

| Crate | Version | License | Upstream |
| --- | --- | --- | --- |
| gpui / gpui_platform | git rev `1a28cff` (zed, 2026-09-27) | Apache-2.0 | https://github.com/zed-industries/zed |
| ely-gpui-component | git rev `2f8b2f6` | MIT OR Apache-2.0 | https://github.com/ZacharyZhang-NY/Ely-GPUI-Components |
| syntect | 5.3.0 | MIT | https://github.com/trishume/syntect |
| pulldown-cmark | 0.13.4 | MIT | https://github.com/pulldown-cmark/pulldown-cmark |
| portable-pty | 0.9.0 | MIT | https://github.com/wezterm/wezterm |
| alacritty_terminal | 0.26.0 | Apache-2.0 | https://github.com/alacritty/alacritty |
| windows | 0.62 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| serde / serde_json | 1.x | MIT OR Apache-2.0 | https://github.com/serde-rs |
| uuid | 1.x | Apache-2.0 OR MIT | https://github.com/uuid-rs/uuid |
| futures | 0.3 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| base64 | 0.22 | MIT OR Apache-2.0 | https://github.com/marshallpierce/rust-base64 |
| crc32fast | 1.x | MIT OR Apache-2.0 | https://github.com/srijs/rust-crc32fast |

## ely-gpui-component (MIT OR Apache-2.0)

Used as a git dependency through its public API; the component library embeds
assets that are redistributed with the binary via `ely_gpui_component::Assets`:
font families Inter, JetBrains Mono, IBM Plex Sans and Noto Sans Hebrew
(licensed under the SIL Open Font License 1.1) and Lucide icons (ISC). Their
license texts ship inside the ely-gpui-component source checkout.

## Apache-2.0 notice (gpui, alacritty_terminal)
Licensed under the Apache License, Version 2.0 (the "License"); you may not
use these files except in compliance with the License. You may obtain a copy
of the License at http://www.apache.org/licenses/LICENSE-2.0. The full text
ships with each crate (LICENSE-APACHE) and with the Rust dependency sources
in the local Cargo registry.

## MIT notice (syntect, pulldown-cmark, portable-pty, and dual-licensed crates)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, subject to the following conditions: the above
copyright notice and this permission notice shall be included in all copies
or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

Copyright holders are listed in each crate's `Cargo.toml` / LICENSE file in
the Cargo registry (e.g. Zed Industries, Inc. for gpui; trishume for syntect;
Raphael Cohn and contributors for pulldown-cmark; Wez Furlong for
portable-pty; the Alacritty Project contributors for alacritty_terminal).

## Transitive dependencies

Transitive crate dependencies (e.g. vte, unicode-width, parking_lot, regex)
inherit the notices of their own crates, all published under MIT and/or
Apache-2.0; run `cargo license` inside `apps/zcode-gpui` for the complete
generated list.
