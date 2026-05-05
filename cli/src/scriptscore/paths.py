# SPDX-License-Identifier: AGPL-3.0-only
"""Small shared path helpers for command validation and artifact writes."""

from __future__ import annotations

from pathlib import Path, PurePosixPath, PureWindowsPath


def require_absolute_path(path: Path, *, field_name: str) -> Path:
    """Validate that a path is absolute and return its resolved form."""

    if not path.is_absolute():
        raise ValueError(f"{field_name} must be an absolute path.")
    return path.resolve()


def require_safe_path_component(value: str, *, field_name: str) -> str:
    """Validate that a caller-supplied identifier is safe as one path segment."""

    for path_cls in (PurePosixPath, PureWindowsPath):
        path = path_cls(value)
        if (
            path.is_absolute()
            or bool(path.drive)
            or bool(path.root)
            or bool(path.anchor)
            or len(path.parts) != 1
            or path.parts[0] in {".", ".."}
        ):
            raise ValueError(
                f"{field_name} must not contain path separators or traversal components."
            )
    return value


def ensure_within_root(path: Path, *, root: Path, field_name: str) -> Path:
    """Resolve a path and verify that it stays within the resolved root."""

    resolved_root = root.resolve()
    resolved_path = path.resolve()
    try:
        resolved_path.relative_to(resolved_root)
    except ValueError as exc:
        raise ValueError(f"{field_name} must stay within {resolved_root}.") from exc
    return resolved_path


def join_under_root(root: Path, *parts: str) -> Path:
    """Join caller-controlled path components under a resolved root."""

    return ensure_within_root(root.joinpath(*parts), root=root, field_name="derived artifact path")
