# SPDX-License-Identifier: AGPL-3.0-only
"""Enforce subsystem coverage thresholds for core modules."""

from __future__ import annotations

import json
import sys
from pathlib import Path

THRESHOLDS = {
    "src/scriptscore/contracts/": 90.0,
    "src/scriptscore/commands/": 90.0,
    "src/scriptscore/runtime/": 90.0,
    "src/scriptscore/transport/": 90.0,
}

EXCLUDED_PREFIXES = (
    "src/scriptscore/providers/fake.py",
    "src/scriptscore/providers/__init__.py",
)


def file_coverage(entry: dict[str, object]) -> float:
    summary = entry["summary"]
    assert isinstance(summary, dict)
    covered = float(summary["covered_lines"])
    total = float(summary["num_statements"])
    if total == 0:
        return 100.0
    return (covered / total) * 100.0


def main(argv: list[str] | None = None) -> int:
    args = argv or sys.argv[1:]
    if len(args) != 1:
        raise SystemExit("usage: check_coverage_thresholds.py <coverage.json>")
    payload = json.loads(Path(args[0]).read_text(encoding="utf-8"))
    files = payload["files"]
    assert isinstance(files, dict)

    failed: list[str] = []
    for prefix, threshold in THRESHOLDS.items():
        matched = [
            file_coverage(entry)
            for path, entry in files.items()
            if path.startswith(prefix) and not path.startswith(EXCLUDED_PREFIXES)
        ]
        if not matched:
            failed.append(f"{prefix}: no measured files")
            continue
        average = sum(matched) / len(matched)
        if average < threshold:
            failed.append(f"{prefix}: {average:.2f}% < {threshold:.2f}%")
    if failed:
        for line in failed:
            print(line, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
