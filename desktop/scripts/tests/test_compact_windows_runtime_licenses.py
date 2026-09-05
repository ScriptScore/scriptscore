# SPDX-License-Identifier: AGPL-3.0-only
from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path, PureWindowsPath

MODULE_PATH = Path(__file__).resolve().parents[1] / "compact_windows_runtime_licenses.py"
SPEC = importlib.util.spec_from_file_location("compact_windows_runtime_licenses", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class CompactWindowsRuntimeLicensesTests(unittest.TestCase):
    def test_compacts_failing_torch_path_and_preserves_license_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            runtime_root = root / "bundled-runtime"
            legal_root = root / "legal"
            dist_info = (
                runtime_root / "python" / "Lib" / "site-packages" / "torch-2.13.0+cpu.dist-info"
            )
            top_level_license = dist_info / "licenses" / "LICENSE"
            deep_license = (
                dist_info
                / "licenses"
                / "third_party"
                / "kineto"
                / "libkineto"
                / "third_party"
                / "dynolog"
                / "third_party"
                / "prometheus-cpp"
                / "3rdparty"
                / "civetweb"
                / "src"
                / "third_party"
                / "duktape-1.5.2"
                / "LICENSE.txt"
            )
            top_level_license.parent.mkdir(parents=True)
            top_level_license.write_text("Torch license", encoding="utf-8")
            deep_license.parent.mkdir(parents=True)
            deep_license.write_text("Duktape license", encoding="utf-8")

            bundle_source_root = MODULE.default_windows_bundle_source_root(
                PureWindowsPath(r"D:\a\scriptscore\scriptscore")
            )
            before = MODULE.overlong_windows_runtime_paths(runtime_root, bundle_source_root)
            result = MODULE.compact_torch_third_party_licenses(runtime_root, legal_root)
            after = MODULE.overlong_windows_runtime_paths(runtime_root, bundle_source_root)

            self.assertEqual(before, [(deep_license.relative_to(runtime_root), 267)])
            self.assertEqual(after, [])
            self.assertEqual(result.file_count, 1)
            self.assertTrue(top_level_license.is_file())
            self.assertFalse(deep_license.exists())
            with zipfile.ZipFile(result.archive_path, mode="r") as archive:
                archived_name = (
                    "torch-2.13.0+cpu.dist-info/licenses/third_party/kineto/libkineto/"
                    "third_party/dynolog/third_party/prometheus-cpp/3rdparty/civetweb/src/"
                    "third_party/duktape-1.5.2/LICENSE.txt"
                )
                self.assertEqual(archive.namelist(), [archived_name])
                self.assertEqual(archive.read(archived_name), b"Duktape license")

    def test_leaves_other_distribution_license_trees_in_place(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            runtime_root = root / "bundled-runtime"
            other_license = (
                runtime_root
                / "python"
                / "Lib"
                / "site-packages"
                / "example-1.0.dist-info"
                / "licenses"
                / "third_party"
                / "LICENSE"
            )
            other_license.parent.mkdir(parents=True)
            other_license.write_text("Example license", encoding="utf-8")

            result = MODULE.compact_torch_third_party_licenses(runtime_root, root / "legal")

            self.assertEqual(result.file_count, 0)
            self.assertTrue(other_license.is_file())
            self.assertFalse(result.archive_path.exists())


if __name__ == "__main__":
    unittest.main()
