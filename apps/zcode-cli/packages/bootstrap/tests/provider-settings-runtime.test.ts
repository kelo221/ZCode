import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { ProviderSettingsFacade, createProviderSettingsMutationTarget } from "@zcode/provider";
import { NodeProviderRegistryRuntime } from "@zcode/provider-node";
import {
  mutateProviderSettings,
  readProviderSettings,
} from "../src/zcode-protocol/provider-settings.js";
import type { ZCodeProtocolAgentServerContext } from "../src/zcode-protocol/server-types.js";

const BUILTIN = resolve(__dirname, "../../../../../config/provider/zcode-builtin.json");

describe("providerSettings protocol over the real provider runtime", () => {
  const cleanups: Array<() => Promise<void> | void> = [];
  afterEach(async () => {
    for (const cleanup of cleanups.splice(0).reverse()) await cleanup();
  });

  async function setup() {
    const dir = await mkdtemp(join(tmpdir(), "provider-settings-"));
    const personalFilePath = join(dir, "provider_config.json");
    const runtime = new NodeProviderRegistryRuntime({
      zcodeBuiltinFilePath: BUILTIN,
      personalFilePath,
      personalPollingIntervalMs: false,
    });
    await runtime.start();
    cleanups.push(() => rm(dir, { recursive: true, force: true }), () => runtime.dispose());
    const providerSettings = new ProviderSettingsFacade(
      runtime.registryService,
      createProviderSettingsMutationTarget({
        configService: runtime.configService,
        refresh: (reason) => runtime.registryService.refresh(reason),
      }),
    );
    const context = { deps: { providerSettings } } as unknown as ZCodeProtocolAgentServerContext;
    return { context, personalFilePath };
  }

  it("creates, edits, adds models to and deletes a custom provider", async () => {
    const { context, personalFilePath } = await setup();
    const initial = await readProviderSettings(context, {});
    expect(initial.view.revision).toBeGreaterThanOrEqual(0);

    const created = await mutateProviderSettings(context, {
      operation: { type: "createProvider", providerName: "My Gateway" },
    });
    const providerId = created.providerId!;
    expect(providerId).toBeTruthy();

    await mutateProviderSettings(context, {
      operation: {
        type: "updateProvider",
        providerId,
        apiType: "openai-chat-completions",
        baseUrl: "https://gateway.test/v1/",
        apiKey: "sk-test",
      },
    });
    const withModel = await mutateProviderSettings(context, {
      operation: { type: "addModel", providerId, modelId: "my-model" },
    });
    const provider = (withModel.view.providers as Array<Record<string, any>>).find(
      (item) => item.providerId === providerId,
    )!;
    expect(provider.effectiveConfig.api).toMatchObject({
      type: "openai-chat-completions",
      baseUrl: "https://gateway.test/v1",
    });
    expect(provider.effectiveConfig.access.apiKey).toBe("sk-test");
    expect(provider.models.map((model: { modelId: string }) => model.modelId)).toContain("my-model");

    const disabled = await mutateProviderSettings(context, {
      operation: { type: "setModelEnabled", providerId, modelId: "my-model", enabled: false },
    });
    const disabledModel = (disabled.view.providers as Array<Record<string, any>>)
      .find((item) => item.providerId === providerId)!
      .models.find((model: { modelId: string }) => model.modelId === "my-model");
    expect(disabledModel.enabled).toBe(false);

    const onDisk = JSON.parse(await readFile(personalFilePath, "utf8"));
    expect(JSON.stringify(onDisk)).toContain("https://gateway.test/v1");

    const deleted = await mutateProviderSettings(context, {
      operation: { type: "deleteProvider", providerId },
    });
    expect(
      (deleted.view.providers as Array<{ providerId: string }>).some(
        (item) => item.providerId === providerId,
      ),
    ).toBe(false);
  });

  it("refuses to delete a preset provider", async () => {
    const { context } = await setup();
    const view = (await readProviderSettings(context, {})).view;
    const preset = (view.providers as Array<Record<string, any>>).find(
      (item) => item.effectiveConfig.group !== "standard-personal",
    );
    if (!preset) return;
    await expect(
      mutateProviderSettings(context, {
        operation: { type: "deleteProvider", providerId: preset.providerId },
      }),
    ).rejects.toThrow("Only custom providers");
  });
});
