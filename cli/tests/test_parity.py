# SPDX-License-Identifier: AGPL-3.0-only
"""Reusable direct-vs-sidecar parity harness tests."""

from __future__ import annotations

import json
from pathlib import Path

from tests.support.images import make_rgb_page
from tests.support.llm import llm_request_fields
from tests.support.parity import DirectInvocation, ParityInvocation, assert_parity
from tests.support.pdfs import TemplateQuestionSpec, make_student_pdf, make_template_pdf


def _fake_pii_model_root(root: Path) -> Path:
    model_root = (root / "models" / "paddle").resolve()
    for leaf in ("det", "rec"):
        target = model_root / leaf
        target.mkdir(parents=True, exist_ok=True)
        (target / "inference.yml").write_text("test", encoding="utf-8")
        (target / "inference.json").write_text("{}", encoding="utf-8")
    return model_root


def test_smoke_command_matches_between_direct_cli_and_sidecar() -> None:
    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=[
                    "_smoke",
                    "ping",
                    "--options",
                    '{"message":"hello","steps":2}',
                    "--emit-events",
                    "--request-id",
                    "req_parity",
                ]
            ),
            method="smoke.ping",
            params={"message": "hello", "steps": 2},
            request_id="req_parity",
        )
    )
    assert result.direct_exit_code == 0


def test_scans_transform_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png")
    output_dir = (tmp_path / "transform_out").resolve()
    payload = {
        "transform_targets": [
            {
                "page": {
                    "page_type": "student_scan",
                    "page_number": 1,
                    "image_path": str(page),
                    "student_ref": "scan_001",
                },
                "transform": {
                    "rotation": 0.0,
                    "scale": 1.0,
                    "translate_x": 0.0,
                    "translate_y": 0.0,
                },
            }
        ],
        "output_artifacts_dir": str(output_dir),
    }
    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=[
                    "scans",
                    "transform",
                    "--stdin",
                    "--emit-events",
                    "--request-id",
                    "req_transform",
                ],
                stdin_json=payload,
            ),
            method="scans.transform",
            params=payload,
            request_id="req_transform",
        )
    )
    assert result.direct_exit_code == 0


def test_scans_canonicalize_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png")
    template = make_rgb_page(tmp_path / "template" / "page_001.png", size=(24, 24))
    output_dir = (tmp_path / "canonicalize_out").resolve()
    payload = {
        "canonicalize_targets": [
            {
                "page": {
                    "page_type": "student_scan",
                    "page_number": 1,
                    "image_path": str(page),
                    "student_ref": "scan_001",
                },
                "template_page": {
                    "page_type": "template",
                    "page_number": 1,
                    "image_path": str(template),
                },
                "transform": {
                    "rotation": 0.0,
                    "scale": 1.0,
                    "translate_x": 0.0,
                    "translate_y": 0.0,
                },
            }
        ],
        "output_artifacts_dir": str(output_dir),
    }
    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=[
                    "scans",
                    "canonicalize",
                    "--stdin",
                    "--emit-events",
                    "--request-id",
                    "req_canonicalize",
                ],
                stdin_json=payload,
            ),
            method="scans.canonicalize",
            params=payload,
            request_id="req_canonicalize",
        )
    )
    assert result.direct_exit_code == 0


def test_scans_align_auto_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    template = make_rgb_page(tmp_path / "template" / "page_001.png", size=(20, 20))
    student = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(20, 20))
    output_dir = (tmp_path / "align_out").resolve()
    payload = {
        "template_pages": [
            {
                "page_type": "template",
                "page_number": 1,
                "image_path": str(template),
            }
        ],
        "student_pages": [
            {
                "page_type": "student_scan",
                "page_number": 1,
                "image_path": str(student),
                "student_ref": "scan_001",
            }
        ],
        "output_artifacts_dir": str(output_dir),
        "providers": {"alignment_engine": "core_template_match"},
    }
    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=[
                    "scans",
                    "align-auto",
                    "--stdin",
                    "--emit-events",
                    "--request-id",
                    "req_align_auto",
                ],
                stdin_json=payload,
            ),
            method="scans.align-auto",
            params=payload,
            request_id="req_align_auto",
        )
    )
    assert result.direct_exit_code == 0


def test_scans_crop_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(20, 20))
    output_dir = (tmp_path / "crop_out").resolve()
    payload = {
        "pages": [
            {
                "page_type": "student_scan",
                "page_number": 1,
                "image_path": str(page),
                "student_ref": "scan_001",
            }
        ],
        "question_crop_targets": [
            {
                "question_id": "q1",
                "page_number": 1,
                "region": {
                    "x": 0,
                    "y": 0,
                    "width": 10,
                    "height": 8,
                    "units": "rendered_page_pixels",
                },
            }
        ],
        "output_artifacts_dir": str(output_dir),
    }
    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=["scans", "crop", "--stdin", "--emit-events", "--request-id", "req_crop"],
                stdin_json=payload,
            ),
            method="scans.crop",
            params=payload,
            request_id="req_crop",
        )
    )
    assert result.direct_exit_code == 0


def test_scans_pii_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    fixture = (
        Path(__file__).resolve().parent
        / "fixtures"
        / "scans_pii"
        / "positive"
        / "pii-synthetic-name.png"
    ).resolve()
    output_dir = (tmp_path / "pii_out").resolve()
    payload = {
        "students": [
            {
                "student_ref": "scan_001",
                "pii_trigger_words": ["Harper Rivera", "Rivera"],
                "pii_targets": [{"question_id": "q1", "question_crop_path": str(fixture)}],
            }
        ],
        "output_artifacts_dir": str(output_dir),
        "pii_runtime_config": {"paddle_model_dir": str(_fake_pii_model_root(tmp_path))},
    }
    ocr_words = [
        {
            "text": "Harper",
            "confidence": 0.81,
            "left": 290,
            "top": 128,
            "right": 420,
            "bottom": 188,
        },
        {
            "text": "Rivera",
            "confidence": 0.79,
            "left": 700,
            "top": 118,
            "right": 840,
            "bottom": 190,
        },
    ]

    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=["scans", "pii", "--stdin", "--emit-events", "--request-id", "req_pii"],
                stdin_json=payload,
            ),
            method="scans.pii",
            params=payload,
            request_id="req_pii",
            env_overrides={"SCRIPTSCORE_TEST_PII_OCR_WORDS": json.dumps(ocr_words)},
        )
    )
    assert result.direct_exit_code == 0


def test_scans_pdf_render_page_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "scan_preview.pdf", page_texts=["preview text"])
    payload = {"pdf_path": str(pdf), "page_number": 1, "zoom": 2.0, "max_width_px": 1600}

    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=[
                    "scans",
                    "pdf-render-page",
                    "--stdin",
                    "--request-id",
                    "req_pdf_render",
                ],
                stdin_json=payload,
            ),
            method="scans.pdf-render-page",
            params=payload,
            request_id="req_pdf_render",
        )
    )

    assert result.direct_exit_code == 0


def test_scans_pdf_create_redacted_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "parity_redact.pdf", page_texts=["line a", "line b"])
    out = (tmp_path / "redacted_out.pdf").resolve()
    payload = {
        "input_pdf_path": str(pdf.resolve()),
        "output_pdf_path": str(out),
        "regions": [{"page_number": 1, "x": 10, "y": 10, "width": 200, "height": 40}],
        "raster_sizes_by_page": {
            1: {"width_px": 612, "height_px": 792},
            2: {"width_px": 612, "height_px": 792},
        },
    }

    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=[
                    "scans",
                    "pdf-create-redacted",
                    "--stdin",
                    "--emit-events",
                    "--request-id",
                    "req_pdf_redact",
                ],
                stdin_json=payload,
            ),
            method="scans.pdf-create-redacted",
            params=payload,
            request_id="req_pdf_redact",
        )
    )

    assert result.direct_exit_code == 0


def test_scans_pdf_detect_aruco_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "detect_aruco.pdf", page_texts=["plain"])
    payload = {"pdf_path": str(pdf)}

    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=[
                    "scans",
                    "pdf-detect-aruco",
                    "--stdin",
                    "--emit-events",
                    "--request-id",
                    "req_pdf_detect_aruco",
                ],
                stdin_json=payload,
            ),
            method="scans.pdf-detect-aruco",
            params=payload,
            request_id="req_pdf_detect_aruco",
        )
    )

    assert result.direct_exit_code == 0


def test_scans_pdf_stamp_aruco_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "stamp_aruco.pdf", page_texts=["plain"])
    output_dir = (tmp_path / "stamp_aruco_out").resolve()
    payload = {"input_pdf_path": str(pdf), "output_artifacts_dir": str(output_dir)}

    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=[
                    "scans",
                    "pdf-stamp-aruco",
                    "--stdin",
                    "--emit-events",
                    "--request-id",
                    "req_pdf_stamp_aruco",
                ],
                stdin_json=payload,
            ),
            method="scans.pdf-stamp-aruco",
            params=payload,
            request_id="req_pdf_stamp_aruco",
        )
    )

    assert result.direct_exit_code == 0


def test_scans_detect_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(20, 20))
    output_dir = (tmp_path / "detect_out").resolve()
    ocr_boxes = [
        {
            "text": "Question 1",
            "left": 0,
            "top": 0,
            "right": 11,
            "bottom": 7,
            "confidence": 0.99,
        }
    ]
    payload = {
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
                            "x": 0,
                            "y": 0,
                            "width": 10,
                            "height": 8,
                            "units": "rendered_page_pixels",
                        },
                        "question_text_hint": "Question one",
                    }
                ],
            }
        ],
        "output_artifacts_dir": str(output_dir),
    }
    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=["scans", "detect", "--stdin", "--emit-events", "--request-id", "req_detect"],
                stdin_json=payload,
            ),
            method="scans.detect",
            params=payload,
            request_id="req_detect",
            env_overrides={"SCRIPTSCORE_TEST_EASYOCR_BOXES": json.dumps(ocr_boxes)},
        )
    )
    assert result.direct_exit_code == 0


def test_scans_ingest_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "scan_001.pdf", page_texts=["page 1", "page 2"])
    output_dir = (tmp_path / "ingest_out").resolve()
    ocr_boxes = [
        {
            "text": "page one text",
            "left": 12,
            "top": 16,
            "right": 96,
            "bottom": 28,
            "confidence": 0.98,
        }
    ]
    payload = {
        "pdf_targets": [{"student_ref": "scan_001", "pdf_path": str(pdf)}],
        "output_artifacts_dir": str(output_dir),
    }
    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=["scans", "ingest", "--stdin", "--emit-events", "--request-id", "req_ingest"],
                stdin_json=payload,
            ),
            method="scans.ingest",
            params=payload,
            request_id="req_ingest",
            env_overrides={"SCRIPTSCORE_TEST_EASYOCR_BOXES": json.dumps(ocr_boxes)},
        )
    )
    assert result.direct_exit_code == 0


def test_scans_ingest_with_page_order_matches_between_direct_cli_and_sidecar(
    tmp_path: Path,
) -> None:
    pdf = make_student_pdf(tmp_path / "scan_001.pdf", page_texts=["page 1", "page 2", "page 3"])
    output_dir = (tmp_path / "ingest_out_ordered").resolve()
    payload = {
        "pdf_targets": [
            {
                "student_ref": "scan_001",
                "pdf_path": str(pdf),
                "page_order": [3, 1],
            }
        ],
        "output_artifacts_dir": str(output_dir),
    }
    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=[
                    "scans",
                    "ingest",
                    "--stdin",
                    "--emit-events",
                    "--request-id",
                    "req_ingest_ordered",
                ],
                stdin_json=payload,
            ),
            method="scans.ingest",
            params=payload,
            request_id="req_ingest_ordered",
        )
    )
    assert result.direct_exit_code == 0


def test_scans_parse_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png", size=(20, 20))
    crop_image = crop
    from tests.support.images import put_pixel

    put_pixel(crop_image, xy=(0, 0), color=(0, 0, 0))
    template = make_rgb_page(tmp_path / "template" / "q1.png", size=(20, 20))
    output_dir = (tmp_path / "parse_out").resolve()
    payload = {
        "parse_targets": [
            {
                "student_ref": "scan_001",
                "question_id": "q1",
                "parse_question_context": {
                    "question_number": 1,
                    "question_text_clean": "Explain the result.",
                },
                "question_crop_path": str(crop_image),
                "template_question_png_path": str(template),
            }
        ],
        "output_artifacts_dir": str(output_dir),
        **llm_request_fields("ollama_native"),
    }
    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=["scans", "parse", "--stdin", "--emit-events", "--request-id", "req_parse"],
                stdin_json=payload,
            ),
            method="scans.parse",
            params=payload,
            request_id="req_parse",
        )
    )
    assert result.direct_exit_code == 0


def test_exam_setup_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    template_pdf = make_template_pdf(
        tmp_path / "template.pdf",
        questions=[
            TemplateQuestionSpec(number=1, text="Compute 2 + 2.", points=4, y=120),
            TemplateQuestionSpec(number=2, text="Name one loop.", points=2, y=260),
        ],
    )
    output_dir = (tmp_path / "setup_out").resolve()
    payload = {
        "template_pdf_path": str(template_pdf),
        "output_artifacts_dir": str(output_dir),
    }
    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=["exam", "setup", "--stdin", "--emit-events", "--request-id", "req_setup"],
                stdin_json=payload,
            ),
            method="exam.setup",
            params=payload,
            request_id="req_setup",
        )
    )
    assert result.direct_exit_code == 0


def test_exam_analyze_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    template = make_rgb_page(tmp_path / "template" / "q1.png", size=(20, 20))
    output_dir = (tmp_path / "analyze_out").resolve()
    payload = {
        "question_targets": [
            {
                "question_id": "q1",
                "template_question_png_path": str(template),
                "baseline_pdf_text": "1. Explain the output.",
            }
        ],
        "output_artifacts_dir": str(output_dir),
        **llm_request_fields("ollama_native"),
    }
    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=["exam", "analyze", "--stdin", "--emit-events", "--request-id", "req_analyze"],
                stdin_json=payload,
            ),
            method="exam.analyze",
            params=payload,
            request_id="req_analyze",
        )
    )
    assert result.direct_exit_code == 0


def test_exam_generate_rubric_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    output_dir = (tmp_path / "rubric_out").resolve()
    payload = {
        "question_id": "q1",
        "max_points": 3,
        "subject": "python",
        "question_text_clean": "Explain list slicing.",
        "question_context": "",
        "instructor_profile": {
            "grading_strictness": "balanced",
            "syntax_leniency": "medium",
            "ocr_tolerance": "medium",
            "partial_credit_style": "balanced",
            "feedback_style": "brief",
            "additional_guidance": None,
        },
        "output_artifacts_dir": str(output_dir),
        **llm_request_fields("ollama_native"),
    }
    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=[
                    "exam",
                    "generate-rubric",
                    "--stdin",
                    "--emit-events",
                    "--request-id",
                    "req_rubric",
                ],
                stdin_json=payload,
            ),
            method="exam.generate-rubric",
            params=payload,
            request_id="req_rubric",
        )
    )
    assert result.direct_exit_code == 0


def test_grading_score_preliminary_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    output_dir = (tmp_path / "preliminary_out").resolve()
    payload = {
        "score_requests": [
            {
                "student_ref": "scan_001",
                "question_id": "q1",
                "subject": "Python",
                "student_answer": "return xs[1:]",
                "question_text_clean": "Explain list slicing.",
                "question_context": "",
                "rubric_criterion": {
                    "criterion_index": 0,
                    "label": "Criterion 0",
                    "points": 1,
                    "partial_credit_guidance": "Award between 0 and 1 points.",
                },
                "instructor_profile": {
                    "grading_strictness": "balanced",
                    "syntax_leniency": "medium",
                    "ocr_tolerance": "medium",
                    "partial_credit_style": "balanced",
                    "feedback_style": "brief",
                    "additional_guidance": None,
                },
            }
        ],
        "grading_runtime_config": {"max_workers": 1},
        "output_artifacts_dir": str(output_dir),
        **llm_request_fields("ollama_native"),
    }
    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=[
                    "grading",
                    "score-preliminary",
                    "--stdin",
                    "--emit-events",
                    "--request-id",
                    "req_prelim",
                ],
                stdin_json=payload,
            ),
            method="grading.score-preliminary",
            params=payload,
            request_id="req_prelim",
        )
    )
    assert result.direct_exit_code == 0


def test_grading_run_consistency_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    output_dir = (tmp_path / "consistency_out").resolve()
    payload = {
        "consistency_requests": [
            {
                "question_id": "q1",
                "subject": "Python",
                "question_text_clean": "Explain list slicing.",
                "question_context": "",
                "rubric_criterion": {
                    "criterion_index": 0,
                    "label": "Criterion 0",
                    "points": 1,
                    "partial_credit_guidance": "Award between 0 and 1 points.",
                },
                "instructor_profile": {
                    "grading_strictness": "balanced",
                    "syntax_leniency": "medium",
                    "ocr_tolerance": "medium",
                    "partial_credit_style": "balanced",
                    "feedback_style": "brief",
                    "additional_guidance": None,
                },
                "student_scores": [
                    {
                        "student_ref": "scan_001",
                        "student_answer": "return xs[1:]",
                        "blank": False,
                        "preliminary_points_awarded": 1,
                        "preliminary_rationale": "Looks fine.",
                        "preliminary_status": "ok",
                        "warnings": [],
                    }
                ],
            }
        ],
        "output_artifacts_dir": str(output_dir),
        **llm_request_fields("ollama_native"),
    }
    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=[
                    "grading",
                    "run-consistency",
                    "--stdin",
                    "--emit-events",
                    "--request-id",
                    "req_consistency",
                ],
                stdin_json=payload,
            ),
            method="grading.run-consistency",
            params=payload,
            request_id="req_consistency",
        )
    )
    assert result.direct_exit_code == 0


def test_grading_draft_feedback_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    output_dir = (tmp_path / "feedback_out").resolve()
    payload = {
        "feedback_requests": [
            {
                "student_ref": "scan_001",
                "question_id": "q1",
                "subject": "Python",
                "total_points_awarded": 1,
                "question_max_points": 1,
                "student_answer": "return xs[1:]",
                "question_text_clean": "Explain list slicing.",
                "question_context": "",
                "rubric_criteria": [
                    {
                        "criterion_index": 0,
                        "label": "Criterion 0",
                        "points": 1,
                        "partial_credit_guidance": "Award between 0 and 1 points.",
                    }
                ],
                "criterion_results": [
                    {"criterion_index": 0, "points_awarded": 1, "rationale": "Looks fine."}
                ],
            }
        ],
        "output_artifacts_dir": str(output_dir),
        **llm_request_fields("ollama_native"),
    }
    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=[
                    "grading",
                    "draft-feedback",
                    "--stdin",
                    "--emit-events",
                    "--request-id",
                    "req_feedback",
                ],
                stdin_json=payload,
            ),
            method="grading.draft-feedback",
            params=payload,
            request_id="req_feedback",
        )
    )
    assert result.direct_exit_code == 0


def test_grading_markup_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    output_dir = (tmp_path / "markup_out").resolve()
    payload = {
        "markup_requests": [
            {
                "student_ref": "scan_001",
                "question_id": "q1",
                "subject": "Python",
                "total_points_awarded": 1,
                "question_max_points": 1,
                "student_answer": "return xs[1:]",
                "question_text_clean": "Explain list slicing.",
                "question_context": "",
                "rubric_criteria": [
                    {
                        "criterion_index": 0,
                        "label": "Criterion 0",
                        "points": 1,
                        "partial_credit_guidance": "Award between 0 and 1 points.",
                    }
                ],
                "criterion_results": [
                    {"criterion_index": 0, "points_awarded": 1, "rationale": "Looks fine."}
                ],
            }
        ],
        "output_artifacts_dir": str(output_dir),
        **llm_request_fields("ollama_native"),
    }
    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=[
                    "grading",
                    "markup",
                    "--stdin",
                    "--emit-events",
                    "--request-id",
                    "req_markup",
                ],
                stdin_json=payload,
            ),
            method="grading.markup",
            params=payload,
            request_id="req_markup",
        )
    )
    assert result.direct_exit_code == 0


def test_grading_export_matches_between_direct_cli_and_sidecar(tmp_path: Path) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png", size=(20, 20))
    output_dir = (tmp_path / "export_out").resolve()
    payload = {
        "export_requests": [
            {
                "student_ref": "scan_001",
                "student_display_name": "Ada Lovelace",
                "questions": [
                    {
                        "question_id": "q1",
                        "question_max_points": 1,
                        "total_points_awarded": 1,
                        "question_text_clean": "Explain list slicing.",
                        "student_answer": "return xs[1:]",
                        "question_crop_path": str(crop),
                        "feedback_text": "Strong work overall.",
                        "highlights": [
                            {"kind": "correct", "start_char": 0, "end_char": 6, "text": "return"}
                        ],
                    }
                ],
            }
        ],
        "output_artifacts_dir": str(output_dir),
    }
    result = assert_parity(
        ParityInvocation(
            direct=DirectInvocation(
                args=[
                    "grading",
                    "export",
                    "--stdin",
                    "--emit-events",
                    "--request-id",
                    "req_export",
                ],
                stdin_json=payload,
            ),
            method="grading.export",
            params=payload,
            request_id="req_export",
        )
    )
    assert result.direct_exit_code == 0
