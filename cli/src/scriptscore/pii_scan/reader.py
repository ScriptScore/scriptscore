# SPDX-License-Identifier: AGPL-3.0-only
"""OCR backend wrapper for local handwriting and PII scanning."""

from __future__ import annotations

import importlib.util
import json
import os
from collections.abc import Iterable, Sequence
from pathlib import Path
from typing import Any, cast

import cv2

from scriptscore.pii_scan.types import ReadResult, VisionToken


def verify_model_root(model_root: Path) -> None:
    """Validate the expected PaddleOCR model layout."""

    for name in ("det", "rec"):
        model_dir = model_root / name
        if not model_dir.exists() or not model_dir.is_dir():
            raise RuntimeError(f"missing PaddleOCR {name} model directory: {model_dir}")
        if not (
            (model_dir / "inference.json").exists() or (model_dir / "inference.pdmodel").exists()
        ):
            raise RuntimeError(
                "PaddleOCR "
                f"{name} model directory is missing inference.json or inference.pdmodel: {model_dir}"
            )
        if not (model_dir / "inference.yml").exists():
            raise RuntimeError(
                f"PaddleOCR {name} model directory is missing inference.yml: {model_dir}"
            )


def _token_from_payload(payload: dict[str, Any]) -> VisionToken:
    return VisionToken(
        text=str(payload["text"]).strip(),
        confidence=max(0.0, min(1.0, float(payload["confidence"]))),
        corners=(
            (int(payload["left"]), int(payload["top"])),
            (int(payload["right"]), int(payload["top"])),
            (int(payload["right"]), int(payload["bottom"])),
            (int(payload["left"]), int(payload["bottom"])),
        ),
    )


def _test_override_tokens() -> list[VisionToken] | None:
    raw_payload = os.environ.get("SCRIPTSCORE_TEST_PII_OCR_WORDS")
    if raw_payload is None:
        return None
    decoded = json.loads(raw_payload)
    if not isinstance(decoded, list):
        raise RuntimeError("SCRIPTSCORE_TEST_PII_OCR_WORDS must decode to a list.")
    return [_token_from_payload(item) for item in decoded if isinstance(item, dict)]


class _TestOverrideReader:
    """Lightweight OCR reader used by test-only env overrides."""

    def read(self, image: object) -> ReadResult:
        del image
        return ReadResult(
            tokens=_test_override_tokens() or [], backend_name="paddleocr_test_override"
        )


class PaddleTextReader:
    """Small adapter around PaddleOCR that returns normalized tokens."""

    name = "paddleocr"

    def __init__(self, model_root: Path) -> None:
        verify_model_root(model_root)
        if importlib.util.find_spec("paddle") is None:
            raise RuntimeError(
                "paddleocr backend unavailable: install the 'paddlepaddle' Python package"
            )

        os.environ["DISABLE_MODEL_SOURCE_CHECK"] = "True"
        os.environ["PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK"] = "True"
        from paddleocr import PaddleOCR  # type: ignore[import-untyped]

        self._engine = PaddleOCR(
            lang="en",
            text_detection_model_name="PP-OCRv5_mobile_det",
            text_detection_model_dir=str(model_root / "det"),
            text_recognition_model_name="PP-OCRv5_mobile_rec",
            text_recognition_model_dir=str(model_root / "rec"),
            use_doc_orientation_classify=False,
            use_doc_unwarping=False,
            use_textline_orientation=False,
        )

    def read(self, image: object) -> ReadResult:
        test_tokens = _test_override_tokens()
        if test_tokens is not None:
            return ReadResult(tokens=test_tokens, backend_name="paddleocr_test_override")

        if hasattr(image, "shape") and len(image.shape) == 2:
            grayscale_image = cast("cv2.typing.MatLike", image)
            image = cv2.cvtColor(grayscale_image, cv2.COLOR_GRAY2BGR)

        result = self._engine.ocr(image)
        if not result:
            return ReadResult(tokens=[], backend_name=self.name)

        tokens: list[VisionToken] = []
        first_page = result[0]
        if isinstance(first_page, dict):
            texts = first_page.get("rec_texts") or []
            scores = first_page.get("rec_scores") or []
            polygons = first_page.get("rec_polys") or []
            for text, score, polygon in zip(texts, scores, polygons, strict=False):
                token = _normalize_token(text=text, confidence=score, polygon=polygon)
                if token is not None:
                    tokens.append(token)
            return ReadResult(tokens=tokens, backend_name=self.name)

        for row in first_page:
            if not isinstance(row, list | tuple) or len(row) != 2:
                continue
            polygon, payload = row
            if not isinstance(payload, list | tuple) or len(payload) != 2:
                continue
            text, score = payload
            token = _normalize_token(text=text, confidence=score, polygon=polygon)
            if token is not None:
                tokens.append(token)
        return ReadResult(tokens=tokens, backend_name=self.name)


def _normalize_token(*, text: object, confidence: object, polygon: object) -> VisionToken | None:
    cleaned = str(text).strip()
    if not cleaned:
        return None
    try:
        points = cast(Iterable[Sequence[object]], polygon)
        corners = tuple((int(cast(Any, point[0])), int(cast(Any, point[1]))) for point in points)
    except Exception:
        return None
    if len(corners) != 4:
        return None
    top_left, top_right, bottom_right, bottom_left = corners
    return VisionToken(
        text=cleaned,
        confidence=max(0.0, min(1.0, float(cast(Any, confidence)))),
        corners=(top_left, top_right, bottom_right, bottom_left),
    )


def create_reader(model_root: Path) -> PaddleTextReader:
    """Create the local OCR reader for scans.pii."""

    if os.environ.get("SCRIPTSCORE_TEST_PII_OCR_WORDS") is not None:
        verify_model_root(model_root)
        return _TestOverrideReader()  # type: ignore[return-value]
    try:
        return PaddleTextReader(model_root)
    except Exception as exc:
        raise RuntimeError(f"paddleocr backend unavailable: {exc}") from exc
