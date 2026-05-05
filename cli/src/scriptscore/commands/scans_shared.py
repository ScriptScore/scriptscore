# SPDX-License-Identifier: AGPL-3.0-only
"""Shared helpers for scan command implementations."""

from __future__ import annotations

from collections import defaultdict
from collections.abc import Sequence
from pathlib import Path
from typing import Literal

from pydantic import BaseModel, ConfigDict

from scriptscore.contracts import (
    CanonicalizeTarget,
    ErrorCategory,
    Page,
    ParseQuestionContext,
    QuestionCropTarget,
    ScriptscoreError,
    TransformTarget,
    WriteState,
)
from scriptscore.paths import join_under_root
from scriptscore.prompts import render_xml, xml_node, xml_text


class HandwritingVerifyPayload(BaseModel):
    """Structured handwriting-prescreen payload."""

    model_config = ConfigDict(extra="forbid")

    has_handwriting: bool
    confidence: Literal["high", "medium", "low"]
    status: Literal["complete", "error"]


def ensure_paths_exist(
    paths: list[Path], *, command: str, code: str = "input_artifact_not_found"
) -> None:
    """Raise a shared not-found error if any input artifact path is missing."""

    missing = [path for path in paths if not path.exists()]
    if not missing:
        return
    raise ScriptscoreError(
        code=code,
        message=f"One or more input artifacts for {command} were not found.",
        category=ErrorCategory.NOT_FOUND,
        retryable=True,
        details={"missing_paths": [str(path) for path in missing]},
        write_state=WriteState.NO_WRITE,
    )


def transform_output_path(output_dir: Path, target: TransformTarget) -> Path:
    """Build the output path for one transformed student page."""

    assert target.page.student_ref is not None
    return join_under_root(
        output_dir,
        target.page.student_ref,
        f"page_{target.page.page_number:03d}.png",
    )


def transform_output_page(target: TransformTarget, output_path: Path) -> Page:
    """Build the shared output page model for one transformed page."""

    return Page(
        page_type=target.page.page_type,
        page_number=target.page.page_number,
        image_path=output_path,
        source_pdf_path=target.page.source_pdf_path,
    )


def canonicalize_output_path(output_dir: Path, target: CanonicalizeTarget) -> Path:
    """Build the output path for one canonicalized student page."""

    assert target.page.student_ref is not None
    return join_under_root(
        output_dir,
        target.page.student_ref,
        f"page_{target.page.page_number:03d}.png",
    )


def canonicalize_output_page(target: CanonicalizeTarget, output_path: Path) -> Page:
    """Build the shared output page model for one canonicalized page."""

    return Page(
        page_type=target.page.page_type,
        page_number=target.page.page_number,
        image_path=output_path,
        source_pdf_path=target.page.source_pdf_path,
    )


def matched_crop_jobs(
    pages: list[Page],
    targets: list[QuestionCropTarget],
) -> list[tuple[Page, QuestionCropTarget]]:
    """Match crop targets by page number within each student page set."""

    pages_by_student: dict[str, dict[int, Page]] = defaultdict(dict)
    for page in pages:
        assert page.student_ref is not None
        pages_by_student[page.student_ref][page.page_number] = page

    jobs: list[tuple[Page, QuestionCropTarget]] = []
    for student_ref in sorted(pages_by_student):
        page_map = pages_by_student[student_ref]
        for target in targets:
            if target.student_ref is not None and target.student_ref != student_ref:
                continue
            matched_page = page_map.get(target.page_number)
            if matched_page is not None:
                jobs.append((matched_page, target))
    return jobs


def crop_output_paths(
    output_dir: Path,
    jobs: Sequence[tuple[Page, QuestionCropTarget]],
) -> list[Path]:
    """Build unique crop output paths for each matched crop row."""

    counts: dict[tuple[str, str], int] = defaultdict(int)
    for page, target in jobs:
        assert page.student_ref is not None
        counts[(page.student_ref, target.question_id)] += 1

    seen: dict[tuple[str, str], int] = defaultdict(int)
    paths: list[Path] = []
    for page, target in jobs:
        assert page.student_ref is not None
        key = (page.student_ref, target.question_id)
        seen[key] += 1
        if counts[key] == 1:
            filename = f"{target.question_id}.png"
        else:
            filename = f"{target.question_id}__p{page.page_number:03d}__r{seen[key]:02d}.png"
        paths.append(join_under_root(output_dir, page.student_ref, filename))
    return paths


def render_parse_question_context_xml(context: ParseQuestionContext) -> str:
    """Render escaped parse-question context XML for the OCR prompt."""

    return render_xml(
        xml_node(
            "parse_question_context",
            xml_text("question_number", context.question_number),
            xml_text("question_text_clean", context.question_text_clean),
        )
    )
