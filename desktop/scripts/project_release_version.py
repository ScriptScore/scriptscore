#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
from __future__ import annotations

import argparse
import json
import re
import shutil
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


class VersionProjectionError(ValueError):
    pass


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
        else:
            raise AssertionError(args.command)
    except VersionProjectionError as err:
        raise SystemExit(f"error: {err}") from err
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
