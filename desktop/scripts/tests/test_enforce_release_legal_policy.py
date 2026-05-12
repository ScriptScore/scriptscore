# SPDX-License-Identifier: AGPL-3.0-only
from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().parents[1] / "enforce_release_legal_policy.py"
SPEC = importlib.util.spec_from_file_location("enforce_release_legal_policy", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class EnforceReleaseLegalPolicyTests(unittest.TestCase):
    def _write_legal_artifacts(
        self,
        legal_root: Path,
        *,
        python_packages: list[str],
        findings: list[dict[str, object]],
    ) -> None:
        legal_root.mkdir(parents=True)
        (legal_root / "license-policy-report.json").write_text(
            json.dumps({"findings": findings}),
            encoding="utf-8",
        )
        (legal_root / "sbom-python.json").write_text(
            json.dumps(
                {
                    "components": [
                        {"name": name, "version": "1.0", "scope": "python-runtime"}
                        for name in python_packages
                    ]
                }
            ),
            encoding="utf-8",
        )
        (legal_root / "sbom-assets.json").write_text(
            json.dumps({"components": []}),
            encoding="utf-8",
        )

    def test_policy_passes_when_release_findings_are_resolved(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            legal_root = Path(tmp_dir) / "legal"
            self._write_legal_artifacts(
                legal_root,
                python_packages=[
                    "opencv-contrib-python-headless",
                    "paddlepaddle",
                    "PyMuPDF",
                    "python-bidi",
                ],
                findings=[
                    {
                        "severity": "review_required",
                        "item": "dev-tool",
                        "source": "npm",
                        "scope": "npm-dev",
                        "message": "non-release review",
                    }
                ],
            )

            MODULE.enforce_release_legal_policy(legal_root)

    def test_policy_rejects_release_review_required_finding(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            legal_root = Path(tmp_dir) / "legal"
            self._write_legal_artifacts(
                legal_root,
                python_packages=["scipy"],
                findings=[
                    {
                        "severity": "review_required",
                        "item": "scipy",
                        "source": "python",
                        "scope": "python-runtime",
                        "message": "GPL/LGPL or custom copyleft terms require release review.",
                    }
                ],
            )

            with self.assertRaisesRegex(MODULE.ReleaseLegalPolicyError, "scipy"):
                MODULE.enforce_release_legal_policy(legal_root)

    def test_policy_rejects_native_library_review_required_finding(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            legal_root = Path(tmp_dir) / "legal"
            self._write_legal_artifacts(
                legal_root,
                python_packages=[],
                findings=[
                    {
                        "severity": "review_required",
                        "item": "desktop/dist/bundled-runtime/python/lib/custom.so",
                        "source": "runtime",
                        "scope": "native-library",
                        "message": "Asset or native binary terms require release review.",
                    }
                ],
            )

            with self.assertRaisesRegex(MODULE.ReleaseLegalPolicyError, "custom.so"):
                MODULE.enforce_release_legal_policy(legal_root)

    def test_policy_rejects_non_headless_opencv_in_python_sbom(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            legal_root = Path(tmp_dir) / "legal"
            self._write_legal_artifacts(
                legal_root,
                python_packages=["opencv-contrib-python", "opencv-contrib-python-headless"],
                findings=[],
            )

            with self.assertRaisesRegex(MODULE.ReleaseLegalPolicyError, "opencv-contrib-python"):
                MODULE.enforce_release_legal_policy(legal_root)

    def test_policy_rejects_crc32c_candidate_exclusion_in_python_sbom(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            legal_root = Path(tmp_dir) / "legal"
            self._write_legal_artifacts(
                legal_root,
                python_packages=["crc32c"],
                findings=[],
            )

            with self.assertRaisesRegex(MODULE.ReleaseLegalPolicyError, "crc32c"):
                MODULE.enforce_release_legal_policy(legal_root)


if __name__ == "__main__":
    unittest.main()
