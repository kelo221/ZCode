# ZCode browser extension (unofficial GPUI client)

A Manifest V3 extension for Helium and Google Chrome. It lets the agent's
browser tool, running behind the unofficial GPUI client (`apps/zcode-gpui`),
drive tabs in your own browser. The protocol, owners and security notes are in
[`apps/zcode-gpui/BROWSER_BRIDGE.md`](../zcode-gpui/BROWSER_BRIDGE.md).

- **Connection:** `connectNative("dev.zcode.gpui.browser")` starts
  `zcode-browser-host`, which relays to GPUI's loopback bridge.
- **Commands:** run through `packages/browser-commands`, the same command layer
  Desktop's in-app browser uses, on top of `chrome.debugger`.
- **Tabs:** agent tabs live in a "ZCode" tab group. Your own tabs are touched
  only after the agent claims them.

## Build and load

```sh
bun scripts/build.ts                # -> dist/ (bundles the pinned Playwright injected script)
bun scripts/build.ts --outdir <dir> # used by scripts/package-gpui.mjs
```

1. Open `chrome://extensions`, turn on Developer mode, click **Load unpacked**
   and choose `dist/`.
2. Open GPUI's Settings › Browser Use and click **Register**.

The fixed `key` in `manifest.json` keeps the extension id stable
(`ogdkmkfjcfnbhcankdfaeddjjkohdojh`). That id is the only origin the native
host accepts.

## Tests

```sh
node node_modules/vitest/vitest.mjs run apps/zcode-browser-extension   # from the repo root, with Node
```

The tests run against fakes of `chrome.debugger`, `chrome.tabs` and native
messaging (`test/fakeChrome.ts`).

## Licensing

`dist/NOTICE.md` carries the Apache-2.0 LICENSE and NOTICE of the bundled
`playwright-core` injected script. The design follows OpenAI's Codex browser
extension, but none of its proprietary code is used.
