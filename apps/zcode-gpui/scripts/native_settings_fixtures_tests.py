"""English-only default synthetic data; no real profile or preference access."""
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
import uuid

from native_settings_fixtures import LOCK, MARKER, PREFIX, TICKET, populate, provider_fixture


@unittest.skipUnless(os.name == "nt", "Windows native harness only")
class NativeLocaleEnvironmentTests(unittest.TestCase):
    def test_preference_mode_omits_override_even_when_parent_environment_has_one(self):
        from native_settings_acceptance import locale_environment
        for preference in (None, "preference"):
            env = {"ZCODE_GPUI_ACCEPTANCE_LOCALE": "zh-CN", "unchanged": "value"}
            locale_environment(env, preference)
            self.assertNotIn("ZCODE_GPUI_ACCEPTANCE_LOCALE", env)
            self.assertEqual(env["unchanged"], "value")
        for locale in ("en-US", "zh-CN"):
            env = {}
            locale_environment(env, locale)
            self.assertEqual(env["ZCODE_GPUI_ACCEPTANCE_LOCALE"], locale)


class DefaultFixtureLanguageTests(unittest.TestCase):
    def scratch(self):
        identity = str(uuid.uuid4())
        root = Path(tempfile.gettempdir()).resolve() / f"{PREFIX}{identity}"
        root.mkdir()
        self.addCleanup(shutil.rmtree, root)
        for relative in ("home", "data", "workspace", "temp", "home/AppData/Local", "home/AppData/Roaming"):
            (root / relative).mkdir(parents=True, exist_ok=True)
        (root / MARKER).write_text(json.dumps({"version": 1, "identity": identity, "root": str(root)}))
        (root / TICKET).write_text(json.dumps({"version": 1, "identity": identity, "token": "a" * 64}))
        (root / LOCK).write_bytes(b"0")
        return root

    def test_default_provider_display_name_is_english_only(self):
        provider = provider_fixture()["config"]["providerConfigRules"]["providerRules"][0]
        self.assertEqual(provider["providerName"], "Native synthetic metadata")

    def test_default_compact_and_long_profiles_are_monolingual_english(self):
        for long_inventory in (False, True):
            with self.subTest(long_inventory=long_inventory):
                root = self.scratch()
                counts = populate(root, long_inventory)
                custom = list((root / "home/.zcode/agents").glob("*.md"))
                plugin = list((root / "home/.zcode/cli/plugins/cache").rglob("*.md"))
                self.assertEqual(len(custom), counts["customAgents"])
                self.assertEqual(len(plugin), counts["pluginAgents"])
                for profile in custom + plugin:
                    text = profile.read_text(encoding="utf-8")
                    self.assertTrue(text.isascii(), profile.name)
                    self.assertIn("Synthetic", text)


if __name__ == "__main__":
    unittest.main()
