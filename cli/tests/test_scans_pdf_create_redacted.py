# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for `scans.pdf-create-redacted`."""

from __future__ import annotations

import json
from pathlib import Path

import fitz  # type: ignore[import-untyped]
import pytest

from scriptscore.commands import build_command_registry
from scriptscore.contracts import CommandErrorEnvelope
from scriptscore.contracts.envelopes import CommandSuccessEnvelope
from scriptscore.providers import ProviderRegistry
from scriptscore.runtime import CommandRunner
from tests.support.pdfs import make_student_pdf


def _runner() -> CommandRunner:
    return CommandRunner(
        registry=build_command_registry(),
        provider_registry=ProviderRegistry.with_builtin_fakes(),
    )


def _make_rotated_pdf(
    path: Path, *, rotation: int, width: float = 612, height: float = 792
) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    document = fitz.open()
    try:
        page = document.new_page(width=width, height=height)
        page.insert_text((72, 120), "Visible page content", fontsize=12)
        page.set_rotation(rotation)
        document.save(path)
    finally:
        document.close()
    return path.resolve()


def _render_page(path: Path, *, width_px: int, height_px: int) -> tuple[bytes, int, int]:
    document = fitz.open(path)
    try:
        page = document.load_page(0)
        pixmap = page.get_pixmap(
            matrix=fitz.Matrix(width_px / page.rect.width, height_px / page.rect.height),
            colorspace=fitz.csRGB,
            alpha=False,
        )
        return bytes(pixmap.samples), pixmap.width, pixmap.height
    finally:
        document.close()


def _is_black(samples: bytes, index: int) -> bool:
    offset = index * 3
    return samples[offset] <= 16 and samples[offset + 1] <= 16 and samples[offset + 2] <= 16


def _black_coverage(samples: bytes, *, image_width: int, rect: tuple[int, int, int, int]) -> float:
    left, top, right, bottom = rect
    total = max(1, (right - left) * (bottom - top))
    black = 0
    for y in range(top, bottom):
        row = y * image_width
        for x in range(left, right):
            if _is_black(samples, row + x):
                black += 1
    return black / total


def _largest_black_bbox(
    samples: bytes, *, image_width: int, image_height: int
) -> tuple[int, int, int, int]:
    visited = bytearray(image_width * image_height)
    largest: tuple[int, int, int, int, int] | None = None
    for start in range(image_width * image_height):
        if visited[start] or not _is_black(samples, start):
            continue
        visited[start] = 1
        queue = [start]
        count = 0
        min_x = image_width
        min_y = image_height
        max_x = 0
        max_y = 0
        while queue:
            current = queue.pop()
            count += 1
            x = current % image_width
            y = current // image_width
            min_x = min(min_x, x)
            min_y = min(min_y, y)
            max_x = max(max_x, x)
            max_y = max(max_y, y)
            for neighbor in (
                current - 1 if x > 0 else -1,
                current + 1 if x + 1 < image_width else -1,
                current - image_width if y > 0 else -1,
                current + image_width if y + 1 < image_height else -1,
            ):
                if neighbor >= 0 and not visited[neighbor] and _is_black(samples, neighbor):
                    visited[neighbor] = 1
                    queue.append(neighbor)
        if largest is None or count > largest[0]:
            largest = (count, min_x, min_y, max_x + 1, max_y + 1)
    assert largest is not None
    return largest[1:]


def test_scans_pdf_create_redacted_redacts_and_data_has_no_paths(tmp_path: Path) -> None:
    sensitive_name = "SECRET_STUDENT_UPLOAD_NAME.pdf"
    pdf = tmp_path / sensitive_name
    _ = make_student_pdf(pdf, page_texts=["Jordan Example", "Rest stays"])
    out = tmp_path / "out_redacted.pdf"

    result = _runner().run(
        "scans.pdf-create-redacted",
        {
            "input_pdf_path": str(pdf.resolve()),
            "output_pdf_path": str(out.resolve()),
            "regions": [{"page_number": 1, "x": 50, "y": 100, "width": 520, "height": 80}],
            "raster_sizes_by_page": {1: {"width_px": 612, "height_px": 792}},
            "student_ref": "student_1",
        },
    )

    assert result.exit_code == 0
    envelope = result.envelope
    assert isinstance(envelope, CommandSuccessEnvelope)
    raw = json.dumps(envelope.model_dump(mode="json"), sort_keys=True)
    assert sensitive_name not in raw

    data = envelope.data
    assert data["status"] == "ok"
    assert data["page_count"] == 2
    assert "input_pdf_path" not in data
    assert "output_pdf_path" not in data

    doc = fitz.open(out)
    try:
        text = "".join(page.get_text("text") for page in doc)
    finally:
        doc.close()
    assert "Jordan" not in text
    assert "Rest stays" in text


def test_scans_pdf_create_redacted_accepts_worker_injected_output_artifacts_dir(
    tmp_path: Path,
) -> None:
    """Desktop worker merges `output_artifacts_dir` into the JSON-RPC request; model must accept it."""
    pdf = tmp_path / "in.pdf"
    _ = make_student_pdf(pdf, page_texts=["x"])
    job_dir = tmp_path / "job_out"
    job_dir.mkdir()
    out = job_dir / "redacted.pdf"

    result = _runner().run(
        "scans.pdf-create-redacted",
        {
            "input_pdf_path": str(pdf.resolve()),
            "output_pdf_path": str(out.resolve()),
            "regions": [],
            "raster_sizes_by_page": {},
            "output_artifacts_dir": str(job_dir.resolve()),
        },
    )

    assert result.exit_code == 0


def test_scans_pdf_create_redacted_rejects_missing_raster_for_region_page(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "x.pdf", page_texts=["a"])
    out = tmp_path / "o.pdf"

    result = _runner().run(
        "scans.pdf-create-redacted",
        {
            "input_pdf_path": str(pdf.resolve()),
            "output_pdf_path": str(out.resolve()),
            "regions": [{"page_number": 1, "x": 0, "y": 0, "width": 10, "height": 10}],
            "raster_sizes_by_page": {},
        },
    )

    assert result.exit_code != 0
    assert isinstance(result.envelope, CommandErrorEnvelope)


@pytest.mark.parametrize("rotation", [0, 90, 180, 270])
def test_scans_pdf_create_redacted_burns_visual_region_for_rotated_pages(
    tmp_path: Path, rotation: int
) -> None:
    pdf = _make_rotated_pdf(tmp_path / f"rotated_{rotation}.pdf", rotation=rotation)
    out = tmp_path / f"redacted_{rotation}.pdf"
    document = fitz.open(pdf)
    try:
        page = document.load_page(0)
        width_px = round(page.rect.width)
        height_px = round(page.rect.height)
    finally:
        document.close()

    region = {"page_number": 1, "x": 100, "y": 120, "width": 220, "height": 70}
    result = _runner().run(
        "scans.pdf-create-redacted",
        {
            "input_pdf_path": str(pdf),
            "output_pdf_path": str(out.resolve()),
            "regions": [region],
            "raster_sizes_by_page": {1: {"width_px": width_px, "height_px": height_px}},
        },
    )

    assert result.exit_code == 0
    samples, image_width, _image_height = _render_page(out, width_px=width_px, height_px=height_px)
    expected = (100, 120, 320, 190)
    assert _black_coverage(samples, image_width=image_width, rect=expected) >= 0.98
    assert _largest_black_bbox(samples, image_width=image_width, image_height=height_px) == expected


def test_scans_pdf_create_redacted_handles_hunter_shape_rotation_regression(
    tmp_path: Path,
) -> None:
    pdf = _make_rotated_pdf(
        tmp_path / "hunter_shape.pdf",
        rotation=270,
        width=792,
        height=612,
    )
    out = tmp_path / "hunter_shape_redacted.pdf"

    result = _runner().run(
        "scans.pdf-create-redacted",
        {
            "input_pdf_path": str(pdf),
            "output_pdf_path": str(out.resolve()),
            "regions": [{"page_number": 1, "x": 95, "y": 202, "width": 495, "height": 76}],
            "raster_sizes_by_page": {1: {"width_px": 900, "height_px": 1165}},
        },
    )

    assert result.exit_code == 0
    samples, image_width, image_height = _render_page(out, width_px=900, height_px=1165)
    bbox = _largest_black_bbox(samples, image_width=image_width, image_height=image_height)
    assert bbox == (95, 202, 590, 278)


def test_scans_pdf_create_redacted_fails_when_geometry_verification_fails(
    tmp_path: Path,
) -> None:
    pdf = make_student_pdf(tmp_path / "student.pdf", page_texts=["Jordan Example"])
    out = tmp_path / "redacted.pdf"

    result = _runner().run(
        "scans.pdf-create-redacted",
        {
            "input_pdf_path": str(pdf),
            "output_pdf_path": str(out.resolve()),
            "regions": [{"page_number": 1, "x": 900, "y": 900, "width": 40, "height": 40}],
            "raster_sizes_by_page": {1: {"width_px": 612, "height_px": 792}},
        },
    )

    assert result.exit_code != 0
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.code == "redaction_integrity_failed"
    assert result.envelope.error.details["page_number"] == 1
