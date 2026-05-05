# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for `scans.ocr` (stdin PNG -> transient hint text)."""

from __future__ import annotations

import io
from pathlib import Path
from typing import Any

import pytest
from PIL import Image

from scriptscore.commands import build_command_registry, scans_ocr
from scriptscore.contracts import ScriptscoreError
from scriptscore.contracts.envelopes import CommandErrorEnvelope, CommandSuccessEnvelope
from scriptscore.providers import ProviderRegistry
from scriptscore.runtime import CommandRunner


class _BinaryStdin:
    def __init__(self, data: bytes) -> None:
        self.buffer = io.BytesIO(data)


def _png_bytes() -> bytes:
    image = Image.new("RGB", (32, 16), color=(255, 255, 255))
    buf = io.BytesIO()
    image.save(buf, format="PNG")
    return buf.getvalue()


def _runner() -> CommandRunner:
    return CommandRunner(
        registry=build_command_registry(),
        provider_registry=ProviderRegistry.with_builtin_fakes(),
    )


def test_scans_ocr_returns_hint_text_and_no_artifacts(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    monkeypatch.setenv("SCRIPTSCORE_TEST_OCR_HINT", "SMITH, JORDAN A. - #20391")
    monkeypatch.setattr("sys.stdin", _BinaryStdin(_png_bytes()))

    result = _runner().run("scans.ocr", {}, request_id="req_test")

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    data = result.envelope.data
    assert data["hint_text"] == "SMITH, JORDAN A. - #20391"
    assert data["segment_count"] == 0
    assert result.envelope.artifacts == []


def test_scans_ocr_joins_multiple_easyocr_rows_sorted_top_down(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    monkeypatch.delenv("SCRIPTSCORE_TEST_OCR_HINT", raising=False)
    monkeypatch.setattr("scriptscore.commands.scans_ocr._read_paddle_hint", lambda _raw: None)

    class _Reader:
        def readtext(
            self, _image: object, *, detail: int, paragraph: bool
        ) -> list[tuple[Any, Any, Any]]:
            return [
                ([[0, 40], [80, 40], [80, 55], [0, 55]], "World", 0.9),
                ([[0, 10], [60, 10], [60, 25], [0, 25]], "Hello", 0.85),
            ]

    monkeypatch.setattr("sys.stdin", _BinaryStdin(_png_bytes()))
    monkeypatch.setattr("scriptscore.commands.scans_ocr._load_easyocr_reader", lambda: _Reader())

    result = _runner().run("scans.ocr", {}, request_id="req_test")

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    data = result.envelope.data
    assert data["hint_text"] == "Hello World"
    assert data["segment_count"] == 2


def test_scans_ocr_scrubs_leading_printed_name_label(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("SCRIPTSCORE_TEST_OCR_HINT", raising=False)
    monkeypatch.setattr("scriptscore.commands.scans_ocr._read_paddle_hint", lambda _raw: None)

    class _Reader:
        def readtext(
            self, _image: object, *, detail: int, paragraph: bool
        ) -> list[tuple[Any, Any, Any]]:
            return [
                ([[0, 10], [60, 10], [60, 25], [0, 25]], "Name:", 0.95),
                ([[65, 10], [140, 10], [140, 25], [65, 25]], "Jane Doe", 0.85),
            ]

    monkeypatch.setattr("sys.stdin", _BinaryStdin(_png_bytes()))
    monkeypatch.setattr("scriptscore.commands.scans_ocr._load_easyocr_reader", lambda: _Reader())

    result = _runner().run("scans.ocr", {}, request_id="req_test")

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.data == {"hint_text": "Jane Doe", "segment_count": 2}


def test_scans_ocr_rejects_label_only_easyocr_output(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("SCRIPTSCORE_TEST_OCR_HINT", raising=False)
    monkeypatch.setattr("scriptscore.commands.scans_ocr._read_paddle_hint", lambda _raw: None)

    class _Reader:
        def readtext(
            self, _image: object, *, detail: int, paragraph: bool
        ) -> list[tuple[Any, Any, Any]]:
            return [
                ([[0, 10], [80, 10], [80, 25], [0, 25]], "Student Name: ____", 0.9),
            ]

    monkeypatch.setattr("sys.stdin", _BinaryStdin(_png_bytes()))
    monkeypatch.setattr("scriptscore.commands.scans_ocr._load_easyocr_reader", lambda: _Reader())

    result = _runner().run("scans.ocr", {}, request_id="req_test")

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.data == {"hint_text": "", "segment_count": 1}


def test_scans_ocr_uses_paddle_hint_before_easyocr(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("SCRIPTSCORE_TEST_OCR_HINT", raising=False)
    monkeypatch.setattr("sys.stdin", _BinaryStdin(_png_bytes()))
    monkeypatch.setattr(
        "scriptscore.commands.scans_ocr._read_paddle_hint",
        lambda _raw: ("Sample Person", 1),
    )
    monkeypatch.setattr(
        "scriptscore.commands.scans_ocr._read_easyocr_hint",
        lambda _raw: pytest.fail("EasyOCR should not run when Paddle returns a usable hint"),
    )

    result = _runner().run("scans.ocr", {}, request_id="req_test")

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.data == {"hint_text": "Sample Person", "segment_count": 1}


def test_scans_ocr_paddle_candidates_choose_best_cleaned_hint(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv("SCRIPTSCORE_TEST_OCR_HINT", raising=False)
    monkeypatch.setattr("sys.stdin", _BinaryStdin(_png_bytes()))

    class _PaddleEngine:
        def __init__(self) -> None:
            self.calls = 0

        def ocr(
            self, _image: object, *, det: bool, rec: bool, cls: bool
        ) -> list[tuple[str, float]]:
            self.calls += 1
            if self.calls == 1:
                return [("Name:", 0.99)]
            if self.calls == 2:
                return [("Name: Jane Doe", 0.75)]
            return [("J Doe", 0.25)]

    monkeypatch.setattr(
        "scriptscore.commands.scans_ocr._load_paddle_ocr_engine", lambda: _PaddleEngine()
    )
    monkeypatch.setattr(
        "scriptscore.commands.scans_ocr._read_easyocr_hint",
        lambda _raw: pytest.fail("EasyOCR should not run when Paddle has a usable cleaned hint"),
    )

    result = _runner().run("scans.ocr", {}, request_id="req_test")

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.data == {"hint_text": "Jane Doe", "segment_count": 1}


def test_scans_ocr_falls_back_when_paddle_returns_label_only(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv("SCRIPTSCORE_TEST_OCR_HINT", raising=False)
    monkeypatch.setattr("sys.stdin", _BinaryStdin(_png_bytes()))

    class _PaddleEngine:
        def ocr(
            self, _image: object, *, det: bool, rec: bool, cls: bool
        ) -> list[tuple[str, float]]:
            return [("Name: ____", 0.95)]

    monkeypatch.setattr(
        "scriptscore.commands.scans_ocr._load_paddle_ocr_engine", lambda: _PaddleEngine()
    )
    monkeypatch.setattr(
        "scriptscore.commands.scans_ocr._read_easyocr_hint",
        lambda _raw: ("fallback text", 2),
    )

    result = _runner().run("scans.ocr", {}, request_id="req_test")

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.data == {"hint_text": "fallback text", "segment_count": 2}


def test_scans_ocr_falls_back_to_easyocr_when_paddle_unusable(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv("SCRIPTSCORE_TEST_OCR_HINT", raising=False)
    monkeypatch.setattr("sys.stdin", _BinaryStdin(_png_bytes()))
    monkeypatch.setattr("scriptscore.commands.scans_ocr._read_paddle_hint", lambda _raw: None)
    monkeypatch.setattr(
        "scriptscore.commands.scans_ocr._read_easyocr_hint",
        lambda _raw: ("fallback text", 2),
    )

    result = _runner().run("scans.ocr", {}, request_id="req_test")

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.data == {"hint_text": "fallback text", "segment_count": 2}


def test_scans_ocr_rejects_empty_stdin(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("SCRIPTSCORE_TEST_OCR_HINT", raising=False)
    monkeypatch.setattr("sys.stdin", _BinaryStdin(b""))

    result = _runner().run("scans.ocr", {}, request_id="req_empty")

    assert result.exit_code == 2
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.code == "ocr_input_missing"
    assert result.envelope.error.category == "validation"
    assert result.envelope.error.write_state == "no_write"


def test_scans_ocr_rejects_invalid_png_stdin(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("SCRIPTSCORE_TEST_OCR_HINT", raising=False)
    monkeypatch.setattr("sys.stdin", _BinaryStdin(b"not a png"))

    result = _runner().run("scans.ocr", {}, request_id="req_invalid")

    assert result.exit_code == 2
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.code == "ocr_input_invalid"
    assert result.envelope.error.category == "validation"
    assert result.envelope.error.details["error_type"]


def test_scans_ocr_rejects_oversized_stdin(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("sys.stdin", _BinaryStdin(b"abcdef"))

    with pytest.raises(ScriptscoreError) as exc_info:
        scans_ocr._read_stdin_png_bytes(max_bytes=5)

    assert exc_info.value.code == "ocr_input_too_large"
    assert exc_info.value.details == {"max_bytes": 5, "byte_size": 6}


def test_scans_ocr_easyocr_dependency_failure_uses_stable_error(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv("SCRIPTSCORE_TEST_OCR_HINT", raising=False)
    monkeypatch.setattr("sys.stdin", _BinaryStdin(_png_bytes()))
    monkeypatch.setattr("scriptscore.commands.scans_ocr._read_paddle_hint", lambda _raw: None)

    def load_reader() -> object:
        raise scans_ocr._external_error(
            code="ocr_dependency_unavailable",
            message="EasyOCR dependencies are not installed.",
            details={"missing_module": "easyocr"},
            retryable=False,
        )

    monkeypatch.setattr("scriptscore.commands.scans_ocr._load_easyocr_reader", load_reader)

    result = _runner().run("scans.ocr", {}, request_id="req_dependency")

    assert result.exit_code == 6
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.code == "ocr_dependency_unavailable"
    assert result.envelope.error.category == "external_dependency"
    assert result.envelope.error.write_state == "no_write"


def test_scans_ocr_easyocr_runtime_failure_uses_stable_error(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv("SCRIPTSCORE_TEST_OCR_HINT", raising=False)
    monkeypatch.setattr("sys.stdin", _BinaryStdin(_png_bytes()))
    monkeypatch.setattr("scriptscore.commands.scans_ocr._read_paddle_hint", lambda _raw: None)

    class _Reader:
        def readtext(self, _image: object, *, detail: int, paragraph: bool) -> list[object]:
            raise RuntimeError("reader exploded")

    monkeypatch.setattr("scriptscore.commands.scans_ocr._load_easyocr_reader", lambda: _Reader())

    result = _runner().run("scans.ocr", {}, request_id="req_runtime")

    assert result.exit_code == 6
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.code == "ocr_runtime_failed"
    assert result.envelope.error.category == "external_dependency"
    assert result.envelope.error.retryable is True


def test_scans_ocr_easyocr_raw_read_failure_falls_back_to_decoded_rgb(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv("SCRIPTSCORE_TEST_OCR_HINT", raising=False)
    monkeypatch.setattr("sys.stdin", _BinaryStdin(_png_bytes()))
    monkeypatch.setattr("scriptscore.commands.scans_ocr._read_paddle_hint", lambda _raw: None)

    calls: list[str] = []

    class _Reader:
        def readtext(
            self, image: object, *, detail: int, paragraph: bool
        ) -> list[tuple[Any, str, float]]:
            calls.append(type(image).__name__)
            if isinstance(image, bytes):
                raise RuntimeError("raw read failed")
            return [([[0, 0], [20, 0], [20, 10], [0, 10]], "Fallback", 0.9)]

    monkeypatch.setattr("scriptscore.commands.scans_ocr._load_easyocr_reader", lambda: _Reader())

    result = _runner().run("scans.ocr", {}, request_id="req_fallback")

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert calls == ["bytes", "ndarray"]
    assert result.envelope.data == {"hint_text": "Fallback", "segment_count": 1}


def test_scans_ocr_paddle_model_dir_env_precedence(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    ocr_dir = tmp_path / "ocr_models"
    pii_dir = tmp_path / "pii_models"
    monkeypatch.setenv("SCRIPTSCORE_OCR_PADDLE_MODEL_DIR", str(ocr_dir))
    monkeypatch.setenv("SCRIPTSCORE_PII_PADDLE_MODEL_DIR", str(pii_dir))

    assert scans_ocr._paddle_model_dir() == ocr_dir

    monkeypatch.delenv("SCRIPTSCORE_OCR_PADDLE_MODEL_DIR")
    assert scans_ocr._paddle_model_dir() == pii_dir


def test_scans_ocr_paddle_reader_creation_failure_is_unavailable(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    monkeypatch.setenv("SCRIPTSCORE_OCR_PADDLE_MODEL_DIR", str(tmp_path / "models"))

    def create_reader(_path: Path) -> object:
        raise RuntimeError("paddle unavailable")

    monkeypatch.setattr("scriptscore.commands.scans_ocr.create_reader", create_reader)

    assert scans_ocr._load_paddle_ocr_engine() is None


def test_scans_ocr_paddle_candidate_images_include_normal_cropped_and_thresholded() -> None:
    image = Image.new("RGB", (160, 24), color=(255, 255, 255))
    for x in range(0, 160, 8):
        image.putpixel((x, 12), (0, 0, 0))
    buffer = io.BytesIO()
    image.save(buffer, format="PNG")

    candidates = scans_ocr._paddle_recognition_candidate_images(buffer.getvalue())

    assert len(candidates) == 3
    assert candidates[0].shape[:2] == (48, 320)
    assert candidates[1].shape[0] == 48
    assert candidates[1].shape[1] < candidates[0].shape[1]
    assert candidates[2].shape[:2] == (48, 320)
