# SPDX-License-Identifier: AGPL-3.0-only
"""Implementation of `scans detect`."""

from __future__ import annotations

import json
import math
import re
from dataclasses import dataclass
from difflib import SequenceMatcher
from pathlib import Path
from time import perf_counter
from typing import Literal

from scriptscore.artifacts import file_sha256, load_page_image
from scriptscore.commands.common import (
    batch_outcome,
    batch_result_data,
    file_artifact,
    progress,
    warning,
)
from scriptscore.commands.scans_shared import ensure_paths_exist
from scriptscore.contracts import (
    ArtifactReference,
    DetectResult,
    DetectTarget,
    ErrorCategory,
    OcrBox,
    PageOcrMetadata,
    PageOcrResult,
    QuestionDetectHint,
    Region,
    ScansDetectRequest,
    ScriptscoreError,
    WarningObject,
    WriteState,
)
from scriptscore.ocr import OcrTextBox, read_page_ocr
from scriptscore.paths import join_under_root
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec

_NON_ALNUM_RE = re.compile(r"[^a-z0-9]+")
_COMMAND_NAME = "scans.detect"
_FLOAT_IDENTITY_ABS_TOL = 1e-12


def _is_full_coverage(value: float) -> bool:
    return math.isclose(value, 1.0, rel_tol=0.0, abs_tol=_FLOAT_IDENTITY_ABS_TOL)


@dataclass(frozen=True)
class _HeaderBoundary:
    top: int
    bottom: int
    boxes: tuple[OcrTextBox, ...]


@dataclass(frozen=True)
class _HeaderTextMatch:
    overlap_count: int
    candidate_token_count: int
    hint_token_count: int
    candidate_coverage: float
    similarity: float
    ordered_similarity: float


def _normalize_text(value: str) -> str:
    return " ".join(_NON_ALNUM_RE.sub(" ", value.lower()).split())


def _text_tokens(value: str) -> tuple[str, ...]:
    return tuple(part for part in _normalize_text(value).split() if len(part) >= 3)


def _hint_terms(hint: QuestionDetectHint) -> set[str]:
    return set(_text_tokens(hint.question_text_hint))


def _hint_term_overlap_count(box_text: str, hint: QuestionDetectHint) -> int:
    return len(_hint_terms(hint).intersection(_text_tokens(box_text)))


def _header_text_match(box_text: str, hint: QuestionDetectHint) -> _HeaderTextMatch:
    box_tokens = _text_tokens(box_text)
    hint_tokens = _text_tokens(hint.question_text_hint)
    if not box_tokens or not hint_tokens:
        return _HeaderTextMatch(
            overlap_count=0,
            candidate_token_count=len(box_tokens),
            hint_token_count=len(hint_tokens),
            candidate_coverage=0.0,
            similarity=0.0,
            ordered_similarity=0.0,
        )
    overlap_count = len(set(box_tokens).intersection(hint_tokens))
    candidate_coverage = overlap_count / len(box_tokens)
    sequence_ratio = SequenceMatcher(None, box_tokens, hint_tokens).ratio()
    prefix_ratio = 0.0
    if len(box_tokens) <= len(hint_tokens):
        prefix_ratio = SequenceMatcher(None, box_tokens, hint_tokens[: len(box_tokens)]).ratio()
    return _HeaderTextMatch(
        overlap_count=overlap_count,
        candidate_token_count=len(box_tokens),
        hint_token_count=len(hint_tokens),
        candidate_coverage=candidate_coverage,
        similarity=max(candidate_coverage, sequence_ratio, prefix_ratio),
        ordered_similarity=max(sequence_ratio, prefix_ratio),
    )


def _is_strong_header_text_match(match: _HeaderTextMatch, hint: QuestionDetectHint) -> bool:
    if match.hint_token_count == 1:
        return match.overlap_count >= 1 and match.candidate_coverage >= 0.5
    if match.hint_token_count == 2:
        return match.overlap_count == 2 and match.candidate_coverage >= 0.5
    if match.candidate_token_count < 2:
        return False
    return (
        match.overlap_count >= 2
        and match.candidate_coverage >= 0.5
        and (match.similarity >= 0.45 or match.ordered_similarity >= 0.6)
    )


def _ordered_header_similarity(candidate_text: str, hint: QuestionDetectHint) -> float:
    candidate_tokens = _text_tokens(candidate_text)
    hint_tokens = _text_tokens(hint.question_text_hint)
    if not candidate_tokens or not hint_tokens:
        return 0.0
    sequence_ratio = SequenceMatcher(None, candidate_tokens, hint_tokens).ratio()
    prefix_ratio = 0.0
    if len(candidate_tokens) <= len(hint_tokens):
        prefix_ratio = SequenceMatcher(
            None, candidate_tokens, hint_tokens[: len(candidate_tokens)]
        ).ratio()
    return max(sequence_ratio, prefix_ratio)


def _is_header_fragment_match(match: _HeaderTextMatch, hint: QuestionDetectHint) -> bool:
    if _is_strong_header_text_match(match, hint):
        return True
    return (
        match.hint_token_count <= 5
        and match.candidate_token_count == 1
        and match.overlap_count == 1
        and _is_full_coverage(match.candidate_coverage)
    )


def _matches_question_number(box_text: str, question_number: int) -> bool:
    normalized = _normalize_text(box_text)
    number = str(question_number)
    return (
        normalized == number
        or normalized.startswith(f"{number} ")
        or normalized.startswith(f"question {number}")
    )


def _header_match_score(box: OcrTextBox, hint: QuestionDetectHint) -> tuple[int, int, float] | None:
    number_match = _matches_question_number(box.text, hint.question_number)
    term_overlap = _hint_term_overlap_count(box.text, hint)
    text_match = _header_text_match(box.text, hint)
    if not number_match and term_overlap == 0:
        return None
    distance = abs(box.top - hint.template_region.y)
    if number_match:
        if distance > max(180, hint.template_region.height // 2):
            return None
        score = 6 if not _is_strong_header_text_match(text_match, hint) else 12
    else:
        if not _is_strong_header_text_match(text_match, hint):
            return None
        near_limit = max(72, hint.template_region.height // 3)
        far_limit = max(260, hint.template_region.height)
        if distance > near_limit:
            if text_match.similarity < 0.75 or box.confidence < 0.45 or distance > far_limit:
                return None
            score = max(1, int(text_match.similarity * 10) - 1)
        else:
            score = int(text_match.similarity * 10)
    if distance <= max(90, hint.template_region.height // 3):
        score += 2
    return score, -distance, -box.confidence


def _number_without_strong_text_match(box: OcrTextBox, hint: QuestionDetectHint) -> bool:
    return _matches_question_number(
        box.text, hint.question_number
    ) and not _is_strong_header_text_match(_header_text_match(box.text, hint), hint)


def _has_nearby_header_text_support(
    boxes: list[OcrTextBox], hint: QuestionDetectHint, seed: OcrTextBox
) -> bool:
    for box in boxes:
        if box is seed:
            continue
        text_match = _header_text_match(box.text, hint)
        if not _matches_question_number(
            box.text, hint.question_number
        ) and not _is_strong_header_text_match(text_match, hint):
            continue
        if box.top <= seed.bottom + 40 and box.bottom >= seed.top - 40:
            return True
    return False


def _supports_split_header_band(
    box: OcrTextBox, hint: QuestionDetectHint, seed: OcrTextBox
) -> bool:
    if _matches_question_number(box.text, hint.question_number):
        return False
    if not _is_header_fragment_match(_header_text_match(box.text, hint), hint):
        return False
    if abs(box.top - hint.template_region.y) > max(36, hint.template_region.height // 2):
        return False
    seed_similarity = _ordered_header_similarity(seed.text, hint)
    if box.bottom < seed.top:
        if seed.top - box.bottom > 12:
            return False
        combined_similarity = _ordered_header_similarity(f"{box.text} {seed.text}", hint)
        return combined_similarity >= 0.75 and combined_similarity > seed_similarity + 0.05
    if seed.bottom < box.top:
        if box.top - seed.bottom > 12:
            return False
        combined_similarity = _ordered_header_similarity(f"{seed.text} {box.text}", hint)
        return combined_similarity >= 0.75 and combined_similarity > seed_similarity + 0.05
    combined_similarity = _ordered_header_similarity(f"{seed.text} {box.text}", hint)
    return combined_similarity >= 0.75 and combined_similarity > seed_similarity + 0.05


def _find_header_anchor(boxes: list[OcrTextBox], hint: QuestionDetectHint) -> OcrTextBox | None:
    best: tuple[tuple[int, int, float], OcrTextBox] | None = None
    for box in boxes:
        score = _header_match_score(box, hint)
        if score is None:
            continue
        if _number_without_strong_text_match(box, hint) and not _has_nearby_header_text_support(
            boxes, hint, box
        ):
            continue
        if best is None or score > best[0]:
            best = (score, box)
    return None if best is None else best[1]


def _header_band_boxes(
    boxes: list[OcrTextBox], hint: QuestionDetectHint, seed: OcrTextBox
) -> list[OcrTextBox]:
    return [
        box
        for box in boxes
        if (
            _header_match_score(box, hint) is not None
            or _supports_split_header_band(box, hint, seed)
        )
        and box.top <= seed.bottom + 24
        and box.bottom >= seed.top - 24
    ]


def _header_band(
    boxes: list[OcrTextBox], hint: QuestionDetectHint, seed: OcrTextBox
) -> tuple[int, int]:
    matched_boxes = _header_band_boxes(boxes, hint, seed)
    if not matched_boxes:
        return seed.top, seed.bottom
    return min(box.top for box in matched_boxes), max(box.bottom for box in matched_boxes)


def _resolve_header_boundary(
    boxes: list[OcrTextBox], hint: QuestionDetectHint
) -> _HeaderBoundary | None:
    anchor = _find_header_anchor(boxes, hint)
    if anchor is None:
        return None
    matched_boxes = tuple(_header_band_boxes(boxes, hint, anchor))
    if not matched_boxes:
        matched_boxes = (anchor,)
    return _HeaderBoundary(
        top=min(box.top for box in matched_boxes),
        bottom=max(box.bottom for box in matched_boxes),
        boxes=matched_boxes,
    )


def _content_anchor_search_bottom(hint: QuestionDetectHint) -> int:
    return hint.template_region.y + max(140, hint.template_region.height // 3)


def _is_content_anchor_match(match: _HeaderTextMatch, hint: QuestionDetectHint) -> bool:
    if match.hint_token_count <= 2:
        return _is_strong_header_text_match(match, hint)
    return match.overlap_count >= 2 and match.candidate_coverage >= 0.35


def _nearby_content_boxes(
    boxes: list[OcrTextBox], hint: QuestionDetectHint, seed: OcrTextBox
) -> list[OcrTextBox]:
    search_bottom = _content_anchor_search_bottom(hint)
    band_bottom = seed.bottom + max(64, hint.template_region.height // 8)
    nearby = [
        box
        for box in boxes
        if box.top >= hint.template_region.y and box.top <= search_bottom and box.top <= band_bottom
    ]
    return sorted(nearby, key=lambda box: (box.top, box.left))


def _resolve_content_boundary(
    boxes: list[OcrTextBox], hint: QuestionDetectHint
) -> _HeaderBoundary | None:
    """Infer a current-question start from answer text when the printed header is obscured."""

    search_bottom = _content_anchor_search_bottom(hint)
    for seed in sorted(boxes, key=lambda box: (box.top, box.left)):
        if seed.top < hint.template_region.y or seed.top > search_bottom:
            continue
        if _matches_question_number(seed.text, hint.question_number):
            continue
        candidate_boxes = tuple(_nearby_content_boxes(boxes, hint, seed))
        if not candidate_boxes:
            continue
        match = _header_text_match(seed.text, hint)
        if not _is_content_anchor_match(match, hint):
            continue
        return _HeaderBoundary(
            top=min(box.top for box in candidate_boxes),
            bottom=max(box.bottom for box in candidate_boxes),
            boxes=candidate_boxes,
        )
    return None


def _intersects_vertically(box: OcrTextBox, *, top: int, bottom: int) -> bool:
    return box.bottom > top and box.top < bottom


def _vertical_bounds(
    *,
    boxes: list[OcrTextBox],
    hint: QuestionDetectHint,
    next_hint: QuestionDetectHint | None,
    page_height: int,
) -> tuple[int, int] | None:
    current_boundary = _resolve_header_boundary(boxes, hint)
    if current_boundary is None:
        current_boundary = _resolve_content_boundary(boxes, hint)
    if current_boundary is None:
        return None
    if next_hint is None:
        nominal_bottom = max(
            current_boundary.bottom,
            hint.template_region.y + hint.template_region.height,
        )
        boundary_boxes: tuple[OcrTextBox, ...] = ()
    else:
        next_boundary = _resolve_header_boundary(boxes, next_hint)
        if next_boundary is None:
            nominal_bottom = next_hint.template_region.y
            boundary_boxes = ()
        else:
            nominal_bottom = next_boundary.top
            boundary_boxes = next_boundary.boxes
    top = max(0, min(page_height, current_boundary.top))
    bottom = max(0, min(page_height, nominal_bottom))
    overlap_boxes = [
        box
        for box in boxes
        if box.top < bottom and box.bottom > bottom and box not in boundary_boxes
    ]
    if overlap_boxes:
        bottom = max(bottom, max(box.bottom for box in overlap_boxes))
        bottom = max(0, min(page_height, bottom))
    if bottom <= top:
        return None
    return top, bottom


def _horizontal_bounds(
    *,
    hint: QuestionDetectHint,
    boxes: list[OcrTextBox],
    top: int,
    bottom: int,
    page_width: int,
) -> tuple[int, int] | None:
    left = hint.template_region.x
    right = hint.template_region.x + hint.template_region.width
    boxes_in_region = [box for box in boxes if _intersects_vertically(box, top=top, bottom=bottom)]
    if any(box.left < left for box in boxes_in_region):
        left = 0
    if any(box.right > right for box in boxes_in_region):
        right = page_width
    left = max(0, min(page_width, left))
    right = max(0, min(page_width, right))
    if right <= left:
        return None
    return left, right


def _template_region_result(
    *,
    target: DetectTarget,
    hint: QuestionDetectHint,
    message: str,
    scope: dict[str, object],
) -> DetectResult:
    assert target.page.student_ref is not None
    return DetectResult(
        student_ref=target.page.student_ref,
        page_number=target.page.page_number,
        question_id=hint.question_id,
        status="warning",
        region=hint.template_region,
        region_source="template_fallback",
        warnings=[warning(code="detect_template_fallback", message=message, scope=scope)],
    )


def _detect_region(
    *,
    target: DetectTarget,
    hint: QuestionDetectHint,
    next_hint: QuestionDetectHint | None,
    boxes: list[OcrTextBox],
    page_width: int,
    page_height: int,
) -> DetectResult:
    assert target.page.student_ref is not None
    scope: dict[str, object] = {
        "student_ref": target.page.student_ref,
        "page_number": target.page.page_number,
        "question_id": hint.question_id,
    }
    vertical_bounds = _vertical_bounds(
        boxes=boxes,
        hint=hint,
        next_hint=next_hint,
        page_height=page_height,
    )
    if vertical_bounds is None:
        return _template_region_result(
            target=target,
            hint=hint,
            message="OCR could not confidently locate the current printed question header; template region was used.",
            scope=scope,
        )
    top, bottom = vertical_bounds
    horizontal_bounds = _horizontal_bounds(
        hint=hint,
        boxes=boxes,
        top=top,
        bottom=bottom,
        page_width=page_width,
    )
    if horizontal_bounds is None:
        return _template_region_result(
            target=target,
            hint=hint,
            message="OCR produced unusable horizontal bounds; template region was used.",
            scope=scope,
        )
    left, right = horizontal_bounds

    return DetectResult(
        student_ref=target.page.student_ref,
        page_number=target.page.page_number,
        question_id=hint.question_id,
        status="ok",
        region=Region(
            x=left,
            y=top,
            width=right - left,
            height=bottom - top,
            units="rendered_page_pixels",
        ),
        region_source="ocr_refined",
        warnings=[],
    )


def _ocr_box(box: OcrBox) -> OcrTextBox:
    return OcrTextBox(
        text=box.text,
        left=box.left,
        top=box.top,
        right=box.right,
        bottom=box.bottom,
        confidence=box.confidence,
    )


def _sanitize_ocr_box_bounds(
    box: OcrTextBox, *, image_width: int, image_height: int
) -> OcrTextBox | None:
    left = max(0, min(image_width, box.left))
    top = max(0, min(image_height, box.top))
    right = max(0, min(image_width, box.right))
    bottom = max(0, min(image_height, box.bottom))
    if right <= left or bottom <= top:
        return None
    return OcrTextBox(
        text=box.text,
        left=left,
        top=top,
        right=right,
        bottom=bottom,
        confidence=box.confidence,
    )


def _sanitize_ocr_boxes(
    boxes: list[OcrTextBox], *, image_width: int, image_height: int
) -> list[OcrTextBox]:
    sanitized: list[OcrTextBox] = []
    for box in boxes:
        clamped = _sanitize_ocr_box_bounds(box, image_width=image_width, image_height=image_height)
        if clamped is not None:
            sanitized.append(clamped)
    return sanitized


def _ocr_metadata_path(output_dir: Path, *, student_ref: str, page_number: int) -> Path:
    return join_under_root(
        output_dir,
        "ocr",
        f"detect_ocr__page_number-{page_number:03d}__student_ref-{student_ref}.json",
    )


def _ocr_metadata_from_boxes(
    *,
    page_number: int,
    image_path: Path,
    image_width: int,
    image_height: int,
    boxes: list[OcrTextBox],
) -> PageOcrMetadata:
    return PageOcrMetadata(
        page_number=page_number,
        image_sha256=file_sha256(image_path),
        image_width=image_width,
        image_height=image_height,
        boxes=[
            OcrBox(
                text=box.text,
                left=box.left,
                top=box.top,
                right=box.right,
                bottom=box.bottom,
                confidence=box.confidence,
            )
            for box in boxes
        ],
    )


def _load_ocr_metadata(
    *, path: Path, target: DetectTarget, image_width: int, image_height: int
) -> PageOcrMetadata:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(payload, dict):
            raise ValueError("OCR metadata artifact must decode to an object.")
        raw_boxes = payload.get("boxes")
        if raw_boxes is None:
            payload["boxes"] = []
        elif not isinstance(raw_boxes, list):
            raise ValueError("OCR metadata boxes must be a list.")
        else:
            sanitized_boxes: list[dict[str, object]] = []
            for raw_box in raw_boxes:
                if not isinstance(raw_box, dict):
                    raise ValueError("OCR metadata box rows must be objects.")
                sanitized = _sanitize_ocr_box_bounds(
                    OcrTextBox(
                        text=str(raw_box["text"]).strip(),
                        left=int(raw_box["left"]),
                        top=int(raw_box["top"]),
                        right=int(raw_box["right"]),
                        bottom=int(raw_box["bottom"]),
                        confidence=float(raw_box["confidence"]),
                    ),
                    image_width=image_width,
                    image_height=image_height,
                )
                if sanitized is None:
                    continue
                sanitized_boxes.append(
                    {
                        "text": sanitized.text,
                        "left": sanitized.left,
                        "top": sanitized.top,
                        "right": sanitized.right,
                        "bottom": sanitized.bottom,
                        "confidence": sanitized.confidence,
                    }
                )
            payload["boxes"] = sanitized_boxes
        metadata = PageOcrMetadata.model_validate(payload)
    except Exception as exc:
        raise ScriptscoreError(
            code="detect_ocr_metadata_invalid",
            message=str(exc) or "OCR metadata artifact could not be parsed.",
            category=ErrorCategory.VALIDATION,
            retryable=True,
            details={"path": str(path)},
            write_state=WriteState.NO_WRITE,
        ) from exc
    expected_sha256 = file_sha256(target.page.image_path)
    if metadata.page_number != target.page.page_number:
        raise ScriptscoreError(
            code="detect_ocr_metadata_mismatch",
            message="OCR metadata page_number did not match the supplied detect target page.",
            category=ErrorCategory.VALIDATION,
            retryable=True,
            details={
                "path": str(path),
                "expected_page_number": target.page.page_number,
                "actual_page_number": metadata.page_number,
            },
            write_state=WriteState.NO_WRITE,
        )
    if metadata.image_width != image_width or metadata.image_height != image_height:
        raise ScriptscoreError(
            code="detect_ocr_metadata_mismatch",
            message="OCR metadata dimensions did not match the supplied detect target page.",
            category=ErrorCategory.VALIDATION,
            retryable=True,
            details={
                "path": str(path),
                "expected_width": image_width,
                "expected_height": image_height,
                "actual_width": metadata.image_width,
                "actual_height": metadata.image_height,
            },
            write_state=WriteState.NO_WRITE,
        )
    if metadata.image_sha256 != expected_sha256:
        raise ScriptscoreError(
            code="detect_ocr_metadata_mismatch",
            message="OCR metadata fingerprint did not match the supplied detect target page.",
            category=ErrorCategory.VALIDATION,
            retryable=True,
            details={
                "path": str(path),
                "expected_image_sha256": expected_sha256,
                "actual_image_sha256": metadata.image_sha256,
            },
            write_state=WriteState.NO_WRITE,
        )
    return metadata


def _write_ocr_metadata(
    *,
    output_dir: Path,
    student_ref: str,
    metadata: PageOcrMetadata,
) -> tuple[Path, ArtifactReference]:
    path = _ocr_metadata_path(output_dir, student_ref=student_ref, page_number=metadata.page_number)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(metadata.model_dump(mode="json"), indent=2, sort_keys=True),
        encoding="utf-8",
    )
    return (
        path,
        file_artifact(
            role="detect_ocr_metadata",
            label=path.name,
            path=path,
            fmt="json",
            scope={"student_ref": student_ref, "page_number": metadata.page_number},
        ),
    )


def _trace_path(output_dir: Path, *, student_ref: str, page_number: int) -> Path:
    return join_under_root(
        output_dir,
        "traces",
        f"detect__page_number-{page_number:03d}__student_ref-{student_ref}.json",
    )


def _write_trace(
    *,
    output_dir: Path,
    target: DetectTarget,
    boxes: list[OcrTextBox],
    results: list[DetectResult],
    ocr_metadata_path: Path,
    ocr_source: str,
    image_width: int,
    image_height: int,
    ocr_timing_ms: int | None = None,
) -> ArtifactReference:
    assert target.page.student_ref is not None
    path = _trace_path(
        output_dir, student_ref=target.page.student_ref, page_number=target.page.page_number
    )
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "student_ref": target.page.student_ref,
        "page_number": target.page.page_number,
        "page_image_path": str(target.page.image_path),
        "ocr_source": ocr_source,
        "ocr_metadata_path": str(ocr_metadata_path),
        "image_width": image_width,
        "image_height": image_height,
        "ocr_box_count": len(boxes),
        "ocr_timing_ms": ocr_timing_ms,
        "ocr_boxes": [
            {
                "text": box.text,
                "left": box.left,
                "top": box.top,
                "right": box.right,
                "bottom": box.bottom,
                "confidence": box.confidence,
            }
            for box in boxes
        ],
        "detect_results": [result.model_dump(mode="json", exclude_none=True) for result in results],
    }
    path.write_text(json.dumps(payload, indent=2, sort_keys=True), encoding="utf-8")
    return file_artifact(
        role="detect_trace",
        label=path.name,
        path=path,
        fmt="json",
        scope={"student_ref": target.page.student_ref, "page_number": target.page.page_number},
    )


def handle_scans_detect(ctx: CommandContext, request: ScansDetectRequest) -> CommandOutcome:
    """Refine per-student question regions using OCR."""

    ensure_paths_exist(
        [target.page.image_path for target in request.detect_targets], command=_COMMAND_NAME
    )
    ensure_paths_exist(
        [
            target.ocr_metadata_path
            for target in request.detect_targets
            if target.ocr_metadata_path is not None
        ],
        command=_COMMAND_NAME,
    )
    total = sum(len(target.question_hints) for target in request.detect_targets)
    results: list[DetectResult] = []
    page_ocr_results: list[PageOcrResult] = []
    artifacts: list[ArtifactReference] = []
    failed_count = 0
    fallback_count = 0
    completed = 0

    if total > 0:
        ctx.emit(
            event="started",
            progress=progress(completed=0, total=total),
            data={"result_row_count": total, "total_stages": 1},
        )
    else:
        ctx.emit(event="started", data={"result_row_count": 0, "total_stages": 1})

    for target in request.detect_targets:
        ctx.check_cancelled()
        assert target.page.student_ref is not None
        page_scope = {
            "student_ref": target.page.student_ref,
            "page_number": target.page.page_number,
        }
        image = load_page_image(target.page.image_path)
        ocr_timing_ms: int | None = None
        ordered_hints = sorted(
            target.question_hints, key=lambda hint: (hint.template_region.y, hint.question_number)
        )
        if target.ocr_metadata_path is not None:
            try:
                metadata = _load_ocr_metadata(
                    path=target.ocr_metadata_path,
                    target=target,
                    image_width=image.width,
                    image_height=image.height,
                )
                boxes = _sanitize_ocr_boxes(
                    [_ocr_box(box) for box in metadata.boxes],
                    image_width=image.width,
                    image_height=image.height,
                )
                ocr_source: Literal["generated", "artifact", "inline_boxes"] = "artifact"
            except ScriptscoreError as exc:
                fallback_message = (
                    "Supplied OCR metadata could not be reused; template region was used. "
                    f"{exc.message}"
                )
                page_results = []
                for hint in ordered_hints:
                    scope = {**page_scope, "question_id": hint.question_id}
                    ctx.emit(
                        event="item_started",
                        progress=progress(completed=completed, total=total),
                        scope=scope,
                    )
                    result = _template_region_result(
                        target=target,
                        hint=hint,
                        message=fallback_message,
                        scope=scope,
                    )
                    page_results.append(result)
                    results.append(result)
                    fallback_count += 1
                    completed += 1
                    ctx.emit(
                        event="item_completed",
                        progress=progress(completed=completed, total=total),
                        scope=scope,
                        data={"status": result.status, "region_source": result.region_source},
                    )
                artifacts.append(
                    _write_trace(
                        output_dir=request.output_artifacts_dir,
                        target=target,
                        boxes=[],
                        results=page_results,
                        ocr_metadata_path=target.ocr_metadata_path,
                        ocr_source="artifact",
                        image_width=image.width,
                        image_height=image.height,
                        ocr_timing_ms=None,
                    )
                )
                continue
        elif target.ocr_boxes is not None:
            boxes = _sanitize_ocr_boxes(
                [_ocr_box(box) for box in target.ocr_boxes],
                image_width=image.width,
                image_height=image.height,
            )
            metadata = _ocr_metadata_from_boxes(
                page_number=target.page.page_number,
                image_path=target.page.image_path,
                image_width=image.width,
                image_height=image.height,
                boxes=boxes,
            )
            ocr_source = "inline_boxes"
        else:
            ctx.emit(
                event="page_ocr_started",
                scope=page_scope,
                data={"image_width": image.width, "image_height": image.height},
            )
            ocr_started = perf_counter()
            boxes = _sanitize_ocr_boxes(
                read_page_ocr(target.page.image_path),
                image_width=image.width,
                image_height=image.height,
            )
            ocr_timing_ms = round((perf_counter() - ocr_started) * 1000)
            metadata = _ocr_metadata_from_boxes(
                page_number=target.page.page_number,
                image_path=target.page.image_path,
                image_width=image.width,
                image_height=image.height,
                boxes=boxes,
            )
            ocr_source = "generated"
            ctx.emit(
                event="page_ocr_completed",
                scope=page_scope,
                data={
                    "image_width": image.width,
                    "image_height": image.height,
                    "ocr_box_count": len(boxes),
                    "ocr_timing_ms": ocr_timing_ms,
                },
            )
        ocr_metadata_path, ocr_artifact = _write_ocr_metadata(
            output_dir=request.output_artifacts_dir,
            student_ref=target.page.student_ref,
            metadata=metadata,
        )
        artifacts.append(ocr_artifact)
        page_ocr_results.append(
            PageOcrResult(
                student_ref=target.page.student_ref,
                page_number=target.page.page_number,
                ocr_metadata_path=ocr_metadata_path,
                ocr_source=ocr_source,
            )
        )
        page_results = []
        for index, hint in enumerate(ordered_hints):
            scope = {**page_scope, "question_id": hint.question_id}
            ctx.emit(
                event="item_started",
                progress=progress(completed=completed, total=total),
                scope=scope,
            )
            next_hint = ordered_hints[index + 1] if index + 1 < len(ordered_hints) else None
            result = _detect_region(
                target=target,
                hint=hint,
                next_hint=next_hint,
                boxes=boxes,
                page_width=image.width,
                page_height=image.height,
            )
            if result.status == "warning":
                fallback_count += 1
            if result.status == "error":
                failed_count += 1
            page_results.append(result)
            results.append(result)
            completed += 1
            ctx.emit(
                event="item_completed",
                progress=progress(completed=completed, total=total),
                scope=scope,
                data={"status": result.status, "region_source": result.region_source},
            )
        artifacts.append(
            _write_trace(
                output_dir=request.output_artifacts_dir,
                target=target,
                boxes=boxes,
                results=page_results,
                ocr_metadata_path=ocr_metadata_path,
                ocr_source=ocr_source,
                image_width=image.width,
                image_height=image.height,
                ocr_timing_ms=ocr_timing_ms,
            )
        )

    if total > 0:
        ctx.emit(
            event="completed",
            progress=progress(completed=completed, total=total),
            data={
                "result_row_count": total,
                "failed_count": failed_count,
                "fallback_count": fallback_count,
            },
        )
    else:
        ctx.emit(
            event="completed", data={"result_row_count": 0, "failed_count": 0, "fallback_count": 0}
        )

    extra_warnings: list[WarningObject] = []
    if fallback_count:
        noun = "row" if fallback_count == 1 else "rows"
        extra_warnings.append(
            warning(
                code="detect_template_fallback",
                message=f"Detection used template-region fallback for {fallback_count} result {noun}.",
                scope={"row_count": fallback_count},
            )
        )

    return batch_outcome(
        data=batch_result_data(
            rows_key="detect_results",
            rows=[result.model_dump(mode="json", exclude_none=True) for result in results],
            output_artifacts_dir=request.output_artifacts_dir,
            extra={
                "page_ocr_results": [
                    row.model_dump(mode="json", exclude_none=True) for row in page_ocr_results
                ]
            },
        ),
        output_artifacts_dir=request.output_artifacts_dir,
        artifacts=artifacts,
        result_row_count=len(results),
        failed_count=failed_count,
        command_label="Detect",
        extra_warnings=extra_warnings,
    )


def scans_detect_spec() -> CommandSpec:
    return CommandSpec(
        name=_COMMAND_NAME, request_model=ScansDetectRequest, handler=handle_scans_detect
    )
