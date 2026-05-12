# SPDX-License-Identifier: AGPL-3.0-only
from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().parents[1] / "generate_legal_artifacts.py"
SPEC = importlib.util.spec_from_file_location("generate_legal_artifacts", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class GenerateLegalArtifactsTests(unittest.TestCase):
    def test_native_runtime_provenance_maps_matching_binary(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            provenance_path = Path(tmp_dir) / "native-runtime-provenance.json"
            provenance_path.write_text(
                """
{
  "format": "scriptscore-native-runtime-provenance-v1",
  "entries": [
    {
      "path_patterns": ["desktop/dist/bundled-runtime/python/**/site-packages/demo/**/*.so"],
      "source_package": "demo 1.0 wheel",
      "license": "MIT",
      "obligations": "Include generated notices.",
      "evidence": ["demo-1.0.dist-info/METADATA"]
    }
  ]
}
""",
                encoding="utf-8",
            )
            provenance = MODULE.native_runtime_provenance(provenance_path)
            native_path = (
                "desktop/dist/bundled-runtime/python/lib/python3.12/"
                "site-packages/demo/_fast.so"
            )
            item = MODULE.InventoryItem(
                name=native_path,
                version=None,
                license=None,
                source="runtime",
                scope="native-library",
                path=native_path,
                runtime=True,
                checksum_sha256="0" * 64,
            )

            mapped = MODULE.native_runtime_provenance_item(item, provenance)

        self.assertEqual(mapped.license, "MIT")
        self.assertEqual(mapped.version, "demo 1.0 wheel")
        self.assertIsNone(MODULE.classify_item(mapped))

    def test_unmatched_native_runtime_binary_still_requires_review(self) -> None:
        item = MODULE.InventoryItem(
            name="desktop/dist/bundled-runtime/python/lib/custom.so",
            version=None,
            license=None,
            source="runtime",
            scope="native-library",
            path="desktop/dist/bundled-runtime/python/lib/custom.so",
            runtime=True,
        )

        finding = MODULE.classify_item(item)

        self.assertIsNotNone(finding)
        assert finding is not None
        self.assertEqual(finding.severity, "review_required")

    def test_ujson_license_expression_is_allowed(self) -> None:
        self.assertTrue(MODULE.license_expression_is_allowed("BSD-3-Clause AND TCL"))

    def test_current_scipy_expression_is_approved_for_runtime(self) -> None:
        item = MODULE.InventoryItem(
            name="scipy",
            version="1.17.1",
            license=(
                "BSD-3-Clause AND BSD-3-Clause-Open-MPI AND "
                "GPL-3.0-or-later WITH GCC-exception-3.1"
            ),
            source="python",
            scope="python-runtime",
            runtime=True,
        )

        self.assertIsNone(MODULE.classify_item(item))


if __name__ == "__main__":
    unittest.main()
