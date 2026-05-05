#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Fail when private ScriptScorePlus implementation code enters public sources."""

from __future__ import annotations

import argparse
import os
import re
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parents[1]

FORBIDDEN_PATTERNS = [
    re.compile(r"\bfrom\s+scriptscore_plus\b"),
    re.compile(r"\bimport\s+scriptscore_plus\b"),
    re.compile(r"\bscriptscore_plus\.[A-Za-z_]"),
    re.compile(r"\bscriptscore-plus\s+wheel\b", re.IGNORECASE),
    re.compile(r"\bscriptscore-plus.*\.whl\b", re.IGNORECASE),
    re.compile(r"\bprivate\s+wheel\b", re.IGNORECASE),
    re.compile(r"\bprivate[_-]?wheel\b", re.IGNORECASE),
    re.compile(r"\bprivate\s+plugin\b", re.IGNORECASE),
    re.compile(r"\bprivate\s+desktop\s+add-on\b", re.IGNORECASE),
    re.compile(r"\bproprietary[_-]?provider\b", re.IGNORECASE),
    re.compile(r"\bscriptscore_plus_provider\b"),
]

SCAN_ROOTS = ("cli", "desktop", "docs", ".github", "scripts")
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
TEXT_SUFFIXES = {
    ".json",
    ".md",
    ".py",
    ".rs",
    ".sh",
    ".svelte",
    ".toml",
    ".ts",
    ".tsx",
    ".yml",
    ".yaml",
}
ALLOWLIST = {
    "desktop/frontend/src/lib/components/desktop/AiAssistanceStep.svelte",
    "cli/tests/test_desktop_legal_artifacts.py",
}


def project_relative(path: Path, root: Path) -> str:
    return path.resolve().relative_to(root).as_posix()


def os_walk_pruned(base: Path):
    for current, dirs, filenames in os.walk(base):
        dirs[:] = [name for name in dirs if name not in SKIP_PARTS]
        yield Path(current), dirs, filenames


def candidate_files(root: Path) -> list[Path]:
    files: list[Path] = []
    for scan_root in SCAN_ROOTS:
        base = root / scan_root
        if not base.exists():
            continue
        for current, _dirs, filenames in os_walk_pruned(base):
            for filename in filenames:
                path = current / filename
                if path.suffix in TEXT_SUFFIXES:
                    files.append(path)
    return sorted(files)


def scan(root: Path) -> list[str]:
    violations: list[str] = []
    for path in candidate_files(root):
        relative = project_relative(path, root)
        if relative in ALLOWLIST:
            continue
        text = path.read_text(encoding="utf-8", errors="ignore")
        for line_number, line in enumerate(text.splitlines(), start=1):
            if any(pattern.search(line) for pattern in FORBIDDEN_PATTERNS):
                violations.append(f"{relative}:{line_number}: {line.strip()}")
    return violations


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=PROJECT_ROOT)
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    violations = scan(args.root.resolve())
    if not violations:
        return 0
    print("Forbidden ScriptScorePlus implementation boundary patterns found:", file=sys.stderr)
    for violation in violations:
        print(f"- {violation}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
