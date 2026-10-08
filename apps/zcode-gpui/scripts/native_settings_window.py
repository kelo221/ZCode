"""Bounded owned-window activation, never a foreground/secure-desktop bypass."""
import ctypes


class POINT(ctypes.Structure):
    _fields_ = [("x", ctypes.c_long), ("y", ctypes.c_long)]


class RECT(ctypes.Structure):
    _fields_ = [(key, ctypes.c_long) for key in ("left", "top", "right", "bottom")]


class MOUSEINPUT(ctypes.Structure):
    _fields_ = [("dx", ctypes.c_long), ("dy", ctypes.c_long), ("mouseData", ctypes.c_ulong),
                ("dwFlags", ctypes.c_ulong), ("time", ctypes.c_ulong), ("dwExtraInfo", ctypes.c_size_t)]


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [("wVk", ctypes.c_ushort), ("wScan", ctypes.c_ushort), ("dwFlags", ctypes.c_ulong),
                ("time", ctypes.c_ulong), ("dwExtraInfo", ctypes.c_size_t)]


class INPUTUNION(ctypes.Union):
    _fields_ = [("mi", MOUSEINPUT), ("ki", KEYBDINPUT)]


class INPUT(ctypes.Structure):
    _fields_ = [("type", ctypes.c_ulong), ("data", INPUTUNION)]


def configure(user32):
    user32.GetAncestor.argtypes = [ctypes.c_void_p, ctypes.c_uint]
    user32.GetAncestor.restype = ctypes.c_void_p
    user32.WindowFromPoint.argtypes = [POINT]
    user32.WindowFromPoint.restype = ctypes.c_void_p
    user32.SendMessageTimeoutW.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_size_t,
                                          ctypes.c_ssize_t, ctypes.c_uint, ctypes.c_uint,
                                          ctypes.POINTER(ctypes.c_size_t)]
    user32.SendMessageTimeoutW.restype = ctypes.c_ssize_t
    user32.SetCursorPos.argtypes = [ctypes.c_int, ctypes.c_int]
    user32.SendInput.argtypes = [ctypes.c_uint, ctypes.POINTER(INPUT), ctypes.c_int]
    user32.SendInput.restype = ctypes.c_uint


def call(user32, name, *args):
    ctypes.set_last_error(0)
    value = getattr(user32, name)(*args)
    return {"ok": bool(value), "error": ctypes.get_last_error()}


def message(user32, hwnd, kind, location=0, timeout=1000):
    result = ctypes.c_size_t()
    outcome = call(user32, "SendMessageTimeoutW", hwnd, kind, 0, location, 0x2, timeout, ctypes.byref(result))
    outcome["result"] = result.value
    return outcome


def foreground(user32):
    hwnd, pid = user32.GetForegroundWindow(), ctypes.c_ulong()
    thread = user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    return {"hwnd": hwnd, "pid": pid.value, "thread": thread}


def caption_owned(user32, hwnd, point, validate, candidates):
    validate()
    hit, pid = user32.WindowFromPoint(point), ctypes.c_ulong()
    root = user32.GetAncestor(hit, 2)
    thread = user32.GetWindowThreadProcessId(hit, ctypes.byref(pid))
    row = {"point": [point.x, point.y], "hit": hit, "root": root, "pid": pid.value, "thread": thread}
    candidates.append(row)
    if root != hwnd:
        row["reason"] = "foreign-or-covered"
        return False
    packed = (point.x & 0xFFFF) | ((point.y & 0xFFFF) << 16)
    reply = message(user32, hwnd, 0x84, packed, 100)
    row["hitTest"] = reply
    return reply["ok"] and reply["result"] == 2  # WM_NCHITTEST 必须明确返回 HTCAPTION。


def caption_click(user32, hwnd, validate, interactive, candidates):
    rect = RECT()
    if not user32.GetWindowRect(hwnd, ctypes.byref(rect)):
        return {"ok": False, "reason": "geometry-unavailable"}
    for offset in (12, 22, 30):
        for fraction in (0.5, 0.25):
            point = POINT(int(rect.left + (rect.right - rect.left) * fraction), rect.top + offset)
            if not rect.left < point.x < rect.right or not rect.top < point.y < rect.bottom:
                continue
            interactive()
            if not caption_owned(user32, hwnd, point, validate, candidates):
                continue
            moved = call(user32, "SetCursorPos", point.x, point.y)
            if not moved["ok"]:
                return {"ok": False, "reason": "cursor-refused", "move": moved}
            # 移动后遮挡/身份可能改变；再次验证才发送自身标题栏点击，不能点击其他窗口。
            interactive()
            if not caption_owned(user32, hwnd, point, validate, candidates):
                return {"ok": False, "reason": "caption-changed"}
            inputs = (INPUT * 2)()
            inputs[0].data.mi.dwFlags, inputs[1].data.mi.dwFlags = 0x2, 0x4
            ctypes.set_last_error(0)
            sent = user32.SendInput(2, inputs, ctypes.sizeof(INPUT))
            return {"ok": sent == 2, "sent": sent, "error": ctypes.get_last_error(), "point": [point.x, point.y]}
    return {"ok": False, "reason": "no-visible-owned-caption"}


def activate(user32, hwnd, validate, interactive, context, persist):
    configure(user32)
    validate()
    interactive()
    diagnostic = {"beforeDesktop": context(), "beforeForeground": foreground(user32), "target": hwnd}
    pid = ctypes.c_ulong()
    diagnostic["targetThread"] = user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    diagnostic["targetPid"] = pid.value
    try:
        diagnostic["show"] = call(user32, "ShowWindow", hwnd, 9)
        diagnostic["position"] = call(user32, "SetWindowPos", hwnd, ctypes.c_void_p(-1), 0, 0, 0, 0, 0x53)
        diagnostic["request"] = call(user32, "SetForegroundWindow", hwnd)
        # 跨线程激活异步完成；WM_NULL 有界 ACK 代替固定 sleep，拒绝把遮挡截图算作成功。
        diagnostic["ack"] = message(user32, hwnd, 0)
        if not diagnostic["ack"]["ok"]:
            raise RuntimeError("Owned window did not acknowledge activation within one second")
        validate()
        interactive()
        if user32.GetForegroundWindow() != hwnd:
            diagnostic["captionCandidates"] = []
            diagnostic["caption"] = caption_click(user32, hwnd, validate, interactive, diagnostic["captionCandidates"])
            diagnostic["captionAck"] = message(user32, hwnd, 0)
            if not diagnostic["captionAck"]["ok"]:
                raise RuntimeError("Owned window did not acknowledge caption activation within one second")
        validate()
        interactive()
        diagnostic["afterDesktop"] = context()
        diagnostic["afterForeground"] = foreground(user32)
        diagnostic["ok"] = diagnostic["afterForeground"]["hwnd"] == hwnd
        if not diagnostic["ok"]:
            raise RuntimeError("Owned native foreground activation refused; see focusDiagnostics")
    finally:
        diagnostic.setdefault("afterDesktop", context())
        diagnostic.setdefault("afterForeground", foreground(user32))
        persist(diagnostic)
