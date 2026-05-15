# SPDX-License-Identifier: AGPL-3.0-only
from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "project_release_version.py"
SPEC = importlib.util.spec_from_file_location("project_release_version", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class ProjectReleaseVersionTests(unittest.TestCase):
    def test_project_msi_version_keeps_stable_version(self) -> None:
        self.assertEqual(MODULE.project_msi_version("0.10.0"), "0.10.0")

    def test_project_msi_version_maps_channels_to_numeric_ordinals(self) -> None:
        self.assertEqual(MODULE.project_msi_version("0.1.0-alpha.1"), "0.1.0-1001")
        self.assertEqual(MODULE.project_msi_version("0.1.0-beta.12"), "0.1.0-2012")
        self.assertEqual(MODULE.project_msi_version("0.1.0-rc.1"), "0.1.0-3001")

    def test_project_msi_version_rejects_non_numeric_or_unknown_prereleases(self) -> None:
        for version in ("0.1.0-preview.1", "0.1.0-rc.one", "0.1.0-rc.0"):
            with self.subTest(version=version):
                with self.assertRaises(MODULE.VersionProjectionError):
                    MODULE.project_msi_version(version)

    def test_project_msi_version_rejects_projection_above_msi_limit(self) -> None:
        with self.assertRaisesRegex(MODULE.VersionProjectionError, "exceeds"):
            MODULE.project_msi_version("0.1.0-rc.62536")

    def test_write_tauri_config_with_version_preserves_other_config(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            source = root / "tauri.conf.json"
            output = root / "generated" / "tauri.msi.conf.json"
            source.write_text(
                json.dumps({"productName": "ScriptScore Desktop", "version": "0.1.0-rc.1"}),
                encoding="utf-8",
            )

            MODULE.write_tauri_config_with_version(source, output, "0.1.0-3001")

            projected = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(projected["productName"], "ScriptScore Desktop")
            self.assertEqual(projected["version"], "0.1.0-3001")


if __name__ == "__main__":
    unittest.main()
