#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
from __future__ import annotations

import argparse
import json
import re
import shutil
import tomllib
from dataclasses import asdict, dataclass
from pathlib import Path


VERSION_RE = re.compile(
    r"^(?P<major>0|[1-9]\d*)\."
    r"(?P<minor>0|[1-9]\d*)\."
    r"(?P<patch>0|[1-9]\d*)"
    r"(?:-(?P<channel>alpha|beta|rc|latest)\.(?P<ordinal>0|[1-9]\d*))?$"
)
CHANNEL_OFFSETS = {
    "alpha": 1000,
    "beta": 2000,
    "rc": 3000,
    "latest": 4000,
}
MAX_MSI_PRERELEASE = 65535
RELEASE_CHANNELS = {"latest", "rc"}
PACKAGE_SUFFIXES = {".AppImage", ".deb", ".dmg", ".exe", ".msi", ".rpm"}
RELEASE_BRANCH_RE = re.compile(r"^release/(?P<version>\d+\.\d+\.\d+)$")
CARGO_VERSION_RE = re.compile(r'^version\s*=\s*"(?P<version>[^"]+)"\s*$', re.MULTILINE)
INIT_VERSION_RE = re.compile(r'^__version__\s*=\s*"(?P<version>[^"]+)"\s*$', re.MULTILINE)
UV_LOCK_PACKAGE_RE = re.compile(
    r'^\[\[package\]\]\s*\nname = "scriptscore"\s*\nversion = "(?P<version>[^"]+)"',
    re.MULTILINE,
)


class VersionProjectionError(ValueError):
    pass


@dataclass(frozen=True)
class ReleaseBranchVersionMismatch:
    path: str
    field: str
    expected: str
    actual: str


@dataclass(frozen=True)
class ReleaseMetadata:
    public_version: str
    msi_version: str
    release_tag: str
    latest_tag: str
    release_title: str
    asset_prefix: str
    prerelease: bool
    channel: str
    base_version: str
    run_number: int
    source_sha: str
    source_ref: str


def project_msi_version(version: str) -> str:
    match = VERSION_RE.fullmatch(version)
    if match is None:
        raise VersionProjectionError(
            "expected a stable version or alpha/beta/rc/latest prerelease like 0.1.0-rc.1"
        )

    base = ".".join(match.group(name) for name in ("major", "minor", "patch"))
    channel = match.group("channel")
    if channel is None:
        return base

    ordinal = int(match.group("ordinal"))
    if ordinal < 1:
        raise VersionProjectionError("prerelease ordinal must be at least 1")

    projected = CHANNEL_OFFSETS[channel] + ordinal
    if projected > MAX_MSI_PRERELEASE:
        raise VersionProjectionError(
            f"projected MSI prerelease {projected} exceeds {MAX_MSI_PRERELEASE}"
        )
    return f"{base}-{projected}"


def stable_base_version(version: str) -> str:
    match = VERSION_RE.fullmatch(version)
    if match is None or match.group("channel") is not None:
        raise VersionProjectionError("base version must be a stable semver like 0.1.0")
    return version


def resolve_release_metadata(
    *,
    base_version: str,
    channel: str,
    run_number: int,
    source_sha: str = "",
    source_ref: str = "",
) -> ReleaseMetadata:
    base = stable_base_version(base_version)
    if channel not in RELEASE_CHANNELS:
        raise VersionProjectionError(
            f"release channel must be one of: {', '.join(sorted(RELEASE_CHANNELS))}"
        )
    if run_number < 1:
        raise VersionProjectionError("run number must be at least 1")

    public_version = f"{base}-{channel}.{run_number}"
    msi_version = project_msi_version(public_version)
    return ReleaseMetadata(
        public_version=public_version,
        msi_version=msi_version,
        release_tag="ci/latest" if channel == "latest" else f"ci/{channel}/{public_version}",
        latest_tag="ci/latest" if channel == "latest" else f"ci/{channel}/latest",
        release_title=f"ScriptScore {public_version}",
        asset_prefix=f"ScriptScore-Desktop-{public_version}",
        prerelease=True,
        channel=channel,
        base_version=base,
        run_number=run_number,
        source_sha=source_sha,
        source_ref=source_ref,
    )


def tauri_config_version(config_path: Path) -> str:
    config = json.loads(config_path.read_text(encoding="utf-8"))
    version = config.get("version")
    if not isinstance(version, str):
        raise VersionProjectionError(f"{config_path} does not contain a string version")
    return version


def write_tauri_config_with_version(source_path: Path, output_path: Path, version: str) -> None:
    config = json.loads(source_path.read_text(encoding="utf-8"))
    if not isinstance(config.get("version"), str):
        raise VersionProjectionError(f"{source_path} does not contain a string version")
    config["version"] = version
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(config, indent=2) + "\n", encoding="utf-8")


def desktop_asset_name(version: str, label: str, suffix: str) -> str | None:
    names = {
        ("macos-intel", ".dmg"): f"ScriptScore-Desktop-{version}-macos-intel.dmg",
        ("macos-arm64", ".dmg"): f"ScriptScore-Desktop-{version}-macos-arm64.dmg",
        ("windows-x64", ".exe"): f"ScriptScore-Desktop-{version}-windows-x64-setup.exe",
        ("windows-x64", ".msi"): f"ScriptScore-Desktop-{version}-windows-x64.msi",
        ("linux-x64", ".AppImage"): f"ScriptScore-Desktop-{version}-linux-x64.AppImage",
        ("linux-x64", ".deb"): f"ScriptScore-Desktop-{version}-linux-x64.deb",
        ("linux-x64", ".rpm"): f"ScriptScore-Desktop-{version}-linux-x64.rpm",
    }
    return names.get((label, suffix))


def expected_desktop_asset_names(version: str, label: str, bundles: str) -> set[str]:
    selected = [bundle.strip() for bundle in bundles.split(",") if bundle.strip()]
    names: set[str] = set()
    for bundle in selected:
        suffix = ".exe" if bundle == "nsis" else f".{bundle}"
        if bundle == "appimage":
            suffix = ".AppImage"
        name = desktop_asset_name(version, label, suffix)
        if name is not None:
            names.add(name)
    return names


def stage_desktop_assets(
    *, version: str, label: str, bundles: str, bundle_root: Path, output_dir: Path
) -> list[str]:
    package_dirs = {"appimage", "deb", "dmg", "msi", "nsis", "rpm"}
    if output_dir.exists():
        shutil.rmtree(output_dir)
    output_dir.mkdir(parents=True)

    staged: dict[str, Path] = {}
    for package_file in sorted(bundle_root.rglob("*")):
        if not package_file.is_file() or package_file.parent.name.lower() not in package_dirs:
            continue
        suffix = package_file.suffix
        if suffix.lower() != ".appimage":
            suffix = suffix.lower()
        destination_name = desktop_asset_name(version, label, suffix)
        if destination_name is None:
            continue
        if destination_name in staged:
            raise VersionProjectionError(
                f"multiple package files map to {destination_name}: "
                f"{staged[destination_name]} and {package_file}"
            )
        staged[destination_name] = package_file
        shutil.copy2(package_file, output_dir / destination_name)

    missing = sorted(expected_desktop_asset_names(version, label, bundles).difference(staged))
    if missing:
        raise VersionProjectionError(f"missing staged release asset(s): {', '.join(missing)}")

    return sorted(staged)


def parse_release_branch(source_ref: str) -> str | None:
    match = RELEASE_BRANCH_RE.fullmatch(source_ref.strip())
    if match is None:
        return None
    return match.group("version")


def _read_cargo_package_version(path: Path, package_name: str) -> str:
    if path.name == "Cargo.toml":
        text = path.read_text(encoding="utf-8")
        match = CARGO_VERSION_RE.search(text)
        if match is None:
            raise VersionProjectionError(f"{path} does not contain a string version")
        return match.group("version")

    text = path.read_text(encoding="utf-8")
    pattern = re.compile(
        rf'^\[\[package\]\]\s*\nname = "{re.escape(package_name)}"\s*\nversion = "(?P<version>[^"]+)"',
        re.MULTILINE,
    )
    match = pattern.search(text)
    if match is None:
        raise VersionProjectionError(f"{path} does not contain package {package_name}")
    return match.group("version")


def _read_uv_lock_scriptscore_version(path: Path) -> str:
    text = path.read_text(encoding="utf-8")
    match = UV_LOCK_PACKAGE_RE.search(text)
    if match is None:
        raise VersionProjectionError(f"{path} does not contain scriptscore package version")
    return match.group("version")


def _read_init_version(path: Path) -> str:
    text = path.read_text(encoding="utf-8")
    match = INIT_VERSION_RE.search(text)
    if match is None:
        raise VersionProjectionError(f"{path} does not contain __version__")
    return match.group("version")


def _read_pyproject_version(path: Path) -> str:
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    version = data.get("project", {}).get("version")
    if not isinstance(version, str):
        raise VersionProjectionError(f"{path} does not contain [project].version")
    return version


def _read_package_lock_root_version(path: Path) -> tuple[str, str]:
    data = json.loads(path.read_text(encoding="utf-8"))
    root_version = data.get("version")
    packages = data.get("packages", {})
    workspace_version = packages.get("", {}).get("version") if isinstance(packages, dict) else None
    if not isinstance(root_version, str):
        raise VersionProjectionError(f"{path} does not contain root version")
    if not isinstance(workspace_version, str):
        raise VersionProjectionError(f'{path} does not contain packages[""].version')
    return root_version, workspace_version


def collect_release_branch_version_mismatches(
    repo_root: Path, branch_version: str
) -> list[ReleaseBranchVersionMismatch]:
    python_dev_version = f"{branch_version}.dev0"
    semver_dev_version = f"{branch_version}-dev.0"
    checks: list[tuple[str, str, str, str]] = []

    tauri_path = repo_root / "desktop/src-tauri/tauri.conf.json"
    actual = tauri_config_version(tauri_path)
    checks.append((str(tauri_path.relative_to(repo_root)), "version", branch_version, actual))

    pyproject_path = repo_root / "cli/pyproject.toml"
    actual = _read_pyproject_version(pyproject_path)
    checks.append((str(pyproject_path.relative_to(repo_root)), "[project].version", python_dev_version, actual))

    init_path = repo_root / "cli/src/scriptscore/__init__.py"
    actual = _read_init_version(init_path)
    checks.append((str(init_path.relative_to(repo_root)), "__version__", python_dev_version, actual))

    uv_lock_path = repo_root / "cli/uv.lock"
    actual = _read_uv_lock_scriptscore_version(uv_lock_path)
    checks.append(
        (str(uv_lock_path.relative_to(repo_root)), '[[package]] name = "scriptscore"', python_dev_version, actual)
    )

    cargo_toml_path = repo_root / "desktop/src-tauri/Cargo.toml"
    actual = _read_cargo_package_version(cargo_toml_path, "scriptscore-desktop-host")
    checks.append((str(cargo_toml_path.relative_to(repo_root)), "version", semver_dev_version, actual))

    cargo_lock_path = repo_root / "desktop/src-tauri/Cargo.lock"
    actual = _read_cargo_package_version(cargo_lock_path, "scriptscore-desktop-host")
    checks.append(
        (
            str(cargo_lock_path.relative_to(repo_root)),
            '[[package]] name = "scriptscore-desktop-host"',
            semver_dev_version,
            actual,
        )
    )

    package_json_path = repo_root / "desktop/frontend/package.json"
    package_json = json.loads(package_json_path.read_text(encoding="utf-8"))
    actual = package_json.get("version")
    if not isinstance(actual, str):
        raise VersionProjectionError(f"{package_json_path} does not contain a string version")
    checks.append((str(package_json_path.relative_to(repo_root)), "version", semver_dev_version, actual))

    package_lock_path = repo_root / "desktop/frontend/package-lock.json"
    root_version, workspace_version = _read_package_lock_root_version(package_lock_path)
    rel = str(package_lock_path.relative_to(repo_root))
    checks.append((rel, "version", semver_dev_version, root_version))
    checks.append((rel, 'packages[""].version', semver_dev_version, workspace_version))

    mismatches: list[ReleaseBranchVersionMismatch] = []
    for path, field, expected, actual_value in checks:
        if actual_value != expected:
            mismatches.append(
                ReleaseBranchVersionMismatch(
                    path=path,
                    field=field,
                    expected=expected,
                    actual=actual_value,
                )
            )
    return mismatches


def validate_release_branch_versions(repo_root: Path, source_ref: str) -> None:
    branch_version = parse_release_branch(source_ref)
    if branch_version is None:
        raise VersionProjectionError(
            f"source ref {source_ref!r} is not a release branch like release/0.1.0"
        )

    mismatches = collect_release_branch_version_mismatches(repo_root, branch_version)
    if not mismatches:
        return

    lines = [
        (
            f"- {item.path} ({item.field}): expected {item.expected!r}, "
            f"found {item.actual!r}"
        )
        for item in mismatches
    ]
    raise VersionProjectionError(
        "release branch version metadata does not match "
        f"release/{branch_version}:\n" + "\n".join(lines)
    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    resolve = subparsers.add_parser("resolve")
    resolve.add_argument("--base-version", required=True)
    resolve.add_argument("--channel", choices=sorted(RELEASE_CHANNELS), required=True)
    resolve.add_argument("--run-number", type=int, required=True)
    resolve.add_argument("--source-sha", default="")
    resolve.add_argument("--source-ref", default="")

    msi_version = subparsers.add_parser("msi-version")
    msi_version.add_argument("version")

    tauri_version = subparsers.add_parser("tauri-version")
    tauri_version.add_argument("config", type=Path)

    write_tauri_config = subparsers.add_parser("write-tauri-config")
    write_tauri_config.add_argument("--source", type=Path, required=True)
    write_tauri_config.add_argument("--output", type=Path, required=True)
    write_tauri_config.add_argument("--version", required=True)

    stage_assets = subparsers.add_parser("stage-assets")
    stage_assets.add_argument("--version", required=True)
    stage_assets.add_argument("--label", required=True)
    stage_assets.add_argument("--bundles", required=True)
    stage_assets.add_argument("--bundle-root", type=Path, required=True)
    stage_assets.add_argument("--output-dir", type=Path, required=True)

    validate_release_branch = subparsers.add_parser("validate-release-branch")
    validate_release_branch.add_argument("--source-ref", required=True)
    validate_release_branch.add_argument("--repo-root", type=Path, default=Path("."))

    return parser


def main() -> int:
    args = build_parser().parse_args()
    try:
        if args.command == "resolve":
            metadata = resolve_release_metadata(
                base_version=args.base_version,
                channel=args.channel,
                run_number=args.run_number,
                source_sha=args.source_sha,
                source_ref=args.source_ref,
            )
            print(json.dumps(asdict(metadata), sort_keys=True))
        elif args.command == "msi-version":
            print(project_msi_version(args.version))
        elif args.command == "tauri-version":
            print(tauri_config_version(args.config))
        elif args.command == "write-tauri-config":
            write_tauri_config_with_version(args.source, args.output, args.version)
        elif args.command == "stage-assets":
            staged = stage_desktop_assets(
                version=args.version,
                label=args.label,
                bundles=args.bundles,
                bundle_root=args.bundle_root,
                output_dir=args.output_dir,
            )
            for asset_name in staged:
                print(asset_name)
        elif args.command == "validate-release-branch":
            validate_release_branch_versions(args.repo_root.resolve(), args.source_ref)
        else:
            raise AssertionError(args.command)
    except VersionProjectionError as err:
        raise SystemExit(f"error: {err}") from err
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
