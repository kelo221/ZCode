import { describe, expect, it } from "vitest";
import type { ProviderSettingsProviderView } from "@zcode/provider";
import {
  mutateProviderSettings,
  resolveProviderModelConfig,
  resolveProviderUpdate,
} from "../src/zcode-protocol/provider-settings.js";

function provider(overrides: Partial<ProviderSettingsProviderView> = {}): ProviderSettingsProviderView {
  return {
    providerId: "p1",
    providerName: "Custom",
    templateId: undefined,
    enabled: true,
    executable: true,
    effectiveConfig: {
      group: "standard-personal",
      access: { type: "api-key", apiKey: "old" },
      api: {
        type: "openai-chat-completions",
        baseUrl: "https://example.test/v1",
        headers: { "x-hidden": "keep" },
      },
    },
    personalConfig: {
      group: "standard-personal",
      api: { headers: { "x-hidden": "keep" } },
      personalModelIds: ["m1"],
    },
    issues: [],
    models: [],
    ...overrides,
  } as ProviderSettingsProviderView;
}

describe("providerSettings updateProvider overlay", () => {
  it("writes only changed leaves and keeps hidden api fields", () => {
    const update = resolveProviderUpdate(provider(), {
      type: "updateProvider",
      providerId: "p1",
      apiKey: "new",
      baseUrl: " https://other.test/v1/ ",
      apiType: "openai-chat-completions",
    });
    expect(update).toEqual({
      config: {
        group: "standard-personal",
        access: { type: "api-key", apiKey: "new" },
        api: { headers: { "x-hidden": "keep" }, baseUrl: "https://other.test/v1" },
      },
    });
  });

  it("returns nothing when the values did not change", () => {
    expect(
      resolveProviderUpdate(provider(), {
        type: "updateProvider",
        providerId: "p1",
        apiKey: "old",
        baseUrl: "https://example.test/v1/",
        providerName: "Custom",
        enabled: true,
      }),
    ).toBeUndefined();
  });

  it("sends name and enabled as metadata", () => {
    expect(
      resolveProviderUpdate(provider(), {
        type: "updateProvider",
        providerId: "p1",
        providerName: "Renamed",
        enabled: false,
      }),
    ).toEqual({
      config: { group: "standard-personal", api: { headers: { "x-hidden": "keep" } } },
      metadata: { providerName: "Renamed", enabled: false },
    });
  });

  it("keeps preset endpoints read-only but allows the API key", () => {
    const preset = provider({
      effectiveConfig: {
        group: "zai-family",
        access: { type: "zhipu-coding-plan-api-key", apiKey: "" },
        api: { type: "anthropic-messages", baseUrl: "https://preset.test" },
      },
      personalConfig: {},
    });
    expect(() =>
      resolveProviderUpdate(preset, { type: "updateProvider", providerId: "p1", baseUrl: "https://x.test" }),
    ).toThrow("read-only");
    expect(
      resolveProviderUpdate(preset, { type: "updateProvider", providerId: "p1", apiKey: "k" }),
    ).toEqual({ config: { access: { type: "zhipu-coding-plan-api-key", apiKey: "k" } } });
  });

  it("forwards addModel config and recommended mode to the facade with defaults", async () => {
    const view = { revision: 1, providerTemplates: [], providerOrder: [], providers: [] };
    const calls: unknown[][] = [];
    const facade = {
      addPersonalModel: async (...args: unknown[]) => {
        calls.push(args);
        return view;
      },
    };
    const context = { deps: { providerSettings: facade } } as never;
    await mutateProviderSettings(context, {
      operation: { type: "addModel", providerId: "p1", modelId: "m1" },
    });
    const config = { properties: { contextWindow: 1000 } };
    await mutateProviderSettings(context, {
      operation: {
        type: "addModel",
        providerId: "p1",
        modelId: "m2",
        config,
        useRecommendedConfig: false,
      },
    });
    expect(calls).toEqual([
      ["p1", "m1", {}, true],
      ["p1", "m2", config, false],
    ]);
  });

  it("wraps resolveModelConfig, switching to the draft preview when personalConfig is set", async () => {
    const inputs: unknown[] = [];
    const facade = {
      getView: () => ({ providers: [provider()] }),
      resolveModelConfig: (input: unknown) => {
        inputs.push(input);
        return { inheritedConfig: { enabled: true }, effectiveConfig: {}, issues: [] };
      },
    };
    const context = { deps: { providerSettings: facade } } as never;
    const result = await resolveProviderModelConfig(context, { providerId: "p1", modelId: "m" });
    expect(result).toEqual({ inheritedConfig: { enabled: true }, effectiveConfig: {}, issues: [] });
    await resolveProviderModelConfig(context, {
      providerId: "p1",
      modelId: "m",
      personalConfig: { enabled: false },
    });
    expect(inputs).toEqual([
      { providerId: "p1", modelId: "m" },
      { providerId: "p1", originalModelId: "m", modelId: "m", personalConfig: { enabled: false } },
    ]);
    await expect(resolveProviderModelConfig(context, { providerId: "p1" })).rejects.toThrow();
  });

  it("rejects API keys and toggles for account providers", () => {
    const account = provider({
      effectiveConfig: {
        group: "zai-family",
        access: { type: "zhipu-account", accountType: "zai", mode: "start-plan", entitled: true },
      },
    });
    expect(() =>
      resolveProviderUpdate(account, { type: "updateProvider", providerId: "p1", apiKey: "k" }),
    ).toThrow("does not use an API key");
    expect(() =>
      resolveProviderUpdate(account, { type: "updateProvider", providerId: "p1", enabled: false }),
    ).toThrow("cannot be toggled");
  });
});
