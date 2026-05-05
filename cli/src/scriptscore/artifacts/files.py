# SPDX-License-Identifier: AGPL-3.0-only
"""Shared filesystem helpers for artifact handling."""

from __future__ import annotations

from hashlib import sha256
from pathlib import Path


def file_sha256(path: Path) -> str:
    """Return the sha256 digest for one file."""

    digest = sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(8192), b""):
            digest.update(chunk)
    return digest.hexdigest()
