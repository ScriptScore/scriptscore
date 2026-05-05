#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Check first-party source files for AGPL SPDX headers."""

from __future__ import annotations

import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HEADER = "SPDX-License-Identifier: AGPL-3.0-only"
CHECK_ROOTS = ("cli/src", "cli/tests", "desktop/frontend/src", "desktop/src-tauri/src", "desktop/src-tauri/tests", "desktop/scripts", "scripts")
SUFFIXES = {".py", ".rs", ".sh", ".svelte", ".ts", ".tsx", ".js"}
SKIP_PARTS = {
    ".git",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".svelte-kit",
    ".venv",
    ".wheel-smoke",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "target",
}


def candidates() -> list[Path]:
    files: list[Path] = []
    for root_name in CHECK_ROOTS:
        base = ROOT / root_name
        if not base.exists():
            continue
        for current, dirs, filenames in os.walk(base):
            dirs[:] = [name for name in dirs if name not in SKIP_PARTS]
            for filename in filenames:
                path = Path(current) / filename
                if path.suffix in SUFFIXES:
                    files.append(path)
    return sorted(files)


def main() -> int:
    missing: list[str] = []
    for path in candidates():
        text = path.read_text(encoding="utf-8", errors="ignore")
        first_lines = "\n".join(text.splitlines()[:8])
        if HEADER not in first_lines:
            missing.append(path.relative_to(ROOT).as_posix())
    if not missing:
        return 0
    print("Files missing AGPL SPDX header:", file=sys.stderr)
    for item in missing:
        print(f"- {item}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
