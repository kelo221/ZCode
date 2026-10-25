import { describe, expect, it } from "vitest";
import { SessionEventType, createSessionEvent, createSessionId } from "@zcode/contracts";
import {
  buildSubagentContextReport,
  extractSubagentRemainingItems,
} from "../src/subagent/completion-notification.js";

const sessionId = createSessionId("subagent-completion-report");

function modelComplete(payload: Record<string, unknown>) {
  return createSessionEvent(SessionEventType.ModelComplete, sessionId, payload);
}

describe("subagent completion report", () => {
  it("strips the remaining_items block and returns its items", () => {
    expect(
      extractSubagentRemainingItems(
        "Two items done.\n\n<remaining_items>\n- Enemy effects\n2. Master bugs\n\n</remaining_items>",
      ),
    ).toEqual({
      incomplete: { remainingItems: ["Enemy effects", "Master bugs"] },
      text: "Two items done.",
    });
  });

  it("accepts an unclosed block at the end and ignores an empty one", () => {
    expect(extractSubagentRemainingItems("Done.\n<remaining_items>\n* Item three")).toEqual({
      incomplete: { remainingItems: ["Item three"] },
      text: "Done.",
    });
    expect(
      extractSubagentRemainingItems("All done.\n<remaining_items>\n</remaining_items>"),
    ).toEqual({ text: "All done." });
    expect(extractSubagentRemainingItems("All done.")).toEqual({ text: "All done." });
  });

  it("reports peak context, compactions and stop reason, ignoring compact and nested usage", () => {
    const report = buildSubagentContextReport({
      contextWindow: 100_000,
      cumulativeBilledTokens: 846_000,
      events: [
        modelComplete({
          querySource: "subagent",
          stopReason: "tool-calls",
          usage: { inputTokens: 64_000, outputTokens: 1_000, totalTokens: 65_000 },
        }),
        modelComplete({
          querySource: "compact",
          stopReason: "stop",
          usage: { inputTokens: 66_000, outputTokens: 15_000, totalTokens: 81_000 },
        }),
        createSessionEvent(SessionEventType.CompactBoundary, sessionId, {}),
        modelComplete({
          stopReason: "tool_internal",
          usage: { inputTokens: 90_000, outputTokens: 1, totalTokens: 90_001 },
        }),
        modelComplete({
          querySource: "subagent",
          stopReason: "stop",
          usage: { inputTokens: 26_000, outputTokens: 500, totalTokens: 26_500 },
        }),
        createSessionEvent(SessionEventType.CompactBoundary, sessionId, {}),
      ],
    });

    expect(report).toEqual({
      compactionCount: 2,
      contextWindow: 100_000,
      cumulativeBilledTokens: 846_000,
      peakContextTokens: 65_000,
      stopReason: "stop",
    });
  });
});
