import type { ProviderConfigService } from "./config-service.js";
import type { ProviderSettingsMutationTarget } from "./facades.js";
import type { ProviderRegistryServiceSnapshot } from "./registry-service.js";

export interface ProviderSettingsMutationTargetOptions {
  readonly configService: ProviderConfigService;
  readonly refresh: (reason: string) => Promise<ProviderRegistryServiceSnapshot>;
  /** 主动重读上游事实（远端 Built-in、Account）后再刷新；缺省时 refresh 即可。 */
  readonly refreshSources?: (reason: string) => Promise<ProviderRegistryServiceSnapshot>;
}

/**
 * Settings Facade 的写入端口：Desktop Services 与 CLI app-server 共用同一份转发，
 * 避免两处装配各自截断写入参数（曾丢失 addPersonalModel 的配置模式）。
 */
export function createProviderSettingsMutationTarget(
  options: ProviderSettingsMutationTargetOptions,
): ProviderSettingsMutationTarget {
  const configService = options.configService;
  return {
    createPersonalProvider: (input) => configService.createPersonalProvider(input),
    savePersonalProviderOverlay: (providerId, config, membership, metadata) =>
      configService.savePersonalProviderOverlay(providerId, config, membership, metadata),
    deletePersonalProvider: (providerId) => configService.deletePersonalProvider(providerId),
    reorderPersonalProviders: (providerIds) => configService.reorderPersonalProviders(providerIds),
    reorderPersonalModels: (providerId, modelIds, membership) =>
      configService.reorderPersonalModels(providerId, modelIds, membership),
    addPersonalModel: configService.addPersonalModel.bind(configService),
    renamePersonalModel: (providerId, currentModelId, nextModelId, membership) =>
      configService.renamePersonalModel(providerId, currentModelId, nextModelId, membership),
    deletePersonalModel: (providerId, modelId, membership) =>
      configService.deletePersonalModel(providerId, modelId, membership),
    setPersonalModelEnabled: (providerId, modelId, enabled, membership) =>
      configService.setPersonalModelEnabled(providerId, modelId, enabled, membership),
    savePersonalModelDraft: (
      providerId,
      originalModelId,
      nextModelId,
      config,
      expectedPersonalRevision,
      useRecommendedConfig,
      membership,
    ) =>
      configService.savePersonalModelDraft(
        providerId,
        originalModelId,
        nextModelId,
        config,
        expectedPersonalRevision,
        useRecommendedConfig,
        membership,
      ),
    refresh: options.refresh,
    ...(options.refreshSources ? { refreshSources: options.refreshSources } : {}),
  };
}
