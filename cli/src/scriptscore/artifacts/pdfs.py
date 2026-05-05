# SPDX-License-Identifier: AGPL-3.0-only
"""Shared PDF validation and rendering helpers."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import fitz  # type: ignore[import-untyped]
from PIL import Image

from scriptscore.contracts import ErrorCategory, Region, ScriptscoreError, WriteState

PDF_RENDER_DPI = 120
PDF_RENDER_SCALE = PDF_RENDER_DPI / 72.0


@dataclass(frozen=True)
class RenderedPdfPage:
    """One rendered PDF page snapshot."""

    page_number: int
    image: Image.Image
    extracted_text: str


def validate_pdf_path(path: Path, *, field_name: str) -> Path:
    """Validate a PDF input path before dispatch."""

    if path.suffix.lower() != ".pdf":
        raise ScriptscoreError(
            code="invalid_pdf_extension",
            message=f"{field_name} must point to a .pdf file.",
            category=ErrorCategory.VALIDATION,
            retryable=True,
            write_state=WriteState.NO_WRITE,
        )
    if not path.exists():
        raise ScriptscoreError(
            code="pdf_not_found",
            message=f"{field_name} was not found.",
            category=ErrorCategory.NOT_FOUND,
            retryable=True,
            details={"path": str(path)},
            write_state=WriteState.NO_WRITE,
        )
    if not path.is_file():
        raise ScriptscoreError(
            code="pdf_not_file",
            message=f"{field_name} must point to a file.",
            category=ErrorCategory.VALIDATION,
            retryable=True,
            write_state=WriteState.NO_WRITE,
        )
    with path.open("rb") as handle:
        magic = handle.read(5)
    if magic != b"%PDF-":
        raise ScriptscoreError(
            code="invalid_pdf_magic",
            message=f"{field_name} does not have a valid PDF header.",
            category=ErrorCategory.VALIDATION,
            retryable=True,
            details={"path": str(path)},
            write_state=WriteState.NO_WRITE,
        )
    return path


def open_pdf_document(path: Path) -> fitz.Document:
    """Open a PDF document using the shared backend."""

    try:
        return fitz.open(path)
    except Exception as exc:
        raise ScriptscoreError(
            code="pdf_open_failed",
            message=str(exc) or "Failed to open PDF document.",
            category=ErrorCategory.EXECUTION,
            retryable=True,
            details={"path": str(path)},
        ) from exc


def render_pdf_page(page: fitz.Page) -> Image.Image:
    """Render one PDF page to a deterministic RGB PIL image."""

    pixmap = page.get_pixmap(dpi=PDF_RENDER_DPI, alpha=False)
    mode = "RGB" if pixmap.n <= 3 else "RGBA"
    image = Image.frombytes(mode, (pixmap.width, pixmap.height), pixmap.samples)
    return image.convert("RGB")


def render_pdf_document(path: Path) -> list[RenderedPdfPage]:
    """Render all pages of a validated PDF path."""

    document = open_pdf_document(path)
    try:
        rendered: list[RenderedPdfPage] = []
        for index, page in enumerate(document, start=1):
            rendered.append(
                RenderedPdfPage(
                    page_number=index,
                    image=render_pdf_page(page),
                    extracted_text=page.get_text("text").strip(),
                )
            )
        return rendered
    finally:
        document.close()


def pdf_rect_to_region(rect: fitz.Rect) -> Region:
    """Convert a PDF-space rectangle to the shared rendered-page region type."""

    x = max(0, round(rect.x0 * PDF_RENDER_SCALE))
    y = max(0, round(rect.y0 * PDF_RENDER_SCALE))
    width = max(1, round((rect.x1 - rect.x0) * PDF_RENDER_SCALE))
    height = max(1, round((rect.y1 - rect.y0) * PDF_RENDER_SCALE))
    return Region(x=x, y=y, width=width, height=height, units="rendered_page_pixels")
