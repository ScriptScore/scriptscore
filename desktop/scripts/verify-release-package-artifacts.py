#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

BUNDLE_EXTENSIONS = {
    "appimage": [".AppImage"],
    "deb": [".deb"],
    "dmg": [".dmg"],
    "msi": [".msi"],
    "nsis": [".exe"],
    "rpm": [".rpm"],
}
SECRET_MARKERS = (
    "SCRIPTSCORE_MACOS_X86_64_PADDLE_WHEEL_URL",
    "SCRIPTSCORE_MACOS_X86_64_PADDLE_WHEEL_TOKEN",
    "PADDLE_WHEEL_TOKEN",
    "Authorization: Bearer",
)
ALLOWED_SECRET_MARKER_PATHS = {
    "Authorization: Bearer": (Path("huggingface_hub") / "cli" / "auth.py",),
}
FORBIDDEN_RUNTIME_PYTHON_PACKAGES = frozenset(
    {
        "astroid",
        "bce-python-sdk",
        "crc32c",
        "opencv-contrib-python",
        "pip-api",
        "pip-audit",
        "pylint",
    }
)
TEXT_SUFFIXES = {
    ".css",
    ".html",
    ".js",
    ".json",
    ".log",
    ".md",
    ".plist",
    ".rs",
    ".sh",
    ".svg",
    ".toml",
    ".txt",
    ".xml",
    ".yaml",
    ".yml",
}
WINDOWS_PROJECT_ICON = Path("icons/icon.ico")


class VerificationError(RuntimeError):
    """Raised when release package verification fails."""


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def desktop_root() -> Path:
    return Path(__file__).resolve().parents[1]


def tauri_config_path() -> Path:
    return desktop_root() / "src-tauri" / "tauri.conf.json"


def parse_bundles(value: str) -> list[str]:
    bundles = [bundle.strip().lower() for bundle in value.replace(" ", ",").split(",")]
    bundles = [bundle for bundle in bundles if bundle]
    unknown = sorted(set(bundles) - set(BUNDLE_EXTENSIONS))
    if unknown:
        raise VerificationError(f"Unsupported bundle target(s): {', '.join(unknown)}")
    return bundles


def candidate_release_roots(target_triple: str | None) -> list[Path]:
    target_root = desktop_root() / "src-tauri" / "target"
    roots: list[Path] = []
    if target_triple:
        roots.append(target_root / target_triple / "release")
    roots.append(target_root / "release")
    return roots


def find_bundle_root(target_triple: str | None) -> Path:
    for release_root in candidate_release_roots(target_triple):
        bundle_root = release_root / "bundle"
        if bundle_root.is_dir():
            return bundle_root
    expected = ", ".join(str(root / "bundle") for root in candidate_release_roots(target_triple))
    raise VerificationError(f"Could not find Tauri bundle output directory. Checked: {expected}")


def files_for_bundle(bundle_root: Path, bundle: str) -> list[Path]:
    extensions = BUNDLE_EXTENSIONS[bundle]
    return sorted(
        path
        for path in bundle_root.rglob("*")
        if path.is_file() and any(path.name.endswith(extension) for extension in extensions)
    )


def runtime_manifest(runtime_root: Path) -> dict[str, object]:
    manifest_path = runtime_root / "runtime-manifest.json"
    if not manifest_path.is_file():
        raise VerificationError(f"Missing bundled runtime manifest: {manifest_path}")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if not isinstance(manifest, dict):
        raise VerificationError(f"Bundled runtime manifest was not a JSON object: {manifest_path}")
    return manifest


def resolved_runtime_python(runtime_root: Path, manifest: dict[str, object]) -> Path:
    python_executable = manifest.get("pythonExecutable")
    if not isinstance(python_executable, str) or not python_executable:
        raise VerificationError("Bundled runtime manifest does not name a Python executable.")
    resolved_python = Path(python_executable)
    if not resolved_python.is_absolute():
        resolved_python = runtime_root / resolved_python
    if not resolved_python.is_file():
        raise VerificationError(f"Bundled Python executable was not found: {resolved_python}")
    return resolved_python


def runtime_python_path_entries(runtime_root: Path, manifest: dict[str, object]) -> list[Path]:
    entries = manifest.get("pythonPathEntries", [])
    if not isinstance(entries, list):
        raise VerificationError("Bundled runtime manifest pythonPathEntries must be a list.")
    paths: list[Path] = []
    for entry in entries:
        if not isinstance(entry, str) or not entry:
            raise VerificationError(
                "Bundled runtime manifest contained an invalid PYTHONPATH entry."
            )
        path = Path(entry)
        paths.append(path if path.is_absolute() else runtime_root / path)
    return paths


def normalize_python_package_name(name: str) -> str:
    return name.strip().lower().replace("_", "-")


def distribution_name(metadata_dir: Path) -> str | None:
    for metadata_name in ("METADATA", "PKG-INFO"):
        metadata_path = metadata_dir / metadata_name
        if not metadata_path.is_file():
            continue
        try:
            for line in metadata_path.read_text(encoding="utf-8", errors="ignore").splitlines():
                if line.lower().startswith("name:"):
                    return normalize_python_package_name(line.split(":", 1)[1])
        except OSError:
            continue
    return None


def installed_python_distribution_names(python_root: Path) -> set[str]:
    names: set[str] = set()
    for suffix in (".dist-info", ".egg-info"):
        for metadata_dir in python_root.rglob(f"*{suffix}"):
            if not metadata_dir.is_dir():
                continue
            name = distribution_name(metadata_dir)
            if name:
                names.add(name)
    return names


def validate_no_forbidden_runtime_python_packages(python_root: Path) -> list[str]:
    found = sorted(
        installed_python_distribution_names(python_root) & FORBIDDEN_RUNTIME_PYTHON_PACKAGES
    )
    if found:
        raise VerificationError(
            "Bundled Python runtime contains dev-only quality/security package(s): "
            + ", ".join(found)
        )
    return found


def validate_runtime(runtime_root: Path) -> dict[str, object]:
    cli_src = runtime_root / "cli-src" / "scriptscore"
    python_root = runtime_root / "python"

    if not cli_src.is_dir():
        raise VerificationError(f"Missing bundled CLI source: {cli_src}")

    manifest = runtime_manifest(runtime_root)
    if not manifest.get("portableRelease"):
        raise VerificationError("Bundled runtime manifest is not marked as a portable release.")

    resolved_python = resolved_runtime_python(runtime_root, manifest)
    if not python_root.exists():
        raise VerificationError(f"Bundled Python root was not found: {python_root}")
    python_path_entries = runtime_python_path_entries(runtime_root, manifest)
    validate_no_forbidden_runtime_python_packages(python_root)

    return {
        "manifest": str(runtime_root / "runtime-manifest.json"),
        "pythonExecutable": str(resolved_python),
        "pythonPathEntries": [str(entry) for entry in python_path_entries],
        "forbiddenPythonPackages": [],
        "runtimeMode": manifest.get("runtimeMode"),
    }


def validate_models(models_root: Path) -> dict[str, object]:
    required_files = [
        models_root / "det" / "inference.yml",
        models_root / "det" / "inference.pdiparams",
        models_root / "rec" / "inference.yml",
        models_root / "rec" / "inference.pdiparams",
    ]
    missing = [str(path) for path in required_files if not path.is_file()]
    if missing:
        raise VerificationError("Missing Paddle model resource(s): " + ", ".join(missing))
    return {"modelRoot": str(models_root)}


def runtime_env(runtime_root: Path, manifest: dict[str, object]) -> dict[str, str]:
    env = os.environ.copy()
    env.pop("PYTHONHOME", None)
    python_path_entries = runtime_python_path_entries(runtime_root, manifest)
    if python_path_entries:
        existing = env.get("PYTHONPATH", "")
        env["PYTHONPATH"] = os.pathsep.join(
            [str(entry) for entry in python_path_entries] + ([existing] if existing else [])
        )
    if sys.platform.startswith("linux"):
        prepend_env_paths(env, "LD_LIBRARY_PATH", runtime_native_library_paths(runtime_root))
    return env


def runtime_native_library_paths(runtime_root: Path) -> list[Path]:
    python_lib = runtime_root / "python" / "lib"
    paths = [python_lib] if python_lib.is_dir() else []
    paths.extend(
        path.parent
        for path in sorted(python_lib.rglob("*"))
        if path.is_file() and is_shared_library(path)
    )
    return list(dict.fromkeys(paths))


def is_shared_library(path: Path) -> bool:
    return path.name.endswith(".so") or ".so." in path.name


def prepend_env_paths(env: dict[str, str], name: str, paths: list[Path]) -> None:
    if not paths:
        return
    existing = env.get(name, "")
    env[name] = os.pathsep.join([str(path) for path in paths] + ([existing] if existing else []))


def validate_packaged_ocr_reader(runtime_root: Path, models_root: Path) -> dict[str, object]:
    manifest = runtime_manifest(runtime_root)
    python_executable = resolved_runtime_python(runtime_root, manifest)
    command = [
        str(python_executable),
        "-c",
        (
            "from pathlib import Path; "
            "import numpy as np; "
            "from scriptscore.pii_scan.reader import create_reader; "
            "reader = create_reader(Path(__import__('sys').argv[1])); "
            "reader.read(np.full((96, 320, 3), 255, dtype=np.uint8))"
        ),
        str(models_root),
    ]
    result = subprocess.run(
        command,
        check=False,
        capture_output=True,
        text=True,
        env=runtime_env(runtime_root, manifest),
    )
    if result.returncode != 0:
        details = (result.stderr or result.stdout or "").strip()
        raise VerificationError(
            "Packaged PaddleOCR runtime smoke failed" + (f": {details[-1000:]}" if details else ".")
        )
    return {
        "pythonExecutable": str(python_executable),
        "modelRoot": str(models_root),
    }


def validate_legal(legal_root: Path) -> dict[str, object]:
    if not legal_root.is_dir():
        raise VerificationError(f"Missing generated legal artifact directory: {legal_root}")
    legal_files = sorted(path for path in legal_root.rglob("*") if path.is_file())
    if not legal_files:
        raise VerificationError(f"Generated legal artifact directory is empty: {legal_root}")
    return {"legalRoot": str(legal_root), "fileCount": len(legal_files)}


def validate_windows_installer_icon_config(
    config_path: Path | None = None,
) -> dict[str, object]:
    config_path = config_path or tauri_config_path()
    config = json.loads(config_path.read_text(encoding="utf-8"))
    bundle = config.get("bundle")
    if not isinstance(bundle, dict):
        raise VerificationError("Tauri config is missing bundle settings.")
    windows = bundle.get("windows")
    if not isinstance(windows, dict):
        raise VerificationError("Tauri config is missing bundle.windows settings.")
    nsis = windows.get("nsis")
    if not isinstance(nsis, dict):
        raise VerificationError("Tauri config is missing bundle.windows.nsis settings.")
    installer_icon = nsis.get("installerIcon")
    expected_icon = WINDOWS_PROJECT_ICON.as_posix()
    if installer_icon != expected_icon:
        raise VerificationError(
            "Tauri NSIS installer icon must be configured as "
            f"{expected_icon}."
        )

    resolved_icon = config_path.parent / installer_icon
    if not resolved_icon.is_file():
        raise VerificationError(f"Tauri NSIS installer icon was not found: {resolved_icon}")
    return {
        "config": str(config_path),
        "nsisInstallerIcon": installer_icon,
        "resolvedIcon": str(resolved_icon),
    }


def ico_image_payloads(icon_path: Path) -> list[bytes]:
    data = icon_path.read_bytes()
    if len(data) < 6:
        raise VerificationError(f"Icon file is too small: {icon_path}")
    reserved = int.from_bytes(data[0:2], "little")
    icon_type = int.from_bytes(data[2:4], "little")
    image_count = int.from_bytes(data[4:6], "little")
    if reserved != 0 or icon_type != 1 or image_count < 1:
        raise VerificationError(f"Icon file is not a valid .ico file: {icon_path}")

    payloads: list[bytes] = []
    for index in range(image_count):
        entry_offset = 6 + index * 16
        if entry_offset + 16 > len(data):
            raise VerificationError(f"Icon file has a truncated directory: {icon_path}")
        image_size = int.from_bytes(data[entry_offset + 8 : entry_offset + 12], "little")
        image_offset = int.from_bytes(data[entry_offset + 12 : entry_offset + 16], "little")
        if image_size < 1 or image_offset + image_size > len(data):
            raise VerificationError(f"Icon file has an invalid image entry: {icon_path}")
        payloads.append(data[image_offset : image_offset + image_size])
    return payloads


def windows_app_executable_candidates(payload_root: Path) -> list[Path]:
    candidates: list[Path] = []
    for path in sorted(payload_root.rglob("*")):
        if not path.is_file() or path.suffix.lower() != ".exe":
            continue
        name = path.name.lower()
        path_parts = {part.lower() for part in path.parts}
        if "scriptscore" not in name:
            continue
        if name.startswith("unins") or "setup" in name:
            continue
        if {"runtime", "python"} <= path_parts:
            continue
        candidates.append(path)
    return candidates


def executable_mode(path: Path) -> bool:
    return path.is_file() and os.access(path, os.X_OK)


def packaged_app_executable_candidates(payload_root: Path) -> list[Path]:
    if sys.platform.startswith("win"):
        return windows_app_executable_candidates(payload_root)

    mac_candidates = sorted(
        path
        for path in payload_root.rglob("*.app/Contents/MacOS/*")
        if executable_mode(path) and "scriptscore" in path.name.lower()
    )
    if mac_candidates:
        return mac_candidates

    return sorted(
        path
        for path in payload_root.rglob("*")
        if executable_mode(path)
        and "scriptscore" in path.name.lower()
        and "runtime" not in {part.lower() for part in path.parts}
    )


def release_smoke_requested() -> bool:
    return os.environ.get("SCRIPTSCORE_RELEASE_SMOKE", "").lower() in {"1", "true", "yes"}


def release_smoke_command(
    app_executable: Path,
    output_path: Path,
    resource_dir: Path,
) -> list[str]:
    return [
        str(app_executable),
        "--release-smoke",
        "--release-smoke-output",
        str(output_path),
        "--release-smoke-mode",
        "document",
        "--release-smoke-resource-dir",
        str(resource_dir),
    ]


def load_release_smoke_summary(path: Path) -> dict[str, object] | None:
    if not path.is_file():
        return None
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise VerificationError(f"Release smoke summary was not a JSON object: {path}")
    return value


def run_packaged_release_smoke(payload_root: Path, resource_dir: Path) -> dict[str, object]:
    if not release_smoke_requested():
        return {"status": "disabled"}

    candidates = packaged_app_executable_candidates(payload_root)
    if not candidates:
        return {
            "status": "skipped",
            "reason": "No runnable packaged ScriptScore app executable was found in this payload.",
        }

    app_executable = candidates[0]
    with tempfile.TemporaryDirectory(prefix="scriptscore-release-smoke-") as tmp_dir:
        output_path = Path(tmp_dir) / "summary.json"
        env = os.environ.copy()
        env["SCRIPTSCORE_RELEASE_SMOKE"] = "1"
        result = subprocess.run(
            release_smoke_command(app_executable, output_path, resource_dir),
            check=False,
            capture_output=True,
            text=True,
            env=env,
            timeout=600,
        )
        summary = load_release_smoke_summary(output_path)
        if result.returncode != 0:
            details = (result.stderr or result.stdout or "").strip()
            raise VerificationError(
                "Packaged release smoke failed"
                + (f": {details[-1000:]}" if details else ".")
            )
        if summary is None:
            raise VerificationError(
                f"Packaged release smoke did not write a summary: {output_path}"
            )
        summary["appExecutable"] = str(app_executable)
        return summary


def validate_windows_payload_icon(
    payload_root: Path,
    reference_icon: Path | None = None,
) -> dict[str, object]:
    candidates = windows_app_executable_candidates(payload_root)
    if not candidates:
        return {"status": "not-applicable", "checked": []}

    reference_icon = reference_icon or desktop_root() / "src-tauri" / WINDOWS_PROJECT_ICON
    icon_payloads = ico_image_payloads(reference_icon)
    matched: list[str] = []
    for candidate in candidates:
        try:
            executable_bytes = candidate.read_bytes()
        except OSError as error:
            raise VerificationError(f"Could not read Windows app executable {candidate}: {error}")
        if any(payload in executable_bytes for payload in icon_payloads):
            matched.append(str(candidate))

    if not matched:
        checked = ", ".join(str(candidate) for candidate in candidates)
        raise VerificationError(
            "Windows package app executable(s) did not contain the ScriptScore icon resource: "
            + checked
        )
    return {
        "status": "verified",
        "referenceIcon": str(reference_icon),
        "checked": [str(candidate) for candidate in candidates],
        "matched": matched,
    }


def find_payload_runtime_root(payload_root: Path) -> Path:
    candidates = sorted(
        path.parent for path in payload_root.rglob("runtime-manifest.json") if path.is_file()
    )
    for candidate in candidates:
        try:
            validate_runtime(candidate)
        except VerificationError:
            continue
        return candidate
    raise VerificationError(f"Packaged payload is missing a valid runtime resource: {payload_root}")


def find_payload_model_root(payload_root: Path) -> Path:
    candidates = sorted(
        path for path in payload_root.rglob("paddle") if path.is_dir() and path.name == "paddle"
    )
    for candidate in candidates:
        try:
            validate_models(candidate)
        except VerificationError:
            continue
        return candidate
    raise VerificationError(
        f"Packaged payload is missing valid Paddle model resources: {payload_root}"
    )


def find_payload_legal_root(payload_root: Path) -> Path:
    candidates = sorted(
        path for path in payload_root.rglob("legal") if path.is_dir() and path.name == "legal"
    )
    for candidate in candidates:
        try:
            validate_legal(candidate)
        except VerificationError:
            continue
        return candidate
    raise VerificationError(
        f"Packaged payload is missing generated legal resources: {payload_root}"
    )


def validate_payload_root(payload_root: Path) -> dict[str, object]:
    if not payload_root.exists():
        raise VerificationError(f"Package payload inspection root was not found: {payload_root}")
    runtime_root = find_payload_runtime_root(payload_root)
    model_root = find_payload_model_root(payload_root)
    legal_root = find_payload_legal_root(payload_root)
    summary = {
        "payloadRoot": str(payload_root),
        "runtime": validate_runtime(runtime_root),
        "models": validate_models(model_root),
        "ocrReaderSmoke": validate_packaged_ocr_reader(runtime_root, model_root),
        "legal": validate_legal(legal_root),
        "windowsIcon": validate_windows_payload_icon(payload_root),
    }
    if release_smoke_requested():
        summary["releaseSmoke"] = run_packaged_release_smoke(payload_root, runtime_root.parent)
    return summary


def should_scan_text(path: Path) -> bool:
    if path.suffix.lower() in TEXT_SUFFIXES:
        return True
    try:
        return path.stat().st_size <= 64 * 1024
    except OSError:
        return False


def is_allowed_secret_marker(path: Path, marker: str) -> bool:
    allowed_suffixes = ALLOWED_SECRET_MARKER_PATHS.get(marker, ())
    path_parts = path.parts
    return any(path_parts[-len(suffix.parts) :] == suffix.parts for suffix in allowed_suffixes)


def scan_for_secret_markers(paths: list[Path]) -> None:
    findings: list[str] = []
    for root in paths:
        if not root.exists():
            continue
        files = [root] if root.is_file() else [path for path in root.rglob("*") if path.is_file()]
        for path in files:
            if not should_scan_text(path):
                continue
            try:
                text = path.read_text(encoding="utf-8", errors="ignore")
            except OSError:
                continue
            for marker in SECRET_MARKERS:
                if marker in text and not is_allowed_secret_marker(path, marker):
                    findings.append(f"{path}: {marker}")

    if findings:
        raise VerificationError(
            "Potential restricted wheel credential marker(s) found in packaged artifacts: "
            + "; ".join(findings)
        )


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_validation_outputs(
    label: str, package_files: list[Path], summary: dict[str, object]
) -> Path:
    output_dir = desktop_root() / "dist" / "release-package-validation" / label
    output_dir.mkdir(parents=True, exist_ok=True)

    sums_path = output_dir / "SHA256SUMS"
    with sums_path.open("w", encoding="utf-8") as handle:
        for package_file in package_files:
            handle.write(f"{sha256_file(package_file)}  {package_file.name}\n")

    (output_dir / "summary.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return output_dir


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Verify ScriptScore RC package artifacts and write checksums.",
    )
    parser.add_argument("--label", required=True, help="Short package matrix label.")
    parser.add_argument("--bundles", required=True, help="Comma-separated Tauri bundle targets.")
    parser.add_argument("--target-triple", default=os.environ.get("SCRIPTSCORE_DESKTOP_TARGET"))
    parser.add_argument(
        "--payload-root",
        action="append",
        default=[],
        help="Extracted or mounted package payload root to inspect. May be passed multiple times.",
    )
    parser.add_argument(
        "--runtime-root",
        default=str(desktop_root() / "dist" / "bundled-runtime"),
        help="Bundled runtime directory prepared before packaging.",
    )
    parser.add_argument(
        "--models-root",
        default=str(repo_root() / "cli" / "models" / "paddle"),
        help="Bundled Paddle model source directory.",
    )
    parser.add_argument(
        "--legal-root",
        default=str(desktop_root() / "dist" / "legal"),
        help="Generated legal artifact directory.",
    )
    return parser


def main() -> int:
    args = build_parser().parse_args()
    bundles = parse_bundles(args.bundles)
    bundle_root = find_bundle_root(args.target_triple)

    package_files: list[Path] = []
    missing_bundles: list[str] = []
    for bundle in bundles:
        found = files_for_bundle(bundle_root, bundle)
        if not found:
            missing_bundles.append(bundle)
        package_files.extend(found)

    if missing_bundles:
        raise VerificationError(
            f"Missing package artifact(s) for bundle target(s): {', '.join(missing_bundles)}"
        )

    payload_roots = [Path(value) for value in args.payload_root]
    if payload_roots:
        payload_summaries = [validate_payload_root(payload_root) for payload_root in payload_roots]
        scan_for_secret_markers([*payload_roots, bundle_root])
    else:
        payload_summaries = []
        runtime_summary = validate_runtime(Path(args.runtime_root))
        models_summary = validate_models(Path(args.models_root))
        legal_summary = validate_legal(Path(args.legal_root))
        scan_for_secret_markers([Path(args.runtime_root), Path(args.legal_root), bundle_root])
        payload_summaries.append(
            {
                "payloadRoot": "staging-directories",
                "runtime": runtime_summary,
                "models": models_summary,
                "ocrReaderSmoke": validate_packaged_ocr_reader(
                    Path(args.runtime_root),
                    Path(args.models_root),
                ),
                "legal": legal_summary,
            }
        )
        if release_smoke_requested():
            payload_summaries[0]["releaseSmoke"] = {
                "status": "skipped",
                "reason": "No packaged app executable is available for staging-directory validation.",
            }

    summary = {
        "label": args.label,
        "bundles": bundles,
        "bundleRoot": str(bundle_root),
        "installerIconConfig": validate_windows_installer_icon_config(),
        "packages": [str(path) for path in package_files],
        "payloads": payload_summaries,
    }
    output_dir = write_validation_outputs(args.label, package_files, summary)
    print(f"Verified {len(package_files)} package artifact(s) for {args.label}")
    print(f"Validation output: {output_dir}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except VerificationError as error:
        print(f"error: {error}")
        raise SystemExit(1) from None
