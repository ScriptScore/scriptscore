#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Check commits in a range for DCO Signed-off-by trailers."""

from __future__ import annotations

import re
import subprocess
import sys

SIGNOFF_RE = re.compile(r"^Signed-off-by:\s+.+\s+<[^>]+>\s*$", re.MULTILINE)


def commits(rev_range: str) -> list[str]:
    result = subprocess.run(
        ["git", "rev-list", "--no-merges", rev_range],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    )
    return [line.strip() for line in result.stdout.splitlines() if line.strip()]


def message(commit: str) -> str:
    result = subprocess.run(
        ["git", "log", "-1", "--format=%B", commit],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    )
    return result.stdout


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print("usage: check_dco.py <rev-range>", file=sys.stderr)
        return 2
    missing: list[str] = []
    for commit in commits(argv[0]):
        if not SIGNOFF_RE.search(message(commit)):
            missing.append(commit)
    if not missing:
        return 0
    print("Commits missing Signed-off-by trailers:", file=sys.stderr)
    for commit in missing:
        print(f"- {commit}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
