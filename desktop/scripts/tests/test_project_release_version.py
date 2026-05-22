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
        self.assertEqual(MODULE.project_msi_version("0.1.0-latest.42"), "0.1.0-4042")

    def test_project_msi_version_rejects_non_numeric_or_unknown_prereleases(self) -> None:
        for version in ("0.1.0-preview.1", "0.1.0-rc.one", "0.1.0-rc.0"):
            with self.subTest(version=version):
                with self.assertRaises(MODULE.VersionProjectionError):
                    MODULE.project_msi_version(version)

    def test_project_msi_version_rejects_projection_above_msi_limit(self) -> None:
        with self.assertRaisesRegex(MODULE.VersionProjectionError, "exceeds"):
            MODULE.project_msi_version("0.1.0-latest.61536")

    def test_resolve_release_metadata_generates_latest_from_run_number(self) -> None:
        metadata = MODULE.resolve_release_metadata(
            base_version="0.1.0",
            channel="latest",
            run_number=123,
            source_sha="abc123",
            source_ref="main",
        )

        self.assertEqual(metadata.public_version, "0.1.0-latest.123")
        self.assertEqual(metadata.msi_version, "0.1.0-4123")
        self.assertEqual(metadata.release_tag, "ci/latest")
        self.assertEqual(metadata.latest_tag, "ci/latest")
        self.assertEqual(metadata.asset_prefix, "ScriptScore-Desktop-0.1.0-latest.123")
        self.assertTrue(metadata.prerelease)
        self.assertEqual(metadata.source_sha, "abc123")
        self.assertEqual(metadata.source_ref, "main")

    def test_resolve_release_metadata_generates_rc_from_run_number(self) -> None:
        metadata = MODULE.resolve_release_metadata(
            base_version="0.1.0",
            channel="rc",
            run_number=7,
        )

        self.assertEqual(metadata.public_version, "0.1.0-rc.7")
        self.assertEqual(metadata.msi_version, "0.1.0-3007")
        self.assertEqual(metadata.release_tag, "ci/rc/0.1.0-rc.7")
        self.assertEqual(metadata.latest_tag, "ci/rc/latest")

    def test_resolve_release_metadata_rejects_prerelease_base_version(self) -> None:
        with self.assertRaisesRegex(MODULE.VersionProjectionError, "base version"):
            MODULE.resolve_release_metadata(
                base_version="0.1.0-rc.1",
                channel="rc",
                run_number=1,
            )

    def test_resolve_release_metadata_rejects_zero_run_number(self) -> None:
        with self.assertRaisesRegex(MODULE.VersionProjectionError, "run number"):
            MODULE.resolve_release_metadata(
                base_version="0.1.0",
                channel="latest",
                run_number=0,
            )

    def test_expected_desktop_asset_names_uses_public_version(self) -> None:
        self.assertEqual(
            MODULE.expected_desktop_asset_names(
                "0.1.0-latest.123", "linux-x64", "appimage,deb,rpm"
            ),
            {
                "ScriptScore-Desktop-0.1.0-latest.123-linux-x64.AppImage",
                "ScriptScore-Desktop-0.1.0-latest.123-linux-x64.deb",
                "ScriptScore-Desktop-0.1.0-latest.123-linux-x64.rpm",
            },
        )
        self.assertEqual(
            MODULE.expected_desktop_asset_names("0.1.0-rc.7", "windows-x64", "nsis,msi"),
            {
                "ScriptScore-Desktop-0.1.0-rc.7-windows-x64-setup.exe",
                "ScriptScore-Desktop-0.1.0-rc.7-windows-x64.msi",
            },
        )

    def test_stage_desktop_assets_renames_built_packages(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            bundle_root = root / "target"
            output_dir = root / "release-assets"
            for directory, name in (
                ("appimage", "ScriptScore Desktop_0.1.0_amd64.AppImage"),
                ("deb", "scriptscore-desktop_0.1.0_amd64.deb"),
                ("rpm", "scriptscore-desktop-0.1.0.x86_64.rpm"),
            ):
                package_dir = bundle_root / "release" / "bundle" / directory
                package_dir.mkdir(parents=True)
                (package_dir / name).write_text("package", encoding="utf-8")

            staged = MODULE.stage_desktop_assets(
                version="0.1.0-latest.123",
                label="linux-x64",
                bundles="appimage,deb,rpm",
                bundle_root=bundle_root,
                output_dir=output_dir,
            )

            self.assertEqual(
                staged,
                [
                    "ScriptScore-Desktop-0.1.0-latest.123-linux-x64.AppImage",
                    "ScriptScore-Desktop-0.1.0-latest.123-linux-x64.deb",
                    "ScriptScore-Desktop-0.1.0-latest.123-linux-x64.rpm",
                ],
            )
            self.assertEqual(
                sorted(path.name for path in output_dir.iterdir()),
                staged,
            )

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

    def test_parse_release_branch_accepts_release_semver(self) -> None:
        self.assertEqual(MODULE.parse_release_branch("release/0.1.1"), "0.1.1")

    def test_parse_release_branch_rejects_non_release_refs(self) -> None:
        for source_ref in ("main", "release/0.1", "release/0.1.0-rc.1", "feature/foo"):
            with self.subTest(source_ref=source_ref):
                self.assertIsNone(MODULE.parse_release_branch(source_ref))

    def _write_release_branch_tree(self, root: Path, branch_version: str) -> None:
        python_dev = f"{branch_version}.dev0"
        semver_dev = f"{branch_version}-dev.0"
        (root / "desktop/src-tauri").mkdir(parents=True)
        (root / "cli/src/scriptscore").mkdir(parents=True)
        (root / "desktop/frontend").mkdir(parents=True)

        (root / "desktop/src-tauri/tauri.conf.json").write_text(
            json.dumps({"version": branch_version}) + "\n",
            encoding="utf-8",
        )
        (root / "cli/pyproject.toml").write_text(
            f'[project]\nname = "scriptscore"\nversion = "{python_dev}"\n',
            encoding="utf-8",
        )
        (root / "cli/src/scriptscore/__init__.py").write_text(
            f'__version__ = "{python_dev}"\n',
            encoding="utf-8",
        )
        (root / "cli/uv.lock").write_text(
            f'[[package]]\nname = "scriptscore"\nversion = "{python_dev}"\n',
            encoding="utf-8",
        )
        (root / "desktop/src-tauri/Cargo.toml").write_text(
            f'[package]\nname = "scriptscore-desktop-host"\nversion = "{semver_dev}"\n',
            encoding="utf-8",
        )
        (root / "desktop/src-tauri/Cargo.lock").write_text(
            f'[[package]]\nname = "scriptscore-desktop-host"\nversion = "{semver_dev}"\n',
            encoding="utf-8",
        )
        (root / "desktop/frontend/package.json").write_text(
            json.dumps({"version": semver_dev}) + "\n",
            encoding="utf-8",
        )
        (root / "desktop/frontend/package-lock.json").write_text(
            json.dumps({"version": semver_dev, "packages": {"": {"version": semver_dev}}}) + "\n",
            encoding="utf-8",
        )

    def test_validate_release_branch_versions_passes_matching_tree(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            self._write_release_branch_tree(root, "0.1.1")
            MODULE.validate_release_branch_versions(root, "release/0.1.1")

    def test_validate_release_branch_versions_reports_multiple_mismatches(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            self._write_release_branch_tree(root, "0.1.1")
            (root / "desktop/src-tauri/tauri.conf.json").write_text(
                json.dumps({"version": "9.9.9"}) + "\n",
                encoding="utf-8",
            )
            (root / "cli/pyproject.toml").write_text(
                '[project]\nname = "scriptscore"\nversion = "0.0.0.dev0"\n',
                encoding="utf-8",
            )

            with self.assertRaisesRegex(MODULE.VersionProjectionError, "does not match"):
                MODULE.validate_release_branch_versions(root, "release/0.1.1")

            mismatches = MODULE.collect_release_branch_version_mismatches(root, "0.1.1")
            paths = {item.path for item in mismatches}
            self.assertIn("desktop/src-tauri/tauri.conf.json", paths)
            self.assertIn("cli/pyproject.toml", paths)
            self.assertGreaterEqual(len(mismatches), 2)

    def test_collect_release_branch_version_mismatches_flags_package_lock_fields(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            self._write_release_branch_tree(root, "0.2.0")
            package_lock = json.loads(
                (root / "desktop/frontend/package-lock.json").read_text(encoding="utf-8")
            )
            package_lock["packages"][""]["version"] = "0.2.0-dev.9"
            (root / "desktop/frontend/package-lock.json").write_text(
                json.dumps(package_lock) + "\n",
                encoding="utf-8",
            )

            mismatches = MODULE.collect_release_branch_version_mismatches(root, "0.2.0")
            fields = {(item.path, item.field) for item in mismatches}
            self.assertIn(("desktop/frontend/package-lock.json", 'packages[""].version'), fields)


if __name__ == "__main__":
    unittest.main()
