// ============================================================
// Code Search Port - 常驻内存的 fff 索引（GPUI 专属）
// ============================================================
//
// 只有 Host 在 runtime preferences 里下发 codeSearchEnabled、且本进程能加载 fff 原生库时
// 才注入；端口在场即代表 FindFiles / SearchCode 可见。端口绑定在会话所属 workspace 上，
// 调用方不传根目录。路径一律相对 workspace 根、以 `/` 分隔。

export type CodeSearchOutputMode = "content" | "files_with_matches" | "count";

export type CodeSearchGrepMode = "plain" | "regex" | "fuzzy";

export interface CodeSearchFindRequest {
  query: string;
  maxResults: number;
  cursor?: string;
}

export interface CodeSearchFileHit {
  path: string;
  /** fff git 状态；clean 时省略。 */
  gitStatus?: string;
}

export interface CodeSearchFindResult {
  files: CodeSearchFileHit[];
  totalMatched: number;
  nextCursor?: string;
}

export interface CodeSearchTextRequest {
  /** 单个查询；可内联 fff 约束（`src/`、`*.ts`、`!test`）。 */
  query?: string;
  /** 多模式 OR 字面量搜索；与 query 二选一。 */
  patterns?: string[];
  /** 额外约束，与 query/patterns 同语法。 */
  constraints?: string;
  outputMode: CodeSearchOutputMode;
  context: number;
  maxResults: number;
  cursor?: string;
}

export interface CodeSearchLineHit {
  path: string;
  /** 1-based。 */
  line: number;
  text: string;
  before?: string[];
  after?: string[];
}

export interface CodeSearchTextResult {
  matches: CodeSearchLineHit[];
  mode: CodeSearchGrepMode;
  /** plain/regex 无结果后改用模糊匹配。 */
  fuzzyFallback: boolean;
  regexError?: string;
  nextCursor?: string;
}

export type CodeSearchFailureReason = "unavailable" | "cursor_expired";

export type CodeSearchOutcome<T> =
  | ({ ok: true } & T)
  | { ok: false; reason: CodeSearchFailureReason; message: string };

export interface CodeSearchPort {
  findFiles(
    request: CodeSearchFindRequest,
    options?: { signal?: AbortSignal },
  ): Promise<CodeSearchOutcome<CodeSearchFindResult>>;
  searchCode(
    request: CodeSearchTextRequest,
    options?: { signal?: AbortSignal },
  ): Promise<CodeSearchOutcome<CodeSearchTextResult>>;
}
