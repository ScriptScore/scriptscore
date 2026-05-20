# SPDX-License-Identifier: AGPL-3.0-only
"""Implementation of `scans.pdf-create-redacted` (burn redaction rectangles into a PDF copy).

Success `data` intentionally excludes input/output paths so persisted job traces do not retain
raw submission locations or filenames. Progress scopes may include `student_ref` only.
"""

from __future__ import annotations

import math
import os
import shutil
import tempfile
from contextlib import suppress
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import fitz  # type: ignore[import-untyped]

from scriptscore.artifacts import validate_pdf_path
from scriptscore.commands.common import progress
from scriptscore.contracts import (
    ErrorCategory,
    ScansPdfCreateRedactedRequest,
    ScriptscoreError,
    WriteState,
)
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec

_BLACK_THRESHOLD = 16
_MIN_REGION_BLACK_COVERAGE = 0.95


@dataclass(frozen=True)
class _RenderedPage:
    samples: bytes
    width: int
    height: int


@dataclass(frozen=True)
class _PixelRect:
    left: int
    top: int
    right: int
    bottom: int

    @property
    def area(self) -> int:
        return max(0, self.right - self.left) * max(0, self.bottom - self.top)


def handle_scans_pdf_create_redacted(
    ctx: CommandContext, request: ScansPdfCreateRedactedRequest
) -> CommandOutcome:
    """Apply black redaction annotations from template pixel regions and save to output path."""

    ctx.check_cancelled()
    validate_pdf_path(request.input_pdf_path, field_name="input_pdf_path")

    scope_base: dict[str, object] = {}
    if request.student_ref:
        scope_base["student_ref"] = request.student_ref

    input_resolved = request.input_pdf_path.resolve()
    output_resolved = request.output_pdf_path.resolve()
    if request.output_pdf_path.parent:
        request.output_pdf_path.parent.mkdir(parents=True, exist_ok=True)

    actual_page_count = 0
    output_page_count = 0
    page_total = 1
    document = None
    use_incremental = False
    selected_page_numbers: list[int] | None = None
    source_page_to_output_page: dict[int, int] = {}
    temp_output_path: Path | None = None
    original_renders_by_page: dict[int, _RenderedPage] = {}
    regions_by_page: dict[int, list[Any]] = {}
    raster_sizes_by_page: dict[int, Any] = {}
    try:
        try:
            if request.page_order:
                document = fitz.open(str(input_resolved))
                use_incremental = False
            elif input_resolved != output_resolved:
                shutil.copyfile(input_resolved, output_resolved)
                document = fitz.open(str(output_resolved))
                use_incremental = bool(document.can_save_incrementally())
                if not use_incremental:
                    document.close()
                    document = fitz.open(str(input_resolved))
            else:
                document = fitz.open(str(input_resolved))
                use_incremental = bool(document.can_save_incrementally())

            actual_page_count = int(document.page_count)
            page_total = max(1, actual_page_count)
            selected_page_numbers = _validated_page_order(request.page_order, actual_page_count)
            if selected_page_numbers is not None:
                use_incremental = False
                source_page_to_output_page = {
                    source_page_number: output_index
                    for output_index, source_page_number in enumerate(
                        selected_page_numbers, start=1
                    )
                }
                if input_resolved == output_resolved:
                    temp_output_path = _temporary_pdf_output_path(output_resolved)

            ctx.emit(
                event="started",
                progress=progress(completed=0, total=page_total),
                data={"target_count": page_total, "total_stages": 1},
            )

            for page in document:
                ctx.check_cancelled()
                pnum = page.number + 1
                page_regions = [r for r in request.regions if r.page_number == pnum]
                if page_regions:
                    rw_rh = request.raster_sizes_by_page.get(pnum)
                    if rw_rh is None:
                        raise ScriptscoreError(
                            code="missing_template_raster_size",
                            message=(
                                "raster_sizes_by_page must include page_number "
                                f"{pnum} for each region page."
                            ),
                            category=ErrorCategory.VALIDATION,
                            retryable=True,
                            details={"page_number": pnum},
                            write_state=WriteState.NO_WRITE,
                        )
                    img_w = float(rw_rh.width_px)
                    img_h = float(rw_rh.height_px)
                    if img_w <= 0 or img_h <= 0:
                        raise ScriptscoreError(
                            code="invalid_template_raster_size",
                            message="Template raster width_px and height_px must be positive.",
                            category=ErrorCategory.VALIDATION,
                            retryable=True,
                            details={"page_number": pnum, "width_px": img_w, "height_px": img_h},
                            write_state=WriteState.NO_WRITE,
                        )
                    original_renders_by_page[pnum] = _render_page_for_integrity(page, rw_rh)
                    regions_by_page[pnum] = page_regions
                    raster_sizes_by_page[pnum] = rw_rh
                    for region in page_regions:
                        rect = _annotation_rect_from_visual_region(page, region, rw_rh)
                        page.add_redact_annot(rect, fill=(0, 0, 0))
                    page.apply_redactions()
                    # Incremental save only after a real mutation: MuPDF can corrupt the file if
                    # saveIncr runs after no-op pages (see scans_pdf_create_redacted tests).
                    if use_incremental:
                        document.saveIncr()

                page_scope = {**scope_base, "page_number": pnum}
                ctx.emit(
                    event="page_completed",
                    progress=progress(completed=pnum, total=page_total),
                    scope=page_scope,
                    data={"page_number": pnum},
                )

            if selected_page_numbers is not None:
                document.select([page_number - 1 for page_number in selected_page_numbers])

            output_page_count = int(document.page_count)

            if not use_incremental:
                document.save(
                    str(temp_output_path or output_resolved),
                    garbage=4,
                    deflate=True,
                )
        finally:
            if document is not None:
                document.close()
        if temp_output_path is not None:
            os.replace(temp_output_path, output_resolved)
            temp_output_path = None
    finally:
        if temp_output_path is not None:
            with suppress(OSError):
                temp_output_path.unlink(missing_ok=True)

    if regions_by_page:
        _verify_redaction_integrity(
            output_resolved,
            original_renders_by_page=original_renders_by_page,
            regions_by_page=regions_by_page,
            raster_sizes_by_page=raster_sizes_by_page,
            source_page_to_output_page=source_page_to_output_page,
        )

    ctx.emit(
        event="item_completed",
        progress=progress(completed=page_total, total=page_total),
        scope=dict(scope_base),
        data={},
    )
    ctx.emit(
        event="completed",
        progress=progress(completed=page_total, total=page_total),
        data={},
    )

    return CommandOutcome(
        data={"status": "ok", "page_count": output_page_count or actual_page_count},
        artifacts=[],
        manifest_data={},
    )


def _validated_page_order(page_order: list[int] | None, page_count: int) -> list[int] | None:
    if not page_order:
        return None
    expected = set(range(1, page_count + 1))
    selected = [int(page_number) for page_number in page_order]
    if len(set(selected)) != len(selected) or any(
        page_number not in expected for page_number in selected
    ):
        raise ScriptscoreError(
            code="invalid_page_order",
            message="page_order must contain valid source PDF page numbers.",
            category=ErrorCategory.VALIDATION,
            retryable=True,
            details={"page_count": page_count},
            write_state=WriteState.NO_WRITE,
        )
    return selected


def _temporary_pdf_output_path(output_path: Path) -> Path:
    fd, temp_name = tempfile.mkstemp(
        prefix=f".{output_path.name}.",
        suffix=".tmp.pdf",
        dir=output_path.parent,
    )
    os.close(fd)
    return Path(temp_name)


def _visual_rect_from_region(
    page: fitz.Page,
    region: Any,
    raster_size: Any,
) -> fitz.Rect:
    img_w = float(raster_size.width_px)
    img_h = float(raster_size.height_px)
    sx = float(page.rect.width) / img_w
    sy = float(page.rect.height) / img_h
    return fitz.Rect(
        float(region.x) * sx,
        float(region.y) * sy,
        float(region.x + region.width) * sx,
        float(region.y + region.height) * sy,
    )


def _annotation_rect_from_visual_region(
    page: fitz.Page,
    region: Any,
    raster_size: Any,
) -> fitz.Rect:
    visual_rect = _visual_rect_from_region(page, region, raster_size)
    annotation_rect = visual_rect * page.derotation_matrix
    return fitz.Rect(
        min(annotation_rect.x0, annotation_rect.x1),
        min(annotation_rect.y0, annotation_rect.y1),
        max(annotation_rect.x0, annotation_rect.x1),
        max(annotation_rect.y0, annotation_rect.y1),
    )


def _render_page_for_integrity(page: fitz.Page, raster_size: Any) -> _RenderedPage:
    matrix = fitz.Matrix(
        float(raster_size.width_px) / float(page.rect.width),
        float(raster_size.height_px) / float(page.rect.height),
    )
    pixmap = page.get_pixmap(matrix=matrix, colorspace=fitz.csRGB, alpha=False)
    return _RenderedPage(samples=bytes(pixmap.samples), width=pixmap.width, height=pixmap.height)


def _pixel_rect_from_region(
    region: Any,
    raster_size: Any,
    rendered: _RenderedPage,
) -> _PixelRect:
    sx = rendered.width / float(raster_size.width_px)
    sy = rendered.height / float(raster_size.height_px)
    return _PixelRect(
        left=max(0, min(rendered.width, math.floor(float(region.x) * sx))),
        top=max(0, min(rendered.height, math.floor(float(region.y) * sy))),
        right=max(0, min(rendered.width, math.ceil(float(region.x + region.width) * sx))),
        bottom=max(0, min(rendered.height, math.ceil(float(region.y + region.height) * sy))),
    )


def _is_black(samples: bytes, index: int) -> bool:
    offset = index * 3
    return (
        samples[offset] <= _BLACK_THRESHOLD
        and samples[offset + 1] <= _BLACK_THRESHOLD
        and samples[offset + 2] <= _BLACK_THRESHOLD
    )


def _is_new_black(before: bytes, after: bytes, index: int) -> bool:
    return _is_black(after, index) and not _is_black(before, index)


def _region_black_coverage(rendered: _RenderedPage, rect: _PixelRect) -> float:
    if rect.area <= 0:
        return 0.0
    black = 0
    for y in range(rect.top, rect.bottom):
        row = y * rendered.width
        for x in range(rect.left, rect.right):
            if _is_black(rendered.samples, row + x):
                black += 1
    return black / rect.area


def _expected_mask(rects: list[_PixelRect], *, width: int, height: int) -> bytearray:
    mask = bytearray(width * height)
    for rect in rects:
        for y in range(rect.top, rect.bottom):
            start = (y * width) + rect.left
            end = (y * width) + rect.right
            mask[start:end] = b"\x01" * (end - start)
    return mask


def _largest_unexpected_new_black_component(
    before: _RenderedPage,
    after: _RenderedPage,
    expected: bytearray,
) -> int:
    visited = bytearray(after.width * after.height)
    largest = 0
    for start in range(after.width * after.height):
        if (
            visited[start]
            or expected[start]
            or not _is_new_black(before.samples, after.samples, start)
        ):
            continue
        visited[start] = 1
        queue = [start]
        size = 0
        while queue:
            current = queue.pop()
            size += 1
            x = current % after.width
            y = current // after.width
            for neighbor in (
                current - 1 if x > 0 else -1,
                current + 1 if x + 1 < after.width else -1,
                current - after.width if y > 0 else -1,
                current + after.width if y + 1 < after.height else -1,
            ):
                if (
                    neighbor >= 0
                    and not visited[neighbor]
                    and not expected[neighbor]
                    and _is_new_black(before.samples, after.samples, neighbor)
                ):
                    visited[neighbor] = 1
                    queue.append(neighbor)
        largest = max(largest, size)
    return largest


def _verify_redaction_integrity(
    output_pdf_path: Any,
    *,
    original_renders_by_page: dict[int, _RenderedPage],
    regions_by_page: dict[int, list[Any]],
    raster_sizes_by_page: dict[int, Any],
    source_page_to_output_page: dict[int, int] | None = None,
) -> None:
    document = fitz.open(str(output_pdf_path))
    try:
        for page_number, page_regions in regions_by_page.items():
            if source_page_to_output_page and page_number not in source_page_to_output_page:
                continue
            output_page_number = (
                source_page_to_output_page.get(page_number, page_number)
                if source_page_to_output_page
                else page_number
            )
            page = document.load_page(output_page_number - 1)
            raster_size = raster_sizes_by_page[page_number]
            original = original_renders_by_page[page_number]
            redacted = _render_page_for_integrity(page, raster_size)
            if original.width != redacted.width or original.height != redacted.height:
                raise _redaction_integrity_error(
                    page_number=page_number,
                    coverage=0.0,
                    largest_unexpected_component=0,
                    expected_area=0,
                    reason="render_size_changed",
                )
            rects = [
                _pixel_rect_from_region(region, raster_size, redacted) for region in page_regions
            ]
            coverages = [_region_black_coverage(redacted, rect) for rect in rects]
            min_coverage = min(coverages, default=0.0)
            expected_area = sum(rect.area for rect in rects)
            expected = _expected_mask(rects, width=redacted.width, height=redacted.height)
            largest_unexpected = _largest_unexpected_new_black_component(
                original,
                redacted,
                expected,
            )
            allowed_unexpected = max(100, round(expected_area * 0.05))
            if min_coverage < _MIN_REGION_BLACK_COVERAGE or largest_unexpected > allowed_unexpected:
                raise _redaction_integrity_error(
                    page_number=page_number,
                    coverage=min_coverage,
                    largest_unexpected_component=largest_unexpected,
                    expected_area=expected_area,
                    reason="coverage_or_unexpected_component",
                )
    finally:
        document.close()


def _redaction_integrity_error(
    *,
    page_number: int,
    coverage: float,
    largest_unexpected_component: int,
    expected_area: int,
    reason: str,
) -> ScriptscoreError:
    return ScriptscoreError(
        code="redaction_integrity_failed",
        message="Redaction geometry verification failed after burning the PDF.",
        category=ErrorCategory.EXECUTION,
        retryable=True,
        details={
            "page_number": page_number,
            "min_black_coverage": round(coverage, 4),
            "largest_unexpected_new_black_component_px": largest_unexpected_component,
            "expected_redaction_area_px": expected_area,
            "reason": reason,
        },
        write_state=WriteState.WRITTEN_BEFORE_FAILURE,
    )


def scans_pdf_create_redacted_spec() -> CommandSpec:
    return CommandSpec(
        name="scans.pdf-create-redacted",
        request_model=ScansPdfCreateRedactedRequest,
        handler=handle_scans_pdf_create_redacted,
    )
