# Third-party notices for zcode-gpui

This file covers the Rust dependencies of the native GPUI frontend
(`apps/zcode-gpui`). It complements the repository-wide
`THIRD-PARTY-NOTICES.md`, which covers the Node/Electron side.

All dependencies below are used as published crates from crates.io through
their public APIs; no source code is copied from the Zed application
repository (which is GPL-3.0 — its app sources are intentionally **not** a
dependency or a code source for this frontend).

| Crate | Version | License | Upstream |
| --- | --- | --- | --- |
| gpui | 0.2.2 | Apache-2.0 | https://github.com/zed-industries/zed (published crate) |
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
