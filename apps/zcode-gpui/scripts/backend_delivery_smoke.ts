import { randomUUID } from "node:crypto";
import { setTimeout as sleep } from "node:timers/promises";
import {
  commandAckSchema,
  commandEnvelopeSchema,
  conversationSnapshotSchema,
  conversationTopicWireFrameSchema,
  v4ConversationSubscribeResultSchema,
  type CommandEnvelope,
  type ConversationSnapshot,
} from "@zcode/shared/zcode-protocol-v4";

type Ports = {
  rpc(method: string, params: unknown): Promise<unknown>;
  messages: { method?: string; params?: { subscriptionId?: string } }[];
  holdNext(): { arrived: Promise<void>; release(): void };
};

export async function backendDeliverySmoke(
  workspace: string,
  modelSelection: unknown,
  ports: Ports,
) {
  const { rpc, messages } = ports;
  let sid: string | null = null;
  async function command(type: CommandEnvelope["type"], payload: unknown, baseRevision?: number) {
    const envelope = commandEnvelopeSchema.parse({
      commandId: randomUUID(),
      clientId: "gpui-delivery-smoke",
      sessionId: sid,
      type,
      payload,
      issuedAt: Date.now(),
      ...(baseRevision === undefined ? {} : { baseRevision }),
    });
    return commandAckSchema.parse(await rpc("v4/command", envelope));
  }
  const created = await command("createSession", {
    workspaceId: workspace,
    config: { modelSelection, mode: "yolo" },
  });
  if (created.status !== "accepted" || created.result?.type !== "createSession")
    throw new Error("Delivery fixture session creation failed");
  sid = created.result.sessionId;
  const topic = `conversation/${sid}`;
  const connectionId = "smoke-held";
  const sub = v4ConversationSubscribeResultSchema.parse(
    await rpc("v4/conversation/subscribe", {
      topic,
      connectionId,
      clientMode: "desktop-continuous",
      workspace: { workspacePath: workspace, workspaceKey: workspace },
      visibility: "foreground",
    }),
  );
  async function snapshot(): Promise<ConversationSnapshot> {
    const start = messages.length;
    await rpc("v4/conversation/resync", {
      topic,
      subscriptionId: sub.ack.subscriptionId,
      connectionId,
      base: null,
      forceSnapshot: true,
    });
    const deadline = Date.now() + 30000;
    while (Date.now() < deadline) {
      for (const message of messages.slice(start)) {
        if (
          message.method !== "v4/conversation/frame" ||
          message.params?.subscriptionId !== sub.ack.subscriptionId
        )
          continue;
        const wire = conversationTopicWireFrameSchema.parse(message.params);
        if (wire.kind === "complete" && wire.frame.payload.kind === "snapshot")
          return conversationSnapshotSchema.parse(wire.frame.payload.snapshot);
      }
      await sleep(20);
    }
    throw new Error("Delivery snapshot observation timed out");
  }
  async function waitSnapshot(predicate: (s: ConversationSnapshot) => boolean) {
    const deadline = Date.now() + 30000;
    while (Date.now() < deadline) {
      const current = await snapshot();
      if (predicate(current)) return current;
      await sleep(20);
    }
    throw new Error("Delivery state observation timed out");
  }
  const hold = ports.holdNext();
  try {
    const started = await command("sendText", {
      text: "held queue setup",
      requestedDelivery: "startNow",
      modelSelection,
    });
    if (started.status !== "accepted") throw new Error("Held fixture did not start");
    await Promise.race([
      hold.arrived,
      sleep(30000).then(() => {
        throw new Error("Held mock request did not arrive");
      }),
    ]);
    const running = await snapshot();
    const pause = await command("setAutoDrain", { autoDrain: false }, running.revision);
    if (pause.status !== "accepted") throw new Error("Held fixture queue pause failed");
    const queued = await command("sendText", {
      text: "disposable held input",
      requestedDelivery: "queue",
      modelSelection,
      mode: "edit",
      planEnabled: true,
    });
    if (
      queued.status !== "accepted" ||
      queued.result?.type !== "inputAccepted" ||
      queued.result.delivery !== "queue"
    )
      throw new Error("Busy queue admission mismatch");
    hold.release();
    const held = await waitSnapshot(
      (s) => s.inputRouting.mode === "choice" && s.queue.items.length === 1,
    );
    if (held.queue.items[0]?.mode !== "edit" || held.queue.items[0]?.planEnabled !== true)
      throw new Error("Plan submission was not frozen in queued intent");
    const ids = held.queue.items.map((item) => item.queueItemId);
    const missing = await command("sendText", { text: "requires decision", modelSelection });
    if (missing.status !== "failed" || missing.reasonCode !== "heldQueueDispositionRequired")
      throw new Error("Missing held disposition was not refused");
    const stale = await command("sendText", {
      text: "stale decision",
      heldQueueDisposition: "clearQueueAndSend",
      expectedHeldQueueItemIds: ["not-reviewed"],
      modelSelection,
    });
    if (stale.status !== "failed" || stale.reasonCode !== "guard.heldQueueConfirmationStale")
      throw new Error("Stale held disposition was not refused");
    const retained = await snapshot();
    if (retained.queue.items.length !== 1 || retained.queue.items[0]?.queueItemId !== ids[0])
      throw new Error("Refused Clear changed held queue");
    const keep = await command("sendText", {
      text: "keep reviewed queue",
      heldQueueDisposition: "keepQueueAndSend",
      expectedHeldQueueItemIds: ids,
      modelSelection,
    });
    if (keep.status !== "accepted" || keep.result?.type !== "inputAccepted")
      throw new Error("Keep decision was not admitted");
    const kept = await waitSnapshot(
      (s) =>
        s.inputRouting.mode === "choice" &&
        s.queue.items.length === 1 &&
        JSON.stringify(s.rows).includes("keep reviewed queue"),
    );
    if (kept.queue.items[0]?.queueItemId !== ids[0])
      throw new Error("Keep changed held queue identity");
    const clear = await command("sendText", {
      text: "clear reviewed queue",
      heldQueueDisposition: "clearQueueAndSend",
      expectedHeldQueueItemIds: ids,
      modelSelection,
    });
    if (clear.status !== "accepted" || clear.result?.type !== "inputAccepted")
      throw new Error("Clear decision was not admitted");
    await waitSnapshot(
      (s) =>
        s.control.phase === "completedSuccess" &&
        s.queue.items.length === 0 &&
        JSON.stringify(s.rows).includes("clear reviewed queue"),
    );
    return {
      busyQueueAdmission: true,
      heldQueueDisposition: true,
      heldQueueStaleRefusal: true,
      planQueuedIntent: true,
    };
  } finally {
    hold.release();
  }
}
