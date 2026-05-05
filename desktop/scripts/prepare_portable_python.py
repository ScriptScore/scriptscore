# SPDX-License-Identifier: AGPL-3.0-only
from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any
import urllib.request


MARKER_FILENAME = ".scriptscore-portable-python.json"
DEFAULT_TORCH_BACKEND = "cpu"
PORTABLE_RUNTIME_EXCLUDED_REQUIREMENT_PREFIXES = (
    "cuda-",
    "nvidia-",
    "triton",
)


class PortablePythonError(RuntimeError):
    """Raised when portable Python preparation fails."""


@dataclass
class PortablePythonMarker:
    python_version: str
    target_triple: str
    requirements_sha256: str
    archive_source: str
    torch_backend: str = DEFAULT_TORCH_BACKEND


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def desktop_root() -> Path:
    return Path(__file__).resolve().parents[1]


def default_output_root() -> Path:
    return desktop_root() / "dist" / "portable-python"


def normalize_machine(machine: str) -> str:
    normalized = machine.lower()
    aliases = {
        "amd64": "x86_64",
        "arm64": "aarch64",
        "arm64e": "aarch64",
    }
    return aliases.get(normalized, normalized)


def default_target_triple(
    system_name: str | None = None,
    machine: str | None = None,
) -> str:
    system_name = system_name or platform.system()
    machine = normalize_machine(machine or platform.machine())

    if system_name == "Linux":
        mapping = {
            "x86_64": "x86_64-unknown-linux-gnu",
            "aarch64": "aarch64-unknown-linux-gnu",
        }
    elif system_name == "Darwin":
        mapping = {
            "x86_64": "x86_64-apple-darwin",
            "aarch64": "aarch64-apple-darwin",
        }
    elif system_name == "Windows":
        mapping = {
            "x86_64": "x86_64-pc-windows-msvc-shared",
            "aarch64": "aarch64-pc-windows-msvc-shared",
        }
    else:
        raise PortablePythonError(
            f"Portable desktop Python auto-prep does not support host OS '{system_name}'."
        )

    if machine not in mapping:
        raise PortablePythonError(
            f"Portable desktop Python auto-prep does not support host architecture '{machine}' on {system_name}."
        )

    return mapping[machine]


def version_regex(version: str) -> str:
    escaped = re.escape(version)
    if version.count(".") == 1:
        return escaped + r"\.\d+"
    return escaped


def archive_filename_pattern(python_version: str, target_triple: str) -> re.Pattern[str]:
    version_part = version_regex(python_version)
    pattern = (
        rf"cpython-{version_part}\+\d{{8}}-"
        rf"{re.escape(target_triple)}-install_only(?:_stripped)?\.tar\.gz$"
    )
    return re.compile(pattern)


def iter_string_values(payload: Any) -> list[str]:
    if isinstance(payload, str):
        return [payload]
    if isinstance(payload, dict):
        values: list[str] = []
        for value in payload.values():
            values.extend(iter_string_values(value))
        return values
    if isinstance(payload, list):
        values = []
        for value in payload:
            values.extend(iter_string_values(value))
        return values
    return []


def select_archive_source_from_metadata(
    metadata_text: str,
    python_version: str,
    target_triple: str,
) -> str:
    payload = json.loads(metadata_text)
    filename_pattern = archive_filename_pattern(python_version, target_triple)
    candidates: list[str] = []
    for value in iter_string_values(payload):
        basename = value.rsplit("/", 1)[-1]
        if filename_pattern.search(basename):
            candidates.append(value)

    if not candidates:
        raise PortablePythonError(
            "Could not find a matching python-build-standalone install_only archive in the latest release metadata "
            f"for Python {python_version} and target {target_triple}. "
            "Set SCRIPTSCORE_DESKTOP_PORTABLE_PYTHON_URL or SCRIPTSCORE_DESKTOP_PORTABLE_PYTHON_ARCHIVE to override."
        )

    candidates.sort(key=lambda value: ("_stripped" not in value, value))
    return candidates[0]


def marker_path(output_root: Path) -> Path:
    return output_root / MARKER_FILENAME


def load_marker(output_root: Path) -> PortablePythonMarker | None:
    path = marker_path(output_root)
    if not path.is_file():
        return None
    payload = json.loads(path.read_text(encoding="utf-8"))
    return PortablePythonMarker(**payload)


def write_marker(output_root: Path, marker: PortablePythonMarker) -> None:
    marker_path(output_root).write_text(
        json.dumps(asdict(marker), indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def subprocess_env() -> dict[str, str]:
    env = os.environ.copy()
    env.setdefault("UV_CACHE_DIR", "/tmp/uv-cache")
    env.setdefault("PIP_CACHE_DIR", "/tmp/scriptscore-pip-cache")
    return env


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def python_executable_for_root(root: Path) -> Path:
    candidates = [
        root / "bin" / "python3",
        root / "bin" / "python",
        root / "python.exe",
        root / "Scripts" / "python.exe",
    ]
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    raise PortablePythonError(
        f"Prepared portable Python root '{root}' does not contain a supported interpreter layout."
    )


def cached_root_matches(
    output_root: Path,
    python_version: str,
    target_triple: str,
    requirements_sha256: str,
    torch_backend: str,
) -> bool:
    marker = load_marker(output_root)
    if marker is None:
        return False
    try:
        python_executable_for_root(output_root)
    except PortablePythonError:
        return False
    return (
        marker.python_version == python_version
        and marker.target_triple == target_triple
        and marker.requirements_sha256 == requirements_sha256
        and marker.torch_backend == torch_backend
    )


def remove_existing_path(path: Path) -> None:
    if path.is_symlink() or path.is_file():
        path.unlink()
    elif path.exists():
        shutil.rmtree(path)


def publish_staged_root(staged_root: Path, output_root: Path) -> None:
    source_root = staged_root.resolve(strict=True)
    remove_existing_path(output_root)
    output_root.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source_root, output_root, symlinks=True)


def export_locked_runtime_requirements(requirements_path: Path) -> None:
    if shutil.which("uv") is None:
        raise PortablePythonError(
            "uv is required to export locked desktop runtime dependencies from cli/uv.lock."
        )

    command = [
        "uv",
        "--directory",
        str(repo_root() / "cli"),
        "export",
        "--format",
        "requirements.txt",
        "--frozen",
        "--no-dev",
        "--no-emit-local",
        "--no-hashes",
        "--output-file",
        str(requirements_path),
    ]
    subprocess.run(command, check=True, env=subprocess_env())
    filter_portable_runtime_requirements(requirements_path)


def requirement_name(requirement_line: str) -> str:
    match = re.match(r"^\s*([A-Za-z0-9_.-]+)", requirement_line)
    if match is None:
        return ""
    return match.group(1).lower().replace("_", "-")


def should_exclude_portable_runtime_requirement(requirement_line: str) -> bool:
    name = requirement_name(requirement_line)
    return any(
        name == prefix.rstrip("-") or name.startswith(prefix)
        for prefix in PORTABLE_RUNTIME_EXCLUDED_REQUIREMENT_PREFIXES
    )


def filter_portable_runtime_requirements(requirements_path: Path) -> None:
    filtered_lines: list[str] = []
    skipping_requirement = False
    for line in requirements_path.read_text(encoding="utf-8").splitlines(keepends=True):
        stripped = line.strip()
        is_requirement = bool(stripped) and not line.startswith((" ", "\t", "#"))
        if is_requirement:
            skipping_requirement = should_exclude_portable_runtime_requirement(line)
        if skipping_requirement:
            continue
        filtered_lines.append(line)
    requirements_path.write_text("".join(filtered_lines), encoding="utf-8")


def install_uv_managed_python(install_dir: Path, python_version: str) -> Path:
    if shutil.which("uv") is None:
        raise PortablePythonError(
            "uv is required to install a managed portable Python runtime."
        )

    command = [
        "uv",
        "python",
        "install",
        "--managed-python",
        "--install-dir",
        str(install_dir),
        python_version,
    ]
    subprocess.run(command, check=True, env=subprocess_env())
    return find_portable_root_with_python(install_dir)


def download_file(url: str, destination: Path) -> None:
    with urllib.request.urlopen(url) as response, destination.open("wb") as handle:
        shutil.copyfileobj(response, handle)


def extract_archive(archive_path: Path, extract_dir: Path) -> None:
    if archive_path.suffixes[-2:] == [".tar", ".gz"]:
        with tarfile.open(archive_path, "r:gz") as archive:
            archive.extractall(extract_dir)
        return
    raise PortablePythonError(
        f"Unsupported portable Python archive format: {archive_path.name}. "
        "Expected a .tar.gz install_only archive."
    )


def extracted_portable_root(extract_dir: Path) -> Path:
    direct_candidates = [extract_dir]
    python_subdir = extract_dir / "python"
    if python_subdir.is_dir():
        direct_candidates.insert(0, python_subdir)

    for candidate in direct_candidates:
        try:
            python_executable_for_root(candidate)
            return candidate
        except PortablePythonError:
            continue

    raise PortablePythonError(
        f"Could not locate a usable portable Python root in extracted archive under '{extract_dir}'."
    )


def find_portable_root_with_python(search_root: Path) -> Path:
    candidates = [search_root]
    candidates.extend(sorted(path for path in search_root.iterdir() if path.is_dir()))

    for candidate in candidates:
        try:
            python_executable_for_root(candidate)
            return candidate
        except PortablePythonError:
            continue

    raise PortablePythonError(
        f"Could not locate a portable Python install root under '{search_root}'."
    )


def install_locked_requirements(
    python_executable: Path,
    requirements_path: Path,
    torch_backend: str,
) -> None:
    command = [
        "uv",
        "pip",
        "install",
        "--python",
        str(python_executable),
        "--break-system-packages",
        "--torch-backend",
        torch_backend,
        "-r",
        str(requirements_path),
    ]
    subprocess.run(command, check=True, env=subprocess_env())


def parse_bool_env(value: str | None) -> bool:
    if value is None:
        return False
    return value.lower() not in {"", "0", "false", "no"}


def prepare_portable_python(
    output_root: Path,
    python_version: str,
    target_triple: str,
    archive_url: str | None,
    archive_path: Path | None,
    torch_backend: str,
    force: bool,
) -> tuple[Path, bool]:
    output_root = output_root.expanduser()
    if not output_root.is_absolute():
        output_root = Path.cwd() / output_root
    with tempfile.TemporaryDirectory(prefix="scriptscore-portable-python-") as tmp_dir:
        tmp_path = Path(tmp_dir)
        requirements_path = tmp_path / "runtime-requirements.txt"
        export_locked_runtime_requirements(requirements_path)
        requirements_sha256 = file_sha256(requirements_path)

        if not force and cached_root_matches(
            output_root,
            python_version,
            target_triple,
            requirements_sha256,
            torch_backend,
        ):
            return output_root, False

        if archive_path is not None:
            resolved_archive = archive_path.resolve()
            archive_source = str(resolved_archive)
            extract_dir = tmp_path / "extract"
            extract_dir.mkdir(parents=True, exist_ok=True)
            extract_archive(resolved_archive, extract_dir)
            staged_root = extracted_portable_root(extract_dir)
        elif archive_url is not None:
            archive_source = archive_url
            resolved_archive = tmp_path / Path(archive_url).name
            download_file(archive_url, resolved_archive)
            extract_dir = tmp_path / "extract"
            extract_dir.mkdir(parents=True, exist_ok=True)
            extract_archive(resolved_archive, extract_dir)
            staged_root = extracted_portable_root(extract_dir)
        else:
            managed_install_dir = tmp_path / "managed-python"
            archive_source = f"uv-managed:{python_version}"
            staged_root = install_uv_managed_python(managed_install_dir, python_version)

        python_executable = python_executable_for_root(staged_root)
        install_locked_requirements(python_executable, requirements_path, torch_backend)

        publish_staged_root(staged_root, output_root)
        write_marker(
            output_root,
            PortablePythonMarker(
                python_version=python_version,
                target_triple=target_triple,
                requirements_sha256=requirements_sha256,
                archive_source=archive_source,
                torch_backend=torch_backend,
            ),
        )
        return output_root, True


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="prepare-portable-python.sh",
        description="Stage a portable Python root for desktop package builds.",
    )
    parser.add_argument(
        "--output-root",
        default=os.environ.get(
            "SCRIPTSCORE_DESKTOP_PORTABLE_PYTHON_DIR",
            str(default_output_root()),
        ),
        help="Directory to populate with the staged portable Python root.",
    )
    parser.add_argument(
        "--python-version",
        default=os.environ.get("SCRIPTSCORE_DESKTOP_PORTABLE_PYTHON_VERSION", "3.12"),
        help="Requested CPython version, usually a minor like 3.12 or an exact patch.",
    )
    parser.add_argument(
        "--target-triple",
        default=os.environ.get("SCRIPTSCORE_DESKTOP_PORTABLE_PYTHON_TARGET"),
        help="python-build-standalone target triple override for explicit archive workflows.",
    )
    parser.add_argument(
        "--archive-url",
        default=os.environ.get("SCRIPTSCORE_DESKTOP_PORTABLE_PYTHON_URL"),
        help="Direct install_only archive URL override.",
    )
    parser.add_argument(
        "--archive-path",
        default=os.environ.get("SCRIPTSCORE_DESKTOP_PORTABLE_PYTHON_ARCHIVE"),
        help="Local install_only archive path override.",
    )
    parser.add_argument(
        "--torch-backend",
        default=os.environ.get(
            "SCRIPTSCORE_DESKTOP_TORCH_BACKEND",
            DEFAULT_TORCH_BACKEND,
        ),
        help="uv torch backend for the portable runtime, default cpu.",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        default=parse_bool_env(os.environ.get("SCRIPTSCORE_DESKTOP_PORTABLE_PYTHON_FORCE")),
        help="Rebuild the staged portable Python root even if a matching cached root exists.",
    )
    parser.add_argument(
        "--print-root",
        action="store_true",
        help="Print only the staged portable Python root path.",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    output_root = Path(args.output_root)
    archive_path = Path(args.archive_path) if args.archive_path else None
    target_triple = args.target_triple or default_target_triple()
    root, rebuilt = prepare_portable_python(
        output_root=output_root,
        python_version=args.python_version,
        target_triple=target_triple,
        archive_url=args.archive_url,
        archive_path=archive_path,
        torch_backend=args.torch_backend,
        force=args.force,
    )

    if args.print_root:
        print(root)
    elif rebuilt:
        print(f"Prepared portable desktop Python at {root}")
        print(f"Python target: {target_triple}")
    else:
        print(f"Reusing portable desktop Python at {root}")
        print(f"Python target: {target_triple}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except PortablePythonError as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
