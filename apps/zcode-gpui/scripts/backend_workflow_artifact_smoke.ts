import { v4ConversationWorkflowRunArtifactReadResultSchema } from "@zcode/shared/zcode-protocol-v4";

export async function backendWorkflowArtifactReadSmoke(
  sessionId: string,
  otherSessionId: string,
  runId: string,
  rpc: (method: string, params: unknown) => Promise<unknown>,
) {
  const params = {
    sessionId,
    runId,
    artifactId: "smoke-report",
    version: 1,
    offset: 0,
    limit: 256 * 1024,
  };
  const response = v4ConversationWorkflowRunArtifactReadResultSchema.parse(
    await rpc("v4/conversation/workflowRunArtifactRead", params),
  );
  const bytes = Buffer.from(response.dataBase64, "base64");
  if (
    response.mediaType !== "text/markdown" ||
    response.totalBytes !== bytes.length ||
    response.nextOffset !== null ||
    bytes.toString("utf8") !== "# Isolated report"
  )
    throw new Error("Authorized Markdown artifact bytes were not returned");
  let unauthorizedRefused = false;
  try {
    await rpc("v4/conversation/workflowRunArtifactRead", { ...params, sessionId: otherSessionId });
  } catch {
    unauthorizedRefused = true;
  }
  if (!unauthorizedRefused)
    throw new Error("Another parent session could read the workflow artifact");
  return { workflowMarkdownContent: true, workflowArtifactParentAuthorization: true };
}
