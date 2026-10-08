"""Windows native probe: only fresh scratch or same validated debug capability."""
import argparse
import ctypes
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

from PIL import ImageGrab
import psutil
from native_settings_fixtures import capability, populate, unlinked
from native_settings_window import activate

if sys.platform != "win32":
    raise SystemExit("Windows native acceptance only")

user32 = ctypes.WinDLL("user32", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
kernel32.GetCurrentThreadId.restype = ctypes.c_ulong
user32.GetThreadDesktop.argtypes = [ctypes.c_ulong]
user32.GetThreadDesktop.restype = ctypes.c_void_p
user32.OpenInputDesktop.argtypes = [ctypes.c_ulong, ctypes.c_bool, ctypes.c_ulong]
user32.OpenInputDesktop.restype = ctypes.c_void_p
user32.CloseDesktop.argtypes = [ctypes.c_void_p]
user32.GetUserObjectInformationW.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong)]
for name in ("GetForegroundWindow",):
    getattr(user32, name).restype = ctypes.c_void_p
user32.IsWindow.argtypes = [ctypes.c_void_p]
user32.IsWindowVisible.argtypes = [ctypes.c_void_p]
user32.SetForegroundWindow.argtypes = [ctypes.c_void_p]
user32.GetWindowThreadProcessId.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ulong)]
user32.GetWindowRect.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
user32.PostMessageW.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_size_t, ctypes.c_ssize_t]
user32.MoveWindow.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_bool]
user32.GetWindowTextLengthW.argtypes = [ctypes.c_void_p]
user32.ShowWindow.argtypes = [ctypes.c_void_p, ctypes.c_int]
user32.SetWindowPos.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_uint]
CALLBACK = ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p)
REPO = Path(__file__).resolve().parents[3]
EXECUTABLE = REPO / "apps/zcode-gpui/target/debug/zcode-gpui.exe"


def identity(process):
    return {"pid": process.pid, "created": process.create_time()}


def live(row):
    try:
        process = psutil.Process(row["pid"])
        return process if process.create_time() == row["created"] and process.is_running() else None
    except psutil.NoSuchProcess:
        return None


def remember(state):
    known = {(row["pid"], row["created"]): row for row in state["owned"]}
    for row in list(known.values()):
        process = live(row)
        if process:
            try:
                for child in process.children(recursive=True):
                    captured = identity(child)
                    if captured["created"] >= row["created"]:
                        known[(captured["pid"], captured["created"])] = captured
            except psutil.NoSuchProcess:
                pass
    state["owned"] = list(known.values())


def save(evidence, state):
    (evidence / "probe.json").write_text(json.dumps(state, indent=2), encoding="utf-8")


def load(evidence):
    unlinked(evidence)
    evidence = evidence.resolve(strict=True)
    if evidence.parent != Path(tempfile.gettempdir()).resolve() or not evidence.name.startswith("gpui-native-acceptance-"):
        raise RuntimeError("Only harness evidence directories are accepted")
    unlinked(evidence / "probe.json")
    state = json.loads((evidence / "probe.json").read_text(encoding="utf-8"))
    if state.get("version") != 2 or state["evidence"] != str(evidence):
        raise RuntimeError("Unknown or legacy probe state; start a new harness")
    root, _ = capability(state["root"], state["ticket"])
    state["root"] = str(root)
    return evidence, state


def hwnd_for(pid):
    found = []
    @CALLBACK
    def visit(hwnd, _):
        owner = ctypes.c_ulong()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(hwnd) and user32.GetWindowTextLengthW(hwnd):
            found.append(hwnd)
        return True
    user32.EnumWindows(visit, 0)
    return found[0] if found else None


def owned_window(state):
    if not live(state["process"]):
        raise RuntimeError("Owned native process exited or PID identity changed")
    hwnd = state["hwnd"]
    owner = ctypes.c_ulong()
    user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
    if not user32.IsWindow(hwnd) or owner.value != state["process"]["pid"]:
        raise RuntimeError("Owned native window identity changed")
    return hwnd


def memory(state):
    remember(state)
    rows = []
    for row in state["owned"]:
        process = live(row)
        if process:
            try:
                info, command = process.memory_info(), process.cmdline()
                role = "gpui" if row == state["process"] else "services" if any("zcode-server.cjs" in arg for arg in command) else "cli" if any("zcode.cjs" in arg or "app-server" in arg for arg in command) else "child"
                rows.append({**row, "role": role, "rssMiB": round(info.rss / 1048576, 2),
                             "privateMiB": round(getattr(info, "private", info.vms) / 1048576, 2)})
            except psutil.NoSuchProcess:
                pass
    return {"processes": rows, "totalRssMiB": round(sum(row["rssMiB"] for row in rows), 2)}


def capture(hwnd, path, evidence, state):
    require_interactive(evidence, state)
    if owned_window(state) != hwnd or user32.GetForegroundWindow() != hwnd:
        raise RuntimeError("Capture refused: owned window is not foreground")
    class RECT(ctypes.Structure):
        _fields_ = [(key, ctypes.c_long) for key in ("left", "top", "right", "bottom")]
    rect = RECT()
    if not user32.GetWindowRect(hwnd, ctypes.byref(rect)):
        raise RuntimeError("Owned window geometry unavailable")
    image = ImageGrab.grab(bbox=(rect.left, rect.top, rect.right, rect.bottom))
    require_interactive(evidence, state)
    if owned_window(state) != hwnd or user32.GetForegroundWindow() != hwnd:
        raise RuntimeError("Capture discarded: foreground changed during grab")
    image.save(path)


def desktop_context():
    def name(handle):
        buffer, size = ctypes.create_unicode_buffer(256), ctypes.c_ulong()
        ok = user32.GetUserObjectInformationW(handle, 2, buffer, ctypes.sizeof(buffer), ctypes.byref(size))
        return buffer.value if ok else "unavailable"
    current = name(user32.GetThreadDesktop(kernel32.GetCurrentThreadId()))
    active = user32.OpenInputDesktop(0, False, 1)
    try:
        target = name(active) if active else "unavailable"
    finally:
        if active:
            user32.CloseDesktop(active)
    return {"threadDesktop": current, "inputDesktop": target, "foregroundAvailable": bool(user32.GetForegroundWindow())}


def require_interactive(evidence=None, state=None):
    context = desktop_context()
    # 已观察到 Default 线程运行时输入桌面却是 Screen-saver；拒绝遮挡截图，不能绕过锁屏。
    if context["threadDesktop"] != context["inputDesktop"] or context["inputDesktop"] == "unavailable" or not context["foregroundAvailable"]:
        if evidence and state:
            state["captureBlocked"] = context
            save(evidence, state)
        raise RuntimeError(f"Native acceptance requires an unlocked interactive desktop: {context}")


def focus(hwnd, evidence, state):
    def validate():
        if owned_window(state) != hwnd:
            raise RuntimeError("Owned window changed during activation")
    def persist(diagnostic):
        state["focusDiagnostics"] = diagnostic
        save(evidence, state)
    activate(user32, hwnd, validate, lambda: require_interactive(evidence, state), desktop_context, persist)


def stopped(state):
    remember(state)
    survivors = [row for row in state["owned"] if live(row)]
    if survivors:
        raise RuntimeError(f"Owned processes must exit before fixture/restart: {survivors}")


def close(evidence, state, force=False):
    remember(state)
    if live(state["process"]):
        hwnd = hwnd_for(state["process"]["pid"])
        if hwnd:
            user32.PostMessageW(hwnd, 0x10, 0, 0)
        elif not force:
            raise RuntimeError("Owned process has no window; refusing unobserved restart")
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        remember(state)
        if not any(live(row) for row in state["owned"]):
            break
        time.sleep(0.1)
    if force:
        for row in reversed(state["owned"]):
            process = live(row)
            if process:
                process.terminate()
        deadline = time.monotonic() + 10
        while any(live(row) for row in state["owned"]) and time.monotonic() < deadline:
            time.sleep(0.1)
    state["survivorsAfterQuit"] = [row for row in state["owned"] if live(row)]
    state["status"] = "stopped" if not state["survivorsAfterQuit"] else "cleanup-blocked"
    save(evidence, state)
    stopped(state)


def locale_environment(env, locale):
    # 默认验收读取 scratch 已保存偏好；显式截图语言覆盖必须保持不落盘。
    env.pop("ZCODE_GPUI_ACCEPTANCE_LOCALE", None)
    if locale not in (None, "preference"):
        env["ZCODE_GPUI_ACCEPTANCE_LOCALE"] = locale


def launch(evidence, previous, args):
    require_interactive(evidence, previous)
    if previous:
        stopped(previous)
    generation = previous["generation"] + 1 if previous else 1
    env = {key: value for key, value in os.environ.items() if not key.startswith("ZCODE_GPUI_ACCEPTANCE_")}
    theme, locale = args.theme or (previous or {}).get("theme", "zai-dark"), args.locale or (previous or {}).get("locale", "preference")
    env["ZCODE_GPUI_ACCEPTANCE_THEME"] = theme
    locale_environment(env, locale)
    if previous:
        _, token = capability(previous["root"], previous["ticket"])
        env.update(ZCODE_GPUI_ACCEPTANCE_TICKET=previous["ticket"], ZCODE_GPUI_ACCEPTANCE_TOKEN=token)
    runtime = Path(os.environ.get("LOCALAPPDATA", "")) / "Programs/ZCode/ZCode.exe"
    if runtime.is_file():
        env["ZCODE_GPUI_SERVICES_RUNTIME"] = str(runtime)
    stdout_path = evidence / f"stdout-{generation}.log"
    with stdout_path.open("wb") as stdout, (evidence / f"stderr-{generation}.log").open("wb") as stderr:
        child = subprocess.Popen([str(EXECUTABLE), "--isolated-settings", "--settings-section", args.section],
                                 cwd=REPO, env=env, stdout=stdout, stderr=stderr)
    process = identity(psutil.Process(child.pid))
    state = {"version": 2, "generation": generation, "evidence": str(evidence), "process": process,
             "pid": child.pid, "owned": [process], "theme": theme, "locale": locale, "status": "starting"}
    if previous:
        state["root"], state["ticket"] = previous["root"], previous["ticket"]
        state["previousOwned"] = previous["owned"]
    save(evidence, state)
    try:
        deadline = time.monotonic() + 30
        while child.poll() is None and time.monotonic() < deadline:
            remember(state)
            if "root" not in state or "announced" not in state:
                for line in stdout_path.read_text(encoding="utf-8", errors="replace")[:65536].splitlines():
                    if line.startswith("ZCODE_GPUI_ACCEPTANCE "):
                        announcement = json.loads(line.removeprefix("ZCODE_GPUI_ACCEPTANCE "))
                        root, _ = capability(announcement["root"], announcement["ticket"])
                        if previous and root != Path(previous["root"]):
                            raise RuntimeError("Restart changed scratch identity")
                        state.update(root=str(root), ticket=str(root / ".zcode-gpui-acceptance.json"), announced=True)
                        break
            hwnd = hwnd_for(child.pid)
            if hwnd and state.get("announced"):
                state["hwnd"] = hwnd
                break
            time.sleep(0.1)
        if "hwnd" not in state:
            raise RuntimeError(f"Native window/capability did not open; evidence {evidence}")
        user32.MoveWindow(state["hwnd"], 80, 60, args.width, args.height, True)
        time.sleep(2)
        state["baseline"] = memory(state)
        state["status"] = "running"
        save(evidence, state)
        return state
    except Exception:
        close(evidence, state, force=True)
        raise


def filename(evidence, name):
    if Path(name).name != name or not name.endswith(".png"):
        raise RuntimeError("Capture must be a PNG basename inside evidence")
    path = evidence / name
    if path.exists():
        unlinked(path)
    return path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("start", "restart", "fixtures", "close", "click", "type", "capture", "resize", "key", "wheel"))
    parser.add_argument("values", nargs="*")
    parser.add_argument("--theme", choices=("zai-dark", "zai-light"))
    parser.add_argument("--locale", choices=("preference", "en-US", "zh-CN"), help="default reads saved scratch preference; other values are non-persisting overrides")
    parser.add_argument("--section", default="subagents")
    parser.add_argument("--width", type=int, default=1280)
    parser.add_argument("--height", type=int, default=900)
    parser.add_argument("--populated", action="store_true", help="start, stop, seed compact synthetic inventory, then reattach")
    parser.add_argument("--long-populated", action="store_true", help="same safe cycle with long scrolling inventory")
    args = parser.parse_args()
    if not 640 <= args.width <= 2000 or not 480 <= args.height <= 1600:
        parser.error("Native size is outside acceptance bounds")
    if args.mode == "start":
        if args.values:
            parser.error("start never accepts a root or evidence path")
        evidence = Path(tempfile.mkdtemp(prefix="gpui-native-acceptance-")).resolve()
        state = launch(evidence, None, args)
        if args.populated or args.long_populated:
            close(evidence, state)
            state["fixtures"] = populate(Path(state["root"]), args.long_populated)
            save(evidence, state)
            state = launch(evidence, state, args)
    else:
        if not args.values:
            parser.error("mode requires harness evidence path")
        evidence, state = load(Path(args.values[0]))
        if args.mode in ("restart", "fixtures"):
            if state["status"] != "stopped":
                close(evidence, state)
            stopped(state)
            if args.mode == "fixtures":
                state["fixtures"] = populate(Path(state["root"]), args.long_populated)
                save(evidence, state)
                print(json.dumps({"evidence": str(evidence), "root": state["root"], "fixtures": state["fixtures"]}))
                return
            state = launch(evidence, state, args)
        elif args.mode == "close":
            close(evidence, state)
            print(json.dumps({"owned": state["owned"], "survivors": state["survivorsAfterQuit"]}))
            return
        else:
            hwnd = owned_window(state)
            focus(hwnd, evidence, state)
            if args.mode in ("click", "type", "wheel"):
                x, y = int(args.values[1]), int(args.values[2])
                if not 0 <= x <= 2000 or not 0 <= y <= 1600:
                    raise RuntimeError("Client coordinates out of bounds")
                location = x | (y << 16)
                if args.mode == "wheel":
                    class POINT(ctypes.Structure):
                        _fields_ = [("x", ctypes.c_long), ("y", ctypes.c_long)]
                    point = POINT(x, y)
                    user32.ClientToScreen.argtypes = [ctypes.c_void_p, ctypes.POINTER(POINT)]
                    if not user32.ClientToScreen(hwnd, ctypes.byref(point)):
                        raise RuntimeError("Owned client coordinates unavailable")
                    location = (point.x & 0xFFFF) | ((point.y & 0xFFFF) << 16)
                    user32.PostMessageW(hwnd, 0x20A, (int(args.values[3]) & 0xFFFF) << 16, location)
                else:
                    user32.PostMessageW(hwnd, 0x201, 1, location)
                    user32.PostMessageW(hwnd, 0x202, 0, location)
                    time.sleep(0.2)
                    if args.mode == "type":
                        for char in args.values[3]:
                            for offset in range(0, len(encoded := char.encode("utf-16-le")), 2):
                                user32.PostMessageW(hwnd, 0x102, int.from_bytes(encoded[offset:offset + 2], "little"), 0)
            elif args.mode == "resize":
                user32.MoveWindow(hwnd, 80, 60, args.width, args.height, True)
            elif args.mode == "key":
                keys = {"tab": 0x09, "enter": 0x0D, "escape": 0x1B, "up": 0x26, "down": 0x28,
                        "pageup": 0x21, "pagedown": 0x22, "home": 0x24, "end": 0x23,
                        "shift": 0x10, "ctrl": 0x11, "comma": 0xBC, "a": 0x41, "backspace": 0x08}
                sequence = [keys[key] for key in args.values[1].lower().split("+")]
                for key in sequence:
                    user32.keybd_event(key, 0, 0, 0)
                for key in reversed(sequence):
                    user32.keybd_event(key, 0, 2, 0)
            time.sleep(1)
    hwnd = owned_window(state)
    try:
        focus(hwnd, evidence, state)
        name = args.values[-1] if args.mode not in ("start", "restart") else f"settings-{state['generation']}.png"
        path = filename(evidence, name)
        capture(hwnd, path, evidence, state)
        snapshot = memory(state)
        save(evidence, state)
        print(json.dumps({"evidence": str(evidence), "root": state["root"], "pid": state["pid"],
                          "created": state["process"]["created"], "hwnd": hwnd, "capture": str(path), "memory": snapshot}))
    except Exception:
        close(evidence, state)
        raise


if __name__ == "__main__":
    main()
