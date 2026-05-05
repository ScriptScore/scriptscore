# SPDX-License-Identifier: AGPL-3.0-only
"""ArUco marker detection and stamping for template PDFs."""

from __future__ import annotations

from pathlib import Path

import fitz  # type: ignore[import-untyped]

from scriptscore.artifacts import (
    open_pdf_document,
    render_pdf_document,
    save_png,
    validate_pdf_path,
)
from scriptscore.commands.common import file_artifact, image_artifact, progress
from scriptscore.contracts import (
    ScansPdfDetectArucoRequest,
    ScansPdfStampArucoRequest,
)
from scriptscore.providers.aruco import (
    ARUCO_DICTIONARY_CAPACITY,
    ARUCO_DICTIONARY_NAME,
    detect_aruco_ids_from_png_bytes,
    generate_marker_png_bytes,
)
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec

_MARKERS_PER_PAGE = 4
_MARKER_SIZE_MM = 8.0
_MARKER_MARGIN_MM = 3.0
_POINTS_PER_MM = 72.0 / 25.4


def _mm_to_points(value: float) -> float:
    return value * _POINTS_PER_MM


def _render_pdf_page_png(page: fitz.Page, *, zoom: float) -> bytes:
    pixmap = page.get_pixmap(matrix=fitz.Matrix(zoom, zoom), alpha=False)
    return bytes(pixmap.tobytes("png"))


def handle_scans_pdf_detect_aruco(
    ctx: CommandContext, request: ScansPdfDetectArucoRequest
) -> CommandOutcome:
    """Render a PDF and return sanitized per-page ArUco marker counts and ids."""

    ctx.check_cancelled()
    validate_pdf_path(request.pdf_path, field_name="pdf_path")
    document = open_pdf_document(request.pdf_path)
    try:
        total_pages = max(1, int(document.page_count))
        ctx.emit(
            event="started",
            progress=progress(completed=0, total=total_pages),
            data={"target_count": total_pages, "total_stages": 1},
        )
        pages: list[dict[str, object]] = []
        total_marker_count = 0
        for page_index, page in enumerate(document, start=1):
            ctx.check_cancelled()
            png = _render_pdf_page_png(page, zoom=request.zoom)
            marker_ids = detect_aruco_ids_from_png_bytes(png)
            total_marker_count += len(marker_ids)
            pages.append(
                {
                    "page_number": page_index,
                    "marker_count": len(marker_ids),
                    "marker_ids": marker_ids,
                }
            )
            ctx.emit(
                event="page_completed",
                progress=progress(completed=page_index, total=total_pages),
                scope={"page_number": page_index},
                data={"page_number": page_index, "marker_count": len(marker_ids)},
            )
        ctx.emit(
            event="completed",
            progress=progress(completed=total_pages, total=total_pages),
            data={"total_marker_count": total_marker_count},
        )
        return CommandOutcome(
            data={
                "dictionary": ARUCO_DICTIONARY_NAME,
                "page_count": int(document.page_count),
                "total_marker_count": total_marker_count,
                "pages": pages,
            }
        )
    finally:
        document.close()


def _marker_rect(page: fitz.Page, corner_index: int) -> fitz.Rect:
    size = _mm_to_points(_MARKER_SIZE_MM)
    margin = _mm_to_points(_MARKER_MARGIN_MM)
    left = margin if corner_index in (0, 2) else float(page.rect.width) - margin - size
    top = margin if corner_index in (0, 1) else float(page.rect.height) - margin - size
    return fitz.Rect(left, top, left + size, top + size)


def _stamp_page(page: fitz.Page, *, page_index: int) -> None:
    for corner_index in range(_MARKERS_PER_PAGE):
        marker_id = ((page_index * _MARKERS_PER_PAGE) + corner_index) % ARUCO_DICTIONARY_CAPACITY
        page.insert_image(
            _marker_rect(page, corner_index),
            stream=generate_marker_png_bytes(marker_id),
            keep_proportion=True,
            overlay=True,
        )


def _save_rendered_pages(output_pdf_path: Path, output_dir: Path) -> list[Path]:
    page_paths: list[Path] = []
    for page in render_pdf_document(output_pdf_path):
        path = output_dir / f"stamped_template_page_{page.page_number:03d}.png"
        save_png(page.image, path)
        page_paths.append(path)
    return page_paths


def handle_scans_pdf_stamp_aruco(
    ctx: CommandContext, request: ScansPdfStampArucoRequest
) -> CommandOutcome:
    """Stamp four ArUco markers per PDF page and render stamped page PNG previews."""

    ctx.check_cancelled()
    validate_pdf_path(request.input_pdf_path, field_name="input_pdf_path")
    output_pdf_path = request.output_artifacts_dir / "stamped_template.pdf"

    document = open_pdf_document(request.input_pdf_path)
    try:
        page_count = int(document.page_count)
        page_total = max(1, page_count)
        request.output_artifacts_dir.mkdir(parents=True, exist_ok=True)
        ctx.emit(
            event="started",
            progress=progress(completed=0, total=page_total),
            data={"target_count": page_total, "total_stages": 1},
        )
        for page_index, page in enumerate(document):
            ctx.check_cancelled()
            _stamp_page(page, page_index=page_index)
            page_number = page_index + 1
            ctx.emit(
                event="page_completed",
                progress=progress(completed=page_number, total=page_total),
                scope={"page_number": page_number},
                data={"page_number": page_number, "marker_count": _MARKERS_PER_PAGE},
            )
        document.save(str(output_pdf_path), garbage=4, deflate=True)
    finally:
        document.close()

    rendered_paths = _save_rendered_pages(output_pdf_path, request.output_artifacts_dir)
    artifacts = [
        file_artifact(
            role="stamped_template_pdf",
            label="stamped_template.pdf",
            path=output_pdf_path,
            fmt="pdf",
        )
    ]
    artifacts.extend(
        image_artifact(
            role="rendered_stamped_template_page",
            label=path.name,
            path=path,
            scope={"page_number": index},
        )
        for index, path in enumerate(rendered_paths, start=1)
    )
    ctx.emit(
        event="completed",
        progress=progress(completed=max(1, page_count), total=max(1, page_count)),
        data={"page_count": page_count, "marker_count": page_count * _MARKERS_PER_PAGE},
    )
    return CommandOutcome(
        data={
            "status": "ok",
            "dictionary": ARUCO_DICTIONARY_NAME,
            "page_count": page_count,
            "marker_count": page_count * _MARKERS_PER_PAGE,
            "rendered_page_count": len(rendered_paths),
        },
        artifacts=artifacts,
        output_artifacts_dir=request.output_artifacts_dir,
        manifest_data={
            "written_artifact_count": len(artifacts),
            "page_count": page_count,
        },
    )


def scans_pdf_detect_aruco_spec() -> CommandSpec:
    return CommandSpec(
        name="scans.pdf-detect-aruco",
        request_model=ScansPdfDetectArucoRequest,
        handler=handle_scans_pdf_detect_aruco,
    )


def scans_pdf_stamp_aruco_spec() -> CommandSpec:
    return CommandSpec(
        name="scans.pdf-stamp-aruco",
        request_model=ScansPdfStampArucoRequest,
        handler=handle_scans_pdf_stamp_aruco,
    )
