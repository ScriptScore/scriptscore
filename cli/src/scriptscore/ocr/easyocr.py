# SPDX-License-Identifier: AGPL-3.0-only
"""Built-in EasyOCR integration for scan-region refinement."""

from __future__ import annotations

import contextlib
import io
import json
import os
import warnings
from dataclasses import dataclass
from numbers import Real
from pathlib import Path
from typing import Any, cast

from scriptscore.contracts import ErrorCategory, ScriptscoreError


@dataclass(frozen=True)
class OcrTextBox:
    """Normalized OCR text box in page pixel coordinates."""

    text: str
    left: int
    top: int
    right: int
    bottom: int
    confidence: float


def _external_error(
    *,
    code: str,
    message: str,
    details: dict[str, Any],
    retryable: bool,
) -> ScriptscoreError:
    return ScriptscoreError(
        code=code,
        message=message,
        category=ErrorCategory.EXTERNAL_DEPENDENCY,
        retryable=retryable,
        details=details,
    )


def _load_dependencies() -> tuple[type[Any], Any]:
    try:
        import torch
        from easyocr import Reader  # type: ignore[import-untyped]
    except ModuleNotFoundError as exc:
        raise _external_error(
            code="ocr_dependency_unavailable",
            message="EasyOCR dependencies are not installed.",
            details={"missing_module": exc.name},
            retryable=False,
        ) from exc
    return Reader, torch


@contextlib.contextmanager
def _suppress_easyocr_runtime_warnings() -> Any:
    with warnings.catch_warnings():
        warnings.filterwarnings(
            "ignore",
            message=r"torch\.ao\.quantization is deprecated and will be removed in 2\.10\..*",
            category=DeprecationWarning,
        )
        warnings.filterwarnings(
            "ignore",
            message=r"'pin_memory' argument is set as true but no accelerator is found.*",
            category=UserWarning,
        )
        warnings.filterwarnings(
            "ignore",
            message=r"'mode' parameter is deprecated and will be removed in Pillow 13.*",
            category=DeprecationWarning,
        )
        yield


def _reader_factory(*, gpu: bool) -> Any:
    reader_class, _ = _load_dependencies()
    with contextlib.redirect_stdout(io.StringIO()), _suppress_easyocr_runtime_warnings():
        return reader_class(["en"], gpu=gpu)


def _reader_readtext(reader: Any, image_path: str) -> list[tuple[Any, Any, Any]]:
    with contextlib.redirect_stdout(io.StringIO()), _suppress_easyocr_runtime_warnings():
        return cast(
            list[tuple[Any, Any, Any]],
            reader.readtext(image_path, detail=1, paragraph=False),
        )


def _normalize_box(raw: Any) -> tuple[int, int, int, int]:
    if not isinstance(raw, list | tuple) or not raw:
        raise ValueError("OCR bounding box was invalid.")
    xs: list[float] = []
    ys: list[float] = []
    for point in raw:
        if (
            not isinstance(point, list | tuple)
            or len(point) != 2
            or not isinstance(point[0], Real)
            or isinstance(point[0], bool)
            or not isinstance(point[1], Real)
            or isinstance(point[1], bool)
        ):
            raise ValueError("OCR bounding box point was invalid.")
        xs.append(float(point[0]))
        ys.append(float(point[1]))
    return round(min(xs)), round(min(ys)), round(max(xs)), round(max(ys))


def _read_test_boxes_from_env() -> list[OcrTextBox] | None:
    payload = os.environ.get("SCRIPTSCORE_TEST_EASYOCR_BOXES")
    if payload is None:
        return None
    try:
        rows = json.loads(payload)
    except json.JSONDecodeError as exc:
        raise _external_error(
            code="ocr_response_invalid",
            message="Test OCR payload was invalid JSON.",
            details={"error": str(exc)},
            retryable=False,
        ) from exc
    if not isinstance(rows, list):
        raise _external_error(
            code="ocr_response_invalid",
            message="Test OCR payload must be a list of OCR boxes.",
            details={"payload_type": type(rows).__name__},
            retryable=False,
        )

    results: list[OcrTextBox] = []
    for entry in rows:
        if not isinstance(entry, dict):
            raise _external_error(
                code="ocr_response_invalid",
                message="Test OCR payload row was invalid.",
                details={"entry": repr(entry)},
                retryable=False,
            )
        try:
            results.append(
                OcrTextBox(
                    text=str(entry["text"]).strip(),
                    left=int(entry["left"]),
                    top=int(entry["top"]),
                    right=int(entry["right"]),
                    bottom=int(entry["bottom"]),
                    confidence=float(entry["confidence"]),
                )
            )
        except (KeyError, TypeError, ValueError) as exc:
            raise _external_error(
                code="ocr_response_invalid",
                message="Test OCR payload row was malformed.",
                details={
                    "entry": repr(entry),
                    "error_type": type(exc).__name__,
                    "error": str(exc),
                },
                retryable=False,
            ) from exc
    return results


def read_page_ocr(image_path: Path) -> list[OcrTextBox]:
    """Read OCR boxes from one rendered student page."""

    test_results = _read_test_boxes_from_env()
    if test_results is not None:
        return test_results

    _, torch = _load_dependencies()
    reader = _reader_factory(gpu=bool(torch.cuda.is_available()))
    try:
        raw_results = _reader_readtext(reader, str(image_path))
    except Exception as exc:  # pragma: no cover - depends on external package/runtime.
        raise _external_error(
            code="ocr_runtime_failed",
            message="EasyOCR failed while reading the page.",
            details={
                "image_path": str(image_path),
                "error_type": type(exc).__name__,
                "error": str(exc),
            },
            retryable=True,
        ) from exc

    results: list[OcrTextBox] = []
    for entry in raw_results:
        if not isinstance(entry, list | tuple) or len(entry) != 3:
            raise _external_error(
                code="ocr_response_invalid",
                message="EasyOCR returned an invalid OCR row.",
                details={"image_path": str(image_path), "entry": repr(entry)},
                retryable=False,
            )
        raw_box, raw_text, raw_confidence = entry
        try:
            left, top, right, bottom = _normalize_box(raw_box)
            results.append(
                OcrTextBox(
                    text=str(raw_text).strip(),
                    left=left,
                    top=top,
                    right=right,
                    bottom=bottom,
                    confidence=float(raw_confidence),
                )
            )
        except (TypeError, ValueError) as exc:
            raise _external_error(
                code="ocr_response_invalid",
                message="EasyOCR returned an OCR row with malformed coordinates or confidence.",
                details={
                    "image_path": str(image_path),
                    "entry": repr(entry),
                    "error_type": type(exc).__name__,
                    "error": str(exc),
                },
                retryable=False,
            ) from exc
    return results
