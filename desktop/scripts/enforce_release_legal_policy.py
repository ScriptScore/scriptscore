#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Enforce release legal policy decisions against generated RC artifacts."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parents[1]
DESKTOP_ROOT = PROJECT_ROOT / "desktop"

ISSUE_28_PACKAGE_NAMES = frozenset(
    {
        "crc32c",
        "bce-python-sdk",
        "opencv-contrib-python",
        "opencv-contrib-python-headless",
        "paddlepaddle",
        "pymupdf",
        "python-bidi",
    }
)
FORBIDDEN_RELEASE_PACKAGES = {
    "bce-python-sdk": "Baidu cloud SDK is not used by ScriptScore's offline OCR path.",
    "crc32c": "Transitive bce-python-sdk dependency is not used by ScriptScore's offline OCR path.",
    "opencv-contrib-python": "Use opencv-contrib-python-headless in release runtimes.",
}
UNRESOLVED_SEVERITIES = frozenset({"blocked", "review_required", "unknown"})


class ReleaseLegalPolicyError(RuntimeError):
    """Raised when generated legal artifacts do not satisfy release policy."""


def normalize_package_name(name: str) -> str:
    return name.strip().lower().replace("_", "-")


def read_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def report_path(legal_root: Path) -> Path:
    path = legal_root / "license-policy-report.json"
    if not path.is_file():
        raise ReleaseLegalPolicyError(f"Missing license policy report: {path}")
    return path


def sbom_path(legal_root: Path, name: str) -> Path:
    path = legal_root / name
    if not path.is_file():
        raise ReleaseLegalPolicyError(f"Missing generated SBOM: {path}")
    return path


def python_component_names(legal_root: Path) -> set[str]:
    sbom = read_json(sbom_path(legal_root, "sbom-python.json"))
    components = sbom.get("components", [])
    if not isinstance(components, list):
        raise ReleaseLegalPolicyError("sbom-python.json components must be a list.")
    names: set[str] = set()
    for component in components:
        if not isinstance(component, dict):
            continue
        name = component.get("name")
        if isinstance(name, str) and name:
            names.add(normalize_package_name(name))
    return names


def validate_asset_sbom(legal_root: Path) -> None:
    sbom = read_json(sbom_path(legal_root, "sbom-assets.json"))
    if not isinstance(sbom.get("components", []), list):
        raise ReleaseLegalPolicyError("sbom-assets.json components must be a list.")


def unresolved_issue_28_findings(legal_root: Path) -> list[str]:
    report = read_json(report_path(legal_root))
    findings = report.get("findings", [])
    if not isinstance(findings, list):
        raise ReleaseLegalPolicyError("license-policy-report.json findings must be a list.")

    unresolved: list[str] = []
    for finding in findings:
        if not isinstance(finding, dict):
            continue
        item = finding.get("item")
        severity = finding.get("severity")
        if not isinstance(item, str) or not isinstance(severity, str):
            continue
        normalized_item = normalize_package_name(item)
        if normalized_item not in ISSUE_28_PACKAGE_NAMES:
            continue
        if severity not in UNRESOLVED_SEVERITIES:
            continue
        source = finding.get("source") if isinstance(finding.get("source"), str) else "unknown"
        scope = finding.get("scope") if isinstance(finding.get("scope"), str) else "unknown"
        message = finding.get("message") if isinstance(finding.get("message"), str) else ""
        unresolved.append(f"{severity}: {item} ({source}, {scope}): {message}".rstrip())
    return unresolved


def enforce_release_legal_policy(legal_root: Path) -> None:
    names = python_component_names(legal_root)
    validate_asset_sbom(legal_root)
    forbidden_found = sorted(set(FORBIDDEN_RELEASE_PACKAGES) & names)
    errors = [
        f"forbidden runtime package present: {name}: {FORBIDDEN_RELEASE_PACKAGES[name]}"
        for name in forbidden_found
    ]
    errors.extend(unresolved_issue_28_findings(legal_root))
    if errors:
        raise ReleaseLegalPolicyError("\n".join(errors))


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Enforce ScriptScore release legal policy against generated artifacts.",
    )
    parser.add_argument(
        "--legal-root",
        type=Path,
        default=DESKTOP_ROOT / "dist" / "legal",
        help="Generated legal artifact directory.",
    )
    return parser


def main() -> int:
    args = build_parser().parse_args()
    enforce_release_legal_policy(args.legal_root)
    print(f"Release legal policy checks passed for {args.legal_root}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ReleaseLegalPolicyError as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from None
