#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Generate release legal inventories, notices, and policy findings."""

from __future__ import annotations

import argparse
import csv
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
    "pip-api": "Dev-only security tooling dependency must not appear in distributed runtime artifacts.",
    "pip-audit": "Dev-only security tooling must not appear in distributed runtime artifacts.",
    "pylint": "Dev-only quality package must not appear in distributed runtime artifacts.",
}
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
PYTHON_LICENSE_REPLACEMENTS = {
    "aistudio-sdk": (
        "LicenseRef-REVIEW-aistudio-sdk",
        "Upstream wheel metadata declares License: UNKNOWN; keep this runtime "
        "dependency in release review until upstream publishes usable license "
        "metadata or the dependency is removed.",
    ),
    "pandas": (
        "BSD-3-Clause",
        "Wheel metadata can embed full bundled dependency notices in the License "
        "field; keep the inventory summary to the package license expression.",
    ),
    "scipy": (
        "BSD-3-Clause AND BSD-3-Clause-Open-MPI AND GPL-3.0-or-later WITH GCC-exception-3.1",
        "SciPy wheel metadata embeds full bundled-library notices. Classify the "
        "effective license expressions so blocked-word scans do not match GPL "
        "runtime exception prose.",
    ),
    "python-dateutil": (
        "BSD-3-Clause OR Apache-2.0",
        "Wheel metadata can declare only 'Dual License'; normalize to the "
        "upstream-documented BSD-3-Clause or Apache-2.0 expression.",
    ),
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


def python_license_metadata(row: dict[str, Any]) -> tuple[str | None, str | None]:
    replacement = PYTHON_LICENSE_REPLACEMENTS.get(row["name"].lower())
    if replacement is not None:
        license_value, notice = replacement
        return normalize_license(license_value), notice
    return normalize_license(row.get("license")), None


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
    items: list[InventoryItem] = []
    for row in sorted(rows, key=lambda row: row["name"].lower()):
        license_value, notice = python_license_metadata(row)
        items.append(
            InventoryItem(
                name=row["name"],
                version=row.get("version"),
                license=license_value,
                source="python",
                scope=scope,
                runtime=portable_release,
                notice=notice,
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


def frontend_build_inventory(root: Path) -> list[InventoryItem]:
    if not root.exists():
        return []
    items: list[InventoryItem] = []
    for path in sorted(item for item in root.rglob("*") if item.is_file()):
        suffix = path.suffix.lower()
        if suffix not in ASSET_SUFFIXES and suffix not in NATIVE_SUFFIXES:
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


def runtime_native_inventory(runtime_root: Path) -> list[InventoryItem]:
    if not runtime_root.exists():
        return []
    return [
        item
        for item in file_inventory(runtime_root, "runtime-file", "runtime", runtime=True)
        if item.scope == "native-library"
    ]


def notice_inventory_version(item: InventoryItem) -> str:
    return item.version or "Not applicable"


def notice_inventory_license(item: InventoryItem) -> str:
    if item.license:
        return item.license
    if item.scope == "frontend-build-output":
        return "Covered by source package"
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


def generate(args: argparse.Namespace) -> int:
    output_dir = args.output_dir
    runtime_manifest = args.runtime_manifest
    runtime_root = runtime_manifest.parent

    python_items = python_inventory(runtime_manifest, args.python_root)
    npm_items = npm_inventory(args.npm_lock)
    cargo_items = cargo_inventory(args.cargo_manifest, args.cargo_metadata_file)
    asset_items = frontend_build_inventory(args.frontend_build)
    asset_items.extend(file_inventory(args.paddle_models, "model-asset", "assets", runtime=True))
    native_items = runtime_native_inventory(runtime_root)

    all_items = python_items + npm_items + cargo_items + asset_items + native_items
    findings = [finding for item in all_items if (finding := classify_item(item)) is not None]
    unknown_failure_scopes = {"python-runtime", "npm-runtime", "cargo-runtime"}
    blocked_skip_scopes = {"python-smoke-runtime", "npm-dev", "cargo-first-party"}
    blocked_or_unknown_runtime = [
        finding
        for finding in findings
        if (finding.severity == "blocked" and finding.scope not in blocked_skip_scopes)
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
        "--paddle-models", type=Path, default=PROJECT_ROOT / "cli" / "models" / "paddle"
    )
    return parser.parse_args(argv)


if __name__ == "__main__":
    raise SystemExit(generate(parse_args(sys.argv[1:])))
