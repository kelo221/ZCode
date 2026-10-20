// 内置语言服务器表、项目根判定与 PATH 查找。
// 合同见 apps/zcode-cli/docs/design/v2/tool/lsp-diagnostics.md「Lifecycle」。

import { execFile } from "node:child_process";
import { access, readdir, stat } from "node:fs/promises";
import { constants } from "node:fs";
import { delimiter, dirname, extname, join, resolve } from "node:path";
import { windowsExecutableCandidates } from "../exec/windows-executable.js";

export type LanguageServerId = "rust-analyzer" | "csharp-ls" | "typescript-language-server";

export interface LanguageServerSpec {
  id: LanguageServerId;
  command: string;
  args: string[];
  extensions: readonly string[];
  languageId(filePath: string): string;
  /** 有项目级编译检查（rust-analyzer flycheck）。 */
  projectCheck: boolean;
  /** 同时作为 initializationOptions 与 workspace/configuration 的应答。 */
  settings?: Record<string, unknown>;
  configurationSection?: string;
  /** 只认这些 source 为当前内容的分析结果；其余（rustc/clippy）属于上一次完整检查。 */
  nativeSources?: readonly string[];
}

// checkOnSave=false：编辑永远不启动或重启 cargo check，只有 Diagnostics 工具显式触发。
// cargo.targetDir=true：检查产物进 target 的子目录，不和用户或 Agent 的 cargo build 抢锁。
// files.watcher=server：关闭的文件由服务器自己监视磁盘。
const RUST_ANALYZER_SETTINGS: Record<string, unknown> = {
  checkOnSave: false,
  cargo: { targetDir: true },
  files: { watcher: "server" },
};

const TS_EXTENSIONS = [".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs"] as const;

export const LANGUAGE_SERVER_SPECS: readonly LanguageServerSpec[] = [
  {
    id: "rust-analyzer",
    command: "rust-analyzer",
    args: [],
    extensions: [".rs"],
    languageId: () => "rust",
    projectCheck: true,
    settings: RUST_ANALYZER_SETTINGS,
    configurationSection: "rust-analyzer",
    nativeSources: ["rust-analyzer"],
  },
  {
    id: "csharp-ls",
    command: "csharp-ls",
    args: [],
    extensions: [".cs"],
    languageId: () => "csharp",
    projectCheck: false,
  },
  {
    id: "typescript-language-server",
    command: "typescript-language-server",
    args: ["--stdio"],
    extensions: TS_EXTENSIONS,
    languageId: (filePath) => {
      const ext = extname(filePath).toLowerCase();
      if (ext === ".tsx") return "typescriptreact";
      if (ext === ".jsx") return "javascriptreact";
      return [".js", ".mjs", ".cjs"].includes(ext) ? "javascript" : "typescript";
    },
    projectCheck: false,
  },
];

export function languageServerForFile(filePath: string): LanguageServerSpec | undefined {
  const ext = extname(filePath).toLowerCase();
  return LANGUAGE_SERVER_SPECS.find((spec) => spec.extensions.includes(ext));
}

export interface RootDetectionDeps {
  /** 解析 Cargo workspace 根；缺省调用 `cargo locate-project --workspace`。 */
  cargoWorkspaceRoot?: (manifestPath: string) => Promise<string | undefined>;
}

/** 文件所属项目根；找不到时返回 undefined（该文件不做诊断）。 */
export async function findDiagnosticsRoot(
  spec: LanguageServerSpec,
  filePath: string,
  deps: RootDetectionDeps = {},
): Promise<string | undefined> {
  const start = dirname(resolve(filePath));
  if (spec.id === "rust-analyzer") {
    const manifestDir = await findUpward(start, async (dir) => isFile(join(dir, "Cargo.toml")));
    if (!manifestDir) return undefined;
    const locate = deps.cargoWorkspaceRoot ?? resolveCargoWorkspaceRoot;
    return (await locate(join(manifestDir, "Cargo.toml"))) ?? manifestDir;
  }
  if (spec.id === "csharp-ls") {
    return findUpward(start, async (dir) => {
      const entries = await readdir(dir).catch(() => [] as string[]);
      return entries.some((entry) => entry.toLowerCase().endsWith(".sln"));
    });
  }
  return (
    (await findUpward(start, async (dir) => isFile(join(dir, "tsconfig.json")))) ??
    (await findUpward(start, async (dir) => isFile(join(dir, "package.json"))))
  );
}

/** `cargo locate-project --workspace` 处理 package.workspace 等情况；失败时返回 undefined。 */
export function resolveCargoWorkspaceRoot(manifestPath: string): Promise<string | undefined> {
  return new Promise((done) => {
    execFile(
      "cargo",
      [
        "locate-project",
        "--workspace",
        "--message-format",
        "plain",
        "--manifest-path",
        manifestPath,
      ],
      { timeout: 15_000, windowsHide: true },
      (error, stdout) => {
        const located = String(stdout ?? "").trim();
        done(error || !located ? undefined : dirname(located));
      },
    );
  });
}

export async function findExecutableOnPath(
  command: string,
  env: NodeJS.ProcessEnv = process.env,
  platform: NodeJS.Platform = process.platform,
): Promise<string | undefined> {
  const candidates =
    platform === "win32"
      ? windowsExecutableCandidates(command, env)
      : (env.PATH ?? "")
          .split(delimiter)
          .filter((dir) => dir.length > 0)
          .map((dir) => join(dir, command));
  for (const candidate of candidates) {
    if (platform === "win32" ? await isFile(candidate) : await isExecutable(candidate)) {
      return candidate;
    }
  }
  return undefined;
}

async function findUpward(
  start: string,
  matches: (dir: string) => Promise<boolean>,
): Promise<string | undefined> {
  let dir = start;
  for (;;) {
    if (await matches(dir)) return dir;
    const parent = dirname(dir);
    if (parent === dir) return undefined;
    dir = parent;
  }
}

async function isFile(path: string): Promise<boolean> {
  try {
    return (await stat(path)).isFile();
  } catch {
    return false;
  }
}

async function isExecutable(path: string): Promise<boolean> {
  try {
    await access(path, constants.X_OK);
    return await isFile(path);
  } catch {
    return false;
  }
}
