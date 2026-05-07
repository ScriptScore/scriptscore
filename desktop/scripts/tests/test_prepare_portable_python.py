# SPDX-License-Identifier: AGPL-3.0-only
from __future__ import annotations

import importlib.util
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


MODULE_PATH = Path(__file__).resolve().parents[1] / "prepare_portable_python.py"
SPEC = importlib.util.spec_from_file_location("prepare_portable_python", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class PreparePortablePythonTests(unittest.TestCase):
    def test_default_target_triple_prefers_gnu_builds_on_linux(self) -> None:
        self.assertEqual(
            MODULE.default_target_triple("Linux", "x86_64"),
            "x86_64-unknown-linux-gnu",
        )
        self.assertEqual(
            MODULE.default_target_triple("Linux", "arm64"),
            "aarch64-unknown-linux-gnu",
        )

    def test_select_archive_source_prefers_matching_stripped_install_only_asset(self) -> None:
        metadata = {
            "assets": [
                {
                    "browser_download_url": (
                        "https://example.test/cpython-3.12.9+20260211-"
                        "x86_64-unknown-linux-musl-install_only.tar.gz"
                    )
                },
                {
                    "browser_download_url": (
                        "https://example.test/cpython-3.12.9+20260211-"
                        "x86_64-unknown-linux-gnu-install_only.tar.gz"
                    )
                },
                {
                    "browser_download_url": (
                        "https://example.test/cpython-3.12.9+20260211-"
                        "x86_64-unknown-linux-gnu-install_only_stripped.tar.gz"
                    )
                },
            ]
        }

        selected = MODULE.select_archive_source_from_metadata(
            json.dumps(metadata),
            python_version="3.12",
            target_triple="x86_64-unknown-linux-gnu",
        )

        self.assertTrue(selected.endswith("install_only_stripped.tar.gz"))

    def test_cached_root_matches_marker_and_interpreter(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            (root / "bin").mkdir()
            (root / "bin" / "python3").write_text("", encoding="utf-8")
            MODULE.write_marker(
                root,
                MODULE.PortablePythonMarker(
                    python_version="3.12",
                    target_triple="x86_64-unknown-linux-gnu",
                    requirements_sha256="abc123",
                    archive_source="https://example.test/python.tar.gz",
                    torch_backend="cpu",
                ),
            )

            self.assertTrue(
                MODULE.cached_root_matches(
                    root,
                    python_version="3.12",
                    target_triple="x86_64-unknown-linux-gnu",
                    requirements_sha256="abc123",
                    torch_backend="cpu",
                )
            )
            self.assertFalse(
                MODULE.cached_root_matches(
                    root,
                    python_version="3.12",
                    target_triple="x86_64-unknown-linux-gnu",
                    requirements_sha256="different",
                    torch_backend="cpu",
                )
            )

    def test_extracted_portable_root_promotes_python_subdirectory(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            extract_dir = Path(tmp_dir)
            (extract_dir / "python" / "bin").mkdir(parents=True)
            (extract_dir / "python" / "bin" / "python3").write_text("", encoding="utf-8")

            root = MODULE.extracted_portable_root(extract_dir)

            self.assertEqual(root, extract_dir / "python")

    def test_find_portable_root_with_python_accepts_uv_install_subdirectory(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            install_dir = Path(tmp_dir)
            managed_root = install_dir / "cpython-3.12.13-linux-x86_64-gnu"
            (managed_root / "bin").mkdir(parents=True)
            (managed_root / "bin" / "python3").write_text("", encoding="utf-8")

            root = MODULE.find_portable_root_with_python(install_dir)

            self.assertEqual(root, managed_root)

    def test_install_locked_requirements_allows_uv_managed_python(self) -> None:
        python_path = Path("/portable/bin/python3")
        requirements_path = Path("/tmp/runtime-requirements.txt")
        with mock.patch.object(MODULE.subprocess, "run") as run:
            MODULE.install_locked_requirements(
                python_path,
                requirements_path,
                "cpu",
            )

        run.assert_called_once_with(
            [
                "uv",
                "pip",
                "install",
                "--python",
                str(python_path),
                "--break-system-packages",
                "--torch-backend",
                "cpu",
                "-r",
                str(requirements_path),
            ],
            check=True,
            env=mock.ANY,
        )

    def test_filter_portable_runtime_requirements_removes_gpu_torch_helpers(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            requirements_path = Path(tmp_dir) / "requirements.txt"
            requirements_path.write_text(
                "\n".join(
                    [
                        "# generated",
                        "torch==2.11.0",
                        "    # via scriptscore",
                        "nvidia-cublas==13.1.0.3 ; sys_platform == 'linux'",
                        "    # via torch",
                        "cuda-toolkit==13.0.2 ; sys_platform == 'linux'",
                        "    # via torch",
                        "triton==3.6.0 ; sys_platform == 'linux'",
                        "    # via torch",
                        "torchvision==0.26.0",
                        "    # via easyocr",
                        "",
                    ]
                ),
                encoding="utf-8",
            )

            MODULE.filter_portable_runtime_requirements(requirements_path)

            filtered = requirements_path.read_text(encoding="utf-8")
            self.assertIn("torch==2.11.0", filtered)
            self.assertIn("torchvision==0.26.0", filtered)
            self.assertNotIn("nvidia-cublas", filtered)
            self.assertNotIn("cuda-toolkit", filtered)
            self.assertNotIn("triton", filtered)
            self.assertNotIn("# via torch\ntriton", filtered)

    @unittest.skipIf(not hasattr(os, "symlink"), "symlink support required")
    def test_publish_staged_root_materializes_uv_symlink(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            base = Path(tmp_dir)
            target = base / "managed" / "cpython-3.12.13-linux-x86_64-gnu"
            target_bin = target / "bin"
            target_bin.mkdir(parents=True)
            (target_bin / "python3").write_text("", encoding="utf-8")
            uv_alias = base / "managed" / "cpython-3.12-linux-x86_64-gnu"
            try:
                uv_alias.symlink_to(target, target_is_directory=True)
            except OSError as exc:
                self.skipTest(f"symlink creation is unavailable: {exc}")

            output = base / "dist" / "portable-python"
            stale_target = base / "gone" / "python"
            output.parent.mkdir(parents=True)
            try:
                output.symlink_to(stale_target, target_is_directory=True)
            except OSError as exc:
                self.skipTest(f"symlink creation is unavailable: {exc}")

            MODULE.publish_staged_root(uv_alias, output)

            self.assertFalse(output.is_symlink())
            self.assertTrue((output / "bin" / "python3").is_file())


if __name__ == "__main__":
    unittest.main()
