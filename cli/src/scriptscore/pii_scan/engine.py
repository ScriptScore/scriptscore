# SPDX-License-Identifier: AGPL-3.0-only
"""Local scans.pii analysis engine."""

from __future__ import annotations

import math
import statistics
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


def _infer_handwriting(
    raster: RasterBundle,
    tokens: list[VisionToken],
    *,
    backend_name: str,
) -> WritingSignal:
    reasons: list[str] = []
    alpha_tokens = [
        token for token in tokens if any(character.isalpha() for character in token.text)
    ]
    structure_score = text_structure_score(raster)

    if not alpha_tokens and structure_score < 0.08:
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

    if not alpha_tokens:
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

    confidences = [token.confidence for token in alpha_tokens]
    heights = [token.height for token in alpha_tokens]
    mean_confidence = statistics.fmean(confidences)
    low_confidence_share = sum(1 for confidence in confidences if confidence < 0.55) / len(
        confidences
    )
    height_variation = statistics.pstdev(heights) / max(1.0, statistics.fmean(heights))

    slopes: list[float] = []
    for token in alpha_tokens:
        delta_x = max(1, token.corners[1][0] - token.corners[0][0])
        delta_y = token.corners[1][1] - token.corners[0][1]
        slopes.append(abs(math.degrees(math.atan2(delta_y, delta_x))))
    average_slope = statistics.fmean(slopes)

    grayscale = cast(np.ndarray, raster.grayscale)
    binary_mask = cast(np.ndarray, raster.binary_mask)
    overall_fragment_density = _fragment_density(binary_mask)

    lower_start = int(grayscale.shape[0] * 0.45)
    lower_mask = binary_mask[lower_start:, :]
    lower_dark_share = (
        float((lower_mask < 160).sum()) / float(lower_mask.size) if lower_mask.size else 0.0
    )
    lower_fragment_density = _fragment_density(lower_mask)
    max_y_share = max((token.bottom for token in alpha_tokens), default=0) / max(
        1.0,
        grayscale.shape[0],
    )

    answer_band_start = int(grayscale.shape[0] * 0.19)
    answer_band_end = int(grayscale.shape[0] * 0.82)
    answer_band_mask = binary_mask[answer_band_start:answer_band_end, :]
    answer_band_dark_share = (
        float((answer_band_mask < 160).sum()) / float(answer_band_mask.size)
        if answer_band_mask.size
        else 0.0
    )
    answer_band_fragment_density = (
        _fragment_density(answer_band_mask) if answer_band_mask.size else 0.0
    )
    answer_band_tokens = [
        token
        for token in alpha_tokens
        if 0.19 <= (token.center_y / max(1.0, grayscale.shape[0])) <= 0.82
    ]
    answer_band_alpha_chars = sum(
        sum(1 for character in token.text if character.isalpha()) for token in answer_band_tokens
    )
    widest_answer_band_share = max(
        (token.width / max(1.0, grayscale.shape[1]) for token in answer_band_tokens),
        default=0.0,
    )
    footer_token_count = sum(
        1 for token in alpha_tokens if (token.center_y / max(1.0, grayscale.shape[0])) > 0.82
    )
    prompt_token_count = sum(
        1 for token in alpha_tokens if (token.center_y / max(1.0, grayscale.shape[0])) < 0.19
    )

    metrics = {
        "backend_name": backend_name,
        "presence": round(structure_score, 4),
        "alpha_word_count": len(alpha_tokens),
        "mean_confidence": round(mean_confidence, 4),
        "low_conf_ratio": round(low_confidence_share, 4),
        "height_cv": round(height_variation, 4),
        "angle_mean": round(average_slope, 4),
        "component_density": round(overall_fragment_density, 4),
        "lower_dark_ratio": round(lower_dark_share, 4),
        "lower_component_density": round(lower_fragment_density, 4),
        "max_y_ratio": round(max_y_share, 4),
        "answer_band_dark_ratio": round(answer_band_dark_share, 4),
        "answer_band_component_density": round(answer_band_fragment_density, 4),
        "answer_band_word_count": len(answer_band_tokens),
        "answer_band_alpha_chars": answer_band_alpha_chars,
        "max_answer_band_width_ratio": round(widest_answer_band_share, 4),
        "footer_word_count": footer_token_count,
        "top_prompt_word_count": prompt_token_count,
    }

    if (
        mean_confidence >= 0.76
        and low_confidence_share <= 0.3
        and lower_dark_share <= 0.02
        and lower_fragment_density <= 0.35
        and len(alpha_tokens) >= 6
        and height_variation <= 0.25
        and average_slope <= 1.2
        and (
            not answer_band_tokens
            or (
                len(answer_band_tokens) == 1
                and (
                    widest_answer_band_share >= 0.55
                    or prompt_token_count >= (len(alpha_tokens) - footer_token_count - 1)
                )
            )
        )
    ):
        return WritingSignal(
            state="false",
            score=0.0,
            reasons=[
                "OCR is mostly high-confidence printed text",
                "lower answer region has minimal ink",
            ],
            metrics={**metrics, "score": 0.0, "status": "false"},
        )

    if (
        mean_confidence >= 0.72
        and low_confidence_share <= 0.15
        and lower_dark_share <= 0.01
        and lower_fragment_density <= 0.2
        and not answer_band_tokens
        and prompt_token_count <= 2
        and len(alpha_tokens) <= (prompt_token_count + footer_token_count + 1)
    ):
        return WritingSignal(
            state="false",
            score=0.0,
            reasons=[
                "OCR is mostly high-confidence printed text",
                "only a short top-of-page prompt is present",
            ],
            metrics={**metrics, "score": 0.0, "status": "false"},
        )

    if (
        mean_confidence >= 0.85
        and low_confidence_share <= 0.1
        and height_variation <= 0.2
        and average_slope <= 0.6
        and prompt_token_count >= 4
        and answer_band_alpha_chars >= 30
        and len(answer_band_tokens) <= prompt_token_count
        and lower_dark_share <= 0.015
        and lower_fragment_density <= 0.25
    ):
        return WritingSignal(
            state="false",
            score=0.0,
            reasons=[
                "OCR is highly printed-looking across the prompt and answer band",
                "answer-band text does not outweigh the printed prompt",
            ],
            metrics={**metrics, "score": 0.0, "status": "false"},
        )

    if (
        answer_band_alpha_chars < 8
        and footer_token_count >= 2
        and prompt_token_count >= 3
        and lower_dark_share <= 0.01
        and lower_fragment_density <= 0.25
    ):
        return WritingSignal(
            state="false",
            score=0.0,
            reasons=[
                "OCR is concentrated in the printed prompt and footer",
                "no answer-band handwriting evidence was recovered",
            ],
            metrics={**metrics, "score": 0.0, "status": "false"},
        )

    score = 0.0
    if structure_score > 0.12:
        score += 0.15
        reasons.append("image contains clear text-like structure")
    if lower_dark_share > 0.02:
        score += 0.25
        reasons.append("lower answer region contains ink")
    if lower_fragment_density > 0.6:
        score += 0.25
        reasons.append("lower answer region contains fragmented components")
    if answer_band_alpha_chars >= 8:
        score += 0.1
        reasons.append("OCR recovered text in the answer band")
    if len(answer_band_tokens) >= 2:
        score += 0.05
        reasons.append("multiple OCR segments appear in the answer band")
    elif (
        len(answer_band_tokens) == 1
        and answer_band_alpha_chars >= 24
        and widest_answer_band_share <= 0.55
    ):
        score += 0.05
        reasons.append("a compact OCR segment appears in the answer band")
    if answer_band_dark_share > 0.01:
        score += 0.1
        reasons.append("answer band contains ink")
    if answer_band_fragment_density > 0.6:
        score += 0.1
        reasons.append("answer band contains fragmented components")
    if height_variation > 0.22:
        score += 0.15
        reasons.append("token heights vary more than typical printed text")
    if average_slope > 1.0:
        score += 0.1
        reasons.append("text baseline is irregular")
    if overall_fragment_density > 2.5:
        score += 0.1
        reasons.append("stroke fragmentation suggests freehand writing")
    if mean_confidence < 0.75:
        score += 0.1
        reasons.append("OCR confidence is lower than clean printed text")
    if low_confidence_share > 0.2:
        score += 0.1
        reasons.append("many OCR tokens have low confidence")
    if (
        len(alpha_tokens) <= 4
        and structure_score > 0.1
        and len(answer_band_tokens) >= 1
        and answer_band_alpha_chars >= 12
        and height_variation > 0.15
    ):
        score += 0.15
        reasons.append("few OCR segments in the answer band suggest a short handwritten answer")

    score = min(1.0, score)
    if score >= 0.45:
        state: ScanHandwritingState = "true"
    elif score <= 0.15:
        state = "false"
    else:
        state = "unknown"
    if not reasons:
        reasons.append("handwriting heuristics were inconclusive")
    rounded = round(score, 3)
    return WritingSignal(
        state=state,
        score=rounded,
        reasons=reasons,
        metrics={**metrics, "score": rounded, "status": state},
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
