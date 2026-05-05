# SPDX-License-Identifier: AGPL-3.0-only
"""Shared path helper tests."""

from __future__ import annotations

from pathlib import Path

import pytest

from scriptscore.paths import (
    ensure_within_root,
    join_under_root,
    require_absolute_path,
    require_safe_path_component,
)


def test_require_absolute_path_rejects_relative_path(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="must be an absolute path"):
        require_absolute_path(Path("relative.txt"), field_name="demo")

    resolved = require_absolute_path((tmp_path / "file.txt").resolve(), field_name="demo")
    assert resolved.is_absolute()


def test_require_safe_path_component_rejects_traversal() -> None:
    with pytest.raises(ValueError, match="must not contain path separators"):
        require_safe_path_component("../escape", field_name="student_ref")

    assert require_safe_path_component("scan_001", field_name="student_ref") == "scan_001"


def test_join_under_root_and_ensure_within_root_block_escape(tmp_path: Path) -> None:
    root = (tmp_path / "out").resolve()
    child = join_under_root(root, "scan_001", "page_001.png")
    assert child == root / "scan_001" / "page_001.png"

    with pytest.raises(ValueError, match="must stay within"):
        ensure_within_root(tmp_path / "outside.txt", root=root, field_name="artifact.path")
