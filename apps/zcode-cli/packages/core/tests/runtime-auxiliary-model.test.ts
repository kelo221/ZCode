import { describe, expect, it, vi } from "vitest";
import {
  createModelId,
  createModelProviderId,
  createTraceId,
  type Model,
  type ModelSelection,
} from "@zcode/contracts";
import { createRuntimeAuxiliaryModelResolver } from "../src/runtime/helpers/runtime-tools.js";
import type { AgentRuntimeInternal } from "../src/runtime/internal.js";
import type { AgentProfile } from "../src/subagent/profile.js";

const turnSelection: ModelSelection = {
  providerId: createModelProviderId("test"),
  modelId: createModelId("main"),
  reasoningLevel: "high",
};
const exploreSelection: ModelSelection = {
  providerId: createModelProviderId("test"),
  modelId: createModelId("explore"),
  reasoningLevel: "low",
};

describe("runtime auxiliary model resolver", () => {
  it("builds the Explore profile's model when it is set explicitly", () => {
    const { runtime, modelFactory } = createRuntime({ exploreSelection });
    const model = createRuntimeAuxiliaryModelResolver(
      runtime,
      {},
    )({
      turnModel: fakeModel(turnSelection),
    });

    expect(model).toMatchObject({ providerId: "test", modelId: "explore" });
    expect(modelFactory).toHaveBeenCalledWith(
      expect.objectContaining({ selection: exploreSelection }),
    );
  });

  it("returns undefined when Explore inherits the parent model", () => {
    const { runtime, modelFactory } = createRuntime({});
    expect(
      createRuntimeAuxiliaryModelResolver(runtime, {})({ turnModel: fakeModel(turnSelection) }),
    ).toBeUndefined();
    expect(modelFactory).not.toHaveBeenCalled();
  });

  it("returns undefined when the Explore model equals the turn model", () => {
    const { runtime, modelFactory } = createRuntime({
      exploreSelection: { ...turnSelection, reasoningLevel: "low" },
    });
    expect(
      createRuntimeAuxiliaryModelResolver(runtime, {})({ turnModel: fakeModel(turnSelection) }),
    ).toBeUndefined();
    expect(modelFactory).not.toHaveBeenCalled();
  });

  it("defers to a Core Server subagent override", () => {
    const { runtime, modelFactory } = createRuntime({ exploreSelection });
    expect(
      createRuntimeAuxiliaryModelResolver(
        runtime,
        {},
      )({
        turnModel: fakeModel(turnSelection),
        subagentModelOverride: { selection: turnSelection },
      }),
    ).toBeUndefined();
    expect(modelFactory).not.toHaveBeenCalled();
  });

  it("falls back with a warning when the Explore selection does not resolve", () => {
    const { runtime, modelFactory, warn } = createRuntime({ exploreSelection });
    const model = createRuntimeAuxiliaryModelResolver(runtime, {
      resolveEffectiveModelSelection: () => ({
        effectiveSelection: null,
        selectionIssue: "account-connection-unavailable",
      }),
    })({ turnModel: fakeModel(turnSelection) });

    expect(model).toBeUndefined();
    expect(modelFactory).not.toHaveBeenCalled();
    expect(warn).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({
        event: "runtime.auxiliary_model.fallback",
        reason: "account-connection-unavailable",
      }),
    );
  });

  it("falls back with a warning when the model factory throws", () => {
    const { runtime, warn } = createRuntime({
      exploreSelection,
      modelFactory: () => {
        throw new Error("provider missing");
      },
    });
    expect(
      createRuntimeAuxiliaryModelResolver(runtime, {})({ turnModel: fakeModel(turnSelection) }),
    ).toBeUndefined();
    expect(warn).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ reason: "provider missing" }),
    );
  });
});

function createRuntime(input: {
  exploreSelection?: ModelSelection;
  modelFactory?: (target: { selection: ModelSelection }) => Model;
}) {
  const modelFactory = vi.fn(
    input.modelFactory ?? ((target: { selection: ModelSelection }) => fakeModel(target.selection)),
  );
  const warn = vi.fn();
  const explore = {
    name: "Explore",
    description: "explore",
    source: "built-in",
    systemPrompt: "",
    tools: [],
    ...(input.exploreSelection ? { modelSelection: input.exploreSelection } : {}),
  } as AgentProfile;
  const runtime = {
    config: { taskType: "interactive" },
    getAgentDefinitions: () => ({ activeAgents: [explore] }),
    logger: { warn },
    modelFactory,
    rootTraceContext: { traceId: createTraceId(), spanId: "span" },
  } as unknown as AgentRuntimeInternal;
  return { runtime, modelFactory, warn };
}

function fakeModel(selection: ModelSelection): Model {
  return {
    providerId: selection.providerId,
    modelId: selection.modelId,
    properties: {} as Model["properties"],
    requestCapabilities: {} as Model["requestCapabilities"],
    optionSpecs: {
      reasoningLevel: { values: ["low", "high"] },
      maxOutputTokens: { max: 32_000 },
    },
    options: { maxOutputTokens: 32_000, reasoningLevel: selection.reasoningLevel ?? "low" },
    bind() {
      return this;
    },
    generateText: vi.fn(),
    streamText: vi.fn(),
  } as unknown as Model;
}
