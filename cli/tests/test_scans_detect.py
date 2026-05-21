# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for `scans detect`."""

from __future__ import annotations

import json
import math
from pathlib import Path
from typing import Any

import pytest
from pydantic import ValidationError

from scriptscore.artifacts import file_sha256
from scriptscore.commands import build_command_registry
from scriptscore.commands.scans_detect import _HeaderTextMatch, _is_header_fragment_match
from scriptscore.contracts import (
    CommandSuccessEnvelope,
    QuestionDetectHint,
    Region,
    ScansDetectRequest,
)
from scriptscore.ocr import OcrTextBox
from scriptscore.providers import ProviderRegistry
from scriptscore.runtime import CommandRunner
from tests.support.images import make_rgb_page


def _runner() -> CommandRunner:
    return CommandRunner(
        registry=build_command_registry(),
        provider_registry=ProviderRegistry.for_runtime(include_builtin_fakes=True),
    )


def _detect_request(*, page: Path, output_dir: Path) -> dict[str, Any]:
    return {
        "detect_targets": [
            {
                "page": {
                    "page_type": "student_scan",
                    "page_number": 1,
                    "image_path": str(page),
                    "student_ref": "scan_001",
                },
                "question_hints": [
                    {
                        "question_id": "q1",
                        "question_number": 1,
                        "template_region": {
                            "x": 10,
                            "y": 12,
                            "width": 60,
                            "height": 20,
                            "units": "rendered_page_pixels",
                        },
                        "question_text_hint": "First question",
                    },
                    {
                        "question_id": "q2",
                        "question_number": 2,
                        "template_region": {
                            "x": 10,
                            "y": 60,
                            "width": 60,
                            "height": 20,
                            "units": "rendered_page_pixels",
                        },
                        "question_text_hint": "Second question",
                    },
                ],
            }
        ],
        "output_artifacts_dir": str(output_dir),
    }


def test_header_fragment_match_treats_near_full_coverage_as_full() -> None:
    hint = QuestionDetectHint(
        question_id="q1",
        question_number=1,
        template_region=Region(
            x=10,
            y=12,
            width=60,
            height=20,
            units="rendered_page_pixels",
        ),
        question_text_hint="First question prompt",
    )
    match = _HeaderTextMatch(
        overlap_count=1,
        candidate_token_count=1,
        hint_token_count=3,
        candidate_coverage=math.nextafter(1.0, 0.0),
        similarity=0.0,
        ordered_similarity=0.0,
    )

    assert _is_header_fragment_match(match, hint) is True


def test_scans_detect_request_rejects_duplicate_question_numbers(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png")
    with pytest.raises(ValidationError):
        ScansDetectRequest.model_validate(
            {
                "detect_targets": [
                    {
                        "page": {
                            "page_type": "student_scan",
                            "page_number": 1,
                            "image_path": str(page),
                            "student_ref": "scan_001",
                        },
                        "question_hints": [
                            {
                                "question_id": "q1",
                                "question_number": 1,
                                "template_region": {
                                    "x": 10,
                                    "y": 12,
                                    "width": 60,
                                    "height": 20,
                                    "units": "rendered_page_pixels",
                                },
                                "question_text_hint": "First question",
                            },
                            {
                                "question_id": "q2",
                                "question_number": 1,
                                "template_region": {
                                    "x": 10,
                                    "y": 60,
                                    "width": 60,
                                    "height": 20,
                                    "units": "rendered_page_pixels",
                                },
                                "question_text_hint": "Second question",
                            },
                        ],
                    }
                ],
                "output_artifacts_dir": str((tmp_path / "detect_out").resolve()),
            }
        )


def test_scans_detect_request_allows_subpart_labels_with_shared_number(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png")

    request = ScansDetectRequest.model_validate(
        {
            "detect_targets": [
                {
                    "page": {
                        "page_type": "student_scan",
                        "page_number": 1,
                        "image_path": str(page),
                        "student_ref": "scan_001",
                    },
                    "question_hints": [
                        {
                            "question_id": "q1a",
                            "question_number": 1,
                            "question_label": "1a",
                            "template_region": {
                                "x": 10,
                                "y": 12,
                                "width": 60,
                                "height": 20,
                                "units": "rendered_page_pixels",
                            },
                            "question_text_hint": "First subpart",
                        },
                        {
                            "question_id": "q1b",
                            "question_number": 1,
                            "question_label": "1b",
                            "template_region": {
                                "x": 10,
                                "y": 60,
                                "width": 60,
                                "height": 20,
                                "units": "rendered_page_pixels",
                            },
                            "question_text_hint": "Second subpart",
                        },
                    ],
                }
            ],
            "output_artifacts_dir": str((tmp_path / "detect_out").resolve()),
        }
    )

    assert [hint.question_label for hint in request.detect_targets[0].question_hints] == [
        "1a",
        "1b",
    ]


def test_scans_detect_refines_vertical_bounds_and_extends_horizontal_overflow(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(100, 100))
    output_dir = (tmp_path / "detect_out").resolve()

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(
                text="1. First question", left=12, top=8, right=50, bottom=16, confidence=0.99
            ),
            OcrTextBox(
                text="student writing", left=5, top=24, right=92, bottom=44, confidence=0.75
            ),
            OcrTextBox(text="tail writing", left=18, top=58, right=70, bottom=72, confidence=0.65),
            OcrTextBox(
                text="2. Second question", left=12, top=60, right=54, bottom=68, confidence=0.99
            ),
        ],
    )

    result = _runner().run("scans.detect", _detect_request(page=page, output_dir=output_dir))

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    rows = result.envelope.data["detect_results"]
    q1 = rows[0]
    assert q1["status"] == "ok"
    assert q1["region_source"] == "ocr_refined"
    assert q1["region"] == {
        "x": 0,
        "y": 8,
        "width": 100,
        "height": 64,
        "units": "rendered_page_pixels",
    }
    q2 = rows[1]
    assert q2["status"] == "ok"
    assert q2["region"]["y"] == 60
    assert q2["region"]["height"] == 20
    assert (output_dir / "traces" / "detect__page_number-001__student_ref-scan_001.json").exists()


def test_scans_detect_matches_numbered_subpart_labels(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(100, 120))
    output_dir = (tmp_path / "detect_out").resolve()
    payload = _detect_request(page=page, output_dir=output_dir)
    hints = payload["detect_targets"][0]["question_hints"]
    hints[0]["question_id"] = "q1a"
    hints[0]["question_label"] = "1a"
    hints[0]["question_text_hint"] = "First subpart"
    hints[1]["question_id"] = "q1b"
    hints[1]["question_number"] = 1
    hints[1]["question_label"] = "1b"
    hints[1]["question_text_hint"] = "Second subpart"

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(
                text="1a. First subpart", left=12, top=8, right=58, bottom=16, confidence=0.99
            ),
            OcrTextBox(
                text="student writing", left=5, top=24, right=92, bottom=44, confidence=0.75
            ),
            OcrTextBox(
                text="1b. Second subpart", left=12, top=60, right=62, bottom=68, confidence=0.99
            ),
        ],
    )

    result = _runner().run("scans.detect", payload)

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    rows = result.envelope.data["detect_results"]
    assert [(row["question_id"], row["region_source"]) for row in rows] == [
        ("q1a", "ocr_refined"),
        ("q1b", "ocr_refined"),
    ]


def test_scans_detect_falls_back_to_template_region_when_question_boundary_missing(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(100, 100))
    output_dir = (tmp_path / "detect_out").resolve()

    monkeypatch.setattr("scriptscore.commands.scans_detect.read_page_ocr", lambda _path: [])

    result = _runner().run("scans.detect", _detect_request(page=page, output_dir=output_dir))

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    q1 = result.envelope.data["detect_results"][0]
    assert q1["status"] == "warning"
    assert q1["region_source"] == "template_fallback"
    assert q1["region"] == {
        "x": 10,
        "y": 12,
        "width": 60,
        "height": 20,
        "units": "rendered_page_pixels",
    }


def test_scans_detect_does_not_refine_from_nearby_unrelated_ocr_text(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(100, 100))
    output_dir = (tmp_path / "detect_out").resolve()

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(text="scribble", left=12, top=11, right=40, bottom=18, confidence=0.92),
            OcrTextBox(
                text="more answer text", left=14, top=20, right=70, bottom=34, confidence=0.88
            ),
        ],
    )

    result = _runner().run("scans.detect", _detect_request(page=page, output_dir=output_dir))

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    q1 = result.envelope.data["detect_results"][0]
    assert q1["status"] == "warning"
    assert q1["region_source"] == "template_fallback"
    assert q1["region"]["y"] == 12


def test_scans_detect_anchors_top_edge_to_detected_printed_question_text(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(100, 100))
    output_dir = (tmp_path / "detect_out").resolve()

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(
                text="1. First question", left=12, top=24, right=64, bottom=32, confidence=0.99
            ),
            OcrTextBox(
                text="continued writing", left=14, top=40, right=72, bottom=56, confidence=0.88
            ),
            OcrTextBox(
                text="2. Second question", left=12, top=60, right=54, bottom=68, confidence=0.99
            ),
        ],
    )

    result = _runner().run("scans.detect", _detect_request(page=page, output_dir=output_dir))

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    q1 = result.envelope.data["detect_results"][0]
    assert q1["status"] == "ok"
    assert q1["region_source"] == "ocr_refined"
    assert q1["region"]["y"] == 24
    assert q1["region"]["height"] == 36


def test_scans_detect_can_anchor_from_header_text_without_visible_question_number(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(100, 100))
    output_dir = (tmp_path / "detect_out").resolve()
    request = _detect_request(page=page, output_dir=output_dir)
    request["detect_targets"][0]["question_hints"][0]["question_text_hint"] = (
        "Explain transportation improvements during the Market Revolution."
    )

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(
                text="Explain transportation improvements during the Market Revolution",
                left=12,
                top=14,
                right=92,
                bottom=22,
                confidence=0.96,
            ),
            OcrTextBox(
                text="student answer", left=14, top=28, right=74, bottom=42, confidence=0.88
            ),
            OcrTextBox(
                text="2. Second question", left=12, top=60, right=54, bottom=68, confidence=0.99
            ),
        ],
    )

    result = _runner().run("scans.detect", request)

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    q1 = result.envelope.data["detect_results"][0]
    assert q1["status"] == "ok"
    assert q1["region_source"] == "ocr_refined"
    assert q1["region"]["y"] == 14


def test_scans_detect_prefers_numbered_header_over_answer_text_echo(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(100, 100))
    output_dir = (tmp_path / "detect_out").resolve()
    request = _detect_request(page=page, output_dir=output_dir)
    request["detect_targets"][0]["question_hints"][0]["question_text_hint"] = (
        "Explain transportation improvements during the Market Revolution."
    )

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(text="1.", left=12, top=10, right=20, bottom=18, confidence=0.99),
            OcrTextBox(
                text="Explain transportation improvements during the Market Revolution",
                left=12,
                top=42,
                right=92,
                bottom=52,
                confidence=0.96,
            ),
            OcrTextBox(
                text="2. Second question", left=12, top=60, right=54, bottom=68, confidence=0.99
            ),
        ],
    )

    result = _runner().run("scans.detect", request)

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    q1 = result.envelope.data["detect_results"][0]
    assert q1["status"] == "ok"
    assert q1["region"]["y"] == 10


def test_scans_detect_keeps_split_header_number_for_short_hint(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(120, 120))
    output_dir = (tmp_path / "detect_out").resolve()
    request = _detect_request(page=page, output_dir=output_dir)
    request["detect_targets"][0]["question_hints"][1]["question_text_hint"] = "Derivative"
    request["detect_targets"][0]["question_hints"][1]["template_region"]["y"] = 60

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(
                text="1. First question", left=12, top=8, right=50, bottom=16, confidence=0.99
            ),
            OcrTextBox(text="2", left=12, top=118, right=20, bottom=126, confidence=0.91),
            OcrTextBox(text="Derivative", left=24, top=118, right=72, bottom=126, confidence=0.93),
        ],
    )

    result = _runner().run("scans.detect", request)

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    q2 = result.envelope.data["detect_results"][1]
    assert q2["status"] == "ok"
    assert q2["region_source"] == "ocr_refined"
    assert q2["region"]["y"] == 118


def test_scans_detect_stops_at_top_of_full_next_header_band(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(100, 100))
    output_dir = (tmp_path / "detect_out").resolve()
    request = _detect_request(page=page, output_dir=output_dir)
    request["detect_targets"][0]["question_hints"][1]["question_text_hint"] = (
        "Second question followup"
    )

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(
                text="1. First question", left=12, top=8, right=50, bottom=16, confidence=0.99
            ),
            OcrTextBox(
                text="student writing", left=14, top=24, right=72, bottom=40, confidence=0.88
            ),
            OcrTextBox(text="Second", left=12, top=60, right=32, bottom=68, confidence=0.84),
            OcrTextBox(
                text="question followup", left=34, top=72, right=82, bottom=80, confidence=0.91
            ),
        ],
    )

    result = _runner().run("scans.detect", request)

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    q1 = result.envelope.data["detect_results"][0]
    assert q1["status"] == "ok"
    assert q1["region_source"] == "ocr_refined"
    assert q1["region"]["y"] == 8
    assert q1["region"]["height"] == 52


def test_scans_detect_excludes_single_term_answer_text_from_next_header_band(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(120, 120))
    output_dir = (tmp_path / "detect_out").resolve()
    request = _detect_request(page=page, output_dir=output_dir)
    request["detect_targets"][0]["question_hints"][1]["question_text_hint"] = (
        "Second question followup"
    )

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(
                text="1. First question", left=12, top=8, right=50, bottom=16, confidence=0.99
            ),
            OcrTextBox(
                text="student question echo", left=14, top=44, right=84, bottom=56, confidence=0.88
            ),
            OcrTextBox(text="Second", left=12, top=60, right=32, bottom=68, confidence=0.84),
            OcrTextBox(
                text="question followup", left=34, top=72, right=82, bottom=80, confidence=0.91
            ),
        ],
    )

    result = _runner().run("scans.detect", request)

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    q1 = result.envelope.data["detect_results"][0]
    assert q1["status"] == "ok"
    assert q1["region"]["y"] == 8
    assert q1["region"]["height"] == 52


def test_scans_detect_uses_content_anchor_when_printed_current_header_is_obscured(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_003.png", size=(900, 1165))
    output_dir = (tmp_path / "detect_out").resolve()
    request = _detect_request(page=page, output_dir=output_dir)
    request["detect_targets"][0]["page"]["page_number"] = 3
    request["detect_targets"][0]["question_hints"] = [
        {
            "question_id": "q5",
            "question_number": 5,
            "template_region": {
                "x": 0,
                "y": 382,
                "width": 900,
                "height": 341,
                "units": "rendered_page_pixels",
            },
            "question_text_hint": "Describe the causes and goals of the abolitionist movement.",
        },
        {
            "question_id": "q6",
            "question_number": 6,
            "template_region": {
                "x": 0,
                "y": 723,
                "width": 900,
                "height": 442,
                "units": "rendered_page_pixels",
            },
            "question_text_hint": (
                "Explain the significance of the Seneca Falls Convention and the women's "
                "rights movement."
            ),
        },
    ]

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(
                text="5. Describe the causes and goals of the abolitionist movement.",
                left=80,
                top=444,
                right=450,
                bottom=466,
                confidence=0.97,
            ),
            OcrTextBox(
                text="e quality also influenced them. Their main",
                left=118,
                top=616,
                right=780,
                bottom=677,
                confidence=0.84,
            ),
            OcrTextBox(
                text="goal was to end slavery in the United States",
                left=121,
                top=657,
                right=843,
                bottom=723,
                confidence=0.81,
            ),
            OcrTextBox(
                text="Some wanted immediate freedom for enslaved",
                left=117,
                top=709,
                right=823,
                bottom=763,
                confidence=0.84,
            ),
            OcrTextBox(
                text="The Sendln Falls Convenfion focasedon",
                left=142,
                top=817,
                right=728,
                bottom=876,
                confidence=0.91,
            ),
            OcrTextBox(
                text="Women's rights, Ft called tor eguality",
                left=144,
                top=855,
                right=779,
                bottom=934,
                confidence=0.76,
            ),
            OcrTextBox(
                text="includiny voting rights. tt helpeg",
                left=141,
                top=906,
                right=765,
                bottom=975,
                confidence=0.82,
            ),
        ],
    )

    result = _runner().run("scans.detect", request)

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    q5, q6 = result.envelope.data["detect_results"]
    assert q5["status"] == "ok"
    assert q5["region_source"] == "ocr_refined"
    assert q5["region"]["y"] == 444
    assert q5["region"]["height"] == 319
    assert q6["status"] == "ok"
    assert q6["region_source"] == "ocr_refined"
    assert q6["region"]["y"] == 817


def test_scans_detect_content_anchor_prefers_earliest_acceptable_answer_line(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(900, 1165))
    output_dir = (tmp_path / "detect_out").resolve()
    request = _detect_request(page=page, output_dir=output_dir)
    request["detect_targets"][0]["question_hints"] = [
        {
            "question_id": "q1",
            "question_number": 1,
            "template_region": {
                "x": 0,
                "y": 300,
                "width": 900,
                "height": 420,
                "units": "rendered_page_pixels",
            },
            "question_text_hint": (
                "Explain the significance of the Seneca Falls Convention and the women's "
                "rights movement."
            ),
        }
    ]

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(
                text="Seneca Falls focused on equality",
                left=120,
                top=352,
                right=650,
                bottom=398,
                confidence=0.82,
            ),
            OcrTextBox(
                text="The Seneca Falls Convention and women's rights movement were significant",
                left=120,
                top=410,
                right=820,
                bottom=456,
                confidence=0.9,
            ),
        ],
    )

    result = _runner().run("scans.detect", request)

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    q1 = result.envelope.data["detect_results"][0]
    assert q1["status"] == "ok"
    assert q1["region_source"] == "ocr_refined"
    assert q1["region"]["y"] == 352


def test_scans_detect_rejects_single_token_match_for_two_token_hint(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(100, 100))
    output_dir = (tmp_path / "detect_out").resolve()

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(
                text="1. First question", left=12, top=8, right=50, bottom=16, confidence=0.99
            ),
            OcrTextBox(
                text="student question", left=14, top=54, right=74, bottom=66, confidence=0.88
            ),
        ],
    )

    result = _runner().run("scans.detect", _detect_request(page=page, output_dir=output_dir))

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    q2 = result.envelope.data["detect_results"][1]
    assert q2["status"] == "warning"
    assert q2["region_source"] == "template_fallback"
    assert q2["region"]["y"] == 60


def test_scans_detect_excludes_weak_overlapping_fragments_above_header(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(900, 1165))
    output_dir = (tmp_path / "detect_out").resolve()
    request = _detect_request(page=page, output_dir=output_dir)
    request["detect_targets"][0]["question_hints"] = [
        {
            "question_id": "q3",
            "question_number": 3,
            "template_region": {
                "x": 0,
                "y": 450,
                "width": 900,
                "height": 119,
                "units": "rendered_page_pixels",
            },
            "question_text_hint": (
                "Describe whether the sample statement is valid code and explain the relationship "
                "between classes and object instances"
            ),
        },
        {
            "question_id": "q4",
            "question_number": 4,
            "template_region": {
                "x": 0,
                "y": 732,
                "width": 900,
                "height": 300,
                "units": "rendered_page_pixels",
            },
            "question_text_hint": "Draft pseudocode that repeatedly compares an integer variable",
        },
    ]

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(text="Wath", left=524, top=424, right=610, bottom=452, confidence=0.15),
            OcrTextBox(text="hew", left=636, top=424, right=714, bottom=452, confidence=0.92),
            OcrTextBox(
                text=(
                    "3. Describe whether the sample statement is valid code and explain the "
                    "relationship"
                ),
                left=52,
                top=450,
                right=804,
                bottom=476,
                confidence=0.6,
            ),
            OcrTextBox(
                text="between classes and object instances",
                left=53,
                top=475,
                right=409,
                bottom=495,
                confidence=0.97,
            ),
            OcrTextBox(
                text="Draft pseudocode that repeatedly compares an integer variable",
                left=68,
                top=732,
                right=838,
                bottom=759,
                confidence=0.75,
            ),
        ],
    )

    result = _runner().run("scans.detect", request)

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    q3 = result.envelope.data["detect_results"][0]
    assert q3["status"] == "ok"
    assert q3["region"]["y"] == 450


def test_scans_detect_rejects_unsupported_number_only_next_header(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(900, 1165))
    output_dir = (tmp_path / "detect_out").resolve()
    request = _detect_request(page=page, output_dir=output_dir)
    request["detect_targets"][0]["question_hints"] = [
        {
            "question_id": "q3",
            "question_number": 3,
            "template_region": {
                "x": 0,
                "y": 450,
                "width": 900,
                "height": 119,
                "units": "rendered_page_pixels",
            },
            "question_text_hint": "Describe whether the sample statement is valid code",
        },
        {
            "question_id": "q4",
            "question_number": 4,
            "template_region": {
                "x": 0,
                "y": 524,
                "width": 900,
                "height": 300,
                "units": "rendered_page_pixels",
            },
            "question_text_hint": "Draft pseudocode that repeatedly compares an integer variable",
        },
    ]

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(
                text="3. Describe whether the sample statement is valid code",
                left=52,
                top=450,
                right=804,
                bottom=476,
                confidence=0.92,
            ),
            OcrTextBox(text="Object obj", left=49, top=511, right=143, bottom=553, confidence=0.7),
            OcrTextBox(text="4", left=628, top=524, right=652, bottom=548, confidence=0.9),
            OcrTextBox(
                text="student answer continues",
                left=126,
                top=636,
                right=823,
                bottom=707,
                confidence=0.3,
            ),
            OcrTextBox(
                text="Draft pseudocode that repeatedly compares an integer variable",
                left=68,
                top=732,
                right=838,
                bottom=759,
                confidence=0.75,
            ),
        ],
    )

    result = _runner().run("scans.detect", request)

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    q3 = result.envelope.data["detect_results"][0]
    q4 = result.envelope.data["detect_results"][1]
    assert q3["status"] == "ok"
    assert q3["region"]["y"] == 450
    assert q3["region"]["height"] == 282
    assert q4["region"]["y"] == 732


def test_scans_detect_prefers_strong_next_header_text_over_stray_number(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(900, 1165))
    output_dir = (tmp_path / "detect_out").resolve()
    request = _detect_request(page=page, output_dir=output_dir)
    request["detect_targets"][0]["question_hints"] = [
        {
            "question_id": "q3",
            "question_number": 3,
            "template_region": {
                "x": 0,
                "y": 456,
                "width": 900,
                "height": 160,
                "units": "rendered_page_pixels",
            },
            "question_text_hint": "Describe whether the sample statement is valid code",
        },
        {
            "question_id": "q4",
            "question_number": 4,
            "template_region": {
                "x": 0,
                "y": 855,
                "width": 900,
                "height": 310,
                "units": "rendered_page_pixels",
            },
            "question_text_hint": "Draft pseudocode that repeatedly compares an integer variable",
        },
    ]

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(
                text="3. Describe whether the sample statement is valid code",
                left=53,
                top=456,
                right=804,
                bottom=485,
                confidence=0.96,
            ),
            OcrTextBox(text="Object obj", left=53, top=539, right=143, bottom=557, confidence=0.99),
            OcrTextBox(
                text="Draft pseudocode that repeatedly compares an integer variable",
                left=68,
                top=737,
                right=838,
                bottom=762,
                confidence=0.75,
            ),
            OcrTextBox(
                text="Scanner variable scnr to read each guess, provide feedback",
                left=52,
                top=758,
                right=552,
                bottom=782,
                confidence=0.58,
            ),
            OcrTextBox(
                text="4,Et/= S(hf N(-Ji(+ `",
                left=153,
                top=855,
                right=373,
                bottom=892,
                confidence=0.03,
            ),
        ],
    )

    result = _runner().run("scans.detect", request)

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    q3 = result.envelope.data["detect_results"][0]
    q4 = result.envelope.data["detect_results"][1]
    assert q3["status"] == "ok"
    assert q3["region"]["height"] == 281
    assert q4["region"]["y"] == 737


def test_scans_detect_ignores_stray_number_only_box_far_from_expected_header_band(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(100, 100))
    output_dir = (tmp_path / "detect_out").resolve()
    request = _detect_request(page=page, output_dir=output_dir)

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(
                text="1. First question", left=12, top=8, right=50, bottom=16, confidence=0.99
            ),
            OcrTextBox(text="2", left=90, top=0, right=98, bottom=8, confidence=0.82),
            OcrTextBox(
                text="Second question", left=12, top=60, right=70, bottom=68, confidence=0.94
            ),
            OcrTextBox(
                text="student answer", left=14, top=74, right=74, bottom=88, confidence=0.88
            ),
        ],
    )

    result = _runner().run("scans.detect", request)

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    q2 = result.envelope.data["detect_results"][1]
    assert q2["status"] == "ok"
    assert q2["region_source"] == "ocr_refined"
    assert q2["region"]["y"] == 60
    assert q2["region"]["height"] == 28


def test_scans_detect_last_question_does_not_expand_to_page_bottom_without_boundary_overlap(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(120, 120))
    output_dir = (tmp_path / "detect_out").resolve()
    request = _detect_request(page=page, output_dir=output_dir)

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(
                text="1. First question", left=12, top=8, right=50, bottom=16, confidence=0.99
            ),
            OcrTextBox(
                text="2. Second question", left=12, top=60, right=54, bottom=68, confidence=0.99
            ),
            OcrTextBox(text="footer 2026", left=12, top=108, right=60, bottom=116, confidence=0.91),
        ],
    )

    result = _runner().run("scans.detect", request)

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    q2 = result.envelope.data["detect_results"][1]
    assert q2["status"] == "ok"
    assert q2["region_source"] == "ocr_refined"
    assert q2["region"]["y"] == 60
    assert q2["region"]["height"] == 20


def test_scans_detect_uses_supplied_ocr_boxes_without_rescanning(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(100, 100))
    output_dir = (tmp_path / "detect_out").resolve()

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: (_ for _ in ()).throw(
            AssertionError("should not OCR-scan when ocr_boxes were supplied")
        ),
    )
    request = _detect_request(page=page, output_dir=output_dir)
    request["detect_targets"][0]["ocr_boxes"] = [
        {
            "text": "1. First question",
            "left": 12,
            "top": 8,
            "right": 50,
            "bottom": 16,
            "confidence": 0.99,
        },
        {
            "text": "2. Second question",
            "left": 12,
            "top": 60,
            "right": 54,
            "bottom": 68,
            "confidence": 0.99,
        },
    ]

    result = _runner().run("scans.detect", request)

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.data["page_ocr_results"][0]["ocr_source"] == "inline_boxes"
    trace_payload = (
        output_dir / "traces" / "detect__page_number-001__student_ref-scan_001.json"
    ).read_text(encoding="utf-8")
    assert '"ocr_source": "inline_boxes"' in trace_payload


def test_scans_detect_writes_reusable_ocr_metadata_artifact(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(100, 100))
    output_dir = (tmp_path / "detect_out").resolve()

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(
                text="1. First question", left=12, top=8, right=50, bottom=16, confidence=0.99
            ),
            OcrTextBox(
                text="2. Second question", left=12, top=60, right=54, bottom=68, confidence=0.99
            ),
        ],
    )

    result = _runner().run("scans.detect", _detect_request(page=page, output_dir=output_dir))

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    page_ocr = result.envelope.data["page_ocr_results"][0]
    assert page_ocr["ocr_source"] == "generated"
    metadata_path = Path(page_ocr["ocr_metadata_path"])
    assert metadata_path.exists()
    payload = json.loads(metadata_path.read_text(encoding="utf-8"))
    assert payload["page_number"] == 1
    assert len(payload["image_sha256"]) == 64
    assert payload["image_width"] == 100
    assert payload["image_height"] == 100
    assert len(payload["boxes"]) == 2


def test_scans_detect_clamps_generated_ocr_boxes_to_page_bounds(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(100, 100))
    output_dir = (tmp_path / "detect_out").resolve()

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(
                text="1. First question", left=-7, top=-3, right=50, bottom=16, confidence=0.99
            ),
            OcrTextBox(
                text="2. Second question",
                left=12,
                top=60,
                right=140,
                bottom=108,
                confidence=0.99,
            ),
        ],
    )

    result = _runner().run("scans.detect", _detect_request(page=page, output_dir=output_dir))

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    metadata_path = Path(result.envelope.data["page_ocr_results"][0]["ocr_metadata_path"])
    payload = json.loads(metadata_path.read_text(encoding="utf-8"))
    assert payload["boxes"][0]["left"] == 0
    assert payload["boxes"][0]["top"] == 0
    assert payload["boxes"][1]["right"] == 100
    assert payload["boxes"][1]["bottom"] == 100


def test_scans_detect_reuses_supplied_ocr_metadata_artifact_without_rescanning(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(100, 100))
    initial_output_dir = (tmp_path / "detect_initial").resolve()
    retry_output_dir = (tmp_path / "detect_retry").resolve()

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(
                text="1. First question", left=12, top=8, right=50, bottom=16, confidence=0.99
            ),
            OcrTextBox(
                text="2. Second question", left=12, top=60, right=54, bottom=68, confidence=0.99
            ),
        ],
    )
    initial_result = _runner().run(
        "scans.detect", _detect_request(page=page, output_dir=initial_output_dir)
    )
    assert initial_result.exit_code == 0
    assert isinstance(initial_result.envelope, CommandSuccessEnvelope)
    metadata_path = initial_result.envelope.data["page_ocr_results"][0]["ocr_metadata_path"]

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: (_ for _ in ()).throw(
            AssertionError("should not OCR-scan when ocr_metadata_path was supplied")
        ),
    )
    retry_request = _detect_request(page=page, output_dir=retry_output_dir)
    retry_request["detect_targets"][0]["ocr_metadata_path"] = metadata_path

    retry_result = _runner().run("scans.detect", retry_request)

    assert retry_result.exit_code == 0
    assert isinstance(retry_result.envelope, CommandSuccessEnvelope)
    page_ocr = retry_result.envelope.data["page_ocr_results"][0]
    assert page_ocr["ocr_source"] == "artifact"
    assert Path(page_ocr["ocr_metadata_path"]).exists()


def test_scans_detect_reuses_legacy_ocr_metadata_with_negative_coordinates(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(100, 100))
    output_dir = (tmp_path / "detect_out").resolve()
    metadata_path = (tmp_path / "legacy_ocr.json").resolve()
    metadata_path.write_text(
        json.dumps(
            {
                "page_number": 1,
                "image_sha256": file_sha256(page),
                "image_width": 100,
                "image_height": 100,
                "boxes": [
                    {
                        "text": "1. First question",
                        "left": -4,
                        "top": -2,
                        "right": 50,
                        "bottom": 16,
                        "confidence": 0.99,
                    },
                    {
                        "text": "2. Second question",
                        "left": 12,
                        "top": 60,
                        "right": 112,
                        "bottom": 106,
                        "confidence": 0.99,
                    },
                ],
            }
        ),
        encoding="utf-8",
    )

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: (_ for _ in ()).throw(
            AssertionError("should not OCR-scan when legacy ocr_metadata_path was supplied")
        ),
    )
    request = _detect_request(page=page, output_dir=output_dir)
    request["detect_targets"][0]["ocr_metadata_path"] = str(metadata_path)

    result = _runner().run("scans.detect", request)

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    page_ocr = result.envelope.data["page_ocr_results"][0]
    assert page_ocr["ocr_source"] == "artifact"
    rewritten_payload = json.loads(Path(page_ocr["ocr_metadata_path"]).read_text(encoding="utf-8"))
    assert rewritten_payload["boxes"][0]["left"] == 0
    assert rewritten_payload["boxes"][0]["top"] == 0
    assert rewritten_payload["boxes"][1]["right"] == 100
    assert rewritten_payload["boxes"][1]["bottom"] == 100


def test_scans_detect_falls_back_when_ocr_metadata_artifact_is_mismatched(
    tmp_path: Path,
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(100, 100))
    output_dir = (tmp_path / "detect_out").resolve()
    metadata_path = (tmp_path / "stale_ocr.json").resolve()
    metadata_path.write_text(
        json.dumps(
            {
                "page_number": 1,
                "image_sha256": "a" * 64,
                "image_width": 99,
                "image_height": 100,
                "boxes": [],
            }
        ),
        encoding="utf-8",
    )
    request = _detect_request(page=page, output_dir=output_dir)
    request["detect_targets"][0]["ocr_metadata_path"] = str(metadata_path)

    result = _runner().run("scans.detect", request)

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    rows = result.envelope.data["detect_results"]
    assert rows[0]["status"] == "warning"
    assert rows[0]["region_source"] == "template_fallback"
    assert "Supplied OCR metadata could not be reused" in rows[0]["warnings"][0]["message"]
    assert result.envelope.data["page_ocr_results"] == []


def test_scans_detect_results_feed_into_scans_crop(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(100, 100))
    detect_output_dir = (tmp_path / "detect_out").resolve()
    crop_output_dir = (tmp_path / "crop_out").resolve()

    monkeypatch.setattr(
        "scriptscore.commands.scans_detect.read_page_ocr",
        lambda _path: [
            OcrTextBox(
                text="1. First question", left=12, top=8, right=50, bottom=16, confidence=0.99
            ),
            OcrTextBox(
                text="2. Second question", left=12, top=60, right=54, bottom=68, confidence=0.99
            ),
        ],
    )

    detect_result = _runner().run(
        "scans.detect", _detect_request(page=page, output_dir=detect_output_dir)
    )
    assert detect_result.exit_code == 0
    assert isinstance(detect_result.envelope, CommandSuccessEnvelope)

    crop_targets = [
        {
            "question_id": row["question_id"],
            "page_number": row["page_number"],
            "region": row["region"],
        }
        for row in detect_result.envelope.data["detect_results"]
        if row["status"] != "error"
    ]
    crop_result = _runner().run(
        "scans.crop",
        {
            "pages": [
                {
                    "page_type": "student_scan",
                    "page_number": 1,
                    "image_path": str(page),
                    "student_ref": "scan_001",
                }
            ],
            "question_crop_targets": crop_targets,
            "output_artifacts_dir": str(crop_output_dir),
        },
    )

    assert crop_result.exit_code == 0
    assert isinstance(crop_result.envelope, CommandSuccessEnvelope)
    assert len(crop_result.envelope.data["crop_results"]) == 2
