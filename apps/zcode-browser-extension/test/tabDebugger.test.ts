import { describe, expect, it } from "vitest";
import { TabDebugger } from "../src/tabDebugger.js";
import { createFakeChrome } from "./fakeChrome.js";

function setup() {
  const fake = createFakeChrome();
  const debuggee = new TabDebugger(fake.chrome, 5);
  fake.debuggerEvents.addListener((source, method, params) =>
    debuggee.handleEvent(source, method, params),
  );
  return { fake, debuggee };
}

const iframeTarget = (targetId: string, sessionId: string) => ({
  sessionId,
  targetInfo: { targetId, type: "iframe", url: "https://frame.example/" },
  waitingForDebugger: false,
});

describe("TabDebugger", () => {
  it("attaches once and auto-attaches child frames", async () => {
    const { fake, debuggee } = setup();
    await Promise.all([debuggee.ensureAttached(), debuggee.ensureAttached()]);
    expect(fake.commands.map((c) => c.method)).toEqual([
      "Page.enable",
      "Emulation.setFocusEmulationEnabled",
      "Target.setAutoAttach",
    ]);
    expect(fake.commands[2]?.params).toMatchObject({ autoAttach: true, flatten: true });
  });

  it("answers Target.attachToTarget from auto-attached sessions", async () => {
    const { fake, debuggee } = setup();
    await debuggee.ensureAttached();
    fake.debuggerEvents.emit({ tabId: 5 }, "Target.attachedToTarget", iframeTarget("F1", "S1"));
    expect(await debuggee.send("Target.attachToTarget", { targetId: "F1", flatten: true })).toEqual(
      {
        sessionId: "S1",
      },
    );
    expect(await debuggee.send("Target.getTargets")).toEqual({
      targetInfos: [{ targetId: "F1", type: "iframe", url: "https://frame.example/" }],
    });
    expect(await debuggee.send("Target.detachFromTarget", { sessionId: "S1" })).toEqual({});
    // Nested frames attach through the child session.
    expect(fake.commands.at(-1)).toMatchObject({
      target: { tabId: 5, sessionId: "S1" },
      method: "Target.setAutoAttach",
    });
    await debuggee.send("Runtime.evaluate", { expression: "1" }, "S1");
    expect(fake.commands.at(-1)).toMatchObject({
      target: { tabId: 5, sessionId: "S1" },
      method: "Runtime.evaluate",
    });

    fake.debuggerEvents.emit({ tabId: 5 }, "Target.detachedFromTarget", { sessionId: "S1" });
    await expect(debuggee.send("Target.attachToTarget", { targetId: "F1" })).rejects.toThrow(
      "No target",
    );
  });

  it("a child that registers shortly after the request is still found", async () => {
    const { fake, debuggee } = setup();
    await debuggee.ensureAttached();
    const pending = debuggee.send("Target.attachToTarget", { targetId: "late" });
    setTimeout(
      () =>
        fake.debuggerEvents.emit(
          { tabId: 5 },
          "Target.attachedToTarget",
          iframeTarget("late", "S9"),
        ),
      50,
    );
    expect(await pending).toEqual({ sessionId: "S9" });
  });

  it("re-attaches after the browser dropped the debugger", async () => {
    const { fake, debuggee } = setup();
    await debuggee.ensureAttached();
    debuggee.handleDetach();
    expect(debuggee.isAttached).toBe(false);
    await debuggee.send("Page.reload");
    expect(fake.commands.filter((c) => c.method === "Target.setAutoAttach")).toHaveLength(2);
  });
});
