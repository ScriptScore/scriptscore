# SPDX-License-Identifier: AGPL-3.0-only
"""Deterministic image fixtures for scan command tests."""

from __future__ import annotations

from pathlib import Path

from PIL import Image


def make_rgb_page(
    path: Path,
    *,
    size: tuple[int, int] = (12, 12),
    color: tuple[int, int, int] = (255, 255, 255),
) -> Path:
    """Create a small RGB page fixture."""

    path.parent.mkdir(parents=True, exist_ok=True)
    Image.new("RGB", size, color=color).save(path, format="PNG")
    return path


def put_pixel(path: Path, *, xy: tuple[int, int], color: tuple[int, int, int]) -> None:
    """Mutate one pixel for deterministic transform assertions."""

    with Image.open(path) as image:
        updated = image.copy()
    updated.putpixel(xy, color)
    updated.save(path, format="PNG")
