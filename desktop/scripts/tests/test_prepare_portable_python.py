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
                    package_overrides_sha256="",
                )
            )
            self.assertFalse(
                MODULE.cached_root_matches(
                    root,
                    python_version="3.12",
                    target_triple="x86_64-unknown-linux-gnu",
                    requirements_sha256="different",
                    torch_backend="cpu",
                    package_overrides_sha256="",
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
                "--no-deps",
                "-r",
                str(requirements_path),
            ],
            check=True,
            env=mock.ANY,
        )

    def test_filter_portable_runtime_requirements_removes_blocked_and_gpu_helpers(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            requirements_path = Path(tmp_dir) / "requirements.txt"
            requirements_path.write_text(
                "\n".join(
                    [
                        "# generated",
                        "aistudio-sdk==0.3.8",
                        "    # via paddlex",
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
            self.assertNotIn("aistudio-sdk", filtered)
            self.assertNotIn("# via paddlex", filtered)
            self.assertNotIn("nvidia-cublas", filtered)
            self.assertNotIn("cuda-toolkit", filtered)
            self.assertNotIn("triton", filtered)
            self.assertNotIn("# via torch\ntriton", filtered)

    def test_rewrite_requirement_to_local_wheel_replaces_only_target_requirement(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            tmp_path = Path(tmp_dir)
            wheel_path = tmp_path / MODULE.MACOS_X86_64_PADDLE_WHEEL_NAME
            wheel_path.write_bytes(b"wheel")
            requirements_path = tmp_path / "requirements.txt"
            requirements_path.write_text(
                "\n".join(
                    [
                        "numpy==2.2.0",
                        "paddlepaddle==3.3.1",
                        "    # via scriptscore",
                        "paddleocr==3.4.1",
                        "",
                    ]
                ),
                encoding="utf-8",
            )

            MODULE.rewrite_requirement_to_local_wheel(
                requirements_path,
                "paddlepaddle",
                wheel_path,
                "file:///__scriptscore_package_overrides__/paddlepaddle.whl",
            )

            rewritten = requirements_path.read_text(encoding="utf-8")
            self.assertIn(
                "paddlepaddle @ file:///__scriptscore_package_overrides__/paddlepaddle.whl",
                rewritten,
            )
            self.assertIn("numpy==2.2.0", rewritten)
            self.assertIn("paddleocr==3.4.1", rewritten)
            self.assertNotIn("paddlepaddle==3.3.1", rewritten)
            self.assertNotIn("# via scriptscore", rewritten)

    def test_rewrite_requirement_to_local_wheel_can_append_missing_requirement(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            tmp_path = Path(tmp_dir)
            wheel_path = tmp_path / MODULE.MACOS_X86_64_PADDLE_WHEEL_NAME
            wheel_path.write_bytes(b"wheel")
            requirements_path = tmp_path / "requirements.txt"
            requirements_path.write_text("numpy==2.2.0\npaddleocr==3.4.1\n", encoding="utf-8")

            MODULE.rewrite_requirement_to_local_wheel(
                requirements_path,
                "paddlepaddle",
                wheel_path,
                "file:///__scriptscore_package_overrides__/paddlepaddle.whl",
                append_if_missing=True,
            )

            rewritten = requirements_path.read_text(encoding="utf-8")
            self.assertIn("numpy==2.2.0", rewritten)
            self.assertIn("paddleocr==3.4.1", rewritten)
            self.assertIn(
                "paddlepaddle @ file:///__scriptscore_package_overrides__/paddlepaddle.whl",
                rewritten,
            )

    def test_remove_requirements_drops_target_stack_with_continuations(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            requirements_path = Path(tmp_dir) / "requirements.txt"
            requirements_path.write_text(
                "\n".join(
                    [
                        "easyocr==1.7.2",
                        "    # via scriptscore",
                        "numpy==2.2.0",
                        "torch==2.11.0",
                        "    # via easyocr",
                        "torchvision==0.26.0",
                        "    # via easyocr",
                        "paddleocr==3.4.1",
                        "",
                    ]
                ),
                encoding="utf-8",
            )

            MODULE.remove_requirements(
                requirements_path,
                MODULE.excluded_requirements_for_target(MODULE.MACOS_X86_64_TARGET_TRIPLE),
            )

            filtered = requirements_path.read_text(encoding="utf-8")
            self.assertIn("numpy==2.2.0", filtered)
            self.assertIn("paddleocr==3.4.1", filtered)
            self.assertNotIn("easyocr", filtered)
            self.assertNotIn("torch==", filtered)
            self.assertNotIn("torchvision", filtered)

    def test_force_include_requirements_removes_platform_marker_for_target_deps(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            requirements_path = Path(tmp_dir) / "requirements.txt"
            requirements_path.write_text(
                "\n".join(
                    [
                        "protobuf==7.34.1 ; platform_machine != 'x86_64' or sys_platform != 'darwin'",
                        "    # via paddlepaddle",
                        "networkx==3.6.1 ; platform_machine != 'x86_64' or sys_platform != 'darwin'",
                        "    # via paddlepaddle",
                        "numpy==2.4.3",
                        "",
                    ]
                ),
                encoding="utf-8",
            )

            MODULE.force_include_requirements(
                requirements_path,
                MODULE.forced_requirements_for_target(MODULE.MACOS_X86_64_TARGET_TRIPLE),
            )

            rewritten = requirements_path.read_text(encoding="utf-8")
            self.assertIn("protobuf==7.34.1\n", rewritten)
            self.assertIn("networkx==3.6.1\n", rewritten)
            self.assertNotIn("platform_machine", rewritten)
            self.assertIn("numpy==2.4.3", rewritten)

    def test_macos_x86_paddle_override_verifies_local_wheel_without_persisting_url(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            tmp_path = Path(tmp_dir)
            wheel_path = tmp_path / MODULE.MACOS_X86_64_PADDLE_WHEEL_NAME
            wheel_path.write_bytes(b"release asset bytes")
            wheel_sha256 = MODULE.file_sha256(wheel_path)

            with mock.patch.dict(
                os.environ,
                {
                    "SCRIPTSCORE_MACOS_X86_64_PADDLE_WHEEL_PATH": str(wheel_path),
                    "SCRIPTSCORE_MACOS_X86_64_PADDLE_WHEEL_SHA256": wheel_sha256,
                },
                clear=False,
            ):
                overrides = MODULE.package_overrides_for_target(
                    MODULE.MACOS_X86_64_TARGET_TRIPLE,
                    tmp_path,
                )

            self.assertEqual(len(overrides), 1)
            self.assertEqual(overrides[0].package_name, "paddlepaddle")
            self.assertEqual(overrides[0].wheel_sha256, wheel_sha256)
            self.assertNotIn(str(wheel_path), MODULE.package_overrides_sha256(overrides))

    def test_macos_x86_paddle_override_rejects_sha_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            tmp_path = Path(tmp_dir)
            wheel_path = tmp_path / MODULE.MACOS_X86_64_PADDLE_WHEEL_NAME
            wheel_path.write_bytes(b"release asset bytes")

            with (
                mock.patch.dict(
                    os.environ,
                    {
                        "SCRIPTSCORE_MACOS_X86_64_PADDLE_WHEEL_PATH": str(wheel_path),
                        "SCRIPTSCORE_MACOS_X86_64_PADDLE_WHEEL_SHA256": "0" * 64,
                    },
                    clear=False,
                ),
                self.assertRaisesRegex(MODULE.PortablePythonError, "SHA256 mismatch"),
            ):
                MODULE.package_overrides_for_target(
                    MODULE.MACOS_X86_64_TARGET_TRIPLE,
                    tmp_path,
                )

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
