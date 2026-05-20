# SPDX-License-Identifier: AGPL-3.0-only
"""Local scans.pii analysis engine."""

from __future__ import annotations

import math
import statistics
from dataclasses import dataclass
from pathlib import Path
from time import perf_counter
from typing import Protocol, cast

import cv2
import numpy as np

from scriptscore.pii_scan.images import prepare_crop, text_structure_score
from scriptscore.pii_scan.matching import detect_student_pii
from scriptscore.pii_scan.reader import create_reader
from scriptscore.pii_scan.types import (
    RasterBundle,
    ReadResult,
    ScanFinding,
    ScanHandwritingState,
    ScanPiiKind,
    ScanRuntimeOptions,
    VisionToken,
    WritingSignal,
)


class TokenReader(Protocol):
    """Minimal OCR reader interface for scans.pii."""

    def read(self, image: object) -> ReadResult: ...


@dataclass(frozen=True)
class _HandwritingMetrics:
    backend_name: str
    presence: float
    alpha_word_count: int
    mean_confidence: float
    low_confidence_share: float
    height_variation: float
    average_slope: float
    overall_fragment_density: float
    lower_dark_share: float
    lower_fragment_density: float
    max_y_share: float
    answer_band_dark_share: float
    answer_band_fragment_density: float
    answer_band_word_count: int
    answer_band_alpha_chars: int
    widest_answer_band_share: float
    footer_token_count: int
    prompt_token_count: int

    def as_dict(self) -> dict[str, float | int | str]:
        return {
            "backend_name": self.backend_name,
            "presence": round(self.presence, 4),
            "alpha_word_count": self.alpha_word_count,
            "mean_confidence": round(self.mean_confidence, 4),
            "low_conf_ratio": round(self.low_confidence_share, 4),
            "height_cv": round(self.height_variation, 4),
            "angle_mean": round(self.average_slope, 4),
            "component_density": round(self.overall_fragment_density, 4),
            "lower_dark_ratio": round(self.lower_dark_share, 4),
            "lower_component_density": round(self.lower_fragment_density, 4),
            "max_y_ratio": round(self.max_y_share, 4),
            "answer_band_dark_ratio": round(self.answer_band_dark_share, 4),
            "answer_band_component_density": round(self.answer_band_fragment_density, 4),
            "answer_band_word_count": self.answer_band_word_count,
            "answer_band_alpha_chars": self.answer_band_alpha_chars,
            "max_answer_band_width_ratio": round(self.widest_answer_band_share, 4),
            "footer_word_count": self.footer_token_count,
            "top_prompt_word_count": self.prompt_token_count,
        }


def _fragment_density(binary_mask: object) -> float:
    mask = cast(np.ndarray, binary_mask)
    component_total, _labels, stats, _centroids = cv2.connectedComponentsWithStats(
        255 - mask,
        connectivity=8,
    )
    fragment_count = 0
    for index in range(1, component_total):
        area = int(stats[index, cv2.CC_STAT_AREA])
        if 4 <= area <= 500:
            fragment_count += 1
    return float(fragment_count / max(1.0, (mask.shape[0] * mask.shape[1]) / 10000.0))


def _alpha_tokens(tokens: list[VisionToken]) -> list[VisionToken]:
    return [token for token in tokens if any(character.isalpha() for character in token.text)]


def _no_alpha_signal(
    *,
    structure_score: float,
    backend_name: str,
) -> WritingSignal:
    if structure_score < 0.08:
        return WritingSignal(
            state="false",
            score=0.0,
            reasons=["image has very little text-like structure"],
            metrics={
                "presence": round(structure_score, 4),
                "alpha_word_count": 0,
                "backend_name": backend_name,
            },
        )
    return WritingSignal(
        state="unknown",
        score=round(structure_score, 3),
        reasons=["text-like structure exists but OCR did not recover readable words"],
        metrics={
            "presence": round(structure_score, 4),
            "alpha_word_count": 0,
            "backend_name": backend_name,
        },
    )


def _average_token_slope(tokens: list[VisionToken]) -> float:
    slopes: list[float] = []
    for token in tokens:
        delta_x = max(1, token.corners[1][0] - token.corners[0][0])
        delta_y = token.corners[1][1] - token.corners[0][1]
        slopes.append(abs(math.degrees(math.atan2(delta_y, delta_x))))
    return statistics.fmean(slopes)


def _dark_share(mask: object) -> float:
    array = cast(np.ndarray, mask)
    return float((array < 160).sum()) / float(array.size) if array.size else 0.0


def _handwriting_metrics(
    raster: RasterBundle,
    alpha_tokens: list[VisionToken],
    *,
    backend_name: str,
    structure_score: float,
) -> _HandwritingMetrics:
    confidences = [token.confidence for token in alpha_tokens]
    heights = [token.height for token in alpha_tokens]
    grayscale = cast(np.ndarray, raster.grayscale)
    binary_mask = cast(np.ndarray, raster.binary_mask)
    lower_start = int(grayscale.shape[0] * 0.45)
    lower_mask = binary_mask[lower_start:, :]
    answer_band_start = int(grayscale.shape[0] * 0.19)
    answer_band_end = int(grayscale.shape[0] * 0.82)
    answer_band_mask = binary_mask[answer_band_start:answer_band_end, :]
    answer_band_tokens = [
        token
        for token in alpha_tokens
        if 0.19 <= (token.center_y / max(1.0, grayscale.shape[0])) <= 0.82
    ]
    return _HandwritingMetrics(
        backend_name=backend_name,
        presence=structure_score,
        alpha_word_count=len(alpha_tokens),
        mean_confidence=statistics.fmean(confidences),
        low_confidence_share=sum(1 for confidence in confidences if confidence < 0.55)
        / len(confidences),
        height_variation=statistics.pstdev(heights) / max(1.0, statistics.fmean(heights)),
        average_slope=_average_token_slope(alpha_tokens),
        overall_fragment_density=_fragment_density(binary_mask),
        lower_dark_share=_dark_share(lower_mask),
        lower_fragment_density=_fragment_density(lower_mask),
        max_y_share=max((token.bottom for token in alpha_tokens), default=0)
        / max(1.0, grayscale.shape[0]),
        answer_band_dark_share=_dark_share(answer_band_mask),
        answer_band_fragment_density=_fragment_density(answer_band_mask)
        if answer_band_mask.size
        else 0.0,
        answer_band_word_count=len(answer_band_tokens),
        answer_band_alpha_chars=sum(
            sum(1 for character in token.text if character.isalpha())
            for token in answer_band_tokens
        ),
        widest_answer_band_share=max(
            (token.width / max(1.0, grayscale.shape[1]) for token in answer_band_tokens),
            default=0.0,
        ),
        footer_token_count=sum(
            1 for token in alpha_tokens if (token.center_y / max(1.0, grayscale.shape[0])) > 0.82
        ),
        prompt_token_count=sum(
            1 for token in alpha_tokens if (token.center_y / max(1.0, grayscale.shape[0])) < 0.19
        ),
    )


def _false_writing_signal(metrics: _HandwritingMetrics, reasons: list[str]) -> WritingSignal:
    return WritingSignal(
        state="false",
        score=0.0,
        reasons=reasons,
        metrics={**metrics.as_dict(), "score": 0.0, "status": "false"},
    )


def _mostly_printed_with_empty_answer_region(metrics: _HandwritingMetrics) -> bool:
    return (
        metrics.mean_confidence >= 0.76
        and metrics.low_confidence_share <= 0.3
        and metrics.lower_dark_share <= 0.02
        and metrics.lower_fragment_density <= 0.35
        and metrics.alpha_word_count >= 6
        and metrics.height_variation <= 0.25
        and metrics.average_slope <= 1.2
        and (
            metrics.answer_band_word_count == 0
            or (
                metrics.answer_band_word_count == 1
                and (
                    metrics.widest_answer_band_share >= 0.55
                    or metrics.prompt_token_count
                    >= (metrics.alpha_word_count - metrics.footer_token_count - 1)
                )
            )
        )
    )


def _short_top_prompt_only(metrics: _HandwritingMetrics) -> bool:
    return (
        metrics.mean_confidence >= 0.72
        and metrics.low_confidence_share <= 0.15
        and metrics.lower_dark_share <= 0.01
        and metrics.lower_fragment_density <= 0.2
        and metrics.answer_band_word_count == 0
        and metrics.prompt_token_count <= 2
        and metrics.alpha_word_count
        <= (metrics.prompt_token_count + metrics.footer_token_count + 1)
    )


def _printed_prompt_outweighs_answer(metrics: _HandwritingMetrics) -> bool:
    return (
        metrics.mean_confidence >= 0.85
        and metrics.low_confidence_share <= 0.1
        and metrics.height_variation <= 0.2
        and metrics.average_slope <= 0.6
        and metrics.prompt_token_count >= 4
        and metrics.answer_band_alpha_chars >= 30
        and metrics.answer_band_word_count <= metrics.prompt_token_count
        and metrics.lower_dark_share <= 0.015
        and metrics.lower_fragment_density <= 0.25
    )


def _prompt_footer_without_answer(metrics: _HandwritingMetrics) -> bool:
    return (
        metrics.answer_band_alpha_chars < 8
        and metrics.footer_token_count >= 2
        and metrics.prompt_token_count >= 3
        and metrics.lower_dark_share <= 0.01
        and metrics.lower_fragment_density <= 0.25
    )


def _printed_text_signal(metrics: _HandwritingMetrics) -> WritingSignal | None:
    if _mostly_printed_with_empty_answer_region(metrics):
        return _false_writing_signal(
            metrics,
            [
                "OCR is mostly high-confidence printed text",
                "lower answer region has minimal ink",
            ],
        )

    if _short_top_prompt_only(metrics):
        return _false_writing_signal(
            metrics,
            [
                "OCR is mostly high-confidence printed text",
                "only a short top-of-page prompt is present",
            ],
        )

    if _printed_prompt_outweighs_answer(metrics):
        return _false_writing_signal(
            metrics,
            [
                "OCR is highly printed-looking across the prompt and answer band",
                "answer-band text does not outweigh the printed prompt",
            ],
        )

    if _prompt_footer_without_answer(metrics):
        return _false_writing_signal(
            metrics,
            [
                "OCR is concentrated in the printed prompt and footer",
                "no answer-band handwriting evidence was recovered",
            ],
        )
    return None


def _add_handwriting_score(
    reasons: list[str],
    score: float,
    condition: bool,
    increment: float,
    reason: str,
) -> float:
    if condition:
        reasons.append(reason)
        return score + increment
    return score


def _score_handwriting(metrics: _HandwritingMetrics) -> tuple[float, list[str]]:
    reasons: list[str] = []
    score = 0.0
    score = _add_handwriting_score(
        reasons, score, metrics.presence > 0.12, 0.15, "image contains clear text-like structure"
    )
    score = _add_handwriting_score(
        reasons, score, metrics.lower_dark_share > 0.02, 0.25, "lower answer region contains ink"
    )
    score = _add_handwriting_score(
        reasons,
        score,
        metrics.lower_fragment_density > 0.6,
        0.25,
        "lower answer region contains fragmented components",
    )
    score = _add_handwriting_score(
        reasons,
        score,
        metrics.answer_band_alpha_chars >= 8,
        0.1,
        "OCR recovered text in the answer band",
    )
    score = _add_handwriting_score(
        reasons,
        score,
        metrics.answer_band_word_count >= 2,
        0.05,
        "multiple OCR segments appear in the answer band",
    )
    score = _add_handwriting_score(
        reasons,
        score,
        metrics.answer_band_word_count == 1
        and metrics.answer_band_alpha_chars >= 24
        and metrics.widest_answer_band_share <= 0.55,
        0.05,
        "a compact OCR segment appears in the answer band",
    )
    score = _add_handwriting_score(
        reasons, score, metrics.answer_band_dark_share > 0.01, 0.1, "answer band contains ink"
    )
    score = _add_handwriting_score(
        reasons,
        score,
        metrics.answer_band_fragment_density > 0.6,
        0.1,
        "answer band contains fragmented components",
    )
    score = _add_handwriting_score(
        reasons,
        score,
        metrics.height_variation > 0.22,
        0.15,
        "token heights vary more than typical printed text",
    )
    score = _add_handwriting_score(
        reasons, score, metrics.average_slope > 1.0, 0.1, "text baseline is irregular"
    )
    score = _add_handwriting_score(
        reasons,
        score,
        metrics.overall_fragment_density > 2.5,
        0.1,
        "stroke fragmentation suggests freehand writing",
    )
    score = _add_handwriting_score(
        reasons,
        score,
        metrics.mean_confidence < 0.75,
        0.1,
        "OCR confidence is lower than clean printed text",
    )
    score = _add_handwriting_score(
        reasons,
        score,
        metrics.low_confidence_share > 0.2,
        0.1,
        "many OCR tokens have low confidence",
    )
    score = _add_handwriting_score(
        reasons,
        score,
        metrics.alpha_word_count <= 4
        and metrics.presence > 0.1
        and metrics.answer_band_word_count >= 1
        and metrics.answer_band_alpha_chars >= 12
        and metrics.height_variation > 0.15,
        0.15,
        "few OCR segments in the answer band suggest a short handwritten answer",
    )
    return min(1.0, score), reasons


def _handwriting_state(score: float) -> ScanHandwritingState:
    if score >= 0.45:
        return "true"
    if score <= 0.15:
        return "false"
    return "unknown"


def _infer_handwriting(
    raster: RasterBundle,
    tokens: list[VisionToken],
    *,
    backend_name: str,
) -> WritingSignal:
    alpha_tokens = _alpha_tokens(tokens)
    structure_score = text_structure_score(raster)
    if not alpha_tokens:
        return _no_alpha_signal(structure_score=structure_score, backend_name=backend_name)

    metrics = _handwriting_metrics(
        raster,
        alpha_tokens,
        backend_name=backend_name,
        structure_score=structure_score,
    )
    printed_signal = _printed_text_signal(metrics)
    if printed_signal is not None:
        return printed_signal

    score, reasons = _score_handwriting(metrics)
    if not reasons:
        reasons.append("handwriting heuristics were inconclusive")
    rounded = round(score, 3)
    state = _handwriting_state(score)
    return WritingSignal(
        state=state,
        score=rounded,
        reasons=reasons,
        metrics={**metrics.as_dict(), "score": rounded, "status": state},
    )


def inspect_student_crop(
    image_path: Path,
    *,
    trigger_words: list[str],
    options: ScanRuntimeOptions,
    reader: TokenReader | None = None,
) -> ScanFinding:
    """Run the local handwriting and PII scan for one question crop."""

    started_at = perf_counter()
    durations: dict[str, float] = {}

    preprocess_started = perf_counter()
    try:
        raster = prepare_crop(image_path, max_dimension=options.max_dimension)
    except Exception as exc:
        return ScanFinding(
            image_path=str(image_path),
            handwriting_state="unknown",
            handwriting_score=0.0,
            pii_present=False,
            pii_kinds=[],
            reasons=["image preprocessing failed"],
            duration_seconds=round(perf_counter() - started_at, 4),
            stage_durations={"preprocess": round(perf_counter() - preprocess_started, 4)}
            if options.include_metrics
            else {},
            fatal_error=str(exc),
        )
    durations["preprocess"] = round(perf_counter() - preprocess_started, 4)

    active_reader: TokenReader = reader or create_reader(options.model_root)
    ocr_started = perf_counter()
    try:
        read_result = active_reader.read(raster.color_bgr)
        tokens = read_result.tokens
        backend_warnings: list[str] = []
        backend_name = read_result.backend_name
    except Exception as exc:
        tokens = []
        backend_warnings = [f"OCR failed: {exc}"]
        backend_name = "paddleocr"
    durations["ocr"] = round(perf_counter() - ocr_started, 4)

    handwriting_started = perf_counter()
    writing = _infer_handwriting(raster, tokens, backend_name=backend_name)
    durations["handwriting"] = round(perf_counter() - handwriting_started, 4)

    extracted_text = " ".join(token.text for token in tokens)
    pii_started = perf_counter()
    hits = detect_student_pii(
        extracted_text=extracted_text,
        tokens=tokens,
        raster=raster,
        trigger_words=trigger_words,
    )
    durations["pii"] = round(perf_counter() - pii_started, 4)

    reasons = [f"OCR backend: {backend_name}", *writing.reasons]
    reasons.extend(hit.reason for hit in hits[:3])

    pii_present = writing.state == "true" and bool(hits)
    if hits and writing.state != "true":
        reasons.append(
            "PII-like matches were suppressed because handwriting was not confidently present"
        )

    return ScanFinding(
        image_path=str(image_path),
        handwriting_state=writing.state,
        handwriting_score=writing.score,
        pii_present=pii_present,
        pii_kinds=cast(
            list[ScanPiiKind], sorted({hit.kind for hit in hits}) if pii_present else []
        ),
        reasons=reasons,
        duration_seconds=round(perf_counter() - started_at, 4),
        stage_durations=durations if options.include_metrics else {},
        metrics=writing.metrics if options.include_metrics else {},
        extracted_text=extracted_text if options.include_text else None,
        backend_warnings=backend_warnings,
    )
