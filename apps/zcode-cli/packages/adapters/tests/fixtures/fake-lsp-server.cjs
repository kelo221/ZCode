// Scripted LSP server for lsp-language-server tests. Behaviour comes from the FAKE_LSP env var
// (JSON); every received method is appended to FAKE_LSP_LOG so tests can count requests.
//
// Lines containing ERROR produce a native error, WARN a native warning, CHECKERR an error that
// only the project check (source "rustc") reports.
"use strict";
const fs = require("node:fs");

const config = JSON.parse(process.env.FAKE_LSP || "{}");
const nativeSource = config.nativeSource || "fake";
const logPath = process.env.FAKE_LSP_LOG;
const docs = new Map();
let buffer = Buffer.alloc(0);

function log(method) {
  if (logPath) fs.appendFileSync(logPath, `${method}\n`);
}

function send(message) {
  const body = Buffer.from(JSON.stringify({ jsonrpc: "2.0", ...message }), "utf8");
  process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);
  process.stdout.write(body);
}

function later(ms, fn) {
  setTimeout(fn, ms || 0);
}

function diagnosticsFor(text, marker, source, severity) {
  const items = [];
  text.split("\n").forEach((line, index) => {
    const column = line.indexOf(marker);
    if (column < 0) return;
    if (marker === "ERROR" && line.includes("CHECKERR")) return;
    items.push({
      range: {
        start: { line: index, character: column },
        end: { line: index, character: column + marker.length },
      },
      severity,
      source,
      code: marker === "CHECKERR" ? "E0308" : undefined,
      message: `${marker.toLowerCase()} from ${source}`,
    });
  });
  return items;
}

function nativeDiagnostics(text) {
  return [
    ...diagnosticsFor(text, "ERROR", nativeSource, 1),
    ...diagnosticsFor(text, "WARN", nativeSource, 2),
  ];
}

function publish(uri, diagnostics, version) {
  send({
    method: "textDocument/publishDiagnostics",
    params:
      config.versioned && version !== undefined
        ? { uri, version, diagnostics }
        : { uri, diagnostics },
  });
}

function onDocument(uri, text, version) {
  docs.set(uri, { text, version });
  if (config.pull) return;
  const delays = config.pushDelaysMs || [0];
  // Staggered publications: earlier ones carry only warnings (like a syntactic pass), the last
  // one carries everything.
  delays.forEach((delay, index) => {
    later(delay, () => {
      const current = docs.get(uri);
      if (!current || current.version !== version) return;
      const all = nativeDiagnostics(text);
      publish(
        uri,
        index === delays.length - 1 ? all : all.filter((d) => d.severity !== 1),
        version,
      );
    });
  });
}

function runCheck() {
  const token = "rust-analyzer/flycheck/0";
  const check = config.check || {};
  send({
    id: `progress-${Date.now()}`,
    method: "window/workDoneProgress/create",
    params: { token },
  });
  later(check.beginDelayMs, () => {
    send({
      method: "$/progress",
      params: { token, value: { kind: "begin", title: "cargo check" } },
    });
    later(check.durationMs, () => {
      for (const [uri, doc] of docs)
        publish(uri, diagnosticsFor(doc.text, "CHECKERR", "rustc", 1), undefined);
      for (const [uri, text] of Object.entries(check.diskFiles || {})) {
        publish(uri, diagnosticsFor(text, "CHECKERR", "rustc", 1), undefined);
      }
      later(check.endDelayMs, () =>
        send({ method: "$/progress", params: { token, value: { kind: "end" } } }),
      );
    });
  });
}

function handle(message) {
  if (message.method) log(message.method);
  if (message.method === undefined) return;
  const params = message.params || {};
  switch (message.method) {
    case "initialize":
      later(config.initializeDelayMs, () =>
        send({
          id: message.id,
          result: {
            capabilities: {
              textDocumentSync: 1,
              ...(config.pull ? { diagnosticProvider: {} } : {}),
            },
          },
        }),
      );
      return;
    case "initialized":
      if (config.quiescentAfterMs !== undefined) {
        send({ method: "experimental/serverStatus", params: { health: "ok", quiescent: false } });
        later(config.quiescentAfterMs, () =>
          send({ method: "experimental/serverStatus", params: { health: "ok", quiescent: true } }),
        );
      }
      return;
    case "textDocument/didOpen":
      onDocument(params.textDocument.uri, params.textDocument.text, params.textDocument.version);
      return;
    case "textDocument/didChange":
      onDocument(
        params.textDocument.uri,
        params.contentChanges[0].text,
        params.textDocument.version,
      );
      return;
    case "textDocument/didClose":
      docs.delete(params.textDocument.uri);
      return;
    case "textDocument/diagnostic": {
      const doc = docs.get(params.textDocument.uri);
      later(config.pullDelayMs, () =>
        send({
          id: message.id,
          result: { kind: "full", items: doc ? nativeDiagnostics(doc.text) : [] },
        }),
      );
      return;
    }
    case "rust-analyzer/runFlycheck":
      if (!(config.check && config.check.ignore)) runCheck();
      return;
    case "shutdown":
      send({ id: message.id, result: null });
      return;
    case "exit":
      process.exit(0);
      return;
    default:
      if (message.id !== undefined) send({ id: message.id, result: null });
  }
}

process.stdin.on("data", (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  for (;;) {
    const headerEnd = buffer.indexOf("\r\n\r\n");
    if (headerEnd < 0) return;
    const match = /Content-Length: (\d+)/i.exec(buffer.subarray(0, headerEnd).toString("ascii"));
    const length = match ? Number(match[1]) : 0;
    if (buffer.length < headerEnd + 4 + length) return;
    const body = buffer.subarray(headerEnd + 4, headerEnd + 4 + length).toString("utf8");
    buffer = buffer.subarray(headerEnd + 4 + length);
    handle(JSON.parse(body));
  }
});
