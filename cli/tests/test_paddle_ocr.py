# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for internal PaddleOCR page OCR helpers."""

from __future__ import annotations

from pathlib import Path

import pytest

from scriptscore.ocr.paddle import read_page_ocr
from scriptscore.pii_scan.types import ReadResult, VisionToken


class _Reader:
    def read(self, image: object) -> ReadResult:
        assert image is not None
        return ReadResult(
            backend_name="test",
            tokens=[
                VisionToken(
                    text="1. First question",
                    confidence=0.91,
                    corners=((10, 12), (80, 11), (82, 24), (9, 25)),
                )
            ],
        )


class _NoisyReader:
    def read(self, image: object) -> ReadResult:
        assert image is not None
        print("paddle read banner")
        return ReadResult(backend_name="test", tokens=[])


def test_read_page_ocr_uses_paddle_reader(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    image_path = tmp_path / "page.png"
    image_path.write_bytes(b"not-a-real-png")
    monkeypatch.setenv("SCRIPTSCORE_DETECT_PADDLE_MODEL_DIR", str(tmp_path / "models"))
    monkeypatch.setattr("scriptscore.ocr.paddle.create_reader", lambda _model_dir: _Reader())
    monkeypatch.setattr("scriptscore.ocr.paddle.cv2.imread", lambda _path, _mode: object())

    boxes = read_page_ocr(image_path)

    assert len(boxes) == 1
    assert boxes[0].text == "1. First question"
    assert boxes[0].left == 9
    assert boxes[0].top == 11
    assert boxes[0].right == 82
    assert boxes[0].bottom == 25


def test_read_page_ocr_suppresses_paddle_stdout(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    image_path = tmp_path / "page.png"
    image_path.write_bytes(b"not-a-real-png")
    monkeypatch.setenv("SCRIPTSCORE_DETECT_PADDLE_MODEL_DIR", str(tmp_path / "models"))

    def noisy_create_reader(_model_dir: Path) -> _NoisyReader:
        print("paddle init banner")
        return _NoisyReader()

    monkeypatch.setattr("scriptscore.ocr.paddle.create_reader", noisy_create_reader)
    monkeypatch.setattr("scriptscore.ocr.paddle.cv2.imread", lambda _path, _mode: object())

    assert read_page_ocr(image_path) == []
    assert capsys.readouterr().out == ""
