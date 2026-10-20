import { describe, expect, it } from "vitest";
import {
  encodeLspMessage,
  filePathKey,
  LSP_MAX_HEADER_BYTES,
  LspFramingError,
  LspMessageReader,
  uriKey,
  uriToFilePath,
} from "../src/lsp/index.js";

describe("LSP framing", () => {
  it("counts Content-Length in UTF-8 bytes, not characters", () => {
    const frame = encodeLspMessage({ text: "héllo 🦀" });
    const header = frame.toString("ascii", 0, frame.indexOf("\r\n\r\n"));
    const body = JSON.stringify({ text: "héllo 🦀" });
    expect(header).toBe(`Content-Length: ${Buffer.byteLength(body, "utf8")}`);
    expect(Buffer.byteLength(body, "utf8")).toBeGreaterThan(body.length);
  });

  it("parses messages split at every byte, including inside a multi-byte character", () => {
    const frame = Buffer.concat([encodeLspMessage({ a: "日本" }), encodeLspMessage({ b: 2 })]);
    const reader = new LspMessageReader();
    const messages: unknown[] = [];
    for (let index = 0; index < frame.length; index += 1) {
      messages.push(...reader.push(frame.subarray(index, index + 1)));
    }
    expect(messages).toEqual([{ a: "日本" }, { b: 2 }]);
  });

  it("parses several messages coalesced into one chunk", () => {
    const reader = new LspMessageReader();
    const frame = Buffer.concat([1, 2, 3].map((n) => encodeLspMessage({ n })));
    expect(reader.push(frame)).toEqual([{ n: 1 }, { n: 2 }, { n: 3 }]);
  });

  it("accepts extra headers and any header case", () => {
    const body = Buffer.from('{"ok":true}', "utf8");
    const frame = Buffer.concat([
      Buffer.from(
        `content-length: ${body.length}\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n`,
        "ascii",
      ),
      body,
    ]);
    expect(new LspMessageReader().push(frame)).toEqual([{ ok: true }]);
  });

  it("rejects oversized headers, oversized bodies and missing lengths", () => {
    expect(() => new LspMessageReader().push(Buffer.alloc(LSP_MAX_HEADER_BYTES + 1, 65))).toThrow(
      LspFramingError,
    );
    expect(() =>
      new LspMessageReader().push(Buffer.from("Content-Length: 999999999999\r\n\r\n", "ascii")),
    ).toThrow(LspFramingError);
    expect(() => new LspMessageReader().push(Buffer.from("X: 1\r\n\r\n{}", "ascii"))).toThrow(
      LspFramingError,
    );
    expect(() =>
      new LspMessageReader().push(Buffer.from("Content-Length: 3\r\n\r\n{x}", "ascii")),
    ).toThrow(LspFramingError);
  });
});

describe("LSP file URIs", () => {
  it("decodes Windows drive URIs in both encodings to the same key", () => {
    expect(uriToFilePath("file:///c%3A/Users/V/src/lib.rs", "win32")).toBe(
      "c:\\Users\\V\\src\\lib.rs",
    );
    expect(uriToFilePath("file:///C:/Users/V/src/lib.rs", "win32")).toBe(
      "C:\\Users\\V\\src\\lib.rs",
    );
    expect(uriKey("file:///c%3A/Users/V/src/lib.rs", "win32")).toBe(
      filePathKey("C:\\Users\\V\\src\\lib.rs", "win32"),
    );
  });

  it("decodes percent-encoded spaces and UNC hosts", () => {
    expect(uriToFilePath("file:///C:/My%20Project/a.ts", "win32")).toBe("C:\\My Project\\a.ts");
    expect(uriToFilePath("file://server/share/a.cs", "win32")).toBe("\\\\server\\share\\a.cs");
  });

  it("keeps POSIX paths case-sensitive and rejects non-file URIs", () => {
    expect(uriToFilePath("file:///home/v/A.rs", "linux")).toBe("/home/v/A.rs");
    expect(filePathKey("/home/v/A.rs", "linux")).not.toBe(filePathKey("/home/v/a.rs", "linux"));
    expect(uriToFilePath("untitled:Untitled-1", "linux")).toBeUndefined();
  });
});
