# SPDX-License-Identifier: AGPL-3.0-only
"""Implementation of `scans ocr` (transient OCR hint over stdin PNG).

Privacy requirements:
- The input PNG is read from stdin and must never be written to disk.
- The OCR hint text must only appear in the success JSON `data` payload.
- Do not emit progress events containing OCR text.
"""

from __future__ import annotations

import io
import math
import os
import re
import sys
from pathlib import Path
from typing import Any

from pydantic import BaseModel, ConfigDict

from scriptscore.contracts import ErrorCategory, ScriptscoreError
from scriptscore.ocr.easyocr import _suppress_easyocr_runtime_warnings
from scriptscore.pii_scan.reader import create_reader
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec


class ScansOcrRequest(BaseModel):
    """Empty request model; image bytes are provided via stdin."""

    model_config = ConfigDict(extra="forbid")


def _external_error(
    *, code: str, message: str, details: dict[str, Any], retryable: bool
) -> ScriptscoreError:
    return ScriptscoreError(
        code=code,
        message=message,
        category=ErrorCategory.EXTERNAL_DEPENDENCY,
        retryable=retryable,
        details=details,
    )


def _load_easyocr_reader() -> Any:
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
    # Suppress EasyOCR init stdout.
    import contextlib

    with contextlib.redirect_stdout(io.StringIO()), _suppress_easyocr_runtime_warnings():
        return Reader(["en"], gpu=bool(torch.cuda.is_available()))


def _repo_paddle_model_dir() -> Path:
    return Path(__file__).resolve().parents[3] / "models" / "paddle"


def _paddle_model_dir() -> Path | None:
    for env_name in ("SCRIPTSCORE_OCR_PADDLE_MODEL_DIR", "SCRIPTSCORE_PII_PADDLE_MODEL_DIR"):
        value = os.environ.get(env_name)
        if value:
            return Path(value)
    repo_dir = _repo_paddle_model_dir()
    return repo_dir if repo_dir.exists() else None


def _load_paddle_ocr_engine() -> Any | None:
    model_dir = _paddle_model_dir()
    if model_dir is None:
        return None
    try:
        import contextlib

        with contextlib.redirect_stdout(io.StringIO()):
            reader = create_reader(model_dir)
    except Exception:
        return None
    return getattr(reader, "_engine", None)


def _read_stdin_png_bytes(*, max_bytes: int = 10_000_000) -> bytes:
    raw = sys.stdin.buffer.read()
    if not raw:
        raise ScriptscoreError(
            code="ocr_input_missing",
            message="Expected one PNG image on stdin, but stdin was empty.",
            category=ErrorCategory.VALIDATION,
            retryable=False,
            details={},
        )
    if len(raw) > max_bytes:
        raise ScriptscoreError(
            code="ocr_input_too_large",
            message="OCR input image exceeded the maximum allowed size.",
            category=ErrorCategory.VALIDATION,
            retryable=False,
            details={"max_bytes": max_bytes, "byte_size": len(raw)},
        )
    return raw


def _png_bytes_to_rgb_numpy(raw: bytes) -> Any:
    try:
        from PIL import Image
    except ModuleNotFoundError as exc:
        raise _external_error(
            code="ocr_dependency_unavailable",
            message="Pillow is not installed.",
            details={"missing_module": exc.name},
            retryable=False,
        ) from exc
    try:
        import numpy as np
    except ModuleNotFoundError as exc:
        raise _external_error(
            code="ocr_dependency_unavailable",
            message="NumPy is not installed.",
            details={"missing_module": exc.name},
            retryable=False,
        ) from exc

    try:
        image = Image.open(io.BytesIO(raw)).convert("RGB")
    except Exception as exc:
        raise ScriptscoreError(
            code="ocr_input_invalid",
            message="OCR input could not be decoded as a PNG image.",
            category=ErrorCategory.VALIDATION,
            retryable=False,
            details={"error_type": type(exc).__name__, "error": str(exc)},
        ) from exc
    return np.ascontiguousarray(np.asarray(image), dtype="uint8")


def _png_bytes_to_pil_image(raw: bytes) -> Any:
    """Decode a transient PNG into an RGB PIL image without writing artifacts."""

    try:
        from PIL import Image
    except ModuleNotFoundError as exc:
        raise _external_error(
            code="ocr_dependency_unavailable",
            message="Pillow is not installed.",
            details={"missing_module": exc.name},
            retryable=False,
        ) from exc
    try:
        return Image.open(io.BytesIO(raw)).convert("RGB")
    except Exception as exc:
        raise ScriptscoreError(
            code="ocr_input_invalid",
            message="OCR input could not be decoded as a PNG image.",
            category=ErrorCategory.VALIDATION,
            retryable=False,
            details={"error_type": type(exc).__name__, "error": str(exc)},
        ) from exc


def _pil_image_to_rgb_numpy(image: Any) -> Any:
    try:
        import numpy as np
    except ModuleNotFoundError as exc:
        raise _external_error(
            code="ocr_dependency_unavailable",
            message="NumPy is not installed.",
            details={"missing_module": exc.name},
            retryable=False,
        ) from exc

    return np.ascontiguousarray(np.asarray(image), dtype="uint8")


def _enlarged_image(image: Any) -> Any:
    try:
        from PIL import Image
    except ModuleNotFoundError as exc:
        raise _external_error(
            code="ocr_dependency_unavailable",
            message="Pillow is not installed.",
            details={"missing_module": exc.name},
            retryable=False,
        ) from exc

    if image.width <= 0 or image.height <= 0:
        return image
    return image.resize((image.width * 2, image.height * 2), Image.Resampling.BICUBIC)


def _paddle_recognition_candidate_images(raw: bytes) -> list[Any]:
    """Build a bounded set of recognition-only crops for the name hint."""

    try:
        from PIL import ImageOps
    except ModuleNotFoundError as exc:
        raise _external_error(
            code="ocr_dependency_unavailable",
            message="Pillow is not installed.",
            details={"missing_module": exc.name},
            retryable=False,
        ) from exc

    image = _png_bytes_to_pil_image(raw)
    candidates = [_enlarged_image(image)]

    if image.width > 120:
        left = min(int(image.width * 0.12), max(0, image.width - 1))
        candidates.append(_enlarged_image(image.crop((left, 0, image.width, image.height))))

    gray = ImageOps.grayscale(image)
    contrasted = ImageOps.autocontrast(gray)
    thresholded = contrasted.point(lambda pixel: 255 if pixel > 190 else 0)
    candidates.append(_enlarged_image(thresholded.convert("RGB")))

    return [_pil_image_to_rgb_numpy(candidate) for candidate in candidates[:3]]


_LEADING_NAME_LABEL_RE = re.compile(
    r"^\s*(?:student\s+name|full\s+name|printed\s+name|name)\b\s*[:#.\-_ ]*\s*",
    re.IGNORECASE,
)
_NON_HINT_REMAINDER_RE = re.compile(r"^[\s:#.\-_]+$")


def _clean_ocr_hint_text(hint_text: str) -> str:
    """Remove printed name-field labels while preserving handwritten text."""

    cleaned = _LEADING_NAME_LABEL_RE.sub("", hint_text.strip(), count=1).strip()
    cleaned = cleaned.strip(" \t\r\n:#._-")
    if not cleaned or _NON_HINT_REMAINDER_RE.fullmatch(cleaned):
        return ""
    return cleaned


def _bbox_top_mean(bbox: Any) -> float:
    if not isinstance(bbox, list | tuple) or not bbox:
        return 0.0
    ys: list[float] = []
    for point in bbox:
        if isinstance(point, list | tuple) and len(point) >= 2:
            try:
                ys.append(float(point[1]))
            except (TypeError, ValueError):
                continue
    return sum(ys) / len(ys) if ys else 0.0


def _hint_text_from_easyocr_results(results: Any) -> tuple[str, int]:
    """Build one hint string from EasyOCR readtext rows; sort top-to-bottom.

    Returns ``(hint_text, segment_count)`` where ``segment_count`` is the number of
    rows returned by EasyOCR (before filtering), for diagnostics.
    """

    if not isinstance(results, list):
        return "", 0
    segment_count = len(results)
    rows: list[tuple[float, float, str]] = []
    for entry in results:
        if not isinstance(entry, list | tuple) or len(entry) < 3:
            continue
        bbox, text, confidence = entry[0], entry[1], entry[2]
        try:
            conf = float(confidence)
        except (TypeError, ValueError):
            continue
        if not math.isfinite(conf) or conf < 0.05:
            continue
        text_str = str(text).strip()
        if not text_str:
            continue
        top = _bbox_top_mean(bbox)
        rows.append((top, -conf, text_str))
    if not rows:
        return "", segment_count
    rows.sort(key=lambda r: (r[0], r[1]))
    return _clean_ocr_hint_text(" ".join(r[2] for r in rows)), segment_count


def _collect_text_score_pairs(value: Any) -> list[tuple[str, float | None]]:
    pairs: list[tuple[str, float | None]] = []
    if isinstance(value, dict):
        texts = value.get("rec_texts") or []
        scores = value.get("rec_scores") or []
        for index, raw_text in enumerate(texts):
            raw_score = scores[index] if index < len(scores) else None
            try:
                score = float(raw_score) if raw_score is not None else None
            except (TypeError, ValueError):
                score = None
            text = str(raw_text).strip()
            if text:
                pairs.append((text, score))
        return pairs
    if isinstance(value, list | tuple):
        if len(value) >= 2 and isinstance(value[0], str):
            try:
                score = float(value[1])
            except (TypeError, ValueError):
                score = None
            text = value[0].strip()
            return [(text, score)] if text else []
        for item in value:
            pairs.extend(_collect_text_score_pairs(item))
    return pairs


def _hint_text_from_paddle_recognition(results: Any) -> tuple[str, int, float | None]:
    pairs = _collect_text_score_pairs(results)
    texts = [text for text, _score in pairs if text]
    scores = [score for _text, score in pairs if score is not None and math.isfinite(score)]
    mean_score = sum(scores) / len(scores) if scores else None
    return " ".join(texts).strip(), len(texts), mean_score


def _usable_ocr_hint(hint_text: str, mean_score: float | None = None) -> bool:
    normalized = "".join(character for character in hint_text.lower() if character.isalnum())
    if not normalized or normalized == "name":
        return False
    alpha_count = sum(1 for character in normalized if character.isalpha())
    if alpha_count < 2:
        return False
    return mean_score is None or mean_score >= 0.15


def _paddle_hint_score(hint_text: str, mean_score: float | None) -> float:
    confidence_score = mean_score if mean_score is not None and math.isfinite(mean_score) else 0.5
    alpha_count = sum(1 for character in hint_text if character.isalpha())
    return confidence_score + min(alpha_count, 24) / 100


def _read_paddle_hint(raw: bytes) -> tuple[str, int] | None:
    engine = _load_paddle_ocr_engine()
    if engine is None:
        return None
    try:
        import contextlib

        candidates = _paddle_recognition_candidate_images(raw)
        best_hint: tuple[float, str, int] | None = None
        with contextlib.redirect_stdout(io.StringIO()):
            for image in candidates:
                results = engine.ocr(image, det=False, rec=True, cls=False)
                hint_text, segment_count, mean_score = _hint_text_from_paddle_recognition(results)
                cleaned_hint = _clean_ocr_hint_text(hint_text)
                if not _usable_ocr_hint(cleaned_hint, mean_score):
                    continue
                score = _paddle_hint_score(cleaned_hint, mean_score)
                if best_hint is None or score > best_hint[0]:
                    best_hint = (score, cleaned_hint, segment_count)
    except ScriptscoreError:
        raise
    except Exception:
        return None
    if best_hint is None:
        return None
    return best_hint[1], best_hint[2]


def _read_easyocr_hint(raw: bytes) -> tuple[str, int]:
    reader = _load_easyocr_reader()
    try:
        import contextlib

        with contextlib.redirect_stdout(io.StringIO()), _suppress_easyocr_runtime_warnings():
            # Prefer raw PNG bytes — matches EasyOCR path decoding and avoids an extra
            # PIL round-trip that can interact badly with some PNG variants.
            try:
                results = reader.readtext(raw, detail=1, paragraph=False)
            except Exception:
                rgb = _png_bytes_to_rgb_numpy(raw)
                results = reader.readtext(rgb, detail=1, paragraph=False)
    except ScriptscoreError:
        raise
    except Exception as exc:  # pragma: no cover - depends on external runtime.
        raise _external_error(
            code="ocr_runtime_failed",
            message="EasyOCR failed while reading the image.",
            details={"error_type": type(exc).__name__, "error": str(exc)},
            retryable=True,
        ) from exc
    return _hint_text_from_easyocr_results(results)


def handle_scans_ocr(ctx: CommandContext, request: ScansOcrRequest) -> CommandOutcome:
    """Return a single OCR hint string from a stdin PNG image."""

    _ = request
    ctx.check_cancelled()
    test_hint = os.environ.get("SCRIPTSCORE_TEST_OCR_HINT")
    raw = _read_stdin_png_bytes()
    _png_bytes_to_pil_image(raw)
    if test_hint is not None:
        return CommandOutcome(
            data={"hint_text": test_hint.strip(), "segment_count": 0},
        )
    paddle_hint = _read_paddle_hint(raw)
    hint_text, segment_count = paddle_hint if paddle_hint is not None else _read_easyocr_hint(raw)
    # No progress emits (avoid leaking text through event sinks).
    return CommandOutcome(data={"hint_text": hint_text, "segment_count": segment_count})


def scans_ocr_spec() -> CommandSpec:
    return CommandSpec(name="scans.ocr", request_model=ScansOcrRequest, handler=handle_scans_ocr)
