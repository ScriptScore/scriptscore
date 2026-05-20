# SPDX-License-Identifier: AGPL-3.0-only
"""Scan-domain contract models for explicit page operations."""

from __future__ import annotations

import math
import string
from pathlib import Path
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator

from scriptscore.contracts.common import (
    ConfidenceBucket,
    LlmConfig,
    ProviderSelections,
    WarningObject,
    validate_alignment_provider_selection,
    validate_llm_provider_selection,
)
from scriptscore.paths import require_absolute_path, require_safe_path_component


def _require_finite(value: float, *, field_name: str) -> float:
    if not math.isfinite(value):
        raise ValueError(f"{field_name} must be finite.")
    return value


class Page(BaseModel):
    """Shared explicit page artifact reference."""

    model_config = ConfigDict(extra="forbid")

    page_type: Literal["template", "student_scan"]
    page_number: int = Field(ge=1)
    image_path: Path
    student_ref: str | None = None
    source_pdf_path: Path | None = None

    @model_validator(mode="after")
    def validate_paths(self) -> Page:
        self.image_path = require_absolute_path(self.image_path, field_name="image_path")
        if self.student_ref is not None:
            require_safe_path_component(self.student_ref, field_name="student_ref")
        if self.source_pdf_path is not None:
            self.source_pdf_path = require_absolute_path(
                self.source_pdf_path,
                field_name="source_pdf_path",
            )
        return self


class OcrBox(BaseModel):
    """Normalized OCR text box in page pixel coordinates."""

    model_config = ConfigDict(extra="forbid")

    text: str
    left: int = Field(ge=0)
    top: int = Field(ge=0)
    right: int = Field(gt=0)
    bottom: int = Field(gt=0)
    confidence: float

    @model_validator(mode="after")
    def validate_bounds(self) -> OcrBox:
        if self.right <= self.left:
            raise ValueError("ocr box right must be greater than left.")
        if self.bottom <= self.top:
            raise ValueError("ocr box bottom must be greater than top.")
        return self


class PageOcrMetadata(BaseModel):
    """OCR metadata for one rendered student page."""

    model_config = ConfigDict(extra="forbid")

    page_number: int = Field(ge=1)
    image_sha256: str
    image_width: int = Field(gt=0)
    image_height: int = Field(gt=0)
    boxes: list[OcrBox] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_fingerprint(self) -> PageOcrMetadata:
        if len(self.image_sha256) != 64 or any(
            ch not in string.hexdigits for ch in self.image_sha256
        ):
            raise ValueError("image_sha256 must be a 64-character hexadecimal sha256 digest.")
        self.image_sha256 = self.image_sha256.lower()
        return self


class Transform(BaseModel):
    """Explicit manual page transform."""

    model_config = ConfigDict(extra="forbid")

    rotation: float = 0.0
    scale: float = Field(default=1.0, gt=0)
    translate_x: float = 0.0
    translate_y: float = 0.0

    @model_validator(mode="after")
    def validate_finite_values(self) -> Transform:
        self.rotation = _require_finite(self.rotation, field_name="rotation")
        self.scale = _require_finite(self.scale, field_name="scale")
        self.translate_x = _require_finite(self.translate_x, field_name="translate_x")
        self.translate_y = _require_finite(self.translate_y, field_name="translate_y")
        return self


class Region(BaseModel):
    """Rendered-page crop geometry."""

    model_config = ConfigDict(extra="forbid")

    x: int = Field(ge=0)
    y: int = Field(ge=0)
    width: int = Field(gt=0)
    height: int = Field(gt=0)
    units: Literal["rendered_page_pixels"]


class PdfPointRect(BaseModel):
    """PDF-space rectangle in page points."""

    model_config = ConfigDict(extra="forbid")

    page_number: int = Field(ge=1)
    x_pt: float
    y_pt: float
    width_pt: float = Field(gt=0)
    height_pt: float = Field(gt=0)

    @model_validator(mode="after")
    def validate_finite_values(self) -> PdfPointRect:
        self.x_pt = _require_finite(self.x_pt, field_name="x_pt")
        self.y_pt = _require_finite(self.y_pt, field_name="y_pt")
        self.width_pt = _require_finite(self.width_pt, field_name="width_pt")
        self.height_pt = _require_finite(self.height_pt, field_name="height_pt")
        return self


class TemplateRasterRegion(BaseModel):
    """Template redaction region in rendered-page pixels."""

    model_config = ConfigDict(extra="forbid")

    page_number: int = Field(ge=1)
    x: int = Field(ge=0)
    y: int = Field(ge=0)
    width: int = Field(gt=0)
    height: int = Field(gt=0)


class PdfRasterSize(BaseModel):
    """Rendered raster size for a PDF page."""

    model_config = ConfigDict(extra="forbid")

    width_px: int = Field(gt=0)
    height_px: int = Field(gt=0)


class ScansPdfRenderPageRequest(BaseModel):
    """Command request for scans.pdf-render-page."""

    model_config = ConfigDict(extra="forbid")

    pdf_path: Path
    page_number: int = Field(ge=1)
    zoom: float = Field(gt=0, le=6.0)
    max_width_px: int | None = Field(default=None, gt=0)

    @model_validator(mode="after")
    def validate_request(self) -> ScansPdfRenderPageRequest:
        self.pdf_path = require_absolute_path(self.pdf_path, field_name="pdf_path")
        self.zoom = _require_finite(self.zoom, field_name="zoom")
        return self


class ScansPdfClipRectsRequest(BaseModel):
    """Command request for scans.pdf-clip-rects."""

    model_config = ConfigDict(extra="forbid")

    pdf_path: Path
    rects: list[PdfPointRect] = Field(min_length=1)
    zoom: float = Field(gt=0, le=6.0)

    @model_validator(mode="after")
    def validate_request(self) -> ScansPdfClipRectsRequest:
        self.pdf_path = require_absolute_path(self.pdf_path, field_name="pdf_path")
        self.zoom = _require_finite(self.zoom, field_name="zoom")
        return self


class ScansPdfExtractTextRequest(BaseModel):
    """Command request for scans.pdf-extract-text."""

    model_config = ConfigDict(extra="forbid")

    pdf_path: Path
    page_number: int = Field(ge=1)
    x_pt: float
    y_pt: float
    width_pt: float = Field(gt=0)
    height_pt: float = Field(gt=0)

    @model_validator(mode="after")
    def validate_request(self) -> ScansPdfExtractTextRequest:
        self.pdf_path = require_absolute_path(self.pdf_path, field_name="pdf_path")
        self.x_pt = _require_finite(self.x_pt, field_name="x_pt")
        self.y_pt = _require_finite(self.y_pt, field_name="y_pt")
        self.width_pt = _require_finite(self.width_pt, field_name="width_pt")
        self.height_pt = _require_finite(self.height_pt, field_name="height_pt")
        return self


class ScansPdfMapTemplateRegionsRequest(BaseModel):
    """Command request for scans.pdf-map-template-regions."""

    model_config = ConfigDict(extra="forbid")

    pdf_path: Path
    regions: list[TemplateRasterRegion] = Field(min_length=1)
    raster_sizes_by_page: dict[int, PdfRasterSize]

    @model_validator(mode="after")
    def validate_request(self) -> ScansPdfMapTemplateRegionsRequest:
        self.pdf_path = require_absolute_path(self.pdf_path, field_name="pdf_path")
        if not self.raster_sizes_by_page:
            raise ValueError("raster_sizes_by_page must not be empty.")
        return self


class ScansPdfCreateRedactedRequest(BaseModel):
    """Command request for scans.pdf-create-redacted (burn template redaction rectangles)."""

    model_config = ConfigDict(extra="forbid")

    input_pdf_path: Path
    output_pdf_path: Path
    regions: list[TemplateRasterRegion] = Field(default_factory=list)
    raster_sizes_by_page: dict[int, PdfRasterSize] = Field(default_factory=dict)
    """Template preview pixel size per page number (maps template pixels to PDF points)."""
    page_order: list[int] | None = None
    """Optional selected source PDF pages to keep in the output PDF, in output order."""
    student_ref: str | None = None
    """Optional scope for progress events only; never echoed in success `data`."""
    output_artifacts_dir: Path | None = None
    """Optional; injected by the desktop worker for job layout. Not used for writes — output is only `output_pdf_path`."""

    @model_validator(mode="after")
    def validate_request(self) -> ScansPdfCreateRedactedRequest:
        self.input_pdf_path = require_absolute_path(
            self.input_pdf_path, field_name="input_pdf_path"
        )
        self.output_pdf_path = require_absolute_path(
            self.output_pdf_path, field_name="output_pdf_path"
        )
        if self.output_artifacts_dir is not None:
            self.output_artifacts_dir = require_absolute_path(
                self.output_artifacts_dir,
                field_name="output_artifacts_dir",
            )
        if self.student_ref is not None:
            require_safe_path_component(self.student_ref, field_name="student_ref")
        for page_number, _region in ((r.page_number, r) for r in self.regions):
            if page_number not in self.raster_sizes_by_page:
                raise ValueError(
                    f"raster_sizes_by_page must include page_number {page_number} for each region page."
                )
        if self.page_order is not None:
            if not self.page_order:
                self.page_order = None
            elif len(set(self.page_order)) != len(self.page_order):
                raise ValueError("page_order must not contain duplicate page numbers.")
        return self


class ScansPdfDetectArucoRequest(BaseModel):
    """Command request for scans.pdf-detect-aruco."""

    model_config = ConfigDict(extra="forbid")

    pdf_path: Path
    zoom: float = Field(default=4.0, gt=0, le=6.0)
    output_artifacts_dir: Path | None = None
    """Optional; accepted for desktop worker job layout. Detection writes no artifacts."""

    @model_validator(mode="after")
    def validate_request(self) -> ScansPdfDetectArucoRequest:
        self.pdf_path = require_absolute_path(self.pdf_path, field_name="pdf_path")
        self.zoom = _require_finite(self.zoom, field_name="zoom")
        if self.output_artifacts_dir is not None:
            self.output_artifacts_dir = require_absolute_path(
                self.output_artifacts_dir,
                field_name="output_artifacts_dir",
            )
        return self


class ScansPdfStampArucoRequest(BaseModel):
    """Command request for scans.pdf-stamp-aruco."""

    model_config = ConfigDict(extra="forbid")

    input_pdf_path: Path
    output_artifacts_dir: Path

    @model_validator(mode="after")
    def validate_request(self) -> ScansPdfStampArucoRequest:
        self.input_pdf_path = require_absolute_path(
            self.input_pdf_path, field_name="input_pdf_path"
        )
        self.output_artifacts_dir = require_absolute_path(
            self.output_artifacts_dir,
            field_name="output_artifacts_dir",
        )
        return self


class TransformTarget(BaseModel):
    """Manual transform request row."""

    model_config = ConfigDict(extra="forbid")

    page: Page
    transform: Transform

    @model_validator(mode="after")
    def validate_page(self) -> TransformTarget:
        if self.page.page_type != "student_scan":
            raise ValueError("transform target pages must be student_scan pages.")
        return self


class TransformResult(BaseModel):
    """Manual transform result row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str
    page_number: int = Field(ge=1)
    status: Literal["ok", "error"]
    transform: Transform
    output_page: Page
    warnings: list[WarningObject] = Field(default_factory=list)


class AlignmentResult(BaseModel):
    """Auto-alignment proposal result row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str
    page_number: int = Field(ge=1)
    status: Literal["ok", "low_confidence", "failed"]
    confidence: float | None = None
    transform: Transform | None = None
    warnings: list[WarningObject] = Field(default_factory=list)


class ScansAlignAutoRequest(BaseModel):
    """Command request for scans.align-auto."""

    model_config = ConfigDict(extra="forbid")

    template_pages: list[Page] = Field(min_length=1)
    student_pages: list[Page] = Field(min_length=1)
    output_artifacts_dir: Path
    marker_mode: Literal["ignore", "prefer_aruco"] = "prefer_aruco"
    mode: Literal["fast", "precise"] = "fast"
    providers: ProviderSelections

    @model_validator(mode="after")
    def validate_request(self) -> ScansAlignAutoRequest:
        self.output_artifacts_dir = require_absolute_path(
            self.output_artifacts_dir,
            field_name="output_artifacts_dir",
        )
        validate_alignment_provider_selection(providers=self.providers)
        seen_template_pages: set[int] = set()
        for page in self.template_pages:
            if page.page_type != "template":
                raise ValueError("template_pages must contain template pages only.")
            if page.page_number in seen_template_pages:
                raise ValueError(
                    f"duplicate template page supplied for page_number={page.page_number}."
                )
            seen_template_pages.add(page.page_number)
        seen_student_pages: set[tuple[str, int]] = set()
        for page in self.student_pages:
            if page.page_type != "student_scan":
                raise ValueError("student_pages must contain student_scan pages only.")
            if page.student_ref is None:
                raise ValueError("student_pages must include student_ref.")
            key = (page.student_ref, page.page_number)
            if key in seen_student_pages:
                raise ValueError(
                    f"duplicate student page supplied for student_ref={page.student_ref!r}, "
                    f"page_number={page.page_number}."
                )
            seen_student_pages.add(key)
        return self


class ScansTransformRequest(BaseModel):
    """Command request for scans.transform."""

    model_config = ConfigDict(extra="forbid")

    transform_targets: list[TransformTarget] = Field(min_length=1)
    output_artifacts_dir: Path

    @model_validator(mode="after")
    def validate_output_dir(self) -> ScansTransformRequest:
        self.output_artifacts_dir = require_absolute_path(
            self.output_artifacts_dir,
            field_name="output_artifacts_dir",
        )
        seen: set[tuple[str, int]] = set()
        for target in self.transform_targets:
            if target.page.student_ref is None:
                raise ValueError("transform target pages must include student_ref.")
            key = (target.page.student_ref, target.page.page_number)
            if key in seen:
                raise ValueError(
                    f"duplicate transform target supplied for student_ref={target.page.student_ref!r}, "
                    f"page_number={target.page.page_number}."
                )
            seen.add(key)
        return self


class CanonicalizeTarget(BaseModel):
    """Template-canvas canonicalization request row."""

    model_config = ConfigDict(extra="forbid")

    page: Page
    template_page: Page
    transform: Transform

    @model_validator(mode="after")
    def validate_pages(self) -> CanonicalizeTarget:
        if self.page.page_type != "student_scan":
            raise ValueError("canonicalize target pages must be student_scan pages.")
        if self.page.student_ref is None:
            raise ValueError("canonicalize target pages must include student_ref.")
        if self.template_page.page_type != "template":
            raise ValueError("canonicalize target template_page must be a template page.")
        if self.page.page_number != self.template_page.page_number:
            raise ValueError(
                "canonicalize target page_number must match template_page.page_number."
            )
        return self


class CanonicalizeResult(BaseModel):
    """Canonicalization result row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str
    page_number: int = Field(ge=1)
    status: Literal["ok", "error"]
    transform: Transform
    output_page: Page
    warnings: list[WarningObject] = Field(default_factory=list)


class ScansCanonicalizeRequest(BaseModel):
    """Command request for scans.canonicalize."""

    model_config = ConfigDict(extra="forbid")

    canonicalize_targets: list[CanonicalizeTarget] = Field(min_length=1)
    output_artifacts_dir: Path

    @model_validator(mode="after")
    def validate_output_dir(self) -> ScansCanonicalizeRequest:
        self.output_artifacts_dir = require_absolute_path(
            self.output_artifacts_dir,
            field_name="output_artifacts_dir",
        )
        seen: set[tuple[str, int]] = set()
        for target in self.canonicalize_targets:
            assert target.page.student_ref is not None
            key = (target.page.student_ref, target.page.page_number)
            if key in seen:
                raise ValueError(
                    "duplicate canonicalize target supplied for "
                    f"student_ref={target.page.student_ref!r}, "
                    f"page_number={target.page.page_number}."
                )
            seen.add(key)
        return self


class PdfTarget(BaseModel):
    """One PDF ingest request row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str = Field(min_length=1)
    pdf_path: Path
    page_order: list[int] | None = None

    @model_validator(mode="after")
    def validate_fields(self) -> PdfTarget:
        require_safe_path_component(self.student_ref, field_name="student_ref")
        self.pdf_path = require_absolute_path(self.pdf_path, field_name="pdf_path")
        if self.page_order is not None:
            if len(self.page_order) == 0:
                self.page_order = None
            elif len(set(self.page_order)) != len(self.page_order):
                raise ValueError("page_order must not contain duplicate page numbers.")
        return self


class PdfIngestResult(BaseModel):
    """One PDF ingest output row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str
    source_pdf_path: Path
    status: Literal["ok", "error"]
    pages: list[Page] = Field(default_factory=list)
    warnings: list[WarningObject] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_paths(self) -> PdfIngestResult:
        self.source_pdf_path = require_absolute_path(
            self.source_pdf_path, field_name="source_pdf_path"
        )
        return self


class ScansIngestRequest(BaseModel):
    """Command request for scans.ingest."""

    model_config = ConfigDict(extra="forbid")

    pdf_targets: list[PdfTarget] = Field(min_length=1)
    output_artifacts_dir: Path

    @model_validator(mode="after")
    def validate_request(self) -> ScansIngestRequest:
        self.output_artifacts_dir = require_absolute_path(
            self.output_artifacts_dir,
            field_name="output_artifacts_dir",
        )
        seen: set[str] = set()
        for target in self.pdf_targets:
            if target.student_ref in seen:
                raise ValueError(
                    f"duplicate ingest target supplied for student_ref={target.student_ref!r}."
                )
            seen.add(target.student_ref)
        return self


class QuestionCropTarget(BaseModel):
    """Per-question crop target row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str | None = None
    question_id: str = Field(min_length=1)
    page_number: int = Field(ge=1)
    region: Region

    @model_validator(mode="after")
    def validate_fields(self) -> QuestionCropTarget:
        if self.student_ref is not None:
            require_safe_path_component(self.student_ref, field_name="student_ref")
        require_safe_path_component(self.question_id, field_name="question_id")
        return self


class QuestionDetectHint(BaseModel):
    """Template-derived OCR hint for one question on a student page."""

    model_config = ConfigDict(extra="forbid")

    question_id: str = Field(min_length=1)
    question_number: int = Field(ge=1)
    template_region: Region
    question_text_hint: str = Field(min_length=1)

    @model_validator(mode="after")
    def validate_fields(self) -> QuestionDetectHint:
        require_safe_path_component(self.question_id, field_name="question_id")
        return self


class DetectTarget(BaseModel):
    """One OCR region-detection target page."""

    model_config = ConfigDict(extra="forbid")

    page: Page
    question_hints: list[QuestionDetectHint] = Field(min_length=1)
    ocr_boxes: list[OcrBox] | None = None
    ocr_metadata_path: Path | None = None

    @model_validator(mode="after")
    def validate_fields(self) -> DetectTarget:
        if self.page.page_type != "student_scan":
            raise ValueError("detect target pages must be student_scan pages.")
        if self.page.student_ref is None:
            raise ValueError("detect target pages must include student_ref.")
        if self.ocr_metadata_path is not None:
            self.ocr_metadata_path = require_absolute_path(
                self.ocr_metadata_path,
                field_name="ocr_metadata_path",
            )
        if self.ocr_boxes is not None and self.ocr_metadata_path is not None:
            raise ValueError("detect targets must not supply both ocr_boxes and ocr_metadata_path.")
        seen_question_ids: set[str] = set()
        seen_question_numbers: set[int] = set()
        for hint in self.question_hints:
            if hint.question_id in seen_question_ids:
                raise ValueError(
                    f"duplicate detect hint supplied for question_id={hint.question_id!r}."
                )
            if hint.question_number in seen_question_numbers:
                raise ValueError(
                    f"duplicate detect hint supplied for question_number={hint.question_number}."
                )
            seen_question_ids.add(hint.question_id)
            seen_question_numbers.add(hint.question_number)
        return self


class DetectResult(BaseModel):
    """One OCR region-detection result row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str
    page_number: int = Field(ge=1)
    question_id: str
    status: Literal["ok", "warning", "error", "cancelled"]
    region: Region | None = None
    region_source: Literal["ocr_refined", "template_fallback"] | None = None
    warnings: list[WarningObject] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_region_state(self) -> DetectResult:
        if self.status in {"ok", "warning"} and self.region is None:
            raise ValueError("region is required when status is ok or warning.")
        if self.status in {"ok", "warning"} and self.region_source is None:
            raise ValueError("region_source is required when status is ok or warning.")
        if (
            self.status in {"error", "cancelled"}
            and self.region is not None
            and self.region_source is None
        ):
            raise ValueError("region_source is required when region is supplied.")
        return self


class PageOcrResult(BaseModel):
    """Reusable OCR metadata artifact result for one student page."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str
    page_number: int = Field(ge=1)
    ocr_metadata_path: Path
    ocr_source: Literal["generated", "artifact", "inline_boxes"]

    @model_validator(mode="after")
    def validate_paths(self) -> PageOcrResult:
        self.ocr_metadata_path = require_absolute_path(
            self.ocr_metadata_path,
            field_name="ocr_metadata_path",
        )
        return self


class ScansDetectRequest(BaseModel):
    """Command request for scans.detect."""

    model_config = ConfigDict(extra="forbid")

    detect_targets: list[DetectTarget] = Field(min_length=1)
    output_artifacts_dir: Path

    @model_validator(mode="after")
    def validate_request(self) -> ScansDetectRequest:
        self.output_artifacts_dir = require_absolute_path(
            self.output_artifacts_dir,
            field_name="output_artifacts_dir",
        )
        seen: set[tuple[str, int]] = set()
        for target in self.detect_targets:
            assert target.page.student_ref is not None
            key = (target.page.student_ref, target.page.page_number)
            if key in seen:
                raise ValueError(
                    f"duplicate detect target supplied for student_ref={target.page.student_ref!r}, "
                    f"page_number={target.page.page_number}."
                )
            seen.add(key)
        return self


class QuestionCropResult(BaseModel):
    """Question crop result row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str
    question_id: str
    status: Literal["ok", "error"]
    question_crop_path: Path | None = None
    warnings: list[WarningObject] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_ok_path(self) -> QuestionCropResult:
        if self.status == "ok":
            if self.question_crop_path is None:
                raise ValueError("question_crop_path is required when status is ok.")
            self.question_crop_path = require_absolute_path(
                self.question_crop_path,
                field_name="question_crop_path",
            )
        return self


class ScansCropRequest(BaseModel):
    """Command request for scans.crop."""

    model_config = ConfigDict(extra="forbid")

    pages: list[Page] = Field(min_length=1)
    question_crop_targets: list[QuestionCropTarget] = Field(min_length=1)
    output_artifacts_dir: Path

    @model_validator(mode="after")
    def validate_request(self) -> ScansCropRequest:
        self.output_artifacts_dir = require_absolute_path(
            self.output_artifacts_dir,
            field_name="output_artifacts_dir",
        )
        seen: set[tuple[str, int]] = set()
        for page in self.pages:
            if page.page_type != "student_scan":
                raise ValueError("crop pages must be student_scan pages.")
            if page.student_ref is None:
                raise ValueError("crop pages must include student_ref.")
            key = (page.student_ref, page.page_number)
            if key in seen:
                raise ValueError(
                    f"duplicate crop page supplied for student_ref={page.student_ref!r}, "
                    f"page_number={page.page_number}."
                )
            seen.add(key)
        return self


class ParseQuestionContext(BaseModel):
    """Prompt-facing OCR context."""

    model_config = ConfigDict(extra="forbid")

    question_number: int = Field(ge=1)
    question_text_clean: str = Field(min_length=1)


PiiType = Literal["name", "username", "email", "phone_number"]
HandwritingState = Literal["true", "false", "unknown"]


class PiiRuntimeConfig(BaseModel):
    """Explicit local runtime settings for scans.pii."""

    model_config = ConfigDict(extra="forbid")

    paddle_model_dir: Path
    max_workers: int = Field(default=1, ge=1, le=4)

    @model_validator(mode="after")
    def validate_fields(self) -> PiiRuntimeConfig:
        self.paddle_model_dir = require_absolute_path(
            self.paddle_model_dir,
            field_name="paddle_model_dir",
        )
        return self


class PiiTarget(BaseModel):
    """One cropped answer image to inspect for handwriting and PII."""

    model_config = ConfigDict(extra="forbid")

    question_id: str = Field(min_length=1)
    question_crop_path: Path

    @model_validator(mode="after")
    def validate_fields(self) -> PiiTarget:
        require_safe_path_component(self.question_id, field_name="question_id")
        self.question_crop_path = require_absolute_path(
            self.question_crop_path,
            field_name="question_crop_path",
        )
        return self


class PiiStudentRequest(BaseModel):
    """Student-scoped crop rows and trigger terms for scans.pii."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str = Field(min_length=1)
    pii_trigger_words: list[str] = Field(min_length=1)
    pii_targets: list[PiiTarget] = Field(min_length=1)

    @model_validator(mode="after")
    def validate_fields(self) -> PiiStudentRequest:
        require_safe_path_component(self.student_ref, field_name="student_ref")
        normalized_triggers: list[str] = []
        for index, trigger in enumerate(self.pii_trigger_words):
            cleaned = trigger.strip()
            if not cleaned:
                raise ValueError(f"pii_trigger_words[{index}] must not be blank.")
            normalized_triggers.append(cleaned)
        self.pii_trigger_words = normalized_triggers
        seen_question_ids: set[str] = set()
        for target in self.pii_targets:
            if target.question_id in seen_question_ids:
                raise ValueError(
                    f"duplicate pii target supplied for question_id={target.question_id!r}."
                )
            seen_question_ids.add(target.question_id)
        return self


class PiiPrescreen(BaseModel):
    """Caller-supplied clean upstream scans.pii handoff for scans.parse."""

    model_config = ConfigDict(extra="forbid")

    source_command: Literal["scans.pii"]
    status: Literal["ok", "warning", "error"]
    contains_handwriting: HandwritingState
    contains_pii: bool
    pii_types_detected: list[PiiType] = Field(default_factory=list)
    warnings: list[WarningObject] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_state(self) -> PiiPrescreen:
        if self.contains_handwriting != "true":
            if self.contains_pii:
                raise ValueError(
                    "contains_pii must be false when contains_handwriting is not 'true'."
                )
            if self.pii_types_detected:
                raise ValueError(
                    "pii_types_detected must be empty when contains_handwriting is not 'true'."
                )
        if not self.contains_pii and self.pii_types_detected:
            raise ValueError("pii_types_detected must be empty when contains_pii is false.")
        return self


class ParseTarget(BaseModel):
    """One parse request row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str = Field(min_length=1)
    question_id: str = Field(min_length=1)
    parse_question_context: ParseQuestionContext
    question_crop_path: Path
    template_question_png_path: Path
    pii_prescreen: PiiPrescreen | None = None

    @model_validator(mode="after")
    def validate_fields(self) -> ParseTarget:
        require_safe_path_component(self.student_ref, field_name="student_ref")
        require_safe_path_component(self.question_id, field_name="question_id")
        self.question_crop_path = require_absolute_path(
            self.question_crop_path,
            field_name="question_crop_path",
        )
        self.template_question_png_path = require_absolute_path(
            self.template_question_png_path,
            field_name="template_question_png_path",
        )
        return self


class ParseDraft(BaseModel):
    """One parse result row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str
    question_id: str
    status: Literal["ok", "blank", "warning", "error", "cancelled"]
    parsed_text: str | None = None
    blank: bool
    confidence: ConfidenceBucket | None = None
    confidence_source: (
        Literal[
            "handwriting_verify",
            "pii_prescreen",
            "ocr_parse",
            "combined",
        ]
        | None
    ) = None
    warnings: list[WarningObject] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_blank_state(self) -> ParseDraft:
        if self.blank:
            if self.status != "blank":
                raise ValueError("blank parse drafts must use status='blank'.")
            if self.parsed_text not in {None, ""}:
                raise ValueError("blank parse drafts must use empty parsed_text.")
            self.parsed_text = ""
        return self


class ScansParseRequest(BaseModel):
    """Command request for scans.parse."""

    model_config = ConfigDict(extra="forbid")

    parse_targets: list[ParseTarget] = Field(min_length=1)
    output_artifacts_dir: Path
    providers: ProviderSelections
    llm_config: LlmConfig

    @model_validator(mode="after")
    def validate_request(self) -> ScansParseRequest:
        self.output_artifacts_dir = require_absolute_path(
            self.output_artifacts_dir,
            field_name="output_artifacts_dir",
        )
        validate_llm_provider_selection(providers=self.providers, llm_config=self.llm_config)
        return self


class PiiResult(BaseModel):
    """One scans.pii result row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str
    question_id: str
    status: Literal["ok", "warning", "error"]
    contains_handwriting: HandwritingState
    contains_pii: bool
    pii_types_detected: list[PiiType] = Field(default_factory=list)
    warnings: list[WarningObject] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_state(self) -> PiiResult:
        if self.contains_handwriting != "true":
            if self.contains_pii:
                raise ValueError(
                    "contains_pii must be false when contains_handwriting is not 'true'."
                )
            if self.pii_types_detected:
                raise ValueError(
                    "pii_types_detected must be empty when contains_handwriting is not 'true'."
                )
        if not self.contains_pii and self.pii_types_detected:
            raise ValueError("pii_types_detected must be empty when contains_pii is false.")
        return self


class ScansPiiRequest(BaseModel):
    """Command request for scans.pii."""

    model_config = ConfigDict(extra="forbid")

    students: list[PiiStudentRequest] = Field(min_length=1)
    output_artifacts_dir: Path
    pii_runtime_config: PiiRuntimeConfig

    @model_validator(mode="after")
    def validate_request(self) -> ScansPiiRequest:
        self.output_artifacts_dir = require_absolute_path(
            self.output_artifacts_dir,
            field_name="output_artifacts_dir",
        )
        seen_students: set[str] = set()
        for student in self.students:
            if student.student_ref in seen_students:
                raise ValueError(
                    f"duplicate pii student supplied for student_ref={student.student_ref!r}."
                )
            seen_students.add(student.student_ref)
        return self
