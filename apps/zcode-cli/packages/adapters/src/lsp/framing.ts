// LSP base protocol framing（Content-Length 以 UTF-8 字节计）。
// 合同见 apps/zcode-cli/docs/design/v2/tool/lsp-diagnostics.md「Client」。

export const LSP_MAX_HEADER_BYTES = 8 * 1024;
export const LSP_MAX_BODY_BYTES = 32 * 1024 * 1024;

const HEADER_END = Buffer.from("\r\n\r\n", "ascii");

export class LspFramingError extends Error {}

export function encodeLspMessage(message: unknown): Buffer {
  const body = Buffer.from(JSON.stringify(message), "utf8");
  const header = Buffer.from(`Content-Length: ${body.byteLength}\r\n\r\n`, "ascii");
  return Buffer.concat([header, body]);
}

/**
 * 增量解析：一个 chunk 里可能有半条、一条或多条消息。超过上限或头部损坏时抛
 * LspFramingError；调用方应结束该服务器进程，而不是尝试重新同步字节流。
 */
export class LspMessageReader {
  private buffer: Buffer = Buffer.alloc(0);

  push(chunk: Buffer): unknown[] {
    this.buffer = this.buffer.byteLength === 0 ? chunk : Buffer.concat([this.buffer, chunk]);
    const messages: unknown[] = [];
    for (;;) {
      const headerEnd = this.buffer.indexOf(HEADER_END);
      if (headerEnd < 0) {
        if (this.buffer.byteLength > LSP_MAX_HEADER_BYTES) {
          throw new LspFramingError("LSP header exceeds the size limit");
        }
        return messages;
      }
      if (headerEnd > LSP_MAX_HEADER_BYTES) {
        throw new LspFramingError("LSP header exceeds the size limit");
      }
      const length = parseContentLength(this.buffer.subarray(0, headerEnd).toString("ascii"));
      const bodyStart = headerEnd + HEADER_END.byteLength;
      if (this.buffer.byteLength - bodyStart < length) return messages;
      const body = this.buffer.subarray(bodyStart, bodyStart + length).toString("utf8");
      this.buffer = this.buffer.subarray(bodyStart + length);
      try {
        messages.push(JSON.parse(body));
      } catch {
        throw new LspFramingError("LSP message body is not valid JSON");
      }
    }
  }
}

function parseContentLength(header: string): number {
  let length: number | undefined;
  for (const line of header.split("\r\n")) {
    const separator = line.indexOf(":");
    if (separator <= 0) continue;
    if (line.slice(0, separator).trim().toLowerCase() !== "content-length") continue;
    const value = line.slice(separator + 1).trim();
    if (!/^\d+$/u.test(value)) throw new LspFramingError("Invalid Content-Length header");
    length = Number(value);
  }
  if (length === undefined) throw new LspFramingError("Missing Content-Length header");
  if (length > LSP_MAX_BODY_BYTES) throw new LspFramingError("LSP message exceeds the size limit");
  return length;
}
