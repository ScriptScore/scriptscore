#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


VERSION_RE = re.compile(
    r"^(?P<major>0|[1-9]\d*)\."
    r"(?P<minor>0|[1-9]\d*)\."
    r"(?P<patch>0|[1-9]\d*)"
    r"(?:-(?P<channel>alpha|beta|rc)\.(?P<ordinal>0|[1-9]\d*))?$"
)
CHANNEL_OFFSETS = {
    "alpha": 1000,
    "beta": 2000,
    "rc": 3000,
}
MAX_MSI_PRERELEASE = 65535


class VersionProjectionError(ValueError):
    pass


def project_msi_version(version: str) -> str:
    match = VERSION_RE.fullmatch(version)
    if match is None:
        raise VersionProjectionError(
            "expected a stable version or alpha/beta/rc prerelease like 0.1.0-rc.1"
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


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    msi_version = subparsers.add_parser("msi-version")
    msi_version.add_argument("version")

    tauri_version = subparsers.add_parser("tauri-version")
    tauri_version.add_argument("config", type=Path)

    write_tauri_config = subparsers.add_parser("write-tauri-config")
    write_tauri_config.add_argument("--source", type=Path, required=True)
    write_tauri_config.add_argument("--output", type=Path, required=True)
    write_tauri_config.add_argument("--version", required=True)

    return parser


def main() -> int:
    args = build_parser().parse_args()
    try:
        if args.command == "msi-version":
            print(project_msi_version(args.version))
        elif args.command == "tauri-version":
            print(tauri_config_version(args.config))
        elif args.command == "write-tauri-config":
            write_tauri_config_with_version(args.source, args.output, args.version)
        else:
            raise AssertionError(args.command)
    except VersionProjectionError as err:
        raise SystemExit(f"error: {err}") from err
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
