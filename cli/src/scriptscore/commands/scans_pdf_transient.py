# SPDX-License-Identifier: AGPL-3.0-only
"""Transient PDF helper commands for desktop-owned scan intake flows."""

from __future__ import annotations

import base64
from typing import Any

import fitz  # type: ignore[import-untyped]

from scriptscore.artifacts import open_pdf_document, validate_pdf_path
from scriptscore.contracts import (
    ErrorCategory,
    PdfPointRect,
    ScansPdfClipRectsRequest,
    ScansPdfExtractTextRequest,
    ScansPdfMapTemplateRegionsRequest,
    ScansPdfRenderPageRequest,
    ScriptscoreError,
    WriteState,
)
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec


def _preview_zoom(page: fitz.Page, *, zoom: float, max_width_px: int | None) -> float:
    if max_width_px is None:
        return zoom
    page_width_pt = float(page.rect.width)
    if page_width_pt <= 0:
        return zoom
    return min(zoom, float(max_width_px) / page_width_pt)


def _render_page_snapshot(
    page: fitz.Page, *, zoom: float, max_width_px: int | None = None
) -> tuple[bytes, int, int]:
    render_zoom = _preview_zoom(page, zoom=zoom, max_width_px=max_width_px)
    pixmap = page.get_pixmap(matrix=fitz.Matrix(render_zoom, render_zoom), alpha=False)
    return pixmap.tobytes("png"), pixmap.width, pixmap.height


def _page_or_validation_error(document: fitz.Document, page_number: int) -> fitz.Page:
    if page_number < 1 or page_number > document.page_count:
        raise ScriptscoreError(
            code="page_number_out_of_range",
            message="page_number must reference an existing PDF page.",
            category=ErrorCategory.VALIDATION,
            retryable=True,
            details={"page_number": page_number, "page_count": document.page_count},
            write_state=WriteState.NO_WRITE,
        )
    return document.load_page(page_number - 1)


def _clip_rect(page: fitz.Page, rect: PdfPointRect, *, zoom: float) -> bytes:
    clip = fitz.Rect(
        rect.x_pt,
        rect.y_pt,
        rect.x_pt + rect.width_pt,
        rect.y_pt + rect.height_pt,
    )
    if clip.is_empty or clip.is_infinite:
        raise ScriptscoreError(
            code="pdf_clip_invalid",
            message="Clip rectangle was invalid.",
            category=ErrorCategory.VALIDATION,
            retryable=True,
            details={"page_number": rect.page_number},
            write_state=WriteState.NO_WRITE,
        )
    pixmap = page.get_pixmap(matrix=fitz.Matrix(zoom, zoom), alpha=False, clip=clip)
    return bytes(pixmap.tobytes("png"))


def handle_scans_pdf_render_page(
    ctx: CommandContext, request: ScansPdfRenderPageRequest
) -> CommandOutcome:
    """Render one PDF page to PNG bytes encoded as base64."""

    ctx.check_cancelled()
    validate_pdf_path(request.pdf_path, field_name="pdf_path")
    document = open_pdf_document(request.pdf_path)
    try:
        page = _page_or_validation_error(document, request.page_number)
        png, png_width_px, png_height_px = _render_page_snapshot(
            page,
            zoom=request.zoom,
            max_width_px=request.max_width_px,
        )
        return CommandOutcome(
            data={
                "page_number": request.page_number,
                "page_count": int(document.page_count),
                "page_width_pt": float(page.rect.width),
                "page_height_pt": float(page.rect.height),
                "png_width_px": png_width_px,
                "png_height_px": png_height_px,
                "png_base64": base64.b64encode(png).decode("ascii"),
            }
        )
    finally:
        document.close()


def handle_scans_pdf_clip_rects(
    ctx: CommandContext, request: ScansPdfClipRectsRequest
) -> CommandOutcome:
    """Clip one or more PDF-space rects to base64-encoded PNGs."""

    ctx.check_cancelled()
    validate_pdf_path(request.pdf_path, field_name="pdf_path")
    document = open_pdf_document(request.pdf_path)
    try:
        clips: list[dict[str, Any]] = []
        for rect in request.rects:
            ctx.check_cancelled()
            page = _page_or_validation_error(document, rect.page_number)
            png = _clip_rect(page, rect, zoom=request.zoom)
            clips.append(
                {
                    "page_number": rect.page_number,
                    "png_base64": base64.b64encode(png).decode("ascii"),
                }
            )
        return CommandOutcome(data={"clips": clips})
    finally:
        document.close()


def handle_scans_pdf_extract_text(
    ctx: CommandContext, request: ScansPdfExtractTextRequest
) -> CommandOutcome:
    """Extract transient text from a PDF-space rectangle."""

    ctx.check_cancelled()
    validate_pdf_path(request.pdf_path, field_name="pdf_path")
    document = open_pdf_document(request.pdf_path)
    try:
        page = _page_or_validation_error(document, request.page_number)
        clip = fitz.Rect(
            request.x_pt,
            request.y_pt,
            request.x_pt + request.width_pt,
            request.y_pt + request.height_pt,
        )
        text = page.get_text("text", clip=clip).strip()
        return CommandOutcome(data={"text": text})
    finally:
        document.close()


def handle_scans_pdf_map_template_regions(
    ctx: CommandContext, request: ScansPdfMapTemplateRegionsRequest
) -> CommandOutcome:
    """Map template rendered-page regions into PDF-point rectangles."""

    ctx.check_cancelled()
    validate_pdf_path(request.pdf_path, field_name="pdf_path")
    document = open_pdf_document(request.pdf_path)
    try:
        rects: list[dict[str, Any]] = []
        for page_index in range(document.page_count):
            page = document.load_page(page_index)
            page_number = page_index + 1
            page_regions = [
                region for region in request.regions if region.page_number == page_number
            ]
            if not page_regions:
                continue
            raster_size = request.raster_sizes_by_page.get(page_number)
            if raster_size is None:
                raise ScriptscoreError(
                    code="missing_template_raster_size",
                    message=(
                        "raster_sizes_by_page must include page_number "
                        f"{page_number} for each region page."
                    ),
                    category=ErrorCategory.VALIDATION,
                    retryable=True,
                    details={"page_number": page_number},
                    write_state=WriteState.NO_WRITE,
                )
            sx = float(page.rect.width) / float(raster_size.width_px)
            sy = float(page.rect.height) / float(raster_size.height_px)
            for region in page_regions:
                rects.append(
                    {
                        "page_number": page_number,
                        "x_pt": float(region.x) * sx,
                        "y_pt": float(region.y) * sy,
                        "width_pt": float(region.width) * sx,
                        "height_pt": float(region.height) * sy,
                    }
                )
        return CommandOutcome(data={"rects": rects})
    finally:
        document.close()


def scans_pdf_render_page_spec() -> CommandSpec:
    return CommandSpec(
        name="scans.pdf-render-page",
        request_model=ScansPdfRenderPageRequest,
        handler=handle_scans_pdf_render_page,
    )


def scans_pdf_clip_rects_spec() -> CommandSpec:
    return CommandSpec(
        name="scans.pdf-clip-rects",
        request_model=ScansPdfClipRectsRequest,
        handler=handle_scans_pdf_clip_rects,
    )


def scans_pdf_extract_text_spec() -> CommandSpec:
    return CommandSpec(
        name="scans.pdf-extract-text",
        request_model=ScansPdfExtractTextRequest,
        handler=handle_scans_pdf_extract_text,
    )


def scans_pdf_map_template_regions_spec() -> CommandSpec:
    return CommandSpec(
        name="scans.pdf-map-template-regions",
        request_model=ScansPdfMapTemplateRegionsRequest,
        handler=handle_scans_pdf_map_template_regions,
    )
