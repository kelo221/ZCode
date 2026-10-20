// 单个（语言, 项目根）语言服务器：文档同步、诊断收集与完整检查的新鲜度语义。
// 合同见 apps/zcode-cli/docs/design/v2/tool/lsp-diagnostics.md。
//
// 实测 rust-analyzer 1.98（2026-10-20）：客户端声明 pull 能力后，原生诊断只能通过
// textDocument/diagnostic 拉取（push 对有错文件仍发空数组）；rustc/clippy 结果只走 push，
// 在 flycheck 进度 end 之前发布。因此有 pull 时一律用 pull，push 只补 pull 不覆盖的 source。

import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { basename } from "node:path";
import type {
  DiagnosticsAvailability,
  DiagnosticsCheck,
  DiagnosticsFreshness,
  DiagnosticsMode,
  FileDiagnostic,
  FileDiagnosticsResult,
} from "@zcode/contracts";
import { LspClient, type LspClientLogger, type LspClientOptions } from "./lsp-client.js";
import type { LanguageServerSpec } from "./servers.js";
import { filePathKey, filePathToUri, uriKey } from "./uri.js";

export const LSP_QUIET_WINDOW_MS = 300;
export const LSP_STARTUP_TIMEOUT_MS = 60_000;
export const LSP_CHECK_START_GRACE_MS = 10_000;
const MAX_DIAGNOSTICS_PER_FILE = 200;

interface LspDiagnostic {
  range?: { start?: { line?: number; character?: number } };
  severity?: number;
  code?: unknown;
  source?: string;
  message?: string;
}

interface PublishedEntry {
  diagnostics: LspDiagnostic[];
  version?: number;
  seq: number;
  receivedAt: number;
}

interface OpenDocument {
  uri: string;
  count: number;
  version: number;
}

interface CheckRun {
  generation: number;
  begun: boolean;
  tokens: Set<string>;
  finished: boolean;
  done: Promise<"complete" | "incomplete">;
  resolve: (outcome: "complete" | "incomplete") => void;
  note?: string;
}

export interface LanguageServerOptions {
  spec: LanguageServerSpec;
  root: string;
  env?: NodeJS.ProcessEnv;
  logger?: LspClientLogger;
  spawnProcess?: LspClientOptions["spawnProcess"];
  startupTimeoutMs?: number;
  checkStartGraceMs?: number;
  quietWindowMs?: number;
}

export interface CollectRequest {
  files: string[];
  mode: DiagnosticsMode;
  deadline: number;
  signal?: AbortSignal;
}

export class LanguageServer {
  readonly spec: LanguageServerSpec;
  readonly root: string;
  private readonly client: LspClient;
  private readonly ready: Promise<boolean>;
  private state: "starting" | "ready" | "failed" = "starting";
  private failure?: string;
  private capabilities: Record<string, unknown> = {};
  private readonly store = new Map<string, PublishedEntry>();
  private readonly openDocs = new Map<string, OpenDocument>();
  private readonly syncedHash = new Map<string, string>();
  private readonly wakers = new Set<() => void>();
  private readonly activeCheckTokens = new Set<string>();
  private readonly quietWindowMs: number;
  private readonly checkStartGraceMs: number;
  private publishSeq = 0;
  private generation = 0;
  private quiescent?: boolean;
  private check?: CheckRun;
  private lastFullCheckAt?: number;
  private activeCollects = 0;

  constructor(options: LanguageServerOptions) {
    this.spec = options.spec;
    this.root = options.root;
    this.quietWindowMs = options.quietWindowMs ?? LSP_QUIET_WINDOW_MS;
    this.checkStartGraceMs = options.checkStartGraceMs ?? LSP_CHECK_START_GRACE_MS;
    this.client = LspClient.start({
      command: options.spec.command,
      args: options.spec.args,
      cwd: options.root,
      env: options.env,
      logger: options.logger,
      spawnProcess: options.spawnProcess,
    });
    this.client.onExit((info) => {
      this.state = "failed";
      this.failure ??= info.reason ?? `exited with code ${info.code ?? info.signal ?? "unknown"}`;
      if (this.check && !this.check.finished) this.finishCheck(this.check, "incomplete");
      this.wake();
    });
    this.wireServerMessages();
    this.ready = this.initialize(options.startupTimeoutMs ?? LSP_STARTUP_TIMEOUT_MS);
  }

  get alive(): boolean {
    return this.client.alive && this.state !== "failed";
  }

  get failureReason(): string | undefined {
    return this.failure;
  }

  onExit(listener: () => void): void {
    this.client.onExit(() => listener());
  }

  /** 正在启动、有收集或检查进行中时不能被驱逐或空闲关闭。 */
  isBusy(): boolean {
    return (
      this.state === "starting" ||
      this.activeCollects > 0 ||
      (this.check !== undefined && !this.check.finished)
    );
  }

  async close(): Promise<void> {
    await this.client.close();
  }

  async collect(request: CollectRequest): Promise<FileDiagnosticsResult[]> {
    this.activeCollects += 1;
    try {
      const ready = await raceDeadline(this.ready, request.deadline, request.signal);
      if (ready !== true || this.state !== "ready") {
        const failed = this.state === "failed";
        return request.files.map((file) =>
          this.emptyResult(file, failed ? "unavailable" : "starting", request.mode, {
            note: failed
              ? `${this.spec.id} is not running: ${this.failure ?? "unknown error"}`
              : `${this.spec.id} is still starting`,
          }),
        );
      }
      if (request.mode === "full" && this.spec.projectCheck) {
        return await this.collectWithCheck(request);
      }
      return await Promise.all(
        request.files.map((file) =>
          this.withOpenDocument(file, request.mode === "quick", (opened) =>
            this.readDiagnostics(file, opened, request, undefined),
          ),
        ),
      );
    } finally {
      this.activeCollects -= 1;
    }
  }

  private async collectWithCheck(request: CollectRequest): Promise<FileDiagnosticsResult[]> {
    const opened: Array<{ file: string; doc: OpenedDocument | undefined }> = [];
    try {
      for (const file of request.files) {
        opened.push({ file, doc: await this.openDocument(file, false) });
      }
      const run = await this.ensureCheck(request.deadline, request.signal);
      const outcome = await raceDeadline(run.done, request.deadline, request.signal);
      const check: DiagnosticsCheck = outcome === "complete" ? "complete" : "incomplete";
      if (check === "complete") {
        await sleepUntil(
          Math.min(Date.now() + this.quietWindowMs, request.deadline),
          request.signal,
        );
      }
      return await Promise.all(
        opened.map(({ file, doc }) =>
          this.readDiagnostics(file, doc, request, {
            check,
            note: check === "incomplete" ? (run.note ?? checkIncompleteNote(outcome)) : undefined,
          }),
        ),
      );
    } finally {
      for (const { file, doc } of opened) if (doc) this.closeDocument(file, doc);
    }
  }

  private async readDiagnostics(
    file: string,
    doc: OpenedDocument | undefined,
    request: CollectRequest,
    checkInfo: { check: DiagnosticsCheck; note?: string } | undefined,
  ): Promise<FileDiagnosticsResult> {
    const check: DiagnosticsCheck =
      checkInfo?.check ??
      (request.mode === "quick"
        ? "not_requested"
        : this.spec.projectCheck
          ? "incomplete"
          : "not_supported");
    if (!doc) {
      return this.emptyResult(file, "unavailable", request.mode, {
        note: "the file could not be read",
      });
    }
    const key = filePathKey(file);
    let pulled: LspDiagnostic[] | undefined;
    let freshness: DiagnosticsFreshness = "best_effort";
    let timedOut = false;
    let pullNote: string | undefined;
    if (this.supportsPull()) {
      try {
        const report = await this.client.request<{ kind?: string; items?: LspDiagnostic[] }>(
          "textDocument/diagnostic",
          { textDocument: { uri: doc.uri } },
          { deadline: request.deadline, signal: request.signal },
        );
        pulled = Array.isArray(report?.items) ? report.items : [];
        freshness = "confirmed";
      } catch (error) {
        request.signal?.throwIfAborted();
        // 例如 ContentModified/ServerCancelled：该文件降级为 best_effort，不让整个请求失败。
        timedOut = Date.now() >= request.deadline;
        if (!timedOut)
          pullNote = `the server did not answer: ${error instanceof Error ? error.message : String(error)}`;
      }
    } else {
      const waited = await this.waitForPublish(key, doc, request.deadline, request.signal);
      timedOut = waited.timedOut;
      freshness = waited.confirmed ? "confirmed" : "best_effort";
    }
    const pushed = this.store.get(key)?.diagnostics ?? [];
    const pulledSources = new Set((pulled ?? []).map((item) => item.source ?? ""));
    const fromPush =
      pulled === undefined
        ? pushed
        : pushed.filter((item) => !pulledSources.has(item.source ?? ""));
    // 项目检查服务器的 push 结果属于某次完整检查；只有本次检查完整时才算当前内容。
    const pushIsStale = this.spec.projectCheck && checkInfo?.check !== "complete";
    const diagnostics = [
      ...(pulled ?? []).map((item) => toFileDiagnostic(item, false)),
      ...fromPush.map((item) => toFileDiagnostic(item, pushIsStale && this.isCheckSource(item))),
    ].filter((item): item is FileDiagnostic => item !== undefined);
    const availability: DiagnosticsAvailability =
      timedOut && this.quiescent === false ? "starting" : "ok";
    const loadingNote =
      this.quiescent === false
        ? `${this.spec.id} is still loading the project; results may be incomplete`
        : undefined;
    return buildResult(file, this.spec.id, {
      availability,
      freshness: this.quiescent === false ? "best_effort" : freshness,
      check,
      timedOut,
      diagnostics,
      knownOtherFileErrors: this.knownOtherFileErrors(key),
      lastFullCheckAt:
        pushIsStale && diagnostics.some((item) => item.stale) && this.lastFullCheckAt
          ? new Date(this.lastFullCheckAt).toISOString()
          : undefined,
      note: checkInfo?.note ?? pullNote ?? loadingNote,
    });
  }

  private emptyResult(
    file: string,
    availability: DiagnosticsAvailability,
    mode: DiagnosticsMode,
    extra: { note?: string },
  ): FileDiagnosticsResult {
    return buildResult(file, this.spec.id, {
      availability,
      freshness: "best_effort",
      check:
        mode === "quick"
          ? "not_requested"
          : this.spec.projectCheck
            ? "incomplete"
            : "not_supported",
      timedOut: availability === "starting",
      diagnostics: [],
      knownOtherFileErrors: 0,
      note: extra.note,
    });
  }

  // ---- documents --------------------------------------------------------

  private async withOpenDocument<T>(
    file: string,
    announceChange: boolean,
    body: (doc: OpenedDocument | undefined) => Promise<T>,
  ): Promise<T> {
    const doc = await this.openDocument(file, announceChange);
    try {
      return await body(doc);
    } finally {
      if (doc) this.closeDocument(file, doc);
    }
  }

  /** 以磁盘当前内容打开（或在已打开时整体替换）；收集结束后由 closeDocument 关闭。 */
  private async openDocument(
    file: string,
    announceChange: boolean,
  ): Promise<OpenedDocument | undefined> {
    let text: string;
    try {
      text = await readFile(file, "utf8");
    } catch {
      return undefined;
    }
    const key = filePathKey(file);
    const uri = filePathToUri(file);
    const hash = createHash("sha1").update(text).digest("hex");
    if (this.syncedHash.get(key) !== hash) {
      // 只有内容变化才算编辑：重新打开同样的内容不应让进行中的检查失效。首次同步也算变化，
      // 因为无法证明检查开始后它没被改过。
      this.generation += 1;
      this.syncedHash.set(key, hash);
    }
    if (announceChange) {
      this.client.notify("workspace/didChangeWatchedFiles", { changes: [{ uri, type: 2 }] });
    }
    const existing = this.openDocs.get(key);
    const startSeq = this.publishSeq;
    if (existing) {
      existing.count += 1;
      existing.version += 1;
      this.client.notify("textDocument/didChange", {
        textDocument: { uri: existing.uri, version: existing.version },
        contentChanges: [{ text }],
      });
      return { uri: existing.uri, version: existing.version, startSeq };
    }
    const doc: OpenDocument = { uri, count: 1, version: 1 };
    this.openDocs.set(key, doc);
    this.client.notify("textDocument/didOpen", {
      textDocument: { uri, languageId: this.spec.languageId(file), version: doc.version, text },
    });
    return { uri, version: doc.version, startSeq };
  }

  private closeDocument(file: string, opened: OpenedDocument): void {
    const key = filePathKey(file);
    const doc = this.openDocs.get(key);
    if (!doc || doc.uri !== opened.uri) return;
    doc.count -= 1;
    if (doc.count > 0) return;
    this.openDocs.delete(key);
    this.client.notify("textDocument/didClose", { textDocument: { uri: doc.uri } });
  }

  private async waitForPublish(
    key: string,
    doc: OpenedDocument,
    deadline: number,
    signal: AbortSignal | undefined,
  ): Promise<{ confirmed: boolean; timedOut: boolean }> {
    for (;;) {
      signal?.throwIfAborted();
      const entry = this.store.get(key);
      const now = Date.now();
      const matches =
        entry !== undefined &&
        entry.seq > doc.startSeq &&
        (entry.version === undefined || entry.version === doc.version);
      if (matches) {
        // 静默窗口只用来合并分批到达的通知，不证明结果正确。
        if (now - entry.receivedAt >= this.quietWindowMs || now >= deadline) {
          return { confirmed: entry.version === doc.version, timedOut: false };
        }
        await this.waitForChange(Math.min(entry.receivedAt + this.quietWindowMs, deadline), signal);
        continue;
      }
      if (now >= deadline || !this.alive) return { confirmed: false, timedOut: true };
      await this.waitForChange(deadline, signal);
    }
  }

  // ---- project check ----------------------------------------------------

  private async ensureCheck(deadline: number, signal: AbortSignal | undefined): Promise<CheckRun> {
    const current = this.check;
    if (current && !current.finished && current.generation === this.generation) return current;
    if (current && !current.finished) this.finishCheck(current, "incomplete");
    let resolve!: (outcome: "complete" | "incomplete") => void;
    const run: CheckRun = {
      generation: this.generation,
      begun: false,
      tokens: new Set(),
      finished: false,
      done: new Promise((done) => (resolve = done)),
      resolve: (outcome) => resolve(outcome),
    };
    this.check = run;
    // rust-analyzer 加载项目期间的 runFlycheck 会被忽略：等 quiescent 再触发（受截止时间约束）。
    while (this.quiescent === false && Date.now() < deadline && this.alive) {
      await this.waitForChange(deadline, signal);
    }
    if (run.finished) return run;
    this.client.notify("rust-analyzer/runFlycheck", { textDocument: null });
    setTimeout(() => {
      if (!run.begun && !run.finished) {
        run.note = "the language server did not start a check";
        this.finishCheck(run, "incomplete");
      }
    }, this.checkStartGraceMs).unref?.();
    return run;
  }

  private finishCheck(run: CheckRun, outcome: "complete" | "incomplete"): void {
    if (run.finished) return;
    run.finished = true;
    const final =
      outcome === "complete" && run.generation === this.generation && this.check === run
        ? "complete"
        : "incomplete";
    if (final === "complete") this.lastFullCheckAt = Date.now();
    else run.note ??= "the files changed or a newer check started while this check ran";
    run.resolve(final);
    this.wake();
  }

  private onProgress(token: string, kind: string, title: string | undefined): void {
    const isCheck =
      token.startsWith("rust-analyzer/flycheck") || /cargo (check|clippy)/iu.test(title ?? "");
    const run = this.check;
    if (kind === "begin" && isCheck) {
      this.activeCheckTokens.add(token);
      if (run && !run.finished) {
        run.begun = true;
        run.tokens.add(token);
      }
      return;
    }
    if (kind !== "end" || !this.activeCheckTokens.delete(token)) return;
    if (!run || run.finished || !run.tokens.delete(token)) return;
    if (run.begun && run.tokens.size === 0) this.finishCheck(run, "complete");
  }

  private isCheckSource(item: LspDiagnostic): boolean {
    const native = this.spec.nativeSources ?? [];
    return !native.includes(item.source ?? "");
  }

  // ---- server messages --------------------------------------------------

  private async initialize(startupTimeoutMs: number): Promise<boolean> {
    const rootUri = filePathToUri(this.root);
    try {
      const result = await this.client.request<{ capabilities?: Record<string, unknown> }>(
        "initialize",
        {
          processId: process.pid,
          clientInfo: { name: "zcode" },
          rootUri,
          rootPath: this.root,
          workspaceFolders: [{ uri: rootUri, name: basename(this.root) }],
          initializationOptions: this.spec.settings ?? {},
          capabilities: CLIENT_CAPABILITIES,
        },
        { deadline: Date.now() + startupTimeoutMs },
      );
      this.capabilities = result?.capabilities ?? {};
      this.client.notify("initialized", {});
      this.state = "ready";
      return true;
    } catch (error) {
      this.state = "failed";
      this.failure ??= error instanceof Error ? error.message : String(error);
      void this.client.kill();
      return false;
    } finally {
      this.wake();
    }
  }

  private wireServerMessages(): void {
    const settings = this.spec.settings ?? {};
    const section = this.spec.configurationSection;
    this.client.onRequest("workspace/configuration", (params) => {
      const items = isRecord(params) && Array.isArray(params.items) ? params.items : [];
      return items.map((item) => configurationFor(item, section, settings));
    });
    this.client.onRequest("window/workDoneProgress/create", () => null);
    this.client.onRequest("client/registerCapability", () => null);
    this.client.onRequest("client/unregisterCapability", () => null);
    this.client.onRequest("window/showMessageRequest", () => null);
    this.client.onRequest("workspace/workspaceFolders", () => [
      { uri: filePathToUri(this.root), name: basename(this.root) },
    ]);
    this.client.onRequest("workspace/applyEdit", () => ({
      applied: false,
      failureReason: "ZCode does not apply edits requested by language servers",
    }));
    this.client.onNotification("textDocument/publishDiagnostics", (params) => {
      if (!isRecord(params) || typeof params.uri !== "string" || !Array.isArray(params.diagnostics))
        return;
      const key = uriKey(params.uri);
      if (key === undefined) return;
      // 每次发布替换该文件的整组诊断；空数组即清空。
      this.store.set(key, {
        diagnostics: params.diagnostics as LspDiagnostic[],
        version: typeof params.version === "number" ? params.version : undefined,
        seq: ++this.publishSeq,
        receivedAt: Date.now(),
      });
      this.wake();
    });
    this.client.onNotification("$/progress", (params) => {
      if (!isRecord(params) || !isRecord(params.value)) return;
      const token = String(params.token ?? "");
      const kind = typeof params.value.kind === "string" ? params.value.kind : "";
      const title = typeof params.value.title === "string" ? params.value.title : undefined;
      this.onProgress(token, kind, title);
    });
    this.client.onNotification("experimental/serverStatus", (params) => {
      if (isRecord(params) && typeof params.quiescent === "boolean") {
        this.quiescent = params.quiescent;
        this.wake();
      }
    });
  }

  private supportsPull(): boolean {
    return (
      this.capabilities.diagnosticProvider !== undefined &&
      this.capabilities.diagnosticProvider !== null
    );
  }

  private knownOtherFileErrors(key: string): number {
    let count = 0;
    for (const [otherKey, entry] of this.store) {
      if (otherKey === key) continue;
      count += entry.diagnostics.filter((item) => (item.severity ?? 1) === 1).length;
    }
    return count;
  }

  private waitForChange(until: number, signal: AbortSignal | undefined): Promise<void> {
    return new Promise((resolve, reject) => {
      let timer: ReturnType<typeof setTimeout> | undefined;
      const done = () => {
        if (timer) clearTimeout(timer);
        this.wakers.delete(done);
        signal?.removeEventListener("abort", onAbort);
        resolve();
      };
      const onAbort = () => {
        done();
        reject(signal?.reason ?? new Error("aborted"));
      };
      if (signal?.aborted) {
        reject(signal.reason ?? new Error("aborted"));
        return;
      }
      timer = setTimeout(done, Math.max(0, until - Date.now()));
      this.wakers.add(done);
      signal?.addEventListener("abort", onAbort, { once: true });
    });
  }

  private wake(): void {
    for (const waker of [...this.wakers]) waker();
  }
}

interface OpenedDocument {
  uri: string;
  version: number;
  startSeq: number;
}

const CLIENT_CAPABILITIES = {
  general: { positionEncodings: ["utf-16"] },
  textDocument: {
    synchronization: { dynamicRegistration: false, didSave: true },
    publishDiagnostics: { versionSupport: true, relatedInformation: false },
    diagnostic: { dynamicRegistration: false, relatedDocumentSupport: false },
  },
  window: { workDoneProgress: true },
  workspace: {
    configuration: true,
    workspaceFolders: true,
    didChangeWatchedFiles: { dynamicRegistration: true },
  },
  experimental: { serverStatusNotification: true },
};

function configurationFor(
  item: unknown,
  section: string | undefined,
  settings: Record<string, unknown>,
): unknown {
  const requested = isRecord(item) && typeof item.section === "string" ? item.section : undefined;
  if (requested === undefined || section === undefined) return null;
  if (requested === section) return settings;
  if (!requested.startsWith(`${section}.`)) return null;
  let value: unknown = settings;
  for (const part of requested.slice(section.length + 1).split(".")) {
    value = isRecord(value) ? value[part] : undefined;
  }
  return value ?? null;
}

function toFileDiagnostic(item: LspDiagnostic, stale: boolean): FileDiagnostic | undefined {
  const severity = item.severity ?? 1;
  if (severity !== 1 && severity !== 2) return undefined;
  const code = isRecord(item.code) ? item.code.value : item.code;
  return {
    severity: severity === 1 ? "error" : "warning",
    line: (item.range?.start?.line ?? 0) + 1,
    column: (item.range?.start?.character ?? 0) + 1,
    ...(code !== undefined && code !== null && code !== ""
      ? { code: String(code).slice(0, 200) }
      : {}),
    ...(item.source ? { source: item.source.slice(0, 100) } : {}),
    message: String(item.message ?? "").slice(0, 4_000),
    ...(stale ? { stale: true } : {}),
  };
}

function buildResult(
  filePath: string,
  server: string,
  fields: Omit<FileDiagnosticsResult, "filePath" | "server" | "errorCount" | "warningCount">,
): FileDiagnosticsResult {
  const errorCount = fields.diagnostics.filter((item) => item.severity === "error").length;
  const warningCount = fields.diagnostics.length - errorCount;
  const sorted = [...fields.diagnostics].sort(
    (a, b) => severityRank(a) - severityRank(b) || a.line - b.line || a.column - b.column,
  );
  const { lastFullCheckAt, note, ...rest } = fields;
  return {
    filePath,
    server,
    ...rest,
    errorCount,
    warningCount,
    diagnostics: sorted.slice(0, MAX_DIAGNOSTICS_PER_FILE),
    ...(lastFullCheckAt ? { lastFullCheckAt } : {}),
    ...(note ? { note } : {}),
  };
}

function severityRank(item: FileDiagnostic): number {
  return item.severity === "error" ? 0 : 1;
}

function checkIncompleteNote(outcome: unknown): string {
  return outcome === "incomplete"
    ? "the check did not finish for the current contents"
    : "the check did not finish before the deadline";
}

/** 截止或中止时返回 "timeout"（调用方只是不再等待，共享的工作继续）。 */
async function raceDeadline<T>(
  promise: Promise<T>,
  deadline: number,
  signal: AbortSignal | undefined,
): Promise<T | "timeout"> {
  signal?.throwIfAborted();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let onAbort: (() => void) | undefined;
  try {
    return await Promise.race([
      promise,
      new Promise<"timeout">((resolve) => {
        timer = setTimeout(() => resolve("timeout"), Math.max(0, deadline - Date.now()));
      }),
      new Promise<never>((_, reject) => {
        onAbort = () => reject(signal?.reason ?? new Error("aborted"));
        signal?.addEventListener("abort", onAbort, { once: true });
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
    if (onAbort) signal?.removeEventListener("abort", onAbort);
  }
}

function sleepUntil(until: number, signal: AbortSignal | undefined): Promise<void> {
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, Math.max(0, until - Date.now()));
    signal?.addEventListener(
      "abort",
      () => {
        clearTimeout(timer);
        resolve();
      },
      { once: true },
    );
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
