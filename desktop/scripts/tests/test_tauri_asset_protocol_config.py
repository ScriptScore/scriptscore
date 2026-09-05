# SPDX-License-Identifier: AGPL-3.0-only
import json
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[3]
TAURI_CONFIG_PATH = REPO_ROOT / "desktop" / "src-tauri" / "tauri.conf.json"
BUNDLE_RESOURCES_CONFIG_PATH = (
    REPO_ROOT / "desktop" / "src-tauri" / "tauri.bundle-resources.conf.json"
)
BUILD_DESKTOP_PACKAGE_PATH = (
    REPO_ROOT / "desktop" / "scripts" / "build-desktop-package.sh"
)


class TauriAssetProtocolConfigTests(unittest.TestCase):
    def test_asset_protocol_allows_hidden_project_paths(self) -> None:
        config = json.loads(TAURI_CONFIG_PATH.read_text(encoding="utf-8"))

        scope = config["app"]["security"]["assetProtocol"]["scope"]

        self.assertIn("**", scope["allow"])
        self.assertIs(scope["requireLiteralLeadingDot"], False)

    def test_generated_bundle_resources_are_not_in_the_development_config(self) -> None:
        config = json.loads(TAURI_CONFIG_PATH.read_text(encoding="utf-8"))

        self.assertNotIn("resources", config["bundle"])

    def test_packaging_config_includes_generated_bundle_resources(self) -> None:
        config = json.loads(BUNDLE_RESOURCES_CONFIG_PATH.read_text(encoding="utf-8"))

        self.assertEqual(
            config["bundle"]["resources"],
            {
                "../../cli/models/paddle/": "models/paddle/",
                "../dist/legal/": "legal/",
                "../dist/bundled-runtime/": "runtime/",
            },
        )

    def test_packaging_script_layers_the_bundle_resources_config(self) -> None:
        script = BUILD_DESKTOP_PACKAGE_PATH.read_text(encoding="utf-8")

        self.assertIn("tauri.bundle-resources.conf.json", script)
        self.assertIn('"${BUNDLE_RESOURCES_CONFIG}"\n    --config\n    "${config}"', script)


if __name__ == "__main__":
    unittest.main()
