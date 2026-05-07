#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Generate release legal inventories, notices, and policy findings."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import re
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any


SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parents[1]
DESKTOP_ROOT = PROJECT_ROOT / "desktop"

ALLOWED_LICENSE_TOKENS = {
    "0BSD",
    "AGPL-3.0",
    "AGPL-3.0-only",
    "Apache-2.0",
    "BSD",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "CC0",
    "CC0-1.0",
    "ISC",
    "MIT",
    "MIT-0",
    "MPL-2.0",
    "OFL-1.1",
    "Unicode-3.0",
    "Unicode-DFS-2016",
    "Zlib",
}
REVIEW_LICENSE_TOKENS = {
    "GPL-2.0",
    "GPL-2.0-only",
    "GPL-2.0-or-later",
    "GPL-3.0",
    "GPL-3.0-only",
    "GPL-3.0-or-later",
    "LGPL-2.1",
    "LGPL-2.1-only",
    "LGPL-2.1-or-later",
    "LGPL-3.0",
    "LGPL-3.0-only",
    "LGPL-3.0-or-later",
}
BLOCKED_PATTERNS = (
    "SSPL",
    "BUSL",
    "Business Source",
    "NonCommercial",
    "Non-Commercial",
    "no redistribution",
    "no-redistribution",
    "field-of-use",
    "proprietary",
)
NATIVE_SUFFIXES = {".so", ".dylib", ".dll", ".pyd"}
ASSET_SUFFIXES = {
    ".avif",
    ".css",
    ".html",
    ".ico",
    ".js",
    ".json",
    ".map",
    ".otf",
    ".pdiparams",
    ".png",
    ".svg",
    ".ttf",
    ".txt",
    ".wasm",
    ".woff",
    ".woff2",
    ".yml",
}


@dataclass(frozen=True)
class InventoryItem:
    name: str
    version: str | None
    license: str | None
    source: str
    scope: str
    path: str | None = None
    runtime: bool = False
    checksum_sha256: str | None = None
    notice: str | None = None


@dataclass(frozen=True)
class PolicyFinding:
    severity: str
    item: str
    source: str
    scope: str
    license: str | None
    message: str


def project_relative(path: Path) -> str:
    try:
        return path.resolve().relative_to(PROJECT_ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def read_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        json.dump(value, handle, indent=2, sort_keys=True)
        handle.write("\n")


def digest_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def normalize_license(value: str | None) -> str | None:
    if value is None:
        return None
    normalized = " ".join(value.replace("\n", " ").split())
    if normalized in {"", "UNKNOWN", "Unknown", "LicenseRef-UNKNOWN"}:
        return None
    lowered = normalized.lower()
    if lowered.startswith("mit license") or lowered == "expat license":
        return "MIT"
    if lowered in {"apache license 2.0", "apache license, version 2.0"}:
        return "Apache-2.0"
    if lowered.startswith("bsd license"):
        return "BSD"
    if lowered.startswith("mozilla public license 2.0"):
        return "MPL-2.0"
    return normalized


def license_tokens(value: str | None) -> set[str]:
    if not value:
        return set()
    return set(re.findall(r"[A-Za-z0-9][A-Za-z0-9+.-]*", value))


def classify_item(item: InventoryItem) -> PolicyFinding | None:
    license_value = normalize_license(item.license)
    lowered = (license_value or "").lower()
    for pattern in BLOCKED_PATTERNS:
        if pattern.lower() in lowered:
            return PolicyFinding(
                "blocked",
                item.name,
                item.source,
                item.scope,
                item.license,
                "Blocked license or redistribution term detected.",
            )

    if item.scope in {"frontend-asset", "model-asset", "native-library"}:
        return PolicyFinding(
            "review_required",
            item.name,
            item.source,
            item.scope,
            item.license,
            "Asset or native binary terms require release review.",
        )

    if not license_value:
        severity = "unknown" if item.runtime else "review_required"
        return PolicyFinding(
            severity,
            item.name,
            item.source,
            item.scope,
            item.license,
            "No usable license metadata was found.",
        )

    tokens = license_tokens(license_value)
    if tokens & REVIEW_LICENSE_TOKENS:
        return PolicyFinding(
            "review_required",
            item.name,
            item.source,
            item.scope,
            item.license,
            "GPL/LGPL or custom copyleft terms require release review.",
        )
    if tokens & ALLOWED_LICENSE_TOKENS:
        return None
    if license_value.lower().startswith("http"):
        return PolicyFinding(
            "review_required",
            item.name,
            item.source,
            item.scope,
            item.license,
            "License is expressed as a URL and needs explicit classification.",
        )
    return PolicyFinding(
        "review_required",
        item.name,
        item.source,
        item.scope,
        item.license,
        "License is not in the default allowed set.",
    )


def load_runtime_manifest(path: Path) -> dict[str, Any]:
    if not path.is_file():
        return {}
    return read_json(path)


def resolve_manifest_path(runtime_root: Path, value: str) -> Path:
    candidate = Path(value)
    return candidate if candidate.is_absolute() else runtime_root / candidate


def python_inventory(runtime_manifest_path: Path, python_root: Path | None) -> list[InventoryItem]:
    manifest = load_runtime_manifest(runtime_manifest_path)
    runtime_root = runtime_manifest_path.parent
    portable_release = bool(manifest.get("portableRelease"))
    python_executable = manifest.get("pythonExecutable")
    if python_root is not None:
        for candidate in ("bin/python3", "bin/python", "python.exe", "Scripts/python.exe"):
            if (python_root / candidate).exists():
                python_executable = str(python_root / candidate)
                portable_release = True
                break
    if not python_executable:
        return []

    executable = resolve_manifest_path(runtime_root, str(python_executable))
    if not executable.exists():
        return [
            InventoryItem(
                name="python-runtime",
                version=None,
                license=None,
                source="python",
                scope="python-runtime",
                path=project_relative(executable),
                runtime=portable_release,
                notice="Python runtime executable referenced by the manifest was not found.",
            )
        ]

    snippet = r"""
import importlib.metadata as metadata
import json
rows = []
for dist in metadata.distributions():
    meta = dist.metadata
    classifiers = meta.get_all("Classifier") or []
    license_classifiers = [
        item.rsplit("::", 1)[-1].strip()
        for item in classifiers
        if item.startswith("License ::")
    ]
    rows.append({
        "name": meta.get("Name") or dist.metadata["Name"],
        "version": meta.get("Version"),
        "license": meta.get("License-Expression") or meta.get("License") or " OR ".join(license_classifiers),
    })
print(json.dumps(rows, sort_keys=True))
"""
    try:
        completed = subprocess.run(
            [str(executable), "-c", snippet],
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )
    except (OSError, subprocess.SubprocessError) as err:
        return [
            InventoryItem(
                name="python-runtime",
                version=None,
                license=None,
                source="python",
                scope="python-runtime",
                path=project_relative(executable),
                runtime=portable_release,
                notice=f"Could not inspect installed Python distributions: {err}",
            )
        ]

    rows = json.loads(completed.stdout)
    scope = "python-runtime" if portable_release else "python-smoke-runtime"
    return [
        InventoryItem(
            name=row["name"],
            version=row.get("version"),
            license=normalize_license(row.get("license")),
            source="python",
            scope=scope,
            runtime=portable_release,
        )
        for row in sorted(rows, key=lambda row: row["name"].lower())
    ]


def npm_inventory(lock_path: Path) -> list[InventoryItem]:
    if not lock_path.is_file():
        return []
    lock = read_json(lock_path)
    items: list[InventoryItem] = []
    for package_path, package in sorted(lock.get("packages", {}).items()):
        if package_path == "" or not package_path.startswith("node_modules/"):
            continue
        name = package_path.removeprefix("node_modules/")
        scope = "npm-dev" if package.get("dev") else "npm-runtime"
        items.append(
            InventoryItem(
                name=name,
                version=package.get("version"),
                license=normalize_license(package.get("license")),
                source="npm",
                scope=scope,
                path=package_path,
                runtime=scope == "npm-runtime",
            )
        )
    return items


def cargo_metadata(manifest_path: Path, metadata_file: Path | None) -> dict[str, Any]:
    if metadata_file is not None:
        return read_json(metadata_file)
    completed = subprocess.run(
        [
            "cargo",
            "metadata",
            "--format-version",
            "1",
            "--manifest-path",
            str(manifest_path),
        ],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=60,
    )
    return json.loads(completed.stdout)


def cargo_inventory(manifest_path: Path, metadata_file: Path | None) -> list[InventoryItem]:
    try:
        metadata = cargo_metadata(manifest_path, metadata_file)
    except (OSError, subprocess.SubprocessError) as err:
        return [
            InventoryItem(
                name="cargo-metadata",
                version=None,
                license=None,
                source="cargo",
                scope="cargo-runtime",
                runtime=True,
                notice=f"Could not run cargo metadata: {err}",
            )
        ]
    workspace_members = set(metadata.get("workspace_members", []))
    items: list[InventoryItem] = []
    for package in sorted(metadata.get("packages", []), key=lambda item: item["name"]):
        package_id = package.get("id")
        scope = "cargo-first-party" if package_id in workspace_members else "cargo-runtime"
        items.append(
            InventoryItem(
                name=package["name"],
                version=package.get("version"),
                license=normalize_license(package.get("license")),
                source="cargo",
                scope=scope,
                path=package.get("manifest_path"),
                runtime=scope == "cargo-runtime",
            )
        )
    return items


def file_inventory(root: Path, scope: str, source: str, runtime: bool) -> list[InventoryItem]:
    if not root.exists():
        return []
    items: list[InventoryItem] = []
    for path in sorted(item for item in root.rglob("*") if item.is_file()):
        suffix = path.suffix.lower()
        if suffix not in ASSET_SUFFIXES and suffix not in NATIVE_SUFFIXES:
            continue
        item_scope = "native-library" if suffix in NATIVE_SUFFIXES else scope
        items.append(
            InventoryItem(
                name=project_relative(path),
                version=None,
                license=None,
                source=source,
                scope=item_scope,
                path=project_relative(path),
                runtime=runtime,
                checksum_sha256=digest_file(path),
            )
        )
    return items


def runtime_native_inventory(runtime_root: Path) -> list[InventoryItem]:
    if not runtime_root.exists():
        return []
    return [
        item
        for item in file_inventory(runtime_root, "runtime-file", "runtime", runtime=True)
        if item.scope == "native-library"
    ]


def write_notices(path: Path, items: list[InventoryItem], findings: list[PolicyFinding]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="\n") as handle:
        handle.write("# Third-Party Notices\n\n")
        handle.write(
            "This file is generated from release inputs by "
            "`desktop/scripts/generate_legal_artifacts.py`.\n\n"
        )
        handle.write("## First-Party License\n\n")
        handle.write(
            "ScriptScore Desktop and the bundled open-core runtime are "
            "licensed under AGPL-3.0-only. See the root `LICENSE` and `NOTICE`.\n\n"
        )
        handle.write("## Inventory Summary\n\n")
        writer = csv.writer(handle)
        writer.writerow(["Name", "Version", "License", "Source", "Scope"])
        for item in items:
            writer.writerow(
                [item.name, item.version or "", item.license or "", item.source, item.scope]
            )
        handle.write("\n## Review Findings\n\n")
        if not findings:
            handle.write("No blocked, unknown, or review-required findings.\n")
            return
        for finding in findings:
            handle.write(
                f"- {finding.severity}: {finding.item} "
                f"({finding.source}, {finding.scope}, {finding.license or 'missing license'}) - "
                f"{finding.message}\n"
            )


def write_sbom(path: Path, items: list[InventoryItem]) -> None:
    write_json(
        path,
        {
            "format": "scriptscore-legal-sbom-v1",
            "componentCount": len(items),
            "components": [asdict(item) for item in items],
        },
    )


def generate(args: argparse.Namespace) -> int:
    output_dir = args.output_dir
    runtime_manifest = args.runtime_manifest
    runtime_root = runtime_manifest.parent

    python_items = python_inventory(runtime_manifest, args.python_root)
    npm_items = npm_inventory(args.npm_lock)
    cargo_items = cargo_inventory(args.cargo_manifest, args.cargo_metadata_file)
    asset_items = file_inventory(args.frontend_build, "frontend-asset", "assets", runtime=True)
    asset_items.extend(file_inventory(args.paddle_models, "model-asset", "assets", runtime=True))
    native_items = runtime_native_inventory(runtime_root)

    all_items = python_items + npm_items + cargo_items + asset_items + native_items
    findings = [finding for item in all_items if (finding := classify_item(item)) is not None]
    unknown_failure_scopes = {"python-runtime", "npm-runtime", "cargo-runtime"}
    blocked_skip_scopes = {"python-smoke-runtime", "npm-dev", "cargo-first-party"}
    blocked_or_unknown_runtime = [
        finding
        for finding in findings
        if (
            finding.severity == "blocked"
            and finding.scope not in blocked_skip_scopes
        )
        or (finding.severity == "unknown" and finding.scope in unknown_failure_scopes)
    ]

    output_dir.mkdir(parents=True, exist_ok=True)
    write_sbom(output_dir / "sbom-python.json", python_items)
    write_sbom(output_dir / "sbom-npm.json", npm_items)
    write_sbom(output_dir / "sbom-cargo.json", cargo_items)
    write_sbom(output_dir / "sbom-assets.json", asset_items + native_items)
    write_notices(output_dir / "THIRD_PARTY_NOTICES.md", all_items, findings)
    write_json(
        output_dir / "license-policy-report.json",
        {
            "policy": {
                "defaultLicense": "AGPL-3.0-only",
                "checkModeFailure": "blocked or unknown runtime artifacts",
            },
            "summary": {
                "componentCount": len(all_items),
                "findingCount": len(findings),
                "blockedOrUnknownRuntimeCount": len(blocked_or_unknown_runtime),
            },
            "sourceOffer": {
                "label": "Corresponding Source",
                "url": args.source_url,
                "localNoticesPath": "legal/THIRD_PARTY_NOTICES.md",
            },
            "findings": [asdict(finding) for finding in findings],
        },
    )

    if args.check and blocked_or_unknown_runtime:
        for finding in blocked_or_unknown_runtime:
            print(
                f"{finding.severity}: {finding.item} "
                f"({finding.source}, {finding.scope}): {finding.message}",
                file=sys.stderr,
            )
        return 1
    return 0


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--source-url", default="https://github.com/ScriptScore/scriptscore")
    parser.add_argument("--output-dir", type=Path, default=DESKTOP_ROOT / "dist" / "legal")
    parser.add_argument(
        "--runtime-manifest",
        type=Path,
        default=DESKTOP_ROOT / "dist" / "bundled-runtime" / "runtime-manifest.json",
    )
    parser.add_argument("--python-root", type=Path)
    parser.add_argument("--npm-lock", type=Path, default=DESKTOP_ROOT / "frontend" / "package-lock.json")
    parser.add_argument(
        "--cargo-manifest",
        type=Path,
        default=DESKTOP_ROOT / "src-tauri" / "Cargo.toml",
    )
    parser.add_argument("--cargo-metadata-file", type=Path)
    parser.add_argument("--frontend-build", type=Path, default=DESKTOP_ROOT / "frontend" / "build")
    parser.add_argument("--paddle-models", type=Path, default=PROJECT_ROOT / "cli" / "models" / "paddle")
    return parser.parse_args(argv)


if __name__ == "__main__":
    raise SystemExit(generate(parse_args(sys.argv[1:])))
