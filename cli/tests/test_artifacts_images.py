# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for shared image artifact helpers."""

from __future__ import annotations

from pathlib import Path

import pytest
from PIL import Image

from scriptscore.artifacts.images import save_png


def test_save_png_uses_fast_png_settings_and_creates_parent_dir(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    image = Image.new("RGB", (2, 2), color="white")
    output_path = tmp_path / "nested" / "page.png"
    saved: dict[str, object] = {}

    def _capture_save(
        self: Image.Image, fp: str | Path, format: str | None = None, **params: object
    ) -> None:
        saved["image"] = self
        saved["path"] = fp
        saved["format"] = format
        saved["params"] = params

    monkeypatch.setattr(Image.Image, "save", _capture_save)

    save_png(image, output_path)

    assert output_path.parent.is_dir()
    assert saved == {
        "image": image,
        "path": output_path,
        "format": "PNG",
        "params": {"optimize": False, "compress_level": 6},
    }
