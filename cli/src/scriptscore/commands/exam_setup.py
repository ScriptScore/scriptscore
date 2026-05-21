# SPDX-License-Identifier: AGPL-3.0-only
"""Implementation of `exam setup`."""

from __future__ import annotations

from pathlib import Path

import fitz  # type: ignore[import-untyped]
from PIL import Image

from scriptscore.artifacts import (
    crop_image,
    normalize_page_width,
    open_pdf_document,
    render_pdf_page,
    save_png,
    validate_pdf_path,
)
from scriptscore.commands.common import image_artifact, inventory_manifest_data, progress
from scriptscore.commands.exam_shared import (
    collapse_markers,
    detect_question_markers,
    extract_cover_page_max_points,
    extract_max_points,
    unsupported_layout,
)
from scriptscore.contracts import ArtifactReference, ExamSetupRequest, Region, SetupQuestion
from scriptscore.paths import join_under_root
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec


def _pdf_rect_to_image_region(*, page: fitz.Page, rect: fitz.Rect, image: Image.Image) -> Region:
    sx = image.width / float(page.rect.width)
    sy = image.height / float(page.rect.height)
    return Region(
        x=max(0, round(rect.x0 * sx)),
        y=max(0, round(rect.y0 * sy)),
        width=max(1, round((rect.x1 - rect.x0) * sx)),
        height=max(1, round((rect.y1 - rect.y0) * sy)),
        units="rendered_page_pixels",
    )


def _question_region_rect(
    *,
    page: fitz.Page,
    marker_rect: fitz.Rect,
    next_marker_rect: fitz.Rect | None,
) -> fitz.Rect:
    top_padding = marker_rect.height
    bottom_padding = 0 if next_marker_rect is None else next_marker_rect.height
    top = max(0, marker_rect.y0 - top_padding)
    bottom = (
        page.rect.height
        if next_marker_rect is None
        else max(top + 1, next_marker_rect.y0 - bottom_padding)
    )
    return page.rect.__class__(0, top, page.rect.width, bottom)


def _baseline_text_rect(
    *,
    page: fitz.Page,
    marker_rect: fitz.Rect,
    next_marker_rect: fitz.Rect | None,
) -> fitz.Rect:
    top = max(0, marker_rect.y0)
    bottom = page.rect.height if next_marker_rect is None else max(top + 1, next_marker_rect.y0)
    return page.rect.__class__(0, top, page.rect.width, bottom)


def handle_exam_setup(ctx: CommandContext, request: ExamSetupRequest) -> CommandOutcome:
    """Bootstrap template artifacts and baseline question definitions from the template PDF."""

    validate_pdf_path(request.template_pdf_path, field_name="template_pdf_path")
    document = open_pdf_document(request.template_pdf_path)
    try:
        page_total = document.page_count
        ctx.emit(
            event="started",
            progress=progress(completed=0, total=page_total),
            data={"stage": "render_pages", "total_stages": 2},
        )
        ctx.emit(
            event="stage_started",
            data={"stage_number": 1, "stage": "render_pages", "question_count": page_total},
        )

        rendered_pages: list[tuple[Path, Image.Image]] = []
        page_images: dict[int, Image.Image] = {}
        markers = []
        artifacts: list[ArtifactReference] = []

        for index, page in enumerate(document, start=1):
            ctx.check_cancelled()
            scope: dict[str, object] = {"page_number": index}
            ctx.emit(
                event="item_started",
                progress=progress(completed=index - 1, total=page_total),
                scope=scope,
            )
            image = normalize_page_width(render_pdf_page(page))
            output_path = join_under_root(
                request.output_artifacts_dir, "rendered_pages", f"page_{index:03d}.png"
            )
            rendered_pages.append((output_path, image))
            page_images[index] = image
            markers.extend(detect_question_markers(page))
            artifacts.append(
                image_artifact(
                    role="rendered_template_page",
                    label=output_path.name,
                    path=output_path,
                    scope=scope,
                )
            )
            ctx.emit(
                event="item_completed",
                progress=progress(completed=index, total=page_total),
                scope=scope,
            )

        collapsed = collapse_markers(markers)
        if not collapsed:
            raise unsupported_layout(
                message="No supported line-leading question labels were detected in the template PDF."
            )
        actual_numbers: list[int] = []
        for marker in collapsed:
            if not actual_numbers or actual_numbers[-1] != marker.question_number:
                actual_numbers.append(marker.question_number)
        expected_numbers = list(range(1, len(actual_numbers) + 1))
        if actual_numbers != expected_numbers:
            raise unsupported_layout(
                message="Template question labels must form a contiguous sequence starting at 1.",
                details={"detected_numbers": actual_numbers},
            )
        cover_page_max_points = extract_cover_page_max_points(document.load_page(0))

        question_total = len(collapsed)
        ctx.emit(
            event="stage_started",
            data={"stage_number": 2, "stage": "question_crop", "question_count": question_total},
        )
        questions: list[SetupQuestion] = []
        question_crops: list[tuple[Path, Image.Image]] = []
        for index, marker in enumerate(collapsed, start=1):
            ctx.check_cancelled()
            question_id = f"q{marker.question_label}"
            question_scope: dict[str, object] = {"question_id": question_id}
            ctx.emit(
                event="item_started",
                progress=progress(completed=index - 1, total=question_total),
                scope=question_scope,
            )
            page = document.load_page(marker.page_number - 1)
            next_marker = next(
                (
                    candidate
                    for candidate in collapsed[index:]
                    if candidate.page_number == marker.page_number
                ),
                None,
            )
            region_rect = _question_region_rect(
                page=page,
                marker_rect=marker.rect,
                next_marker_rect=None if next_marker is None else next_marker.rect,
            )
            baseline_rect = _baseline_text_rect(
                page=page,
                marker_rect=marker.rect,
                next_marker_rect=None if next_marker is None else next_marker.rect,
            )
            region = _pdf_rect_to_image_region(
                page=page,
                rect=region_rect,
                image=page_images[marker.page_number],
            )
            max_points = extract_max_points(
                page=page,
                question_rect=baseline_rect,
                marker_rect=marker.rect,
                page_number=marker.page_number,
                question_number=marker.question_number,
                cover_page_max_points=cover_page_max_points,
            )
            baseline_pdf_text = page.get_text("text", clip=baseline_rect).strip()
            crop_output_path = join_under_root(
                request.output_artifacts_dir, "template_questions", f"{question_id}.png"
            )
            template_question_png = crop_image(page_images[marker.page_number], region)
            question_crops.append((crop_output_path, template_question_png))
            question = SetupQuestion(
                question_id=question_id,
                question_number=marker.question_number,
                question_label=marker.question_label,
                page_number=marker.page_number,
                baseline_pdf_text=baseline_pdf_text,
                max_points=max_points,
                region=region,
                template_question_png_path=crop_output_path,
            )
            questions.append(question)
            artifacts.append(
                image_artifact(
                    role="template_question",
                    label=crop_output_path.name,
                    path=crop_output_path,
                    scope=question_scope,
                )
            )
            ctx.emit(
                event="item_completed",
                progress=progress(completed=index, total=question_total),
                scope=question_scope,
            )

        for path, image in rendered_pages:
            save_png(image, path)
        for path, image in question_crops:
            save_png(image, path)

        template_pages = [
            {
                "page_type": "template",
                "page_number": page_number,
                "image_path": str(path),
            }
            for page_number, (path, _image) in enumerate(rendered_pages, start=1)
        ]
        data = {
            "template_pages": template_pages,
            "questions": [
                question.model_dump(mode="json", exclude_none=True) for question in questions
            ],
            "output_metadata_path": str(
                (request.output_artifacts_dir / "output_metadata.json").resolve()
            ),
        }
        ctx.emit(
            event="completed",
            progress=progress(completed=question_total, total=question_total),
            data={"question_count": question_total, "page_count": page_total},
        )
        return CommandOutcome(
            data=data,
            artifacts=artifacts,
            output_artifacts_dir=request.output_artifacts_dir,
            manifest_data=inventory_manifest_data(
                result_row_count=question_total,
                written_artifact_count=len(artifacts),
                failed_count=0,
            ),
        )
    finally:
        document.close()


def exam_setup_spec() -> CommandSpec:
    return CommandSpec(name="exam.setup", request_model=ExamSetupRequest, handler=handle_exam_setup)
