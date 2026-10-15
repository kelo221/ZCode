import { afterEach, describe, expect, it, vi } from "vitest";
import { BridgeConnection, HOST_NAME } from "../src/bridgeConnection.js";
import type { ExecutionContext } from "../src/commandRouter.js";
import { createFakeChrome } from "./fakeChrome.js";

const identity = async () => ({
  profileId: "p-1",
  browser: "Helium",
  profile: "Default",
  extensionVersion: "0.1.0",
});

afterEach(() => {
  vi.useRealTimers();
});

describe("BridgeConnection", () => {
  it("says hello, takes the welcome and runs execute messages", async () => {
    const fake = createFakeChrome();
    const connectNative = vi.spyOn(fake.chrome.runtime, "connectNative");
    const seen: ExecutionContext[] = [];
    const connection = new BridgeConnection(fake.chrome, identity, async (context) => {
      seen.push(context);
      return { ok: true, elapsedMs: 1 };
    });
    await connection.connect();
    await connection.connect();
    expect(connectNative).toHaveBeenCalledTimes(1);
    expect(connectNative).toHaveBeenCalledWith(HOST_NAME);
    const port = fake.ports[0]!;
    expect(port.sent[0]).toEqual({ op: "hello", ...(await identity()) });

    port.onMessage.emit({ op: "welcome", browserId: "extension:p-1", generation: 42 });
    expect(connection.status()).toMatchObject({ connected: true, generation: 42 });

    port.onMessage.emit({
      op: "execute",
      requestId: "r1",
      command: { method: "getState" },
      context: { sessionId: "s1", turnId: "t1", workspaceKey: "ws" },
    });
    await vi.waitFor(() => expect(port.sent).toHaveLength(2));
    expect(port.sent[1]).toEqual({
      op: "result",
      requestId: "r1",
      result: { ok: true, elapsedMs: 1 },
    });
    expect(seen[0]).toEqual({
      requestId: "r1",
      turnId: "t1",
      sessionId: "s1",
      workspaceKey: "ws",
      remoteSessionId: "",
      browserId: "extension:p-1",
      generation: 42,
    });
  });

  it("thrown errors become structured results", async () => {
    const fake = createFakeChrome();
    const connection = new BridgeConnection(fake.chrome, identity, async () => {
      throw new Error("boom");
    });
    await connection.connect();
    const port = fake.ports[0]!;
    port.onMessage.emit({ op: "welcome", browserId: "b", generation: 1 });
    port.onMessage.emit({
      op: "execute",
      requestId: "r",
      command: { method: "list" },
      context: {},
    });
    await vi.waitFor(() => expect(port.sent).toHaveLength(2));
    expect((port.sent[1] as { result: { error: unknown } }).result.error).toMatchObject({
      code: "execution_error",
      message: "boom",
      sideEffect: "uncertain",
    });
  });

  it("reconnects with backoff after GPUI is unavailable", async () => {
    vi.useFakeTimers();
    const fake = createFakeChrome();
    const statuses: boolean[] = [];
    const connection = new BridgeConnection(
      fake.chrome,
      identity,
      async () => ({ ok: true, elapsedMs: 0 }),
      (status) => statuses.push(status.connected),
    );
    await connection.connect();
    const first = fake.ports[0]!;
    first.onMessage.emit({ op: "unavailable", reason: "gpui_not_running" });
    first.onDisconnect.emit();
    expect(connection.status()).toMatchObject({ connected: false, lastError: "gpui_not_running" });
    await vi.advanceTimersByTimeAsync(1_999);
    expect(fake.ports).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(fake.ports).toHaveLength(2);
    // A stale port's late disconnect does not drop the new one.
    first.onDisconnect.emit();
    expect(fake.ports).toHaveLength(2);
    fake.ports[1]!.onMessage.emit({ op: "welcome", browserId: "b", generation: 2 });
    expect(connection.status().connected).toBe(true);
    expect(statuses).toEqual([false, true]);
  });
});
