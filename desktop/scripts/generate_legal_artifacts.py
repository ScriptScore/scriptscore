#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Generate release legal inventories, notices, and policy findings."""

from __future__ import annotations

import argparse
import csv
import fnmatch
import hashlib
import json
import re
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any

SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parents[1]
DESKTOP_ROOT = PROJECT_ROOT / "desktop"
FRONTEND_STATIC_ASSET_PROVENANCE = (
    PROJECT_ROOT / "docs" / "licensing" / "frontend-static-asset-provenance.json"
)
PADDLE_OCR_MODEL_PROVENANCE = (
    PROJECT_ROOT / "docs" / "licensing" / "paddle-ocr-model-provenance.json"
)
NATIVE_RUNTIME_PROVENANCE = PROJECT_ROOT / "docs" / "licensing" / "native-runtime-provenance.json"

ALLOWED_LICENSE_TOKENS = {
    "0BSD",
    "AGPL-3.0",
    "AGPL-3.0-only",
    "Apache-2.0",
    "BlueOak-1.0.0",
    "BSD",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "CDLA-Permissive-2.0",
    "CC0",
    "CC0-1.0",
    "ISC",
    "MIT",
    "MIT-0",
    "MIT-CMU",
    "MPL-2.0",
    "OFL-1.1",
    "PSF-2.0",
    "TCL",
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
BLOCKED_RUNTIME_PACKAGES = {
    "aistudio-sdk": "Package must not appear in distributed runtime artifacts until upstream publishes usable license/source evidence.",
    "astroid": "Dev-only quality package must not appear in distributed runtime artifacts.",
    "bce-python-sdk": "Baidu cloud SDK is not used by ScriptScore's offline OCR path and must not appear in distributed runtime artifacts.",
    "crc32c": "Transitive bce-python-sdk dependency is not used by ScriptScore's offline OCR path and must not appear in distributed runtime artifacts.",
    "opencv-contrib-python": "Non-headless OpenCV must not appear in distributed runtime artifacts; use opencv-contrib-python-headless instead.",
    "pip-api": "Dev-only security tooling dependency must not appear in distributed runtime artifacts.",
    "pip-audit": "Dev-only security tooling must not appear in distributed runtime artifacts.",
    "pylint": "Dev-only quality package must not appear in distributed runtime artifacts.",
}
APPROVED_RUNTIME_REVIEW_PACKAGES: dict[str, tuple[set[str], set[str], str]] = {
    "pymupdf": (
        {"1.27.2.2"},
        {"Dual Licensed - GNU AFFERO GPL 3.0 or Artifex Commercial License"},
        "python-runtime",
    ),
    "python-bidi": (
        {"0.6.7"},
        {"GNU Library or Lesser General Public License (LGPL)"},
        "python-runtime",
    ),
    "scipy": (
        {"1.17.1"},
        {"BSD-3-Clause AND BSD-3-Clause-Open-MPI AND GPL-3.0-or-later WITH GCC-exception-3.1"},
        "python-runtime",
    ),
}
APPROVED_NATIVE_LIBRARY_PATH_PARTS = (
    "/site-packages/bidi/",
    "/site-packages/pymupdf/",
)
LICENSE_NORMALIZATIONS = {
    "apache 2.0": "Apache-2.0",
    "apache 2.0 license": "Apache-2.0",
    "apache license 2.0": "Apache-2.0",
    "apache license, version 2.0": "Apache-2.0",
    "apache license version 2.0": "Apache-2.0",
    "apache software license": "Apache-2.0",
    "apache software license (apache 2.0)": "Apache-2.0",
    "psfl": "PSF-2.0",
    "python software foundation license": "PSF-2.0",
    "python software foundation license (psfl)": "PSF-2.0",
}
PYTHON_LICENSE_REPLACEMENTS: dict[str, dict[str, str]] = {
    "aistudio-sdk": {
        "license": "LicenseRef-REVIEW-aistudio-sdk",
        "release_obligations": (
            "Keep this runtime dependency in release review until upstream publishes "
            "usable license metadata or the dependency is removed."
        ),
    },
    "pandas": {
        "license": "BSD-3-Clause",
        "release_obligations": (
            "Review long upstream wheel license metadata when pandas version or "
            "license metadata changes."
        ),
    },
    "scipy": {
        "license": (
            "BSD-3-Clause AND BSD-3-Clause-Open-MPI AND GPL-3.0-or-later WITH GCC-exception-3.1"
        ),
        "notice": "SciPy wheel metadata includes bundled-library license attributions.",
        "release_obligations": (
            "Include generated notices and bundled-library attributions from the "
            "wheel metadata. Reopen review if SciPy version or license metadata changes."
        ),
    },
    "python-dateutil": {
        "license": "BSD-3-Clause OR Apache-2.0",
        "release_obligations": (
            "Recheck upstream license metadata if python-dateutil changes away from "
            "the reviewed BSD-3-Clause or Apache-2.0 dual-license posture."
        ),
    },
    "pymupdf": {
        "license": "Dual Licensed - GNU AFFERO GPL 3.0 or Artifex Commercial License",
        "notice": (
            "PyMuPDF is included under ScriptScore's AGPL-3.0-only client distribution posture."
        ),
        "release_obligations": (
            "Include PyMuPDF/MuPDF notices and source availability/source-offer "
            "evidence for the released artifact, or use Artifex commercial licensing "
            "for any non-AGPL distribution."
        ),
    },
    "python-bidi": {
        "license": "GNU Library or Lesser General Public License (LGPL)",
        "notice": "python-bidi is included for OCR text handling.",
        "release_obligations": (
            "Include LGPL license text, package notices, and source/relink compliance "
            "notes for the released artifact."
        ),
    },
}
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
FRONTEND_GENERATED_SUFFIXES = {".css", ".html", ".js", ".json", ".map"}
FONTSOURCE_ASSET_MAPPINGS = (
    {
        "filename_prefix": "figtree-",
        "package_name": "@fontsource-variable/figtree",
        "license": "OFL-1.1",
        "license_path": "desktop/frontend/node_modules/@fontsource-variable/figtree/LICENSE",
    },
    {
        "filename_prefix": "inter-",
        "package_name": "@fontsource-variable/inter",
        "license": "OFL-1.1",
        "license_path": "desktop/frontend/node_modules/@fontsource-variable/inter/LICENSE",
    },
)
FRONTEND_STATIC_ASSET_SOURCE = "first-party"
FRONTEND_STATIC_ASSET_SCOPE = "frontend-static-asset"
PADDLE_OCR_MODEL_SOURCE = "paddleocr-model"
PADDLE_OCR_MODEL_SCOPE = "paddle-ocr-model-asset"


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
    release_obligations: str | None = None


@dataclass(frozen=True)
class PolicyFinding:
    severity: str
    item: str
    source: str
    scope: str
    license: str | None
    message: str


@dataclass(frozen=True)
class NativeRuntimeProvenanceEntry:
    path_patterns: tuple[str, ...]
    source_package: str
    license: str
    obligations: str
    evidence: tuple[str, ...]


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
    if lowered in LICENSE_NORMALIZATIONS:
        return LICENSE_NORMALIZATIONS[lowered]
    if lowered.startswith("mit license") or lowered == "expat license":
        return "MIT"
    if lowered.startswith("bsd license"):
        return "BSD"
    if lowered.startswith("mozilla public license 2.0"):
        return "MPL-2.0"
    return normalized


def license_tokens(value: str | None) -> set[str]:
    if not value:
        return set()
    return set(re.findall(r"[A-Za-z0-9][A-Za-z0-9+.-]*", value))


def strip_wrapping_parentheses(value: str) -> str:
    expression = value.strip()
    while expression.startswith("(") and expression.endswith(")"):
        depth = 0
        wraps_full_expression = True
        for index, char in enumerate(expression):
            if char == "(":
                depth += 1
            elif char == ")":
                depth -= 1
                if depth == 0 and index != len(expression) - 1:
                    wraps_full_expression = False
                    break
        if not wraps_full_expression:
            break
        expression = expression[1:-1].strip()
    return expression


def split_top_level_operator(value: str, operator: str) -> list[str]:
    expression = strip_wrapping_parentheses(value)
    parts: list[str] = []
    depth = 0
    start = 0
    pattern = re.compile(rf"\(|\)|(?<![A-Za-z0-9+.-]){operator}(?![A-Za-z0-9+.-])")
    for match in pattern.finditer(expression):
        token = match.group(0)
        if token == "(":
            depth += 1
        elif token == ")":
            depth = max(depth - 1, 0)
        elif depth == 0:
            parts.append(expression[start : match.start()].strip())
            start = match.end()
    parts.append(expression[start:].strip())
    return [part for part in parts if part]


def license_expression_is_allowed(value: str | None) -> bool:
    license_value = normalize_license(value)
    if not license_value:
        return False

    expression = strip_wrapping_parentheses(license_value)
    or_parts = split_top_level_operator(expression, "OR")
    if len(or_parts) > 1:
        return any(license_expression_is_allowed(part) for part in or_parts)

    and_parts = split_top_level_operator(expression, "AND")
    if len(and_parts) > 1:
        return all(license_expression_is_allowed(part) for part in and_parts)

    if expression.lower().startswith("http"):
        return False
    tokens = license_tokens(expression)
    return bool(tokens & ALLOWED_LICENSE_TOKENS) and not bool(tokens & REVIEW_LICENSE_TOKENS)


def python_license_metadata(
    row: dict[str, Any],
) -> tuple[str | None, str | None, str | None]:
    replacement = PYTHON_LICENSE_REPLACEMENTS.get(row["name"].lower())
    if replacement is not None:
        return (
            normalize_license(replacement["license"]),
            replacement.get("notice"),
            replacement.get("release_obligations"),
        )
    return normalize_license(row.get("license")), None, None


def approved_runtime_review_package(item: InventoryItem, license_value: str | None) -> bool:
    approval = APPROVED_RUNTIME_REVIEW_PACKAGES.get(item.name.lower())
    if approval is None or not item.runtime:
        return False
    versions, licenses, scope = approval
    if item.scope != scope:
        return False
    if item.version not in versions:
        return False
    return license_value in licenses


def approved_native_library(item: InventoryItem) -> bool:
    if item.scope != "native-library" or not item.path:
        return False
    if item.license and item.notice:
        return True
    path = item.path.replace("\\", "/")
    return any(part in path for part in APPROVED_NATIVE_LIBRARY_PATH_PARTS)


def classify_item(item: InventoryItem) -> PolicyFinding | None:
    blocked_package_message = BLOCKED_RUNTIME_PACKAGES.get(item.name.lower())
    if item.runtime and blocked_package_message is not None:
        return PolicyFinding(
            "blocked",
            item.name,
            item.source,
            item.scope,
            item.license,
            blocked_package_message,
        )

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

    if item.scope == "frontend-build-output":
        return None

    if approved_native_library(item):
        return None

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

    if approved_runtime_review_package(item, license_value):
        return None

    tokens = license_tokens(license_value)
    if license_expression_is_allowed(license_value):
        return None
    if tokens & REVIEW_LICENSE_TOKENS:
        return PolicyFinding(
            "review_required",
            item.name,
            item.source,
            item.scope,
            item.license,
            "GPL/LGPL or custom copyleft terms require release review.",
        )
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
        for candidate in (
            "bin/python3",
            "bin/python",
            "python.exe",
            "Scripts/python.exe",
        ):
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
    items: list[InventoryItem] = []
    for row in sorted(rows, key=lambda row: row["name"].lower()):
        license_value, notice, release_obligations = python_license_metadata(row)
        items.append(
            InventoryItem(
                name=row["name"],
                version=row.get("version"),
                license=license_value,
                source="python",
                scope=scope,
                runtime=portable_release,
                notice=notice,
                release_obligations=release_obligations,
            )
        )
    return items


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


def npm_package_versions(lock_path: Path, package_names: set[str]) -> dict[str, str]:
    if not lock_path.is_file():
        return {}
    lock = read_json(lock_path)
    versions: dict[str, str] = {}
    for package_name in package_names:
        package = lock.get("packages", {}).get(f"node_modules/{package_name}")
        if package and package.get("version"):
            versions[package_name] = package["version"]
    return versions


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


def frontend_build_scope(root: Path, path: Path) -> str:
    if path.suffix.lower() in NATIVE_SUFFIXES:
        return "native-library"
    rel_path = path.relative_to(root).as_posix()
    if path.suffix.lower() in FRONTEND_GENERATED_SUFFIXES and (
        rel_path == "index.html" or rel_path.startswith("_app/")
    ):
        return "frontend-build-output"
    return "frontend-asset"


def fontsource_asset_item(path: Path, package_versions: dict[str, str]) -> InventoryItem | None:
    if path.suffix.lower() != ".woff2":
        return None
    filename = path.name.lower()
    for mapping in FONTSOURCE_ASSET_MAPPINGS:
        package_name = mapping["package_name"]
        license_value = mapping["license"]
        if filename.startswith(mapping["filename_prefix"]):
            return InventoryItem(
                name=project_relative(path),
                version=package_versions.get(package_name),
                license=license_value,
                source="npm",
                scope="frontend-font-asset",
                path=project_relative(path),
                runtime=True,
                checksum_sha256=digest_file(path),
                notice=(
                    "Emitted frontend font asset mapped to "
                    f"{package_name}; package metadata declares {license_value}, "
                    f"with license text at `{mapping['license_path']}`."
                ),
            )
    return None


def frontend_static_asset_provenance(
    provenance_path: Path,
) -> dict[str, dict[str, Any]]:
    if not provenance_path.exists():
        return {}
    data = read_json(provenance_path)
    entries = data.get("assets", [])
    if not isinstance(entries, list):
        raise ValueError(f"{provenance_path} must contain an assets list")

    provenance: dict[str, dict[str, Any]] = {}
    for entry in entries:
        if not isinstance(entry, dict):
            raise ValueError(f"{provenance_path} contains a non-object asset entry")
        source_path = entry.get("source_path")
        if not isinstance(source_path, str) or not source_path:
            raise ValueError(f"{provenance_path} asset entries must include source_path")
        license_value = normalize_license(entry.get("license"))
        if not license_value:
            raise ValueError(f"{source_path} must include usable license provenance")
        provenance[Path(source_path).name] = {
            **entry,
            "source_path": source_path,
            "license": license_value,
        }
    return provenance


def frontend_static_asset_item(
    path: Path, provenance: dict[str, dict[str, Any]]
) -> InventoryItem | None:
    entry = provenance.get(path.name)
    if entry is None:
        return None

    source_path = entry["source_path"]
    origin = entry.get("origin") or "First-party ScriptScore static frontend asset."
    evidence = entry.get("evidence", [])
    evidence_note = ""
    if isinstance(evidence, list) and evidence:
        evidence_note = " Evidence: " + ", ".join(f"`{item}`" for item in evidence) + "."

    return InventoryItem(
        name=project_relative(path),
        version=None,
        license=entry["license"],
        source=FRONTEND_STATIC_ASSET_SOURCE,
        scope=FRONTEND_STATIC_ASSET_SCOPE,
        path=project_relative(path),
        runtime=True,
        checksum_sha256=digest_file(path),
        notice=f"{origin} Source asset: `{source_path}`.{evidence_note}",
    )


def frontend_build_inventory(
    root: Path,
    package_versions: dict[str, str] | None = None,
    static_asset_provenance: dict[str, dict[str, Any]] | None = None,
) -> list[InventoryItem]:
    if not root.exists():
        return []
    resolved_package_versions = package_versions or {}
    resolved_static_asset_provenance = static_asset_provenance or {}
    items: list[InventoryItem] = []
    for path in sorted(item for item in root.rglob("*") if item.is_file()):
        suffix = path.suffix.lower()
        if suffix not in ASSET_SUFFIXES and suffix not in NATIVE_SUFFIXES:
            continue
        if font_item := fontsource_asset_item(path, resolved_package_versions):
            items.append(font_item)
            continue
        if static_item := frontend_static_asset_item(path, resolved_static_asset_provenance):
            items.append(static_item)
            continue
        items.append(
            InventoryItem(
                name=project_relative(path),
                version=None,
                license=None,
                source="assets",
                scope=frontend_build_scope(root, path),
                path=project_relative(path),
                runtime=True,
                checksum_sha256=digest_file(path),
            )
        )
    return items


def paddle_ocr_model_provenance(provenance_path: Path) -> dict[str, dict[str, Any]]:
    if not provenance_path.exists():
        return {}
    data = read_json(provenance_path)
    license_value = normalize_license(data.get("license"))
    if not license_value:
        raise ValueError(f"{provenance_path} must include usable license provenance")
    redistribution_terms = data.get("redistribution_terms")
    if not isinstance(redistribution_terms, str) or not redistribution_terms:
        raise ValueError(f"{provenance_path} must include redistribution_terms")
    release_obligations = data.get("release_obligations")
    if release_obligations is not None and not isinstance(release_obligations, str):
        raise ValueError(f"{provenance_path} release_obligations must be a string")
    license_evidence = data.get("license_evidence", [])
    if not isinstance(license_evidence, list) or not all(
        isinstance(item, str) and item for item in license_evidence
    ):
        raise ValueError(f"{provenance_path} must include license_evidence strings")

    entries: dict[str, dict[str, Any]] = {}
    models = data.get("models", [])
    if not isinstance(models, list):
        raise ValueError(f"{provenance_path} must contain a models list")
    for model in models:
        if not isinstance(model, dict):
            raise ValueError(f"{provenance_path} contains a non-object model entry")
        model_name = model.get("model_name")
        upstream_model_id = model.get("upstream_model_id")
        upstream_commit = model.get("upstream_commit")
        upstream_repository = model.get("upstream_repository")
        files = model.get("files", [])
        if not all(
            isinstance(value, str) and value
            for value in (
                model_name,
                upstream_model_id,
                upstream_commit,
                upstream_repository,
            )
        ):
            raise ValueError(f"{provenance_path} model entries must include upstream metadata")
        if not isinstance(files, list):
            raise ValueError(f"{provenance_path} model files must be a list")
        for file_entry in files:
            if not isinstance(file_entry, dict):
                raise ValueError(f"{provenance_path} contains a non-object file entry")
            path = file_entry.get("path")
            sha256 = file_entry.get("sha256")
            size_bytes = file_entry.get("size_bytes")
            upstream_url = file_entry.get("upstream_url")
            if not isinstance(path, str) or not path:
                raise ValueError(f"{provenance_path} model file entries must include path")
            if not isinstance(sha256, str) or not re.fullmatch(r"[0-9a-f]{64}", sha256):
                raise ValueError(f"{path} must include a sha256 checksum")
            if not isinstance(size_bytes, int) or size_bytes < 0:
                raise ValueError(f"{path} must include size_bytes")
            if not isinstance(upstream_url, str) or not upstream_url:
                raise ValueError(f"{path} must include upstream_url")
            entries[path] = {
                **file_entry,
                "license": license_value,
                "license_evidence": license_evidence,
                "model_name": model_name,
                "release_obligations": release_obligations,
                "redistribution_terms": redistribution_terms,
                "upstream_commit": upstream_commit,
                "upstream_model_id": upstream_model_id,
                "upstream_repository": upstream_repository,
            }
    return entries


def paddle_ocr_model_item(
    path: Path, provenance: dict[str, dict[str, Any]]
) -> InventoryItem | None:
    rel_path = project_relative(path)
    entry = provenance.get(rel_path)
    if entry is None:
        return None

    checksum = digest_file(path)
    if checksum != entry["sha256"]:
        raise ValueError(
            f"{rel_path} checksum {checksum} does not match provenance {entry['sha256']}"
        )
    size_bytes = path.stat().st_size
    if size_bytes != entry["size_bytes"]:
        raise ValueError(
            f"{rel_path} size {size_bytes} does not match provenance {entry['size_bytes']}"
        )

    evidence_note = ", ".join(f"`{item}`" for item in entry["license_evidence"])
    notice = (
        f"{entry['model_name']} bundled PaddleOCR model file from "
        f"`{entry['upstream_model_id']}` at commit `{entry['upstream_commit']}`. "
        f"Source: `{entry['upstream_url']}`. License evidence: {evidence_note}. "
        f"Redistribution terms: {entry['redistribution_terms']}"
    )
    return InventoryItem(
        name=rel_path,
        version=entry["upstream_commit"],
        license=entry["license"],
        source=PADDLE_OCR_MODEL_SOURCE,
        scope=PADDLE_OCR_MODEL_SCOPE,
        path=rel_path,
        runtime=True,
        checksum_sha256=checksum,
        notice=notice,
        release_obligations=entry.get("release_obligations"),
    )


def paddle_model_inventory(
    root: Path, model_provenance: dict[str, dict[str, Any]] | None = None
) -> list[InventoryItem]:
    if not root.exists():
        return []
    resolved_model_provenance = model_provenance or {}
    items: list[InventoryItem] = []
    for path in sorted(item for item in root.rglob("*") if item.is_file()):
        suffix = path.suffix.lower()
        if suffix not in ASSET_SUFFIXES and suffix not in NATIVE_SUFFIXES:
            continue
        if model_item := paddle_ocr_model_item(path, resolved_model_provenance):
            items.append(model_item)
            continue
        item_scope = "native-library" if suffix in NATIVE_SUFFIXES else "model-asset"
        items.append(
            InventoryItem(
                name=project_relative(path),
                version=None,
                license=None,
                source="assets",
                scope=item_scope,
                path=project_relative(path),
                runtime=True,
                checksum_sha256=digest_file(path),
            )
        )
    return items


def native_runtime_provenance(
    provenance_path: Path,
) -> list[NativeRuntimeProvenanceEntry]:
    if not provenance_path.exists():
        return []
    data = read_json(provenance_path)
    entries = data.get("entries", [])
    if not isinstance(entries, list):
        raise ValueError(f"{provenance_path} must contain an entries list")

    provenance: list[NativeRuntimeProvenanceEntry] = []
    for entry in entries:
        if not isinstance(entry, dict):
            raise ValueError(f"{provenance_path} contains a non-object entry")
        path_patterns = entry.get("path_patterns")
        source_package = entry.get("source_package")
        obligations = entry.get("obligations")
        evidence = entry.get("evidence")
        license_value = normalize_license(entry.get("license"))
        if (
            not isinstance(path_patterns, list)
            or not path_patterns
            or not all(isinstance(pattern, str) and pattern for pattern in path_patterns)
        ):
            raise ValueError(f"{provenance_path} entries must include path_patterns")
        if not all(isinstance(value, str) and value for value in (source_package, obligations)):
            raise ValueError(f"{provenance_path} entries must include source_package/obligations")
        if not license_value:
            raise ValueError(f"{source_package} must include usable license provenance")
        if (
            not isinstance(evidence, list)
            or not evidence
            or not all(isinstance(item, str) and item for item in evidence)
        ):
            raise ValueError(f"{source_package} must include evidence strings")
        provenance.append(
            NativeRuntimeProvenanceEntry(
                path_patterns=tuple(path_patterns),
                source_package=source_package,
                license=license_value,
                obligations=obligations,
                evidence=tuple(evidence),
            )
        )
    return provenance


def path_matches_pattern(path: str, pattern: str) -> bool:
    patterns = {pattern}
    queue = [pattern]
    while queue:
        candidate = queue.pop()
        marker = "/**/"
        start = candidate.find(marker)
        while start != -1:
            collapsed = candidate[:start] + "/" + candidate[start + len(marker) :]
            if collapsed not in patterns:
                patterns.add(collapsed)
                queue.append(collapsed)
            start = candidate.find(marker, start + 1)
    return any(fnmatch.fnmatchcase(path, candidate) for candidate in patterns)


def native_runtime_provenance_item(
    item: InventoryItem,
    provenance: list[NativeRuntimeProvenanceEntry],
) -> InventoryItem:
    if item.scope != "native-library" or not item.path:
        return item
    path = item.path.replace("\\", "/")
    for entry in provenance:
        if not any(path_matches_pattern(path, pattern) for pattern in entry.path_patterns):
            continue
        evidence_note = ", ".join(f"`{value}`" for value in entry.evidence)
        return InventoryItem(
            name=item.name,
            version=entry.source_package,
            license=entry.license,
            source=item.source,
            scope=item.scope,
            path=item.path,
            runtime=item.runtime,
            checksum_sha256=item.checksum_sha256,
            notice=(
                f"Native runtime file reviewed as part of {entry.source_package}. "
                f"Evidence: {evidence_note}."
            ),
            release_obligations=entry.obligations,
        )
    return item


def runtime_native_inventory(
    runtime_root: Path,
    native_provenance: list[NativeRuntimeProvenanceEntry] | None = None,
) -> list[InventoryItem]:
    if not runtime_root.exists():
        return []
    items = [
        item
        for item in file_inventory(runtime_root, "runtime-file", "runtime", runtime=True)
        if item.scope == "native-library"
    ]
    resolved_provenance = native_provenance or []
    return [native_runtime_provenance_item(item, resolved_provenance) for item in items]


def notice_inventory_version(item: InventoryItem) -> str:
    return item.version or "Not applicable"


def notice_inventory_license(item: InventoryItem) -> str:
    if item.license:
        return item.license
    if item.scope == "frontend-build-output":
        return "Covered by source package"
    if approved_native_library(item):
        return "Covered by reviewed runtime package"
    if item.scope in {"frontend-asset", "model-asset", "native-library"}:
        return "Release review required"
    return "Not specified"


def write_notices(path: Path, items: list[InventoryItem], _findings: list[PolicyFinding]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    notice_items = [
        (index, item) for index, item in enumerate((item for item in items if item.notice), start=1)
    ]
    notice_ids = {id(item): index for index, item in notice_items}
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
            license_value = notice_inventory_license(item)
            if item_notice_id := notice_ids.get(id(item)):
                license_value = f"{license_value} [{item_notice_id}]"
            writer.writerow(
                [
                    item.name,
                    notice_inventory_version(item),
                    license_value,
                    item.source,
                    item.scope,
                ]
            )
        if notice_items:
            handle.write("\n## License Notes\n\n")
            for index, item in notice_items:
                handle.write(f"- [{index}] {item.name}: {item.notice}\n")


def write_sbom(path: Path, items: list[InventoryItem]) -> None:
    write_json(
        path,
        {
            "format": "scriptscore-legal-sbom-v1",
            "componentCount": len(items),
            "components": [asdict(item) for item in items],
        },
    )


def release_obligation_entries(
    items: list[InventoryItem],
) -> list[dict[str, str | None]]:
    return [
        {
            "name": item.name,
            "version": item.version,
            "license": item.license,
            "source": item.source,
            "scope": item.scope,
            "releaseObligations": item.release_obligations,
        }
        for item in items
        if item.release_obligations
    ]


def generate(args: argparse.Namespace) -> int:
    output_dir = args.output_dir
    runtime_manifest = args.runtime_manifest
    runtime_root = runtime_manifest.parent

    python_items = python_inventory(runtime_manifest, args.python_root)
    npm_items = npm_inventory(args.npm_lock)
    fontsource_package_versions = npm_package_versions(
        args.npm_lock,
        {mapping["package_name"] for mapping in FONTSOURCE_ASSET_MAPPINGS},
    )
    static_asset_provenance = frontend_static_asset_provenance(args.frontend_asset_provenance)
    model_provenance = paddle_ocr_model_provenance(args.paddle_model_provenance)
    native_provenance = native_runtime_provenance(args.native_runtime_provenance)
    cargo_items = cargo_inventory(args.cargo_manifest, args.cargo_metadata_file)
    asset_items = frontend_build_inventory(
        args.frontend_build, fontsource_package_versions, static_asset_provenance
    )
    asset_items.extend(paddle_model_inventory(args.paddle_models, model_provenance))
    native_items = runtime_native_inventory(runtime_root, native_provenance)

    all_items = python_items + npm_items + cargo_items + asset_items + native_items
    findings = [finding for item in all_items if (finding := classify_item(item)) is not None]
    unknown_failure_scopes = {"python-runtime", "npm-runtime", "cargo-runtime"}
    blocked_skip_scopes = {"python-smoke-runtime", "npm-dev", "cargo-first-party"}
    unresolved_failure_scopes = {
        "cargo-runtime",
        "frontend-asset",
        "frontend-font-asset",
        "frontend-static-asset",
        "model-asset",
        "native-library",
        "npm-runtime",
        "paddle-ocr-model-asset",
        "python-runtime",
    }
    unresolved_release_findings = [
        finding
        for finding in findings
        if (finding.severity == "blocked" and finding.scope not in blocked_skip_scopes)
        or (finding.severity == "unknown" and finding.scope in unknown_failure_scopes)
        or (finding.severity == "review_required" and finding.scope in unresolved_failure_scopes)
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
                "checkModeFailure": "unresolved release-scope findings",
            },
            "summary": {
                "componentCount": len(all_items),
                "findingCount": len(findings),
                "blockedOrUnknownRuntimeCount": len(unresolved_release_findings),
                "unresolvedReleaseFindingCount": len(unresolved_release_findings),
            },
            "sourceOffer": {
                "label": "Corresponding Source",
                "url": args.source_url,
                "localNoticesPath": "legal/THIRD_PARTY_NOTICES.md",
            },
            "releaseObligations": release_obligation_entries(all_items),
            "findings": [asdict(finding) for finding in findings],
        },
    )

    if args.check and unresolved_release_findings:
        for finding in unresolved_release_findings:
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
    parser.add_argument(
        "--npm-lock", type=Path, default=DESKTOP_ROOT / "frontend" / "package-lock.json"
    )
    parser.add_argument(
        "--cargo-manifest",
        type=Path,
        default=DESKTOP_ROOT / "src-tauri" / "Cargo.toml",
    )
    parser.add_argument("--cargo-metadata-file", type=Path)
    parser.add_argument("--frontend-build", type=Path, default=DESKTOP_ROOT / "frontend" / "build")
    parser.add_argument(
        "--frontend-asset-provenance",
        type=Path,
        default=FRONTEND_STATIC_ASSET_PROVENANCE,
    )
    parser.add_argument(
        "--paddle-models", type=Path, default=PROJECT_ROOT / "cli" / "models" / "paddle"
    )
    parser.add_argument(
        "--paddle-model-provenance",
        type=Path,
        default=PADDLE_OCR_MODEL_PROVENANCE,
    )
    parser.add_argument(
        "--native-runtime-provenance",
        type=Path,
        default=NATIVE_RUNTIME_PROVENANCE,
    )
    return parser.parse_args(argv)


if __name__ == "__main__":
    raise SystemExit(generate(parse_args(sys.argv[1:])))
