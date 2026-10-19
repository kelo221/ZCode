// fff 常驻索引引擎（GPUI 专属，见 apps/zcode-gpui/PARITY.md「Built-in fff code search」）。
//
// 只在 Bun 运行时加载：Electron 的 Node/SEA 构建把 @ff-labs/fff-bun 标为 external，且永远
// 不会走到这里的 import。原生库由 `bun build --compile` 嵌入（fff-bun 内部 `type: "file"` import）。
// 所有查询都是同步 FFI 调用；生命周期（何时打开、空闲多久关闭）由 bootstrap 的 manager 管。

import type {
  CodeSearchFindRequest,
  CodeSearchFindResult,
  CodeSearchGrepMode,
  CodeSearchLineHit,
  CodeSearchOutcome,
  CodeSearchTextRequest,
  CodeSearchTextResult,
} from "@zcode/contracts";

/** fff-bun 的发布类型引用 bun:ffi；这里只声明用到的结构，避免 Node 侧类型检查依赖 Bun 类型。 */
interface FffResult<T> {
  ok: boolean;
  value?: T;
  error?: string;
}

interface FffFileItem {
  relativePath: string;
  gitStatus: string;
}

interface FffGrepMatch {
  relativePath: string;
  lineNumber: number;
  lineContent: string;
  contextBefore?: string[];
  contextAfter?: string[];
}

type FffGrepCursor = object;

interface FffGrepResult {
  items: FffGrepMatch[];
  nextCursor: FffGrepCursor | null;
  regexFallbackError?: string;
}

interface FffGrepOptions {
  mode?: CodeSearchGrepMode;
  cursor?: FffGrepCursor | null;
  pageSize?: number;
  maxMatchesPerFile?: number;
  beforeContext?: number;
  afterContext?: number;
  smartCase?: boolean;
  timeBudgetMs?: number;
  enforceTimeBudget?: boolean;
}

interface FffFinder {
  readonly isDestroyed: boolean;
  destroy(): void;
  fileSearch(
    query: string,
    options?: { pageIndex?: number; pageSize?: number },
  ): FffResult<{ items: FffFileItem[]; totalMatched: number }>;
  grep(query: string, options?: FffGrepOptions): FffResult<FffGrepResult>;
  multiGrep(
    options: FffGrepOptions & { patterns: string[]; constraints?: string },
  ): FffResult<FffGrepResult>;
  waitForIndexReady(timeoutMs?: number): Promise<FffResult<boolean>>;
}

interface FffModule {
  FileFinder: {
    create(options: { basePath: string; aiMode?: boolean }): FffResult<FffFinder>;
  };
}

export interface CodeSearchEngine {
  find(request: CodeSearchFindRequest): CodeSearchOutcome<CodeSearchFindResult>;
  search(request: CodeSearchTextRequest): CodeSearchOutcome<CodeSearchTextResult>;
  destroy(): void;
}

export type CodeSearchEngineOpener = (root: string) => Promise<CodeSearchEngine>;

export type CodeSearchEngineLoad =
  | { ok: true; open: CodeSearchEngineOpener }
  | { ok: false; error: string };

/** 首次查询前等索引就绪的上限；超时照样返回（扫描中也能搜到部分结果）。 */
const INDEX_READY_TIMEOUT_MS = 10_000;
/** 单次 grep 的时间预算；病态正则在大仓库里不能无限占住事件循环。 */
const GREP_TIME_BUDGET_MS = 10_000;
const MAX_CURSORS = 64;
const MAX_LINE_CHARS = 300;
const COUNT_PAGE_SIZE = 500;

const FFF_MODULE_KEY = Symbol.for("zcode.fffModule");

/**
 * 加载 fff 原生库。模块 promise 缓存在 globalThis：在 Bun 编译的宿主里重复 import 这个
 * 模块图（它有顶层 await 的 `type: "file"` import）会永久挂起。
 */
export async function loadFffCodeSearchEngine(): Promise<CodeSearchEngineLoad> {
  if (typeof (globalThis as { Bun?: unknown }).Bun === "undefined") {
    return { ok: false, error: "fff requires the Bun runtime" };
  }
  const store = globalThis as unknown as Record<symbol, Promise<FffModule> | undefined>;
  try {
    store[FFF_MODULE_KEY] ??= import("@ff-labs/fff-bun") as unknown as Promise<FffModule>;
    const module = await store[FFF_MODULE_KEY];
    return { ok: true, open: (root) => openFffEngine(module, root) };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
}

let engineSequence = 0;

async function openFffEngine(module: FffModule, root: string): Promise<CodeSearchEngine> {
  const created = module.FileFinder.create({ basePath: root, aiMode: true });
  if (!created.ok || created.value === undefined) {
    throw new Error(created.error ?? "fff failed to open the index");
  }
  const finder = created.value;
  await finder.waitForIndexReady(INDEX_READY_TIMEOUT_MS);
  return new FffEngine(finder, ++engineSequence);
}

type StoredCursor = { kind: "find"; page: number } | { kind: "grep"; cursor: FffGrepCursor };

class FffEngine implements CodeSearchEngine {
  private readonly cursors = new Map<string, StoredCursor>();
  private cursorCount = 0;

  constructor(
    private readonly finder: FffFinder,
    private readonly generation: number,
  ) {}

  find(request: CodeSearchFindRequest): CodeSearchOutcome<CodeSearchFindResult> {
    const stored = this.takeCursor(request.cursor, "find");
    if (stored === null) return cursorExpired();
    const page = stored?.kind === "find" ? stored.page : 0;
    const result = this.finder.fileSearch(request.query, {
      pageIndex: page,
      pageSize: request.maxResults,
    });
    if (!result.ok || result.value === undefined) return unavailable(result.error);
    const { items, totalMatched } = result.value;
    const more = (page + 1) * request.maxResults < totalMatched && items.length > 0;
    return {
      ok: true,
      files: items.map((item) => ({
        path: item.relativePath,
        ...(item.gitStatus && item.gitStatus !== "clean" ? { gitStatus: item.gitStatus } : {}),
      })),
      totalMatched,
      ...(more ? { nextCursor: this.putCursor({ kind: "find", page: page + 1 }) } : {}),
    };
  }

  search(request: CodeSearchTextRequest): CodeSearchOutcome<CodeSearchTextResult> {
    const stored = this.takeCursor(request.cursor, "grep");
    if (stored === null) return cursorExpired();
    const cursor = stored?.kind === "grep" ? stored.cursor : null;
    const options: FffGrepOptions = {
      cursor,
      pageSize: request.outputMode === "count" ? COUNT_PAGE_SIZE : request.maxResults,
      maxMatchesPerFile:
        request.outputMode === "files_with_matches" ? 1 : request.outputMode === "count" ? 0 : 10,
      beforeContext: request.outputMode === "content" ? request.context : 0,
      afterContext: request.outputMode === "content" ? request.context : 0,
      smartCase: true,
      timeBudgetMs: GREP_TIME_BUDGET_MS,
      enforceTimeBudget: true,
    };

    if (request.patterns !== undefined) {
      const result = this.finder.multiGrep({
        ...options,
        patterns: request.patterns,
        ...(request.constraints ? { constraints: request.constraints } : {}),
      });
      return this.grepOutcome(result, "plain", false);
    }

    const query = [request.constraints, request.query].filter(Boolean).join(" ");
    const mode: CodeSearchGrepMode = hasRegexSyntax(searchText(request.query ?? ""))
      ? "regex"
      : "plain";
    const first = this.finder.grep(query, { ...options, mode });
    const exhausted =
      first.ok && first.value !== undefined && first.value.items.length === 0 && !first.value.nextCursor;
    if (!exhausted || cursor !== null) return this.grepOutcome(first, mode, false);
    // 与 fff-mcp 相同的退路：精确/正则无结果时改用模糊匹配，容忍拼写差异。
    const fuzzy = this.finder.grep(query, { ...options, mode: "fuzzy", maxMatchesPerFile: 3 });
    const outcome = this.grepOutcome(fuzzy, "fuzzy", true);
    if (outcome.ok && first.value?.regexFallbackError) {
      return { ...outcome, regexError: first.value.regexFallbackError };
    }
    return outcome;
  }

  destroy(): void {
    this.cursors.clear();
    if (!this.finder.isDestroyed) this.finder.destroy();
  }

  private grepOutcome(
    result: FffResult<FffGrepResult>,
    mode: CodeSearchGrepMode,
    fuzzyFallback: boolean,
  ): CodeSearchOutcome<CodeSearchTextResult> {
    if (!result.ok || result.value === undefined) return unavailable(result.error);
    const { items, nextCursor, regexFallbackError } = result.value;
    return {
      ok: true,
      matches: items.map(toLineHit),
      mode,
      fuzzyFallback,
      ...(regexFallbackError ? { regexError: regexFallbackError } : {}),
      ...(nextCursor ? { nextCursor: this.putCursor({ kind: "grep", cursor: nextCursor }) } : {}),
    };
  }

  /** undefined = 没传 cursor；null = 传了但已失效（索引重开过或被挤出）。 */
  private takeCursor(id: string | undefined, kind: StoredCursor["kind"]) {
    if (id === undefined) return undefined;
    const stored = this.cursors.get(id);
    return stored?.kind === kind ? stored : null;
  }

  private putCursor(cursor: StoredCursor): string {
    const id = `${this.generation}.${++this.cursorCount}`;
    this.cursors.set(id, cursor);
    if (this.cursors.size > MAX_CURSORS) {
      const oldest = this.cursors.keys().next().value;
      if (oldest !== undefined) this.cursors.delete(oldest);
    }
    return id;
  }
}

function toLineHit(match: FffGrepMatch): CodeSearchLineHit {
  return {
    path: match.relativePath,
    line: match.lineNumber,
    text: clip(match.lineContent),
    ...(match.contextBefore?.length ? { before: match.contextBefore.map(clip) } : {}),
    ...(match.contextAfter?.length ? { after: match.contextAfter.map(clip) } : {}),
  };
}

function clip(line: string): string {
  const trimmed = line.replace(/\r$/, "");
  return trimmed.length > MAX_LINE_CHARS ? `${trimmed.slice(0, MAX_LINE_CHARS)}…` : trimmed;
}

/** 去掉 fff 约束 token（`src/`、`*.ts`、`!test`、带 `/` 的 glob）后剩下的搜索文本。 */
export function searchText(query: string): string {
  return query
    .split(/\s+/)
    .filter(
      (token) =>
        token.length > 0 &&
        !/^!\S{3,}$/.test(token) &&
        !/^\/?[\w.-]+(\/[\w.-]+)*\/$/.test(token) &&
        !/^\*\.[\w{},]+$/.test(token) &&
        !(token.includes("/") && /[*{]/.test(token)),
    )
    .join(" ");
}

export function hasRegexSyntax(text: string): boolean {
  return /[\\^$.|?*+()[\]{}]/.test(text);
}

function unavailable(error: string | undefined): CodeSearchOutcome<never> {
  return { ok: false, reason: "unavailable", message: error ?? "fff search failed" };
}

function cursorExpired(): CodeSearchOutcome<never> {
  return {
    ok: false,
    reason: "cursor_expired",
    message: "This cursor has expired (the index was reopened). Run the search again without it.",
  };
}
