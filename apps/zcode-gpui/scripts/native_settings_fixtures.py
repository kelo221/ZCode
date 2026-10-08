"""Synthetic metadata only; every writer requires a stopped, validated scratch root."""
import json
import os
from pathlib import Path
import re
import tempfile
import uuid

PREFIX = "zcode-gpui-settings-"
MARKER = ".zcode-gpui-isolated.json"
TICKET = ".zcode-gpui-acceptance.json"
LOCK = ".zcode-gpui-isolated.lock"


def unlinked(path):
    info = path.lstat()
    if path.is_symlink() or getattr(info, "st_file_attributes", 0) & 0x400:
        raise RuntimeError("Scratch link/reparse point refused")


def bounded_json(path):
    unlinked(path)
    if not path.is_file() or path.stat().st_size > 4096:
        raise RuntimeError("Scratch capability file refused")
    return json.loads(path.read_text(encoding="utf-8"))


def plain_path(path):
    text = str(path)
    if os.name == "nt" and text.startswith("\\\\?\\"):
        text = text[4:]
    return Path(text)


def canonical(path):
    return plain_path(Path(path).resolve(strict=True))


def capability(root, ticket):
    requested = Path(root)
    unlinked(requested)
    root = canonical(requested)
    if root.parent != canonical(tempfile.gettempdir()) or not root.name.startswith(PREFIX):
        raise RuntimeError("Only harness-created OS-temp scratch roots are accepted")
    identity = root.name.removeprefix(PREFIX)
    if str(uuid.UUID(identity)) != identity:
        raise RuntimeError("Scratch UUID identity refused")
    expected = root / TICKET
    if canonical(ticket) != expected or Path(ticket).name != TICKET:
        raise RuntimeError("Scratch ticket path refused")
    count = 0
    pending = [(root, 0)]
    while pending:
        path, depth = pending.pop()
        count += 1
        if count > 20000 or depth > 64:
            raise RuntimeError("Scratch tree exceeds acceptance bound")
        unlinked(path)
        if not path.resolve(strict=True).is_relative_to(root):
            raise RuntimeError("Scratch child escapes owned root")
        if path.is_dir():
            pending.extend((child, depth + 1) for child in path.iterdir())
            if len(pending) > 20000:
                raise RuntimeError("Scratch tree exceeds acceptance bound")
        elif not path.is_file():
            raise RuntimeError("Scratch non-file refused")
    for relative in ("home", "data", "workspace", "temp", "home/AppData/Local", "home/AppData/Roaming"):
        if not (root / relative).is_dir():
            raise RuntimeError("Required scratch directory missing")
    marker, ticket_data = bounded_json(root / MARKER), bounded_json(expected)
    if set(marker) != {"version", "identity", "root"} or marker["version"] != 1 or marker["identity"] != identity or plain_path(marker["root"]) != root:
        raise RuntimeError("Scratch marker identity refused")
    if set(ticket_data) != {"version", "identity", "token"} or ticket_data["version"] != 1 or ticket_data["identity"] != identity:
        raise RuntimeError("Scratch capability identity refused")
    token = ticket_data["token"]
    if not isinstance(token, str) or not re.fullmatch(r"[0-9a-f]{64}", token):
        raise RuntimeError("Scratch token refused")
    return root, token


class ScratchLock:
    def __init__(self, root):
        self.path = root / LOCK
        self.file = None

    def __enter__(self):
        unlinked(self.path)
        self.file = self.path.open("r+b", buffering=0)
        try:
            if os.name == "nt":
                import msvcrt
                msvcrt.locking(self.file.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl
                fcntl.flock(self.file, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except Exception:
            self.file.close()
            raise RuntimeError("Scratch is still locked by an owned process") from None
        return self

    def __exit__(self, *_):
        if os.name == "nt":
            import msvcrt
            self.file.seek(0)
            msvcrt.locking(self.file.fileno(), msvcrt.LK_UNLCK, 1)
        self.file.close()


def write(root, relative, data):
    path = root / relative
    if path.is_absolute() and not path.is_relative_to(root) or ".." in path.parts:
        raise RuntimeError("Fixture path refused")
    path.parent.mkdir(parents=True, exist_ok=True)
    unlinked(path.parent)
    if path.exists():
        unlinked(path)
    if not path.parent.resolve().is_relative_to(root):
        raise RuntimeError("Fixture parent escapes scratch")
    path.write_text(data if isinstance(data, str) else json.dumps(data, indent=2), encoding="utf-8")


def provider_fixture():
    properties = {
        "requiresMfjsToolSchema": False, "contextWindow": 8192,
        "inputFormat": {"supportsText": True, "supportsImage": True, "supportsVideo": False,
                        "supportsAudio": False, "supportsPdf": False},
        "outputFormat": {"supportsText": True}, "supportsToolCall": True,
        "supportsJsonSchemaOutput": False, "supportsNativeWebSearch": False,
        "supportsMidConversationSystem": True,
    }
    return {"schemaVersion": 1, "config": {
        "providerConfigRules": {"providerRules": [{
            "providerId": "native-acceptance", "providerName": "Native synthetic metadata",
            "enabled": True, "config": {
                "group": "standard-personal",
                "access": {"type": "api-key", "apiKey": "native-test-placeholder-not-a-credential"},
                "api": {"type": "openai-chat-completions", "baseUrl": "http://127.0.0.1:1/v1"},
                "personalModelIds": ["native-metadata", "native-metadata-long-label-for-layout-acceptance"],
            },
        }]},
        "modelConfigRules": {"providerModelRules": [{
            "providerId": "native-acceptance", "modelId": model,
            "config": {"enabled": True, "properties": properties, "optionSpecs": {
                "reasoningLevel": {"values": ["low", "high"], "map": '{"reasoning_effort": reasoningLevel}'},
                "maxOutputTokens": {"max": 2048, "map": '{"max_tokens": maxOutputTokens}'},
            }},
        } for model in ("native-metadata", "native-metadata-long-label-for-layout-acceptance")],
            "manualProviderModelRules": []},
    }}


def populate(root, long_inventory=False):
    # 只有已停进程且持有同一 OS 文件锁才写合成夹具，不能与 Host 配置写入重叠。
    with ScratchLock(root):
        capability(root, root / TICKET)
        if (root / ".native-fixtures-seeded").exists():
            raise RuntimeError("Fixture inventory already seeded; use restart to preserve action results")
        write(root, "data/.zcode/v2/provider_config.json", provider_fixture())
        colors = ("blue", "green", "yellow", "purple", "red", "cyan")
        custom_count = 12 if long_inventory else 3
        plugin_names = ("Review", "Explore", "LongProfileNameForNativeLayoutAcceptance") if long_inventory else ("Review", "Explore")
        for index in range(custom_count):
            name = f"native-custom-{index + 1:02}"
            if index == 2 and long_inventory:
                name += "-long-name-for-narrow-row-layout-and-truncation"
            description = f"Synthetic acceptance profile {index + 1}; inspect scope and trailing controls."
            if index % 3 == 1:
                # 默认夹具只用英文；界面语言由 selector 决定，不能把双语测试文案当显示标签。
                description = "Synthetic acceptance profile: inspect hierarchy, long descriptions and trailing actions without reading real configuration."
            if index == 2 and long_inventory:
                description *= 4
            text = (f"---\nname: {name}\ndescription: {json.dumps(description, ensure_ascii=False)}\n"
                    f"color: {colors[index % len(colors)]}\ninjectAgentsMd: true\ntools: [Read]\n"
                    "---\n\nDisposable synthetic prompt. Do not make provider requests.\n")
            # canonical 用户 profile 位于 storageRoot/agents；v2 仅保存状态，错误夹具目录不会进入库存。
            write(root, f"home/.zcode/agents/{name}.md", text)
        cli = "home/.zcode/cli"
        installed, enabled = [], {}
        for group in ("review-fixture", "planning-fixture"):
            relative = f"{cli}/plugins/cache/native-tests/{group}/1.0.0"
            plugin_id = f"{group}@native-tests"
            installed.append({"id": plugin_id, "name": group, "marketplace": "native-tests",
                              "version": "1.0.0", "scope": "user", "installPath": str(root / relative)})
            enabled[plugin_id] = True
            write(root, f"{relative}/.zcode-plugin/plugin.json",
                  {"name": group, "version": "1.0.0", "agents": "./profiles"})
            for name in plugin_names:
                write(root, f"{relative}/profiles/{name.lower()}.md",
                      f"---\nname: {name}\ndescription: Synthetic {group} profile\n"
                      "model: native-acceptance/native-metadata\nthoughtLevel: low\n"
                      "tools: [Read]\n---\n\nDisposable immutable plugin prompt.\n")
        write(root, f"{cli}/plugins/installed_plugins.json", {"version": 1, "plugins": installed})
        write(root, f"{cli}/config.json", {"plugins": {"enabledPlugins": enabled}})
        write(root, ".native-fixtures-seeded", "Synthetic fixture inventory, version 1.\n")
    return {"customAgents": custom_count, "pluginGroups": 2, "pluginAgents": len(plugin_names) * 2, "models": 2}
