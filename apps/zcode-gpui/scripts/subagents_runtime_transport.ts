/** Test-only public stdio transports and bounded owned-process cleanup. */
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { randomUUID } from "node:crypto";
import { setTimeout as sleep } from "node:timers/promises";
import {
  CancellationTokenSource,
  ChannelClient,
  Emitter,
  SocketProtocol,
  VSBuffer,
} from "@zcode/rpc";
import {
  helloMessageSchema,
  zcodeSessionRequestRuntimePreferencesParamsSchema,
  zcodeSessionRuntimePreferencesResultSchema,
} from "@zcode/shared";
import {
  commandAckSchema,
  commandEnvelopeSchema,
  conversationTopicWireFrameSchema,
  type CommandEnvelope,
} from "@zcode/shared/zcode-protocol-v4";

const requestTimeout = 20000;
export const children: ChildProcessWithoutNullStreams[] = [];
let failure: Error | undefined;
export function assert(value: unknown, label: string): asserts value {
  if (!value) throw new Error(label);
}
export function fail(label: string) {
  failure ??= new Error(label);
}
export function alive(child: ChildProcessWithoutNullStreams) {
  return child.exitCode === null && child.signalCode === null;
}
export async function bounded<T>(
  promise: Promise<T>,
  label: string,
  ms = requestTimeout,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      promise,
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error(`${label} deadline`)), ms);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}
export async function waitFor(predicate: () => boolean, label: string) {
  const end = Date.now() + requestTimeout;
  while (!predicate()) {
    if (failure) throw failure;
    assert(Date.now() < end, `${label} deadline`);
    await sleep(20);
  }
  if (failure) throw failure;
}
export function launch(executable: string, args: string[], env: NodeJS.ProcessEnv, cwd: string) {
  const child = spawn(executable, args, {
    env,
    cwd,
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true,
  });
  children.push(child);
  // Diagnostics may contain prompts or credentials; drain without retaining or printing.
  child.stderr.on("data", () => {});
  child.once("error", () => fail("Child process launch failed"));
  child.stdin.on("error", () => fail("Child stdin failed"));
  child.once("exit", () => fail("Owned child exited"));
  return child;
}
export async function stop(child: ChildProcessWithoutNullStreams) {
  if (!alive(child)) return;
  const exited = new Promise<void>((ok) => child.once("exit", () => ok()));
  child.stdin.end();
  await Promise.race([exited, sleep(2000)]);
  if (!alive(child)) return;
  if (process.platform === "win32" && child.pid) {
    const killer = spawn("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"], {
      stdio: "ignore",
      windowsHide: true,
    });
    await Promise.race([
      new Promise<void>((ok) => {
        killer.once("exit", () => ok());
        killer.once("error", () => ok());
      }),
      sleep(2000).then(() => {
        killer.kill();
      }),
    ]);
  }
  if (alive(child)) child.kill("SIGKILL");
  await bounded(exited, "Owned process cleanup", 3000);
}
export function record(value: unknown): Record<string, unknown> | undefined {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}
export async function connectHost(host: ChildProcessWithoutNullStreams) {
  const dataEvent = new Emitter<VSBuffer>(),
    closeEvent = new Emitter<void>();
  let remainder = Buffer.alloc(0);
  const hello = await bounded(
    new Promise<ReturnType<typeof helloMessageSchema.parse>>((ok, reject) => {
      let bytes = Buffer.alloc(0);
      const onData = (part: Buffer) => {
        bytes = Buffer.concat([bytes, part]);
        if (bytes.length > 65536) {
          host.stdout.off("data", onData);
          reject(new Error("Host hello bound"));
          return;
        }
        const newline = bytes.indexOf(10);
        if (newline < 0) return;
        host.stdout.off("data", onData);
        remainder = bytes.subarray(newline + 1);
        try {
          ok(helloMessageSchema.parse(JSON.parse(bytes.subarray(0, newline).toString())));
        } catch {
          reject(new Error("Invalid Host hello"));
        }
      };
      host.stdout.on("data", onData);
      host.once("exit", () => reject(new Error("Host exited before hello")));
      host.once("error", () => reject(new Error("Host failed before hello")));
    }),
    "Host hello",
    15000,
  );
  const protocol = new SocketProtocol({
    onData: dataEvent.event,
    onClose: closeEvent.event,
    onEnd: closeEvent.event,
    write: (buffer) => {
      host.stdin.write(Buffer.from(buffer.buffer));
    },
    end: () => {
      host.stdin.end();
    },
    drain: async () => {},
    dispose: () => {},
  });
  const client = new ChannelClient(protocol);
  let hostBytes = 0;
  const receive = (bytes: Buffer) => {
    hostBytes += bytes.length;
    if (hostBytes > 16 * 1024 * 1024) {
      fail("Host traffic bound");
      client.dispose();
      return;
    }
    try {
      dataEvent.fire(VSBuffer.wrap(bytes));
    } catch {
      fail("Host frame rejected");
      client.dispose();
    }
  };
  host.stdout.on("data", receive);
  host.once("exit", () => {
    closeEvent.fire();
    client.dispose();
  });
  if (remainder.length) receive(remainder);
  host.stdin.write(
    JSON.stringify({
      type: "zcode-hello-ack",
      clientId: "gpui-runtime-smoke",
      version: hello.version,
    }) + "\n",
  );
  return {
    async call<T>(channel: string, method: string, args: unknown[]): Promise<T> {
      const cancellation = new CancellationTokenSource();
      try {
        return await bounded(
          client.getChannel(channel).call<T>(method, args, cancellation.token),
          method,
        );
      } finally {
        cancellation.cancel();
        cancellation.dispose();
      }
    },
    dispose() {
      host.stdout.off("data", receive);
      client.dispose();
      protocol.dispose();
      dataEvent.dispose();
      closeEvent.dispose();
    },
  };
}
export function connectCli(cli: ChildProcessWithoutNullStreams) {
  let nextId = 0,
    ready = false,
    buffer = "",
    cliBytes = 0;
  const phases = new Map<string, string>();
  const preferences = new Set<string>();
  const pending = new Map<number, { resolve(value: unknown): void; reject(error: Error): void }>();
  cli.once("exit", () => {
    for (const p of pending.values()) p.reject(new Error("CLI exited"));
    pending.clear();
  });
  cli.stdout.on("data", (part: Buffer) => {
    cliBytes += part.length;
    if (cliBytes > 16 * 1024 * 1024) {
      fail("CLI traffic bound");
      return;
    }
    buffer += part.toString();
    if (buffer.length > 2 * 1024 * 1024) {
      fail("CLI line bound");
      return;
    }
    let newline: number;
    while ((newline = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, newline);
      buffer = buffer.slice(newline + 1);
      try {
        const message = record(JSON.parse(line));
        assert(message, "Invalid CLI message");
        if (message.method && message.id !== undefined) {
          const result =
            message.method === "session/requestRuntimePreferences"
              ? zcodeSessionRuntimePreferencesResultSchema.parse({
                  askUserQuestionAutoResolutionEnabled: true,
                  nativeSearchEnhancementsEnabled: true,
                  memoryEnabled: true,
                })
              : undefined;
          if (result)
            preferences.add(
              zcodeSessionRequestRuntimePreferencesParamsSchema.parse(message.params).scope,
            );
          cli.stdin.write(
            JSON.stringify({
              id: message.id,
              ...(result
                ? { result }
                : { error: { code: -32603, message: "Unavailable in isolated acceptance" } }),
            }) + "\n",
          );
        } else if (typeof message.id === "number") {
          const p = pending.get(message.id);
          pending.delete(message.id);
          if (message.error) p?.reject(new Error("CLI RPC rejected"));
          else p?.resolve(message.result);
        } else if (
          message.method === "startup/storageState" &&
          record(message.params)?.phase === "ready"
        )
          ready = true;
        else if (message.method === "v4/conversation/frame") {
          const wire = conversationTopicWireFrameSchema.parse(message.params);
          assert(wire.kind === "complete", "Unexpected fragmented acceptance frame");
          const payload = wire.frame.payload;
          if (payload.kind === "snapshot")
            phases.set(wire.subscriptionId, payload.snapshot.control.phase);
          else
            for (const delta of payload.deltas)
              if (delta.op === "state.updated" && delta.patch.control)
                phases.set(wire.subscriptionId, delta.patch.control.phase);
        }
      } catch {
        fail("CLI public message validation failed");
      }
    }
  });
  async function rpc(method: string, params: unknown): Promise<unknown> {
    if (failure) throw failure;
    const id = ++nextId;
    assert(id <= 24, "CLI request count bound");
    try {
      return await bounded(
        new Promise((ok, reject) => {
          pending.set(id, { resolve: ok, reject });
          cli.stdin.write(JSON.stringify({ id, method, params }) + "\n");
        }),
        method,
      );
    } finally {
      pending.delete(id);
    }
  }
  return {
    phases,
    preferences,
    rpc,
    waitReady: () => waitFor(() => ready, "CLI storage startup"),
    async command(type: CommandEnvelope["type"], sessionId: string | null, payload: unknown) {
      return commandAckSchema.parse(
        await rpc(
          "v4/command",
          commandEnvelopeSchema.parse({
            commandId: randomUUID(),
            clientId: "gpui-runtime-smoke",
            sessionId,
            type,
            issuedAt: Date.now(),
            payload,
          }),
        ),
      );
    },
  };
}
