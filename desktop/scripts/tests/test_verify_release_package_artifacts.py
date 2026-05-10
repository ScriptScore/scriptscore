# SPDX-License-Identifier: AGPL-3.0-only
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

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
        paddle_libs = (
            runtime / "python" / "lib" / "python3.12" / "site-packages" / "paddle" / "libs"
        )
        paddle_libs.mkdir(parents=True)
        (paddle_libs / "libmklml_intel.so").write_text("", encoding="utf-8")
        (runtime / "python" / "bin" / "python3").write_text("", encoding="utf-8")
        (runtime / "runtime-manifest.json").write_text(
            json.dumps(
                {
                    "manifestVersion": 1,
                    "portableRelease": True,
                    "pythonExecutable": "python/bin/python3",
                    "pythonPathEntries": ["cli-src"],
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

            with mock.patch.object(
                MODULE.subprocess,
                "run",
                return_value=subprocess.CompletedProcess(args=[], returncode=0),
            ) as run:
                summary = MODULE.validate_payload_root(payload_root)

            self.assertEqual(summary["payloadRoot"], str(payload_root))
            self.assertIn("runtime", summary)
            self.assertIn("models", summary)
            self.assertIn("ocrReaderSmoke", summary)
            self.assertIn("legal", summary)
            env = run.call_args.kwargs["env"]
            self.assertIn(
                str(payload_root / "app" / "resources" / "runtime" / "cli-src"),
                env["PYTHONPATH"],
            )
            if sys.platform.startswith("linux"):
                self.assertIn(
                    str(
                        payload_root
                        / "app"
                        / "resources"
                        / "runtime"
                        / "python"
                        / "lib"
                        / "python3.12"
                        / "site-packages"
                        / "paddle"
                        / "libs"
                    ),
                    env["LD_LIBRARY_PATH"],
                )
            self.assertNotIn("PYTHONHOME", env)

    def test_validate_payload_root_rejects_missing_runtime(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            payload_root = Path(tmp_dir) / "payload"
            payload_root.mkdir()

            with self.assertRaisesRegex(MODULE.VerificationError, "missing a valid runtime"):
                MODULE.validate_payload_root(payload_root)

    def test_validate_packaged_ocr_reader_reports_smoke_failure(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            payload_root = Path(tmp_dir) / "payload"
            self._write_payload_layout(payload_root)
            runtime_root = payload_root / "app" / "resources" / "runtime"
            model_root = payload_root / "app" / "resources" / "models" / "paddle"

            with (
                mock.patch.object(
                    MODULE.subprocess,
                    "run",
                    return_value=subprocess.CompletedProcess(
                        args=[],
                        returncode=1,
                        stdout="",
                        stderr="paddleocr backend unavailable",
                    ),
                ),
                self.assertRaisesRegex(
                    MODULE.VerificationError,
                    "paddleocr backend unavailable",
                ),
            ):
                MODULE.validate_packaged_ocr_reader(runtime_root, model_root)

    def test_secret_marker_scan_allows_huggingface_auth_source(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            auth_source = (
                Path(tmp_dir)
                / "runtime"
                / "python"
                / "site-packages"
                / "huggingface_hub"
                / "cli"
                / "auth.py"
            )
            auth_source.parent.mkdir(parents=True)
            auth_source.write_text('"Authorization: Bearer {token}"\n', encoding="utf-8")

            MODULE.scan_for_secret_markers([Path(tmp_dir)])

    def test_secret_marker_scan_rejects_bearer_marker_elsewhere(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            marker_file = Path(tmp_dir) / "runtime" / "config.py"
            marker_file.parent.mkdir(parents=True)
            marker_file.write_text('"Authorization: Bearer {token}"\n', encoding="utf-8")

            with self.assertRaisesRegex(MODULE.VerificationError, "Authorization: Bearer"):
                MODULE.scan_for_secret_markers([Path(tmp_dir)])


if __name__ == "__main__":
    unittest.main()
