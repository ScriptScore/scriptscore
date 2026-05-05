# SPDX-License-Identifier: AGPL-3.0-only
import json
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[3]
TAURI_CONFIG_PATH = REPO_ROOT / "desktop" / "src-tauri" / "tauri.conf.json"


class TauriAssetProtocolConfigTests(unittest.TestCase):
    def test_asset_protocol_allows_hidden_project_paths(self) -> None:
        config = json.loads(TAURI_CONFIG_PATH.read_text(encoding="utf-8"))

        scope = config["app"]["security"]["assetProtocol"]["scope"]

        self.assertIn("**", scope["allow"])
        self.assertIs(scope["requireLiteralLeadingDot"], False)


if __name__ == "__main__":
    unittest.main()
