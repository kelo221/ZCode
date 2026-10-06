import { createHash, randomUUID } from "node:crypto";
import {
  v4AttachmentAbortResultSchema,
  v4AttachmentBeginParamsSchema,
  v4AttachmentBeginResultSchema,
  v4AttachmentChunkParamsSchema,
  v4AttachmentChunkResultSchema,
  v4AttachmentCommitParamsSchema,
  v4AttachmentCommitResultSchema,
  type CommandEnvelope,
  type CommandAck,
} from "@zcode/shared/zcode-protocol-v4";

type Ports = {
  rpc(method: string, params: unknown): Promise<unknown>;
  command(
    type: CommandEnvelope["type"],
    sessionId: string | null,
    payload: unknown,
  ): Promise<CommandAck>;
  requestBodies(): unknown[];
  waitFor(predicate: () => boolean, label: string): Promise<void>;
};

export async function backendAttachmentSmoke(sessionId: string, ports: Ports) {
  const bytes = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==",
    "base64",
  );
  const identity = { sessionId, connectionId: "smoke-live", uploadId: `upload-${randomUUID()}` };
  const params = v4AttachmentBeginParamsSchema.parse({
    ...identity,
    fileName: "scratch-image.png",
    mime: "image/png",
    totalBytes: bytes.length,
    totalChunks: 1,
    checksum: `sha256:${createHash("sha256").update(bytes).digest("hex")}`,
  });
  const begin = v4AttachmentBeginResultSchema.parse(await ports.rpc("v4/attachment/begin", params));
  if (
    begin.uploadId !== identity.uploadId ||
    begin.state !== "staging" ||
    begin.nextChunkIndex !== 0
  )
    throw new Error("Attachment begin identity/progress mismatch");
  const chunkParams = v4AttachmentChunkParamsSchema.parse({
    ...identity,
    chunkIndex: 0,
    dataBase64: bytes.toString("base64"),
  });
  const chunk = v4AttachmentChunkResultSchema.parse(
    await ports.rpc("v4/attachment/chunk", chunkParams),
  );
  if (chunk.uploadId !== identity.uploadId || chunk.nextChunkIndex !== 1)
    throw new Error("Attachment chunk identity/progress mismatch");
  const repeated = v4AttachmentChunkResultSchema.parse(
    await ports.rpc("v4/attachment/chunk", chunkParams),
  );
  if (repeated.nextChunkIndex !== 1) throw new Error("Attachment chunk retry was not idempotent");
  const terminal = v4AttachmentCommitParamsSchema.parse(identity);
  const committed = v4AttachmentCommitResultSchema.parse(
    await ports.rpc("v4/attachment/commit", terminal),
  );
  if (!committed.ref.startsWith("zcode-artifact://"))
    throw new Error("Attachment commit did not return an artifact URI");
  const duplicate = v4AttachmentCommitResultSchema.parse(
    await ports.rpc("v4/attachment/commit", terminal),
  );
  if (duplicate.ref !== committed.ref) throw new Error("Attachment commit retry changed the ref");
  const marker = ports.requestBodies().length;
  const sent = await ports.command("sendText", sessionId, {
    text: "Scratch uploaded image probe",
    attachments: [
      { ref: committed.ref, fileName: "scratch-image.png", mime: "image/png", bytes: bytes.length },
    ],
  });
  if (sent.status !== "accepted") throw new Error("Uploaded attachment input was not admitted");
  try {
    await ports.waitFor(
      () =>
        ports
          .requestBodies()
          .slice(marker)
          .some((body) => {
            const text = JSON.stringify(body);
            return (
              text.includes("Scratch uploaded image probe") &&
              text.includes("data:image/png;base64,")
            );
          }),
      "uploaded image model request",
    );
  } catch {
    const bodies = ports
      .requestBodies()
      .slice(marker)
      .map((body) => JSON.stringify(body));
    throw new Error(
      `Uploaded image observation failed: requests=${bodies.length}, prompt=${bodies.some((text) => text.includes("Scratch uploaded image probe"))}, image=${bodies.some((text) => text.includes("data:image"))}, imageBlock=${bodies.some((text) => text.includes("image_url"))}, unsupported=${bodies.some((text) => text.includes("does not support"))}, delivery=${sent.result?.type === "inputAccepted" ? sent.result.delivery : "unknown"}`,
    );
  }
  const canceled = { ...params, uploadId: `upload-${randomUUID()}` };
  v4AttachmentBeginResultSchema.parse(await ports.rpc("v4/attachment/begin", canceled));
  v4AttachmentAbortResultSchema.parse(
    await ports.rpc(
      "v4/attachment/abort",
      v4AttachmentCommitParamsSchema.parse({ ...identity, uploadId: canceled.uploadId }),
    ),
  );
  let refused = false;
  try {
    await ports.rpc("v4/attachment/chunk", { ...chunkParams, uploadId: canceled.uploadId });
  } catch {
    refused = true;
  }
  if (!refused) throw new Error("Canceled attachment still accepted chunks");
  return {
    attachmentUpload: true,
    attachmentUploadIdempotence: true,
    attachmentAbort: true,
    uploadedImageModelInput: true,
  };
}
