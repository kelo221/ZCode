# ZCode Browser Use extension: notices

This extension bundles the ZCode browser command layer (`packages/browser-commands`, Apache-2.0, part of this repository) and the following third-party code:

- **playwright-core 1.59.1** (Apache-2.0, Copyright (c) Microsoft Corporation). Its injected script (`lib/generated/injectedScriptSource.js`) is inlined unmodified and runs in an isolated world of controlled pages for ARIA snapshots and locators. Its NOTICE and LICENSE follow below in the built extension.
- **zod** (MIT), through `@zcode/shared` schemas.

The architecture (an extension driven over native messaging by a desktop app) follows the publicly visible design of OpenAI's Codex browser extension. No Codex code is included; that extension is proprietary.
