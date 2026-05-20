# SPDX-License-Identifier: AGPL-3.0-only
"""Shared helpers for exam command implementations."""

from __future__ import annotations

import re
from dataclasses import dataclass
from difflib import SequenceMatcher
from html import escape

import fitz  # type: ignore[import-untyped]

from scriptscore.commands.common import warning
from scriptscore.contracts import (
    ErrorCategory,
    ExamGenerateRubricRequest,
    RubricCriterion,
    ScriptscoreError,
    WarningObject,
    WriteState,
)
from scriptscore.prompts import render_xml, xml_node, xml_text

_QUESTION_MARKER_RE = re.compile(
    r"^\s*(?:Question\s+)?(?P<number>\d+)(?:\s*\(\d+\s*(?:pts?|points?)\)|\s*[\].:)])",
    re.IGNORECASE,
)
_QUESTION_PREFIX_RE = re.compile(r"^\s*(?:question\s*)?\d+\s*[\].:)-]*\s*", re.IGNORECASE)
_POINTS_RE = re.compile(r"(?<!\d)(?P<points>\d+)\s*(?:pts?|points?)\b", re.IGNORECASE)
_PAREN_POINTS_RE = re.compile(r"\((?P<points>\d+)\)")
_POINT_DISTRIBUTION_HEADER_RE = re.compile(r"^Point Distribution\b", re.IGNORECASE)
_RUBRIC_WORD_RE = re.compile(r"[a-z0-9]+")
_INTEGER_LINE_RE = re.compile(r"^\d+$")
_POINT_SUMMARY_PATTERNS = (
    re.compile(
        r"^Q(?P<number>\d+)\s*[:\u2013\u2014-]\s*(?P<points>\d+)\s*(?:pts?|points?)$",
        re.IGNORECASE,
    ),
    re.compile(
        r"^Question\s+(?P<number>\d+)\s*[:\u2013\u2014-]\s*(?P<points>\d+)\s*(?:pts?|points?)$",
        re.IGNORECASE,
    ),
    re.compile(
        r"^Question\s+(?P<number>\d+)\s*\((?P<points>\d+)\s*(?:pts?|points?)\)$",
        re.IGNORECASE,
    ),
    re.compile(
        r"^(?P<number>\d+)[.)]\s*(?P<points>\d+)\s*(?:pts?|points?)$",
        re.IGNORECASE,
    ),
)
_RUBRIC_STOPWORDS = frozenset(
    {
        "answer",
        "answers",
        "any",
        "award",
        "awarded",
        "awards",
        "blank",
        "credit",
        "criterion",
        "for",
        "full",
        "if",
        "non",
        "partial",
        "point",
        "points",
        "related",
        "response",
        "student",
    }
)


@dataclass(frozen=True)
class QuestionMarker:
    """Detected numbered question marker in a template PDF."""

    question_number: int
    page_number: int
    rect: fitz.Rect


@dataclass(frozen=True)
class NormalizedCriterion:
    """Normalized rubric criterion used for deterministic local linting."""

    label_text: str
    label_tokens: frozenset[str]
    guidance_text: str
    guidance_tokens: frozenset[str]
    combined_text: str
    combined_tokens: frozenset[str]


@dataclass(frozen=True)
class RubricLintCandidate:
    """One local duplicate/overlap candidate pair."""

    code: str
    left_index: int
    right_index: int
    message: str


def unsupported_layout(
    *, message: str, details: dict[str, object] | None = None
) -> ScriptscoreError:
    """Build the shared unsupported-layout error."""

    return ScriptscoreError(
        code="unsupported_layout",
        message=message,
        category=ErrorCategory.VALIDATION,
        retryable=False,
        details=details or {},
        write_state=WriteState.NO_WRITE,
    )


def question_context_text(question_context: str) -> str:
    """Render escaped question-context text for prompt-local XML wrapping."""

    return escape(question_context, quote=False)


def instructor_profile_xml(request: ExamGenerateRubricRequest) -> str:
    """Render the shared instructor-profile XML projection."""

    profile = request.instructor_profile
    children = [
        xml_text(tag, value)
        for tag, value in [
            ("grading_strictness", profile.grading_strictness),
            ("syntax_leniency", profile.syntax_leniency),
            ("ocr_tolerance", profile.ocr_tolerance),
            ("partial_credit_style", profile.partial_credit_style),
            ("feedback_style", profile.feedback_style),
        ]
        if value is not None
    ]
    children.append(xml_text("additional_guidance", profile.additional_guidance or ""))
    return render_xml(xml_node("instructor_profile", *children))


def normalize_question_text(raw_text: str) -> str:
    """Normalize clean question text returned by the provider."""

    return _QUESTION_PREFIX_RE.sub("", raw_text.strip())


def detect_question_markers(page: fitz.Page) -> list[QuestionMarker]:
    """Detect supported line-leading question markers on one page."""

    candidates: list[QuestionMarker] = []
    page_dict = page.get_text("dict")
    left_margin_cutoff = page.rect.width * 0.25
    point_distribution_top: float | None = None
    for block in page_dict.get("blocks", []):
        for line in block.get("lines", []):
            line_text = "".join(span.get("text", "") for span in line.get("spans", []))
            bbox = line.get("bbox")
            if not bbox:
                continue
            rect = fitz.Rect(bbox)
            if rect.x0 > left_margin_cutoff:
                continue
            stripped_line = line_text.strip()
            if point_distribution_top is None and _POINT_DISTRIBUTION_HEADER_RE.match(
                stripped_line
            ):
                point_distribution_top = rect.y0
            if (
                point_distribution_top is not None
                and rect.y0 >= point_distribution_top
                and _match_point_summary_line(stripped_line) is not None
            ):
                continue
            match = _QUESTION_MARKER_RE.match(stripped_line)
            if not match:
                continue
            candidates.append(
                QuestionMarker(
                    question_number=int(match.group("number")),
                    page_number=page.number + 1,
                    rect=rect,
                )
            )
    return candidates


def collapse_markers(markers: list[QuestionMarker]) -> list[QuestionMarker]:
    """Collapse duplicate detected question numbers to first occurrence."""

    seen: set[int] = set()
    collapsed: list[QuestionMarker] = []
    for marker in sorted(markers, key=lambda item: (item.page_number, item.rect.y0)):
        if marker.question_number in seen:
            continue
        seen.add(marker.question_number)
        collapsed.append(marker)
    return collapsed


def extract_cover_page_max_points(page: fitz.Page) -> dict[int, int]:
    """Extract question max points from a cover-page score table when present."""

    lines = [line.strip() for line in page.get_text("text").splitlines() if line.strip()]
    return _extract_cover_table_max_points(lines) or _extract_cover_point_distribution_max_points(
        lines
    )


def _match_point_summary_line(line: str) -> tuple[int, int] | None:
    """Return one `(question_number, max_points)` match from a cover-page summary line."""

    stripped = line.strip()
    for pattern in _POINT_SUMMARY_PATTERNS:
        match = pattern.match(stripped)
        if match is None:
            continue
        return int(match.group("number")), int(match.group("points"))
    return None


def _extract_cover_table_max_points(lines: list[str]) -> dict[int, int]:
    """Extract question points from the legacy cover-page score table."""

    try:
        header_start = lines.index("Question")
        total_index = lines.index("Total", header_start + 1)
    except ValueError:
        return {}

    header_lines = lines[header_start : header_start + 3]
    if header_lines != ["Question", "Score", "Maximum"]:
        return {}

    value_lines = lines[header_start + 3 : total_index]
    if not value_lines:
        return {}
    if any(not _INTEGER_LINE_RE.fullmatch(line) for line in value_lines):
        return {}
    if len(value_lines) % 2 != 0:
        return {}

    mapping: dict[int, int] = {}
    for index in range(0, len(value_lines), 2):
        mapping[int(value_lines[index])] = int(value_lines[index + 1])
    return mapping


def _extract_cover_point_distribution_max_points(lines: list[str]) -> dict[int, int]:
    """Extract question points from a prose cover-page point-distribution section."""

    header_index = next(
        (index for index, line in enumerate(lines) if _POINT_DISTRIBUTION_HEADER_RE.match(line)),
        None,
    )
    if header_index is None:
        return {}

    mapping: dict[int, int] = {}
    started = False
    for line in lines[header_index + 1 :]:
        matched = _match_point_summary_line(line)
        if matched is not None:
            number, points = matched
            mapping[number] = points
            started = True
            continue
        if started:
            break
    return mapping


def extract_max_points(
    *,
    page: fitz.Page,
    question_rect: fitz.Rect,
    marker_rect: fitz.Rect,
    page_number: int,
    question_number: int,
    cover_page_max_points: dict[int, int] | None = None,
) -> int:
    """Extract max points from template text with the shared heuristics."""

    text = page.get_text("text", clip=question_rect)
    for pattern in (_POINTS_RE, _PAREN_POINTS_RE):
        match = pattern.search(text)
        if match:
            return int(match.group("points"))
    if page_number == 1:
        nearby_rect = fitz.Rect(
            page.rect.width * 0.6, max(0, marker_rect.y0 - 10), page.rect.width, marker_rect.y1 + 25
        )
        nearby_text = page.get_text("text", clip=nearby_rect)
        for pattern in (_POINTS_RE, _PAREN_POINTS_RE):
            match = pattern.search(nearby_text)
            if match:
                return int(match.group("points"))
    if cover_page_max_points and question_number in cover_page_max_points:
        return cover_page_max_points[question_number]
    raise unsupported_layout(
        message="Could not determine max points for one or more setup-derived questions.",
        details={"page_number": page_number},
    )


def normalize_rubric_text(value: str) -> tuple[str, frozenset[str]]:
    """Normalize rubric free text for deterministic similarity checks."""

    tokens = _RUBRIC_WORD_RE.findall(value.lower())
    normalized = " ".join(tokens)
    return normalized, frozenset(token for token in tokens if token not in _RUBRIC_STOPWORDS)


def normalize_rubric_criterion(criterion: RubricCriterion) -> NormalizedCriterion:
    """Build the normalized comparison form of one rubric criterion."""

    label_text, label_tokens = normalize_rubric_text(criterion.label)
    guidance_text, guidance_tokens = normalize_rubric_text(criterion.partial_credit_guidance)
    combined_text = f"{label_text} {guidance_text}".strip()
    return NormalizedCriterion(
        label_text=label_text,
        label_tokens=label_tokens,
        guidance_text=guidance_text,
        guidance_tokens=guidance_tokens,
        combined_text=combined_text,
        combined_tokens=label_tokens | guidance_tokens,
    )


def criteria_are_duplicates(left: NormalizedCriterion, right: NormalizedCriterion) -> bool:
    """Return whether two criteria are exact normalized duplicates."""

    return (
        left.label_text == right.label_text
        or left.guidance_text == right.guidance_text
        or left.combined_text == right.combined_text
    )


def criteria_potentially_overlap(left: NormalizedCriterion, right: NormalizedCriterion) -> bool:
    """Return whether two criteria appear to overlap materially."""

    shared_tokens = left.combined_tokens & right.combined_tokens
    if len(shared_tokens) < 3:
        return False
    min_token_count = min(len(left.combined_tokens), len(right.combined_tokens))
    if min_token_count == 0:
        return False
    overlap_ratio = len(shared_tokens) / min_token_count
    label_overlap = len(left.label_tokens & right.label_tokens) / max(
        1, min(len(left.label_tokens), len(right.label_tokens))
    )
    guidance_overlap = len(left.guidance_tokens & right.guidance_tokens) / max(
        1,
        min(len(left.guidance_tokens), len(right.guidance_tokens)),
    )
    text_similarity = SequenceMatcher(None, left.combined_text, right.combined_text).ratio()
    contains_other = (
        left.combined_text in right.combined_text or right.combined_text in left.combined_text
    )
    return overlap_ratio >= 0.8 and (
        contains_other
        or text_similarity >= 0.8
        or (label_overlap >= 0.7 and guidance_overlap >= 0.6)
    )


def _looks_like_attempt_credit_echo_row(normalized: NormalizedCriterion) -> bool:
    """Heuristic for LLM rows that duplicate a host-managed minimum/attempt-credit slice."""

    text = normalized.combined_text.lower()
    if "attempt" not in text or "credit" not in text:
        return False
    return "non-blank" in text or "non blank" in text or "nonblank" in text


def find_rubric_lint_candidates(
    criteria: list[RubricCriterion],
    *,
    host_prepends_minimum_credit_criterion: bool = False,
) -> list[RubricLintCandidate]:
    """Return local duplicate/overlap candidate pairs for a rubric."""

    candidates: list[RubricLintCandidate] = []
    normalized = [normalize_rubric_criterion(criterion) for criterion in criteria]
    displayed_index_offset = 1 if host_prepends_minimum_credit_criterion else 0

    for left_offset, left in enumerate(normalized):
        if (
            host_prepends_minimum_credit_criterion
            and left_offset == 0
            and _looks_like_attempt_credit_echo_row(left)
        ):
            continue
        left_index = left_offset + 1 + displayed_index_offset
        for right_offset in range(left_offset + 1, len(normalized)):
            right_index = right_offset + 1 + displayed_index_offset
            right = normalized[right_offset]
            if criteria_are_duplicates(left, right):
                candidates.append(
                    RubricLintCandidate(
                        code="rubric_duplicate_criterion",
                        left_index=left_index,
                        right_index=right_index,
                        message=(
                            f"Criteria {left_index} and {right_index} appear duplicative and may double-count "
                            "the same evidence."
                        ),
                    )
                )
                continue

            if criteria_potentially_overlap(left, right):
                candidates.append(
                    RubricLintCandidate(
                        code="rubric_potential_overlap",
                        left_index=left_index,
                        right_index=right_index,
                        message=f"Criteria {left_index} and {right_index} appear to overlap and may not be fully additive.",
                    )
                )

    return candidates


def criterion_pair_warning(
    *,
    code: str,
    message: str,
    scope: dict[str, object],
    left_index: int,
    right_index: int,
) -> WarningObject:
    """Build a pairwise rubric-lint warning."""

    return warning(
        code=code,
        message=message,
        scope={**scope, "criteria": [left_index, right_index]},
    )


def lint_rubric_criteria(
    *,
    criteria: list[RubricCriterion],
    scope: dict[str, object],
    host_prepends_minimum_credit_criterion: bool = False,
) -> list[WarningObject]:
    """Run narrow deterministic duplicate/overlap linting across criteria."""

    return [
        criterion_pair_warning(
            code=candidate.code,
            message=candidate.message,
            scope=scope,
            left_index=candidate.left_index,
            right_index=candidate.right_index,
        )
        for candidate in find_rubric_lint_candidates(
            criteria, host_prepends_minimum_credit_criterion=host_prepends_minimum_credit_criterion
        )
    ]
