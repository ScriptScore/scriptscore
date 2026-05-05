# SPDX-License-Identifier: AGPL-3.0-only
"""Built-in PaddleOCR integration for scan-region refinement."""

from __future__ import annotations

import contextlib
import io
import json
import os
from pathlib import Path
from typing import Any

import cv2

from scriptscore.contracts import ErrorCategory, ScriptscoreError
from scriptscore.ocr.easyocr import OcrTextBox
from scriptscore.pii_scan.reader import create_reader


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


def _repo_paddle_model_dir() -> Path:
    return Path(__file__).resolve().parents[3] / "models" / "paddle"


def _paddle_model_dir() -> Path:
    for env_name in (
        "SCRIPTSCORE_DETECT_PADDLE_MODEL_DIR",
        "SCRIPTSCORE_OCR_PADDLE_MODEL_DIR",
        "SCRIPTSCORE_PII_PADDLE_MODEL_DIR",
    ):
        value = os.environ.get(env_name)
        if value:
            return Path(value)
    return _repo_paddle_model_dir()


def _read_test_boxes_from_env() -> list[OcrTextBox] | None:
    payload = os.environ.get("SCRIPTSCORE_TEST_PADDLEOCR_BOXES") or os.environ.get(
        "SCRIPTSCORE_TEST_EASYOCR_BOXES"
    )
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
    """Read normalized OCR boxes from one rendered student page using PaddleOCR."""

    test_results = _read_test_boxes_from_env()
    if test_results is not None:
        return test_results

    model_dir = _paddle_model_dir()
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            reader = create_reader(model_dir)
    except Exception as exc:
        raise _external_error(
            code="ocr_dependency_unavailable",
            message="PaddleOCR dependencies or models are not available.",
            details={
                "model_dir": str(model_dir),
                "error_type": type(exc).__name__,
                "error": str(exc),
            },
            retryable=False,
        ) from exc

    image = cv2.imread(str(image_path), cv2.IMREAD_COLOR)
    if image is None:
        raise ScriptscoreError(
            code="ocr_page_unreadable",
            message="PaddleOCR could not read the page image.",
            category=ErrorCategory.VALIDATION,
            retryable=True,
            details={"image_path": str(image_path)},
        )

    try:
        with contextlib.redirect_stdout(io.StringIO()):
            result = reader.read(image)
    except Exception as exc:  # pragma: no cover - depends on external package/runtime.
        raise _external_error(
            code="ocr_runtime_failed",
            message="PaddleOCR failed while reading the page.",
            details={
                "image_path": str(image_path),
                "model_dir": str(model_dir),
                "error_type": type(exc).__name__,
                "error": str(exc),
            },
            retryable=True,
        ) from exc

    boxes: list[OcrTextBox] = []
    for token in result.tokens:
        xs = [corner[0] for corner in token.corners]
        ys = [corner[1] for corner in token.corners]
        boxes.append(
            OcrTextBox(
                text=token.text,
                left=round(min(xs)),
                top=round(min(ys)),
                right=round(max(xs)),
                bottom=round(max(ys)),
                confidence=token.confidence,
            )
        )
    return boxes
