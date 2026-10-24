import {
  createUnsupportedModelInputMediaText,
  detectImageMediaType,
  getUnsupportedModelInputMediaKind,
  modelMessageContentToText,
  toModelImageMediaType,
  type DetectedImageMediaType,
  type ModelInputFormat,
  type ModelMessageContentBlock,
} from "@zcode/contracts";

/**
 * Media type to declare for an image part, or undefined when the provider would reject it.
 * 根因：GPUI 粘贴的 Windows 剪贴板 BMP 以 image/bmp 原样进入首条用户消息，provider 以 400 拒绝，
 * 且该消息被持久化，会话此后每次请求都失败。请求出口是所有图片来源（附件、MCP、冷恢复）的
 * 唯一汇合点，在这里按实际字节/声明类型收口为 provider 白名单。
 */
export function providerImageMediaType(
  declared: string,
  base64Data: string,
): DetectedImageMediaType | undefined {
  // 16 个 base64 字符解码为 12 字节，足够覆盖 PNG/JPEG/GIF/WebP 的魔数。
  const sniffed = detectImageMediaType(Buffer.from(base64Data.slice(0, 16), "base64"));
  return sniffed ?? toModelImageMediaType(declared);
}

export function unsupportedImageMediaTypeText(
  block: Extract<ModelMessageContentBlock, { type: "image" }>,
): string {
  const placeholder = modelMessageContentToText([block]) || "[Attached image]";
  return `${placeholder}\n[Image omitted from provider request: ${block.mediaType} is not a supported image format (PNG, JPEG, GIF or WebP).]`;
}

export function unsupportedInputMediaText(
  block: ModelMessageContentBlock,
  inputFormat: ModelInputFormat | undefined,
): string | undefined {
  if (!inputFormat) return undefined;
  const unsupportedKind = getUnsupportedModelInputMediaKind(block, inputFormat);
  return unsupportedKind ? createUnsupportedModelInputMediaText(block, unsupportedKind) : undefined;
}

export function dataUrlToDataContent(
  dataUrl: string,
): { mediaType: string; data: string } | undefined {
  // Provider 格式重构与 PDF 支持发生冲突时，旧正则解析和新实现的
  // 返回语句被错误拼接，导致 mediaType 未定义。保留统一的 ModelInputFormat，
  // 同时完整采用支持参数化 Data URL 的解析路径，避免再次混用两套实现。
  const commaIndex = dataUrl.indexOf(",");
  if (dataUrl.slice(0, "data:".length).toLowerCase() !== "data:" || commaIndex < 0) {
    return undefined;
  }
  const headerParts = dataUrl.slice("data:".length, commaIndex).split(";");
  const mediaType = headerParts.shift()?.trim();
  if (headerParts.at(-1)?.trim().toLowerCase() !== "base64" || !mediaType) return undefined;
  const data = dataUrl.slice(commaIndex + 1);
  if (data.length === 0) return undefined;
  return { mediaType: mediaType.toLowerCase(), data };
}
