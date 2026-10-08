import { spawn } from "node:child_process";
import { mkdtemp, mkdir, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { ChannelClient, Emitter, SocketProtocol, VSBuffer } from "@zcode/rpc";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { setTimeout as sleep } from "node:timers/promises";

async function processCost(pid: number) {
  if (process.platform !== "win32") return undefined;
  const command = `$p=Get-Process -Id ${pid} -ErrorAction Stop; $children=@(Get-CimInstance Win32_Process | Where-Object {$_.ParentProcessId -eq ${pid}}); @{rssMiB=[Math]::Round($p.WorkingSet64/1MB,2);privateMiB=[Math]::Round($p.PrivateMemorySize64/1MB,2);childCount=$children.Count} | ConvertTo-Json -Compress`;
  const { stdout } = await promisify(execFile)("powershell.exe", [
    "-NoProfile",
    "-Command",
    command,
  ]);
  return JSON.parse(stdout) as { rssMiB: number; privateMiB: number; childCount: number };
}

const root = await mkdtemp(join(tmpdir(), "gpui-services-smoke-"));
const home = join(root, "home"),
  data = join(root, "data"),
  workspace = join(root, "workspace");
for (const path of [home, data, workspace, join(root, "temp")]) await mkdir(path);
const runtime = process.env.ZCODE_GPUI_SERVICES_RUNTIME;
if (!runtime)
  throw new Error("Provide an installed compatible runtime; no installation is performed");
const env: Record<string, string> = {};
for (const key of ["PATH", "SystemRoot", "SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT"])
  if (process.env[key]) env[key] = process.env[key]!;
Object.assign(env, {
  HOME: home,
  USERPROFILE: home,
  ZCODE_DESKTOP_HOME_DIR: home,
  ZCODE_DATA_BASE_DIR: data,
  APPDATA: join(home, "AppData/Roaming"),
  LOCALAPPDATA: join(home, "AppData/Local"),
  TEMP: join(root, "temp"),
  TMP: join(root, "temp"),
  ELECTRON_RUN_AS_NODE: "1",
  ZCODE_SERVICE_AUTHORITY_MODE: "desktop-attached-remote",
  ZCODE_RUNTIME_ENV: "desktop",
});
const child = spawn(
  runtime,
  ["--no-warnings", resolve("packages/server/dist/remote/zcode-server.cjs")],
  {
    cwd: workspace,
    env,
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true,
  },
);
const dataEvent = new Emitter<VSBuffer>(),
  close = new Emitter<void>();
let diagnostics = "";
child.stderr.on("data", (bytes) => {
  diagnostics = (diagnostics + bytes.toString()).slice(-8192);
});
child.once("exit", () => close.fire());
const start = performance.now();
const hello = await new Promise<{ version: string }>((ok, fail) => {
  let bytes = Buffer.alloc(0);
  const timer = setTimeout(() => fail(new Error("Host hello timed out")), 15000);
  const listener = (part: Buffer) => {
    bytes = Buffer.concat([bytes, part]);
    if (bytes.length > 65536) {
      clearTimeout(timer);
      fail(new Error("Host hello exceeded bound"));
      return;
    }
    const newline = bytes.indexOf(10);
    if (newline < 0) return;
    child.stdout.off("data", listener);
    clearTimeout(timer);
    try {
      ok(JSON.parse(bytes.subarray(0, newline).toString()));
    } catch {
      fail(new Error("Invalid Host hello"));
    }
  };
  child.stdout.on("data", listener);
  child.once("exit", () => {
    clearTimeout(timer);
    fail(new Error("Host exited before hello"));
  });
});
const protocol = new SocketProtocol({
  onData: dataEvent.event,
  onClose: close.event,
  onEnd: close.event,
  write: (buffer) => {
    child.stdin.write(Buffer.from(buffer.buffer));
  },
  end: () => {
    child.stdin.end();
  },
  drain: async () => {},
  dispose: () => {},
});
const client = new ChannelClient(protocol);
child.stdout.on("data", (bytes) => dataEvent.fire(VSBuffer.wrap(bytes)));
child.stdin.write(
  JSON.stringify({
    type: "zcode-hello-ack",
    clientId: "gpui-isolated-smoke",
    version: hello.version,
  }) + "\n",
);
const timeout = setTimeout(() => {
  child.kill();
}, 45000);
try {
  const subagents = client.getChannel("subagents");
  const listed = (await subagents.call("list", [
    { workspacePath: workspace, mode: "settingsUserOnly" },
  ])) as { agents: { name: string }[] };
  const models = await client.getChannel("model-selection").call("getView", []);
  const startupMs = Math.round(performance.now() - start);
  const activeCost = await processCost(child.pid!);
  await sleep(2000);
  const idleCost = await processCost(child.pid!);
  await subagents.call("createAgent", [
    {
      provider: "glm",
      scope: "user",
      config: {
        name: "scratch-profile",
        description: "Synthetic profile",
        systemPrompt: "Reply with a disposable sentinel.",
        tools: ["Read"],
      },
    },
  ]);
  const file = join(home, ".zcode", "agents", "scratch-profile.md");
  const saved = await readFile(file, "utf8");
  if (!saved.includes("Synthetic profile")) throw new Error("Canonical profile did not persist");
  console.log(
    JSON.stringify({
      root,
      pid: child.pid,
      startupMs,
      activeCost,
      idleCost,
      builtinNames: listed.agents.map((a) => a.name),
      hasModelView: Boolean(models),
      canonicalProfileSaved: true,
    }),
  );
} catch (error) {
  console.error(
    "Isolated Host smoke failed:",
    error instanceof Error ? error.message : "RPC failure",
  );
  console.error(
    "Host diagnostic error count:",
    diagnostics.split("\n").filter((s) => s.includes("Error") || s.includes("failed")).length,
  );
  process.exitCode = 1;
} finally {
  clearTimeout(timeout);
  client.dispose();
  child.stdin.end();
  await Promise.race([
    new Promise<void>((ok) => child.once("exit", () => ok())),
    new Promise<void>((ok) =>
      setTimeout(() => {
        child.kill();
        ok();
      }, 3000),
    ),
  ]);
}
