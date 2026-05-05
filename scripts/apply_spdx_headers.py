#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Apply AGPL SPDX headers to first-party source files."""

from __future__ import annotations

import os
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


def comment_for(path: Path) -> str:
    if path.suffix == ".svelte":
        return f"<!-- {HEADER} -->"
    return f"# {HEADER}" if path.suffix in {".py", ".sh"} else f"// {HEADER}"


def add_header(path: Path) -> bool:
    text = path.read_text(encoding="utf-8")
    if HEADER in "\n".join(text.splitlines()[:8]):
        return False
    lines = text.splitlines(keepends=True)
    header = comment_for(path) + "\n"
    insert_at = 0
    if lines and lines[0].startswith("#!"):
        insert_at = 1
    lines.insert(insert_at, header)
    path.write_text("".join(lines), encoding="utf-8")
    return True


def main() -> int:
    changed = [path for path in candidates() if add_header(path)]
    print(f"updated {len(changed)} files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
