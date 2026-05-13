# SPDX-License-Identifier: AGPL-3.0-only
"""Fake providers used for tests and local scaffold validation."""

from __future__ import annotations

import json
import re
from collections.abc import Callable
from dataclasses import dataclass
from html import unescape
from html.parser import HTMLParser
from pathlib import Path
from typing import Any, Literal

from PIL import Image, ImageChops

from scriptscore.contracts import WarningObject
from scriptscore.providers.constants import PROVIDER_INTERFACE_VERSION
from scriptscore.providers.interfaces import (
    AlignmentProvider,
    AlignmentRequest,
    AlignmentResponse,
    LlmProvider,
    LlmRequest,
    LlmResponse,
    ProviderCapability,
)

_TAG_RE = re.compile(r"<(?P<tag>[a-zA-Z0-9_]+)>(?P<value>.*?)</(?P=tag)>", re.DOTALL)
_QUESTION_PREFIX_RE = re.compile(r"^\s*(?:question\s*)?\d+\s*[\].:)-]*\s*", re.IGNORECASE)


class _AssessmentAttrsParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.attrs: dict[str, str] = {}

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag == "assessment" and not self.attrs:
            self.attrs = {name: value for name, value in attrs if value is not None}


def _extract_assessment_attrs(text: str) -> dict[str, str]:
    parser = _AssessmentAttrsParser()
    parser.feed(text)
    return parser.attrs


def _extract_tag(text: str, tag: str) -> str:
    for match in _TAG_RE.finditer(text):
        if match.group("tag") == tag:
            return unescape(match.group("value").strip())
    return ""


def _extract_candidate_pairs(rendered_text: str) -> list[dict[str, Any]]:
    candidate_pairs_raw = _extract_tag(rendered_text, "candidate_pairs")
    if not candidate_pairs_raw:
        return []
    pairs: list[dict[str, Any]] = []
    for match in re.finditer(
        r'<pair\s+left_index="(?P<left>\d+)"\s+right_index="(?P<right>\d+)">(?P<body>.*?)</pair>',
        candidate_pairs_raw,
        re.DOTALL,
    ):
        body = match.group("body")
        pairs.append(
            {
                "left_index": int(match.group("left")),
                "right_index": int(match.group("right")),
                "code": _extract_tag(body, "code"),
                "message": _extract_tag(body, "message"),
            }
        )
    return pairs


def _image_has_nonwhite_pixels(path: str) -> bool:
    with Image.open(Path(path)) as image:
        normalized = image.convert("L")
        inverted = ImageChops.invert(normalized)
        return inverted.getbbox() is not None


def _default_llm_response(request: LlmRequest) -> LlmResponse:
    if request.prompt_id == "question_text":
        baseline = _extract_tag(request.rendered_text, "baseline_pdf_text")
        if baseline:
            return LlmResponse(
                raw_text=_QUESTION_PREFIX_RE.sub("", baseline).strip() or baseline.strip()
            )
        return LlmResponse(raw_text="Derived question text")

    if request.prompt_id == "question_context":
        return LlmResponse(raw_text="")

    if request.prompt_id == "rubric_generate":
        max_points_raw = _extract_tag(request.rendered_text, "max_points")
        max_points = max(1, int(max_points_raw or "1"))
        criteria: list[dict[str, Any]] = [
            {
                "label": "Overall response quality",
                "points": max_points,
                "partial_credit_guidance": (
                    f"Award between 0 and {max_points} points based on correctness and completeness."
                ),
            }
        ]
        return LlmResponse(raw_text=json.dumps({"criteria": criteria}))

    if request.prompt_id == "rubric_semantic_review":
        candidate_pairs = _extract_candidate_pairs(request.rendered_text)
        return LlmResponse(
            raw_text=json.dumps(
                {
                    "pair_reviews": [
                        {
                            "left_index": pair["left_index"],
                            "right_index": pair["right_index"],
                            "classification": "duplicate"
                            if pair.get("code") == "rubric_duplicate_criterion"
                            else "overlap",
                            "reason": "The candidate pair appears to target materially similar evidence.",
                        }
                        for pair in candidate_pairs
                    ]
                }
            )
        )

    if request.prompt_id == "handwriting_verify":
        has_handwriting = _image_has_nonwhite_pixels(request.file_inputs["question_crop_png"])
        return LlmResponse(
            raw_text=json.dumps(
                {
                    "has_handwriting": has_handwriting,
                    "confidence": "high",
                    "status": "complete",
                }
            )
        )

    if request.prompt_id == "parse_ocr":
        if not _image_has_nonwhite_pixels(request.file_inputs["question_crop_png"]):
            return LlmResponse(raw_text="[blank]")
        return LlmResponse(raw_text="parsed answer")

    if request.prompt_id == "preliminary_score":
        student_answer = _extract_tag(request.rendered_text, "student_answer")
        max_points = max(0, int(_extract_tag(request.rendered_text, "points") or "0"))
        visible_answer = bool(student_answer.strip())
        minimum_positive_award = 1 if max_points > 0 else 0
        points_awarded = min(max_points, minimum_positive_award) if visible_answer else 0
        return LlmResponse(
            raw_text=json.dumps(
                {
                    "points_awarded": points_awarded,
                    "rationale": "Awarded based on the visible evidence for this criterion."
                    if points_awarded
                    else "No criterion evidence was visible in the answer.",
                }
            )
        )

    if request.prompt_id == "consistency_review":
        return LlmResponse(raw_text=json.dumps({"adjustments": []}))

    if request.prompt_id == "feedback_draft":
        attrs = _extract_assessment_attrs(request.rendered_text)
        points_awarded_attr = attrs.get("total_points_awarded")
        max_points_attr = attrs.get("question_max_points")
        if (
            points_awarded_attr is not None
            and max_points_attr is not None
            and points_awarded_attr.isdigit()
            and points_awarded_attr == max_points_attr
        ):
            return LlmResponse(raw_text="Strong work overall.")
        return LlmResponse(raw_text="You showed some understanding but missed a key detail.")

    if request.prompt_id == "markup":
        return LlmResponse(raw_text=json.dumps({"incorrect_segments": []}))

    raise ValueError(f"Unsupported fake prompt id: {request.prompt_id}")


@dataclass(frozen=True)
class FakeProvider:
    """Minimal fake provider implementation for non-LLM capabilities."""

    capability: ProviderCapability
    provider_name: str
    interface_version: str = PROVIDER_INTERFACE_VERSION


@dataclass(frozen=True)
class FakeLlmProvider(LlmProvider):
    """Deterministic fake LLM provider used by tests and local CLI flows."""

    provider_name: str = "ollama_native"
    responder: Callable[[LlmRequest], LlmResponse] | None = None
    interface_version: str = PROVIDER_INTERFACE_VERSION

    @property
    def capability(self) -> Literal["llm_provider"]:
        return "llm_provider"

    def generate(self, request: LlmRequest) -> LlmResponse:
        if self.responder is not None:
            return self.responder(request)
        return _default_llm_response(request)


@dataclass(frozen=True)
class FakeAlignmentProvider(AlignmentProvider):
    """Deterministic fake alignment provider used by tests."""

    provider_name: str = "core_template_match"
    responder: Callable[[AlignmentRequest], AlignmentResponse] | None = None
    interface_version: str = PROVIDER_INTERFACE_VERSION

    @property
    def capability(self) -> Literal["alignment_engine"]:
        return "alignment_engine"

    def align(self, request: AlignmentRequest) -> AlignmentResponse:
        if self.responder is not None:
            return self.responder(request)
        warnings = []
        if request.marker_mode == "prefer_aruco":
            warnings.append(
                WarningObject(
                    code="marker_guided_alignment_not_used",
                    message="Marker-guided alignment was not used; template matching fallback was applied.",
                )
            )
        return AlignmentResponse(
            status="ok",
            confidence=0.99,
            rotation=0.0,
            scale=1.0,
            translate_x=0.0,
            translate_y=0.0,
            warnings=warnings,
        )


def builtin_fake_providers() -> dict[
    tuple[str, str], FakeProvider | FakeLlmProvider | FakeAlignmentProvider
]:
    """Return the built-in fake providers keyed by capability/name."""

    providers: list[FakeProvider | FakeLlmProvider | FakeAlignmentProvider] = [
        FakeLlmProvider(),
        FakeAlignmentProvider(),
    ]
    return {(provider.capability, provider.provider_name): provider for provider in providers}
