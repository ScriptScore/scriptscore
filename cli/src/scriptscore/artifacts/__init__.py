# SPDX-License-Identifier: AGPL-3.0-only
"""Artifact helpers."""

from scriptscore.artifacts.files import file_sha256
from scriptscore.artifacts.images import (
    TransformClipReport,
    apply_canonical_transform,
    apply_manual_transform,
    crop_image,
    load_page_image,
    normalize_page_width,
    save_png,
    transformed_visible_content_clip_report,
)
from scriptscore.artifacts.manifest import write_output_metadata
from scriptscore.artifacts.pdfs import (
    PDF_RENDER_DPI,
    PDF_RENDER_SCALE,
    RenderedPdfPage,
    open_pdf_document,
    pdf_rect_to_region,
    render_pdf_document,
    render_pdf_page,
    validate_pdf_path,
)
from scriptscore.artifacts.traces import write_trace_artifact

__all__ = [
    "PDF_RENDER_DPI",
    "PDF_RENDER_SCALE",
    "RenderedPdfPage",
    "TransformClipReport",
    "apply_canonical_transform",
    "apply_manual_transform",
    "crop_image",
    "file_sha256",
    "load_page_image",
    "normalize_page_width",
    "open_pdf_document",
    "pdf_rect_to_region",
    "render_pdf_document",
    "render_pdf_page",
    "save_png",
    "transformed_visible_content_clip_report",
    "validate_pdf_path",
    "write_output_metadata",
    "write_trace_artifact",
]
