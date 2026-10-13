"""Scripted chat for main-window captures: loopback mock provider plus sample workspace.

Every writer requires the stopped, validated scratch root (same lock as Settings fixtures);
the mock listens on 127.0.0.1 only and the provider key is a placeholder, not a credential.
"""
import json
from pathlib import Path
import shutil
import subprocess
import time

import psutil
from native_settings_fixtures import TICKET, ScratchLock, capability, write
from native_settings_fixtures import provider_fixture

SCRIPTS = Path(__file__).resolve().parent
PROMPT = "Give me an overview of this project"
MODEL = {"providerId": "native-acceptance", "modelId": "native-metadata", "options": {"reasoningLevel": "low"}}


def start_mock(evidence, workspace):
    bun = shutil.which("bun") or str(Path.home() / ".bun/bin/bun.exe")
    state_file = evidence / "chat-mock.json"
    log = (evidence / "chat-mock.log").open("wb")
    child = subprocess.Popen([bun, str(SCRIPTS / "visual_chat_mock.ts"), "--state", str(state_file),
                              "--workspace", str(workspace)],
                             cwd=SCRIPTS, stdout=log, stderr=subprocess.STDOUT)
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        if state_file.is_file():
            data = json.loads(state_file.read_text(encoding="utf-8"))
            if data.get("port"):
                process = psutil.Process(child.pid)
                return {"pid": child.pid, "created": process.create_time(), "port": data["port"],
                        "state": str(state_file)}
        if child.poll() is not None:
            break
        time.sleep(0.1)
    child.kill()
    raise RuntimeError(f"Chat mock did not start; see {evidence / 'chat-mock.log'}")


def stop_mock(state):
    mock = state.get("mock")
    if not mock:
        return
    try:
        process = psutil.Process(mock["pid"])
        if process.create_time() == mock["created"]:
            process.terminate()
            process.wait(5)
    except psutil.NoSuchProcess:
        pass
    state["mock"] = None


def seed(root, port):
    with ScratchLock(root):
        capability(root, root / TICKET)
        config = provider_fixture()
        config["config"]["defaultModelSelection"] = MODEL
        config["config"]["providerConfigRules"]["providerRules"][0]["config"]["api"]["baseUrl"] = f"http://127.0.0.1:{port}/v1"
        write(root, "data/.zcode/v2/provider_config.json", config)
        write(root, "workspace/README.md", "# Sample service\n\nRun `bun run dev` to start.\n")
        write(root, "workspace/src/app.ts", 'import { serve } from "bun";\n\nserve({ port: 3000, fetch: () => new Response("ok") });\n')
        write(root, "workspace/src/routes/health.ts", 'export const health = () => new Response("ok");\n')
    return {"prompt": PROMPT, "model": MODEL["modelId"]}


def send_prompt(user32, hwnd, mock, timeout=60):
    # 新窗口启动时输入框已聚焦；逐字 WM_CHAR 再回车，等待 mock 发出最终回答（可能先经过一次工具调用）。
    for char in PROMPT:
        user32.PostMessageW(hwnd, 0x102, ord(char), 0)
    time.sleep(0.3)
    user32.PostMessageW(hwnd, 0x100, 0x0D, 0)
    user32.PostMessageW(hwnd, 0x101, 0x0D, 0)
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        data = json.loads(Path(mock["state"]).read_text(encoding="utf-8"))
        if data.get("answered"):
            time.sleep(2)
            return {"responses": data["served"], "tools": data.get("toolNames", [])[:40]}
        time.sleep(0.25)
    raise RuntimeError("Scripted chat did not complete; check the transcript and chat-mock.log")



def chat_cycle(evidence, state, args, harness):
    """Stop the first generation, seed the scripted chat, relaunch and send one prompt."""
    harness["close"](evidence, state)
    mock = start_mock(evidence, Path(state["root"]) / "workspace")
    try:
        fixtures = seed(Path(state["root"]), mock["port"])
        state = harness["launch"](evidence, state, args)
    except Exception:
        stop_mock({"mock": mock})
        raise
    state.update(fixtures=fixtures, mock=mock)
    harness["save"](evidence, state)
    time.sleep(6)
    harness["focus"](state["hwnd"], evidence, state)
    state["fixtures"]["served"] = send_prompt(harness["user32"], state["hwnd"], mock)
    return state
