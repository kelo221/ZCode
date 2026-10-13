// Loopback OpenAI-compatible mock that scripts one representative conversation for
// visual comparison captures: reasoning, one read-only tool call, then a Markdown answer
// with a heading, list, inline code, a fenced code block and a table. No credentials.
//
//   bun apps/zcode-gpui/scripts/visual_chat_mock.ts --state <file.json> [--workspace <dir>]
//
// The state file receives {port, pid} once listening and {served} after each response,
// so the harness can wait for the final answer without polling the UI.
import { createServer } from "node:http";
import { writeFileSync } from "node:fs";

const stateFlag = process.argv.indexOf("--state");
const statePath = stateFlag > 0 ? process.argv[stateFlag + 1] : undefined;
if (!statePath) throw new Error("--state <file> is required");
const workspaceFlag = process.argv.indexOf("--workspace");
const workspace = workspaceFlag > 0 ? process.argv[workspaceFlag + 1] : undefined;

type Message = { role: string; content?: unknown };
type Body = { stream?: boolean; messages?: Message[]; tools?: { function?: { name?: string } }[] };

const REASONING =
  "The user wants an overview of the sample project. I'll read the README first, then summarize the layout and show the entry point.";
const ANSWER = `## Sample project overview

The workspace is a small TypeScript service with three parts:

- \`src/app.ts\` starts the HTTP server and registers routes.
- \`src/routes/\` holds one handler per resource.
- \`README.md\` documents setup and the \`bun run dev\` workflow.

\`\`\`ts
import { serve } from "bun";

serve({
  port: 3000,
  fetch(request) {
    return new Response(\`Hello from \${new URL(request.url).pathname}\`);
  },
});
\`\`\`

| File | Purpose |
| --- | --- |
| \`src/app.ts\` | Server entry point |
| \`README.md\` | Setup notes |

Next, I can add request logging or a health-check route.`;

let served = 0;
let answered = false;
let toolNames: string[] = [];
const publish = (extra: Record<string, unknown> = {}) =>
  writeFileSync(statePath, JSON.stringify({ port, pid: process.pid, served, answered, toolNames, ...extra }));

function chunk(delta: Record<string, unknown>, finish: string | null = null) {
  return { id: "visual", object: "chat.completion.chunk", model: "visual-chat", choices: [{ index: 0, delta, finish_reason: finish }] };
}

function split(text: string, size = 24): string[] {
  const parts: string[] = [];
  for (let i = 0; i < text.length; i += size) parts.push(text.slice(i, i + size));
  return parts;
}

function script(body: Body): object[] {
  const messages = body.messages ?? [];
  const afterTool = messages.at(-1)?.role === "tool";
  const names = (body.tools ?? []).map((tool) => tool.function?.name ?? "");
  if (names.length) toolNames = names;
  const tool = workspace && names.includes("Read") ? "Read" : undefined;
  if (!afterTool && tool) {
    return [
      ...split(REASONING).map((text) => chunk({ role: "assistant", reasoning_content: text })),
      chunk({
        tool_calls: [{
          index: 0, id: "call_visual_1", type: "function",
          function: { name: tool, arguments: JSON.stringify({ file_path: `${workspace}${workspace?.includes("\\") ? "\\" : "/"}README.md` }) },
        }],
      }),
      chunk({}, "tool_calls"),
    ];
  }
  answered = true;
  return [...split(ANSWER, 48).map((text) => chunk({ role: "assistant", content: text })), chunk({}, "stop")];
}

const server = createServer(async (request, response) => {
  if (request.url !== "/v1/chat/completions") {
    response.writeHead(404).end("Not found");
    return;
  }
  const chunks: Buffer[] = [];
  for await (const part of request) chunks.push(Buffer.from(part));
  let body: Body;
  try {
    body = JSON.parse(Buffer.concat(chunks).toString()) as Body;
  } catch {
    response.writeHead(400).end("Invalid mock request");
    return;
  }
  const usage = { prompt_tokens: 1200, completion_tokens: 180, total_tokens: 1380 };
  if (!body.stream) {
    // Title and other auxiliary requests.
    response.writeHead(200, { "content-type": "application/json" }).end(JSON.stringify({
      id: "visual", object: "chat.completion", model: "visual-chat", usage,
      choices: [{ index: 0, message: { role: "assistant", content: "Sample project overview" }, finish_reason: "stop" }],
    }));
    return;
  }
  const frames = script(body);
  frames.push({ ...chunk({}), choices: [], usage });
  response.writeHead(200, { "content-type": "text/event-stream" });
  for (const frame of frames) {
    response.write(`data: ${JSON.stringify(frame)}\n\n`);
    await Bun.sleep(15);
  }
  response.end("data: [DONE]\n\n");
  served++;
  publish();
});

await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
const address = server.address();
const port = typeof address === "object" && address ? address.port : 0;
publish();
