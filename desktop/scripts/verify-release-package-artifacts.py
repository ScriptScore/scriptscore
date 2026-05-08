#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
from __future__ import annotations

import argparse
import hashlib
import json
import os
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


class VerificationError(RuntimeError):
    """Raised when release package verification fails."""


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def desktop_root() -> Path:
    return Path(__file__).resolve().parents[1]


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


def validate_runtime(runtime_root: Path) -> dict[str, object]:
    manifest_path = runtime_root / "runtime-manifest.json"
    cli_src = runtime_root / "cli-src" / "scriptscore"
    python_root = runtime_root / "python"

    if not manifest_path.is_file():
        raise VerificationError(f"Missing bundled runtime manifest: {manifest_path}")
    if not cli_src.is_dir():
        raise VerificationError(f"Missing bundled CLI source: {cli_src}")

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if not manifest.get("portableRelease"):
        raise VerificationError("Bundled runtime manifest is not marked as a portable release.")

    python_executable = manifest.get("pythonExecutable")
    if not isinstance(python_executable, str) or not python_executable:
        raise VerificationError("Bundled runtime manifest does not name a Python executable.")
    resolved_python = Path(python_executable)
    if not resolved_python.is_absolute():
        resolved_python = runtime_root / resolved_python
    if not resolved_python.is_file():
        raise VerificationError(f"Bundled Python executable was not found: {resolved_python}")
    if not python_root.exists():
        raise VerificationError(f"Bundled Python root was not found: {python_root}")

    return {
        "manifest": str(manifest_path),
        "pythonExecutable": str(resolved_python),
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


def validate_legal(legal_root: Path) -> dict[str, object]:
    if not legal_root.is_dir():
        raise VerificationError(f"Missing generated legal artifact directory: {legal_root}")
    legal_files = sorted(path for path in legal_root.rglob("*") if path.is_file())
    if not legal_files:
        raise VerificationError(f"Generated legal artifact directory is empty: {legal_root}")
    return {"legalRoot": str(legal_root), "fileCount": len(legal_files)}


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
    raise VerificationError(f"Packaged payload is missing valid Paddle model resources: {payload_root}")


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
    raise VerificationError(f"Packaged payload is missing generated legal resources: {payload_root}")


def validate_payload_root(payload_root: Path) -> dict[str, object]:
    if not payload_root.exists():
        raise VerificationError(f"Package payload inspection root was not found: {payload_root}")
    runtime_root = find_payload_runtime_root(payload_root)
    model_root = find_payload_model_root(payload_root)
    legal_root = find_payload_legal_root(payload_root)
    return {
        "payloadRoot": str(payload_root),
        "runtime": validate_runtime(runtime_root),
        "models": validate_models(model_root),
        "legal": validate_legal(legal_root),
    }


def should_scan_text(path: Path) -> bool:
    if path.suffix.lower() in TEXT_SUFFIXES:
        return True
    try:
        return path.stat().st_size <= 64 * 1024
    except OSError:
        return False


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
                if marker in text:
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


def write_validation_outputs(label: str, package_files: list[Path], summary: dict[str, object]) -> Path:
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
                "legal": legal_summary,
            }
        )

    summary = {
        "label": args.label,
        "bundles": bundles,
        "bundleRoot": str(bundle_root),
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
