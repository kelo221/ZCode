import { createServer } from "node:http";

export function smokeProviderConfig(modelSelection: unknown, port: number) {
  return {
    schemaVersion: 1,
    config: {
      defaultModelSelection: modelSelection,
      providerConfigRules: {
        providerRules: [
          {
            providerId: "smoke-openai",
            providerName: "OpenAI Smoke",
            enabled: true,
            config: {
              group: "standard-personal",
              access: { type: "api-key", apiKey: "local-test-key" },
              api: { type: "openai-chat-completions", baseUrl: `http://127.0.0.1:${port}/v1` },
              personalModelIds: ["mock-chat"],
            },
          },
        ],
      },
      modelConfigRules: {
        providerModelRules: [
          {
            providerId: "smoke-openai",
            modelId: "mock-chat",
            config: {
              enabled: true,
              properties: {
                requiresMfjsToolSchema: false,
                contextWindow: 8192,
                inputFormat: {
                  supportsText: true,
                  supportsImage: true,
                  supportsVideo: false,
                  supportsAudio: false,
                  supportsPdf: false,
                },
                outputFormat: { supportsText: true },
                supportsToolCall: true,
                supportsJsonSchemaOutput: false,
                supportsNativeWebSearch: false,
                supportsMidConversationSystem: true,
              },
              optionSpecs: {
                reasoningLevel: { values: ["low"], map: '{"reasoning_effort": reasoningLevel}' },
                maxOutputTokens: { max: 2048, map: '{"max_tokens": maxOutputTokens}' },
              },
            },
          },
        ],
        manualProviderModelRules: [],
      },
    },
  };
}

export async function backendMock() {
  let requests = 0;
  const requestBodies: unknown[] = [];
  let nextHold: { entered(): void; gate: Promise<void>; release(): void } | undefined;
  const releases = new Set<() => void>();
  const holdNext = () => {
    if (nextHold) throw new Error("A mock request is already armed");
    let entered!: () => void;
    let release!: () => void;
    const arrived = new Promise<void>((resolve) => {
      entered = resolve;
    });
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    releases.add(release);
    nextHold = { entered, gate, release };
    return { arrived, release };
  };
  const server = createServer(async (request, response) => {
    try {
      if (request.url !== "/v1/chat/completions") {
        response.writeHead(404).end("Not found");
        return;
      }
      const chunks: Buffer[] = [];
      for await (const chunk of request) chunks.push(Buffer.from(chunk));
      const body = JSON.parse(Buffer.concat(chunks).toString()) as { stream?: boolean };
      requests++;
      requestBodies.push(body);
      const hold = nextHold;
      nextHold = undefined;
      if (hold) {
        hold.entered();
        await hold.gate;
        releases.delete(hold.release);
      }
      const content = "ZCODE_SMOKE_OK";
      if (!body.stream) {
        response.writeHead(200, { "content-type": "application/json" }).end(
          JSON.stringify({
            id: "mock",
            object: "chat.completion",
            model: "mock-chat",
            choices: [{ index: 0, message: { role: "assistant", content }, finish_reason: "stop" }],
            usage: { prompt_tokens: 10, completion_tokens: 3, total_tokens: 13 },
          }),
        );
        return;
      }
      const frames = [
        {
          id: "mock",
          object: "chat.completion.chunk",
          model: "mock-chat",
          choices: [{ index: 0, delta: { role: "assistant", content }, finish_reason: null }],
        },
        {
          id: "mock",
          object: "chat.completion.chunk",
          model: "mock-chat",
          choices: [{ index: 0, delta: {}, finish_reason: "stop" }],
          usage: { prompt_tokens: 10, completion_tokens: 3, total_tokens: 13 },
        },
      ];
      response
        .writeHead(200, { "content-type": "text/event-stream" })
        .end(
          frames.map((frame) => `data: ${JSON.stringify(frame)}\n\n`).join("") + "data: [DONE]\n\n",
        );
    } catch {
      response.writeHead(400).end("Invalid mock request");
    }
  });
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("No mock server port");
  return {
    server,
    port: address.port,
    requests: () => requests,
    requestBodies: () => requestBodies,
    holdNext,
    releaseAll() {
      for (const release of releases) release();
      releases.clear();
      nextHold = undefined;
    },
  };
}
