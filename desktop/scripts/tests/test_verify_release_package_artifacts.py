# SPDX-License-Identifier: AGPL-3.0-only
from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().parents[1] / "verify-release-package-artifacts.py"
SPEC = importlib.util.spec_from_file_location("verify_release_package_artifacts", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class VerifyReleasePackageArtifactsTests(unittest.TestCase):
    def _write_payload_layout(self, payload_root: Path) -> None:
        resources = payload_root / "app" / "resources"
        runtime = resources / "runtime"
        (runtime / "cli-src" / "scriptscore").mkdir(parents=True)
        (runtime / "python" / "bin").mkdir(parents=True)
        (runtime / "python" / "bin" / "python3").write_text("", encoding="utf-8")
        (runtime / "runtime-manifest.json").write_text(
            json.dumps(
                {
                    "manifestVersion": 1,
                    "portableRelease": True,
                    "pythonExecutable": "python/bin/python3",
                }
            ),
            encoding="utf-8",
        )

        models = resources / "models" / "paddle"
        for name in ("det", "rec"):
            model_dir = models / name
            model_dir.mkdir(parents=True)
            (model_dir / "inference.yml").write_text("model: test\n", encoding="utf-8")
            (model_dir / "inference.pdiparams").write_text("", encoding="utf-8")
            (model_dir / "inference.pdmodel").write_text("", encoding="utf-8")

        legal = resources / "legal"
        legal.mkdir(parents=True)
        (legal / "NOTICE.txt").write_text("notices\n", encoding="utf-8")

    def test_validate_payload_root_finds_packaged_resources(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            payload_root = Path(tmp_dir) / "payload"
            self._write_payload_layout(payload_root)

            summary = MODULE.validate_payload_root(payload_root)

            self.assertEqual(summary["payloadRoot"], str(payload_root))
            self.assertIn("runtime", summary)
            self.assertIn("models", summary)
            self.assertIn("legal", summary)

    def test_validate_payload_root_rejects_missing_runtime(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            payload_root = Path(tmp_dir) / "payload"
            payload_root.mkdir()

            with self.assertRaisesRegex(MODULE.VerificationError, "missing a valid runtime"):
                MODULE.validate_payload_root(payload_root)


if __name__ == "__main__":
    unittest.main()
