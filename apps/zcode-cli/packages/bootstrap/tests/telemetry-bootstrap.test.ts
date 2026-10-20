import { access, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  resetCapturedZCodeAgentTelemetryEnvForTest,
  sanitizeZCodeRuntimeEnvInPlace,
} from "@zcode/shared";
import { afterEach, describe, expect, it } from "vitest";
import { prepareZCodeTelemetryEnv } from "../src/telemetry-bootstrap.js";

describe("prepareZCodeTelemetryEnv", () => {
  afterEach(() => {
    resetCapturedZCodeAgentTelemetryEnvForTest();
  });

  it("Fork：即使配置了 OTLP 端点也保持关闭，不生成 identity，也不回写凭据", async () => {
    const root = await mkdtemp(join(tmpdir(), "zcode-bootstrap-telemetry-"));
    try {
      const env: NodeJS.ProcessEnv = {
        OTEL_EXPORTER_OTLP_HEADERS: "x-arms-license-key=secret",
        OTEL_EXPORTER_OTLP_TRACES_ENDPOINT: "https://arms.example/v1/traces",
        ZCODE_HOME: root,
        ZCODE_MODEL_TELEMETRY_ENABLED: "1",
      };
      sanitizeZCodeRuntimeEnvInPlace(env);

      const prepared = await prepareZCodeTelemetryEnv(env);

      expect(prepared.OTEL_EXPORTER_OTLP_HEADERS).toBeUndefined();
      expect(prepared.OTEL_EXPORTER_OTLP_TRACES_ENDPOINT).toBeUndefined();
      expect(prepared.ZCODE_TELEMETRY_DEVICE_MID).toBeUndefined();
      await expect(access(join(root, "v2", "telemetry-state.json"))).rejects.toThrow();
    } finally {
      await rm(root, { force: true, recursive: true });
    }
  });
});
