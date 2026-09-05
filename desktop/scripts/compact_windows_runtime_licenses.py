#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Compact deeply nested Torch license files before Windows NSIS packaging."""

from __future__ import annotations

import argparse
import os
import shutil
import sys
import zipfile
from dataclasses import dataclass
from pathlib import Path, PureWindowsPath

SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parents[1]
DEFAULT_RUNTIME_ROOT = PROJECT_ROOT / "desktop" / "dist" / "bundled-runtime"
DEFAULT_LEGAL_ROOT = PROJECT_ROOT / "desktop" / "dist" / "legal"
ARCHIVE_NAME = "torch-third-party-license-files.zip"
MAX_WINDOWS_SOURCE_PATH_CHARS = 259
ZIP_TIMESTAMP = (1980, 1, 1, 0, 0, 0)


@dataclass(frozen=True)
class ArchiveResult:
    archive_path: Path
    file_count: int
    source_roots: tuple[str, ...]


def torch_third_party_license_roots(runtime_root: Path) -> list[Path]:
    roots: list[Path] = []
    for candidate in runtime_root.rglob("third_party"):
        if not candidate.is_dir() or candidate.parent.name != "licenses":
            continue
        dist_info_name = candidate.parent.parent.name.lower()
        if dist_info_name.startswith("torch-") and dist_info_name.endswith(".dist-info"):
            roots.append(candidate)
    return sorted(roots)


def archive_entry_name(source_root: Path, source_file: Path) -> str:
    dist_info = source_root.parent.parent
    relative = source_file.relative_to(source_root.parent)
    return (Path(dist_info.name) / "licenses" / relative).as_posix()


def write_archive(archive_path: Path, source_roots: list[Path]) -> int:
    archive_path.parent.mkdir(parents=True, exist_ok=True)
    temporary_path = archive_path.with_name(f".{archive_path.name}.tmp")
    if temporary_path.exists():
        temporary_path.unlink()

    file_count = 0
    try:
        with zipfile.ZipFile(
            temporary_path,
            mode="w",
            compression=zipfile.ZIP_DEFLATED,
            compresslevel=9,
        ) as archive:
            for source_root in source_roots:
                source_files = sorted(path for path in source_root.rglob("*") if path.is_file())
                for source_file in source_files:
                    info = zipfile.ZipInfo(
                        archive_entry_name(source_root, source_file), ZIP_TIMESTAMP
                    )
                    info.compress_type = zipfile.ZIP_DEFLATED
                    info.external_attr = 0o100644 << 16
                    archive.writestr(info, source_file.read_bytes())
                    file_count += 1

        if file_count == 0:
            raise RuntimeError("Torch third-party license directories contained no files")
        with zipfile.ZipFile(temporary_path, mode="r") as archive:
            corrupt_entry = archive.testzip()
            if corrupt_entry is not None:
                raise RuntimeError(f"Generated license archive is corrupt at {corrupt_entry}")
        os.replace(temporary_path, archive_path)
    finally:
        if temporary_path.exists():
            temporary_path.unlink()
    return file_count


def compact_torch_third_party_licenses(runtime_root: Path, legal_root: Path) -> ArchiveResult:
    source_roots = torch_third_party_license_roots(runtime_root)
    archive_path = legal_root / ARCHIVE_NAME
    if not source_roots:
        return ArchiveResult(archive_path=archive_path, file_count=0, source_roots=())

    source_names = tuple(root.parent.parent.name for root in source_roots)
    file_count = write_archive(archive_path, source_roots)
    for source_root in source_roots:
        shutil.rmtree(source_root)
    return ArchiveResult(
        archive_path=archive_path,
        file_count=file_count,
        source_roots=source_names,
    )


def default_windows_bundle_source_root(project_root: PureWindowsPath) -> PureWindowsPath:
    return project_root / "desktop" / "src-tauri" / ".." / "dist" / "bundled-runtime"


def projected_windows_source_path(
    bundle_source_root: PureWindowsPath,
    runtime_relative_path: Path,
) -> PureWindowsPath:
    return bundle_source_root.joinpath(*runtime_relative_path.parts)


def overlong_windows_runtime_paths(
    runtime_root: Path,
    bundle_source_root: PureWindowsPath,
    max_chars: int = MAX_WINDOWS_SOURCE_PATH_CHARS,
) -> list[tuple[Path, int]]:
    overlong: list[tuple[Path, int]] = []
    for source_file in sorted(path for path in runtime_root.rglob("*") if path.is_file()):
        relative = source_file.relative_to(runtime_root)
        projected = projected_windows_source_path(bundle_source_root, relative)
        if len(str(projected)) > max_chars:
            overlong.append((relative, len(str(projected))))
    return overlong


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-root", type=Path, default=DEFAULT_RUNTIME_ROOT)
    parser.add_argument("--legal-root", type=Path, default=DEFAULT_LEGAL_ROOT)
    parser.add_argument(
        "--max-source-path-chars",
        type=int,
        default=MAX_WINDOWS_SOURCE_PATH_CHARS,
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    result = compact_torch_third_party_licenses(args.runtime_root, args.legal_root)
    bundle_source_root = default_windows_bundle_source_root(PureWindowsPath(str(PROJECT_ROOT)))
    overlong = overlong_windows_runtime_paths(
        args.runtime_root,
        bundle_source_root,
        args.max_source_path_chars,
    )
    if overlong:
        for relative, length in overlong:
            print(
                f"error: Windows bundle source path is {length} characters: {relative}",
                file=sys.stderr,
            )
        return 1

    if result.file_count:
        print(
            f"Archived {result.file_count} Torch third-party license files from "
            f"{', '.join(result.source_roots)} to {result.archive_path}"
        )
    else:
        print("No nested Torch third-party license files required compaction")
    print(f"Windows runtime bundle paths are within {args.max_source_path_chars} characters")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
