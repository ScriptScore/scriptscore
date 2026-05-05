# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for transient PDF helper scan commands."""

from __future__ import annotations

import base64
import io
from pathlib import Path

import fitz  # type: ignore[import-untyped]
import pytest
from PIL import Image

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


def test_scans_pdf_render_page_returns_base64_png_without_artifacts(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "student.pdf", page_texts=["Jordan Example"])

    result = _runner().run(
        "scans.pdf-render-page",
        {"pdf_path": str(pdf), "page_number": 1, "zoom": 2.0},
        request_id="req_render",
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    data = result.envelope.data
    png = Image.open(io.BytesIO(base64.b64decode(data["png_base64"])))
    assert data["page_number"] == 1
    assert data["page_count"] == 1
    assert data["png_width_px"] == png.width
    assert data["png_height_px"] == png.height
    assert result.envelope.artifacts == []


def test_scans_pdf_render_page_reports_total_page_count(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "student.pdf", page_texts=["First", "Second"])

    result = _runner().run(
        "scans.pdf-render-page",
        {"pdf_path": str(pdf), "page_number": 1, "zoom": 2.0},
        request_id="req_render",
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.data["page_count"] == 2


def test_scans_pdf_render_page_caps_preview_width_when_requested(tmp_path: Path) -> None:
    pdf = tmp_path / "wide.pdf"
    document = fitz.open()
    try:
        page = document.new_page(width=900, height=600)
        page.insert_text((72, 120), "wide preview", fontsize=12)
        document.save(pdf)
    finally:
        document.close()

    uncapped = _runner().run(
        "scans.pdf-render-page",
        {"pdf_path": str(pdf.resolve()), "page_number": 1, "zoom": 2.0},
    )
    capped = _runner().run(
        "scans.pdf-render-page",
        {"pdf_path": str(pdf.resolve()), "page_number": 1, "zoom": 2.0, "max_width_px": 1600},
    )

    assert uncapped.exit_code == 0
    assert capped.exit_code == 0
    assert isinstance(uncapped.envelope, CommandSuccessEnvelope)
    assert isinstance(capped.envelope, CommandSuccessEnvelope)
    assert uncapped.envelope.data["png_width_px"] == 1800
    assert capped.envelope.data["png_width_px"] <= 1600
    assert capped.envelope.data["png_width_px"] < uncapped.envelope.data["png_width_px"]
    assert capped.envelope.data["png_height_px"] < uncapped.envelope.data["png_height_px"]


def test_scans_pdf_render_page_width_cap_above_natural_width_is_noop(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "student.pdf", page_texts=["Jordan Example"])

    uncapped = _runner().run(
        "scans.pdf-render-page",
        {"pdf_path": str(pdf), "page_number": 1, "zoom": 2.0},
    )
    capped = _runner().run(
        "scans.pdf-render-page",
        {"pdf_path": str(pdf), "page_number": 1, "zoom": 2.0, "max_width_px": 5000},
    )

    assert uncapped.exit_code == 0
    assert capped.exit_code == 0
    assert isinstance(uncapped.envelope, CommandSuccessEnvelope)
    assert isinstance(capped.envelope, CommandSuccessEnvelope)
    assert capped.envelope.data["png_width_px"] == uncapped.envelope.data["png_width_px"]
    assert capped.envelope.data["png_height_px"] == uncapped.envelope.data["png_height_px"]


def test_scans_pdf_render_page_rejects_invalid_width_cap(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "student.pdf", page_texts=["Jordan Example"])

    result = _runner().run(
        "scans.pdf-render-page",
        {"pdf_path": str(pdf), "page_number": 1, "zoom": 2.0, "max_width_px": 0},
    )

    assert result.exit_code == 2
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.details["issues"][0]["path"] == ["max_width_px"]


def test_scans_pdf_clip_rects_returns_ordered_clips(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "student.pdf", page_texts=["Alpha Beta"])

    result = _runner().run(
        "scans.pdf-clip-rects",
        {
            "pdf_path": str(pdf),
            "zoom": 2.0,
            "rects": [
                {
                    "page_number": 1,
                    "x_pt": 60.0,
                    "y_pt": 100.0,
                    "width_pt": 120.0,
                    "height_pt": 40.0,
                },
                {
                    "page_number": 1,
                    "x_pt": 60.0,
                    "y_pt": 140.0,
                    "width_pt": 120.0,
                    "height_pt": 40.0,
                },
            ],
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    data = result.envelope.data
    assert [clip["page_number"] for clip in data["clips"]] == [1, 1]
    assert all(base64.b64decode(clip["png_base64"]) for clip in data["clips"])


def test_scans_pdf_extract_text_returns_clipped_text(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "student.pdf", page_texts=["Jordan Example"])

    result = _runner().run(
        "scans.pdf-extract-text",
        {
            "pdf_path": str(pdf),
            "page_number": 1,
            "x_pt": 60.0,
            "y_pt": 100.0,
            "width_pt": 200.0,
            "height_pt": 40.0,
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    data = result.envelope.data
    assert "Jordan Example" in data["text"]


def test_scans_pdf_map_template_regions_scales_to_page_points(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "student.pdf", page_texts=["Jordan Example"])
    document = fitz.open(pdf)
    try:
        page = document.load_page(0)
        width_pt = float(page.rect.width)
        height_pt = float(page.rect.height)
    finally:
        document.close()

    result = _runner().run(
        "scans.pdf-map-template-regions",
        {
            "pdf_path": str(pdf),
            "regions": [{"page_number": 1, "x": 100, "y": 50, "width": 200, "height": 80}],
            "raster_sizes_by_page": {1: {"width_px": 1000, "height_px": 2000}},
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    rect = result.envelope.data["rects"][0]
    assert rect["x_pt"] == pytest.approx(width_pt * 0.1)
    assert rect["y_pt"] == pytest.approx(height_pt * 0.025)
    assert rect["width_pt"] == pytest.approx(width_pt * 0.2)
    assert rect["height_pt"] == pytest.approx(height_pt * 0.04)


def test_scans_pdf_map_template_regions_keeps_visual_points_for_rotated_pages(
    tmp_path: Path,
) -> None:
    pdf = tmp_path / "rotated.pdf"
    document = fitz.open()
    try:
        page = document.new_page(width=792, height=612)
        page.set_rotation(270)
        document.save(pdf)
    finally:
        document.close()
    document = fitz.open(pdf)
    try:
        page = document.load_page(0)
        assert page.rotation == 270
        width_pt = float(page.rect.width)
        height_pt = float(page.rect.height)
    finally:
        document.close()

    result = _runner().run(
        "scans.pdf-map-template-regions",
        {
            "pdf_path": str(pdf.resolve()),
            "regions": [{"page_number": 1, "x": 100, "y": 50, "width": 200, "height": 80}],
            "raster_sizes_by_page": {1: {"width_px": 1000, "height_px": 2000}},
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    rect = result.envelope.data["rects"][0]
    assert rect["x_pt"] == pytest.approx(width_pt * 0.1)
    assert rect["y_pt"] == pytest.approx(height_pt * 0.025)
    assert rect["width_pt"] == pytest.approx(width_pt * 0.2)
    assert rect["height_pt"] == pytest.approx(height_pt * 0.04)


def test_scans_pdf_map_template_regions_rejects_missing_raster_for_region_page(
    tmp_path: Path,
) -> None:
    pdf = make_student_pdf(tmp_path / "student.pdf", page_texts=["Jordan Example"])

    result = _runner().run(
        "scans.pdf-map-template-regions",
        {
            "pdf_path": str(pdf),
            "regions": [{"page_number": 1, "x": 100, "y": 50, "width": 200, "height": 80}],
            "raster_sizes_by_page": {2: {"width_px": 1000, "height_px": 2000}},
        },
    )

    assert result.exit_code != 0
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.code == "missing_template_raster_size"


def test_scans_pdf_render_page_rejects_out_of_range_page(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "student.pdf", page_texts=["Only one page"])

    result = _runner().run(
        "scans.pdf-render-page",
        {"pdf_path": str(pdf), "page_number": 2, "zoom": 2.0},
    )

    assert result.exit_code == 2
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.code == "page_number_out_of_range"


def test_scans_pdf_detect_aruco_returns_zero_for_unstamped_pdf(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "template.pdf", page_texts=["Unstamped template"])

    result = _runner().run(
        "scans.pdf-detect-aruco",
        {"pdf_path": str(pdf)},
        request_id="req_detect_aruco",
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.data["total_marker_count"] == 0
    assert result.envelope.data["pages"] == [
        {"page_number": 1, "marker_count": 0, "marker_ids": []}
    ]


def test_scans_pdf_stamp_aruco_writes_detectable_four_corner_markers(
    tmp_path: Path,
) -> None:
    pdf = make_student_pdf(tmp_path / "template.pdf", page_texts=["Page one", "Page two"])
    output_dir = (tmp_path / "stamped").resolve()

    stamp = _runner().run(
        "scans.pdf-stamp-aruco",
        {"input_pdf_path": str(pdf), "output_artifacts_dir": str(output_dir)},
        request_id="req_stamp",
    )

    assert stamp.exit_code == 0
    assert isinstance(stamp.envelope, CommandSuccessEnvelope)
    assert stamp.envelope.data["marker_count"] == 8
    assert stamp.envelope.data["page_count"] == 2
    assert "input_pdf_path" not in stamp.envelope.data
    assert "output_pdf_path" not in stamp.envelope.data
    stamped_pdf = output_dir / "stamped_template.pdf"
    assert stamped_pdf.is_file()
    assert (output_dir / "stamped_template_page_001.png").is_file()

    detect = _runner().run(
        "scans.pdf-detect-aruco",
        {"pdf_path": str(stamped_pdf)},
        request_id="req_detect_stamped",
    )

    assert detect.exit_code == 0
    assert isinstance(detect.envelope, CommandSuccessEnvelope)
    assert detect.envelope.data["total_marker_count"] == 8
    assert detect.envelope.data["pages"] == [
        {"page_number": 1, "marker_count": 4, "marker_ids": [0, 1, 2, 3]},
        {"page_number": 2, "marker_count": 4, "marker_ids": [4, 5, 6, 7]},
    ]


def test_scans_pdf_stamp_aruco_wraps_marker_ids_after_dictionary_capacity(
    tmp_path: Path,
) -> None:
    pdf = make_student_pdf(
        tmp_path / "long.pdf", page_texts=[f"Page {index}" for index in range(26)]
    )
    output_dir = (tmp_path / "long_stamped").resolve()

    stamp = _runner().run(
        "scans.pdf-stamp-aruco",
        {
            "input_pdf_path": str(pdf),
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert stamp.exit_code == 0
    assert isinstance(stamp.envelope, CommandSuccessEnvelope)
    assert stamp.envelope.data["marker_count"] == 104

    detect = _runner().run(
        "scans.pdf-detect-aruco",
        {"pdf_path": str(output_dir / "stamped_template.pdf")},
    )

    assert detect.exit_code == 0
    assert isinstance(detect.envelope, CommandSuccessEnvelope)
    pages = detect.envelope.data["pages"]
    assert pages[0] == {"page_number": 1, "marker_count": 4, "marker_ids": [0, 1, 2, 3]}
    assert pages[25] == {"page_number": 26, "marker_count": 4, "marker_ids": [0, 1, 2, 3]}
