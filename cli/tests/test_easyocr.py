# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for internal EasyOCR integration helpers."""

from __future__ import annotations

import json
import warnings
from pathlib import Path

import pytest

from scriptscore.contracts import ErrorCategory, ScriptscoreError
from scriptscore.ocr.easyocr import OcrTextBox, read_page_ocr


def test_read_page_ocr_uses_test_env_override(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    image_path = tmp_path / "page.png"
    image_path.write_bytes(b"not-used")
    monkeypatch.setenv(
        "SCRIPTSCORE_TEST_EASYOCR_BOXES",
        json.dumps(
            [
                {
                    "text": "Question 1",
                    "left": 10,
                    "top": 12,
                    "right": 44,
                    "bottom": 20,
                    "confidence": 0.98,
                }
            ]
        ),
    )

    results = read_page_ocr(image_path)

    assert results == [
        OcrTextBox(text="Question 1", left=10, top=12, right=44, bottom=20, confidence=0.98)
    ]


def test_read_page_ocr_maps_malformed_coordinate_rows_to_external_dependency(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    image_path = tmp_path / "page.png"
    image_path.write_bytes(b"not-used")

    class _Torch:
        class cuda:
            @staticmethod
            def is_available() -> bool:
                return False

    monkeypatch.delenv("SCRIPTSCORE_TEST_EASYOCR_BOXES", raising=False)
    monkeypatch.setattr("scriptscore.ocr.easyocr._load_dependencies", lambda: (object, _Torch))
    monkeypatch.setattr("scriptscore.ocr.easyocr._reader_factory", lambda gpu: object())
    monkeypatch.setattr(
        "scriptscore.ocr.easyocr._reader_readtext",
        lambda _reader, _image_path: [("bad-box", "Question 1", 0.99)],
    )

    with pytest.raises(ScriptscoreError) as excinfo:
        read_page_ocr(image_path)

    assert excinfo.value.category is ErrorCategory.EXTERNAL_DEPENDENCY
    assert excinfo.value.code == "ocr_response_invalid"


def test_read_page_ocr_accepts_numpy_scalar_coordinate_rows(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    np = pytest.importorskip("numpy")
    image_path = tmp_path / "page.png"
    image_path.write_bytes(b"not-used")

    class _Torch:
        class cuda:
            @staticmethod
            def is_available() -> bool:
                return False

    monkeypatch.delenv("SCRIPTSCORE_TEST_EASYOCR_BOXES", raising=False)
    monkeypatch.setattr("scriptscore.ocr.easyocr._load_dependencies", lambda: (object, _Torch))
    monkeypatch.setattr("scriptscore.ocr.easyocr._reader_factory", lambda gpu: object())
    monkeypatch.setattr(
        "scriptscore.ocr.easyocr._reader_readtext",
        lambda _reader, _image_path: [
            (
                [
                    [np.int32(150), np.int32(87)],
                    [np.int32(332), np.int32(87)],
                    [np.int32(332), np.int32(113)],
                    [np.int32(150), np.int32(113)],
                ],
                "Data Structures Quiz",
                np.float64(0.8711818404458288),
            )
        ],
    )

    results = read_page_ocr(image_path)

    assert len(results) == 1
    assert results[0].text == "Data Structures Quiz"
    assert results[0].left == 150
    assert results[0].top == 87
    assert results[0].right == 332
    assert results[0].bottom == 113
    assert results[0].confidence == pytest.approx(0.8711818404458288)


def test_read_page_ocr_suppresses_library_stdout(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    image_path = tmp_path / "page.png"
    image_path.write_bytes(b"not-used")

    class _Torch:
        class cuda:
            @staticmethod
            def is_available() -> bool:
                return False

    class _Reader:
        def __init__(self, _languages: list[str], *, gpu: bool) -> None:
            print("EasyOCR setup noise")
            assert gpu is False

        def readtext(
            self, _image_path: str, *, detail: int, paragraph: bool
        ) -> list[tuple[object, str, float]]:
            print("Downloading detection model, please wait. This should not hit stdout.")
            assert detail == 1
            assert paragraph is False
            return [([[10, 12], [44, 12], [44, 20], [10, 20]], "Question 1", 0.98)]

    monkeypatch.delenv("SCRIPTSCORE_TEST_EASYOCR_BOXES", raising=False)
    monkeypatch.setattr("scriptscore.ocr.easyocr._load_dependencies", lambda: (_Reader, _Torch))

    results = read_page_ocr(image_path)

    captured = capsys.readouterr()
    assert captured.out == ""
    assert results == [
        OcrTextBox(text="Question 1", left=10, top=12, right=44, bottom=20, confidence=0.98)
    ]


def test_read_page_ocr_suppresses_known_easyocr_runtime_warnings(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    image_path = tmp_path / "page.png"
    image_path.write_bytes(b"not-used")

    class _Torch:
        class cuda:
            @staticmethod
            def is_available() -> bool:
                return False

    class _Reader:
        def __init__(self, _languages: list[str], *, gpu: bool) -> None:
            assert gpu is False
            warnings.warn(
                "torch.ao.quantization is deprecated and will be removed in 2.10.",
                DeprecationWarning,
                stacklevel=2,
            )

        def readtext(
            self, _image_path: str, *, detail: int, paragraph: bool
        ) -> list[tuple[object, str, float]]:
            warnings.warn(
                "'pin_memory' argument is set as true but no accelerator is found, then device pinned memory won't be used.",
                UserWarning,
                stacklevel=2,
            )
            warnings.warn(
                "'mode' parameter is deprecated and will be removed in Pillow 13 (2026-10-15)",
                DeprecationWarning,
                stacklevel=2,
            )
            assert detail == 1
            assert paragraph is False
            return [([[10, 12], [44, 12], [44, 20], [10, 20]], "Question 1", 0.98)]

    monkeypatch.delenv("SCRIPTSCORE_TEST_EASYOCR_BOXES", raising=False)
    monkeypatch.setattr("scriptscore.ocr.easyocr._load_dependencies", lambda: (_Reader, _Torch))

    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        results = read_page_ocr(image_path)

    assert caught == []
    assert results == [
        OcrTextBox(text="Question 1", left=10, top=12, right=44, bottom=20, confidence=0.98)
    ]
