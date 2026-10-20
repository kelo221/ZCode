// file:// URI 与本地路径互转。服务器回传的 URI 形态不一（rust-analyzer `file:///c:/x`，
// typescript-language-server `file:///c%3A/x`），比较时一律先转成路径键。

import { posix, resolve, win32 } from "node:path";
import { pathToFileURL } from "node:url";

export function filePathToUri(filePath: string): string {
  return pathToFileURL(resolve(filePath)).href;
}

export function uriToFilePath(
  uri: string,
  platform: NodeJS.Platform = process.platform,
): string | undefined {
  if (!uri.toLowerCase().startsWith("file://")) return undefined;
  let rest = uri.slice("file://".length);
  // file://host/share/x（UNC）保留主机名；file:///x 主机为空。
  const host = rest.startsWith("/") ? "" : rest.slice(0, rest.indexOf("/") >>> 0);
  rest = rest.slice(host.length);
  let decoded: string;
  try {
    decoded = decodeURIComponent(rest);
  } catch {
    return undefined;
  }
  if (platform !== "win32") return host ? undefined : posix.normalize(decoded);
  if (host) return win32.normalize(`\\\\${host}${decoded.replaceAll("/", "\\")}`);
  const withoutLeadingSlash = /^\/[A-Za-z]:/u.test(decoded) ? decoded.slice(1) : decoded;
  return win32.normalize(withoutLeadingSlash.replaceAll("/", "\\"));
}

/** 比较用的路径键：Windows 文件系统大小写不敏感，盘符写法也各不相同。 */
export function filePathKey(
  filePath: string,
  platform: NodeJS.Platform = process.platform,
): string {
  if (platform === "win32") return win32.resolve(filePath).toLowerCase();
  return posix.resolve(filePath);
}

export function uriKey(
  uri: string,
  platform: NodeJS.Platform = process.platform,
): string | undefined {
  const filePath = uriToFilePath(uri, platform);
  return filePath === undefined ? undefined : filePathKey(filePath, platform);
}
