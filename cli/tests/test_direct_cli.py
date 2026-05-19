# SPDX-License-Identifier: AGPL-3.0-only
"""Direct CLI smoke-path tests."""

from __future__ import annotations

import json
from pathlib import Path

from PIL import Image

from tests.support.images import make_rgb_page, put_pixel
from tests.support.pdfs import TemplateQuestionSpec, make_student_pdf, make_template_pdf
from tests.support.process import run_direct_cli


def test_smoke_ping_via_options_returns_success_envelope() -> None:
    result = run_direct_cli(["_smoke", "ping", "--options", '{"message":"hello","steps":1}'])
    assert result.returncode == 0
    assert result.stdout_lines[-1]["ok"] is True
    assert result.stdout_lines[-1]["command"] == "smoke.ping"
    assert result.stdout_lines[-1]["data"]["message"] == "hello"


def test_smoke_ping_wait_for_file_via_options_returns_success_envelope(tmp_path: Path) -> None:
    marker = tmp_path / "release-smoke-ping"
    marker.write_text("release", encoding="utf-8")
    payload = {"message": "hello", "steps": 1, "wait_for_file": str(marker)}

    result = run_direct_cli(["_smoke", "ping", "--options", json.dumps(payload)])

    assert result.returncode == 0
    assert result.stdout_lines[-1]["ok"] is True
    assert result.stdout_lines[-1]["command"] == "smoke.ping"
    assert result.stdout_lines[-1]["data"]["message"] == "hello"


def test_smoke_ping_via_stdin_supports_progress_events() -> None:
    result = run_direct_cli(
        ["_smoke", "ping", "--stdin", "--emit-events", "--request-id", "req_123"],
        stdin_json={"message": "hello", "steps": 2},
    )
    assert result.returncode == 0
    assert result.stdout_lines[0]["type"] == "event"
    assert result.stdout_lines[-1]["request_id"] == "req_123"


def test_scans_transform_via_options_returns_success_envelope(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png")
    put_pixel(page, xy=(0, 0), color=(0, 0, 0))
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
                    "translate_x": 1.0,
                    "translate_y": 0.0,
                },
            }
        ],
        "output_artifacts_dir": str(output_dir),
    }

    result = run_direct_cli(["scans", "transform", "--options", json.dumps(payload)])
    assert result.returncode == 0
    assert result.stdout_lines[-1]["command"] == "scans.transform"
    output_path = Path(
        result.stdout_lines[-1]["data"]["transform_results"][0]["output_page"]["image_path"]
    )
    with Image.open(output_path) as image:
        assert image.getpixel((1, 0)) == (0, 0, 0)


def test_scans_canonicalize_via_options_returns_success_envelope(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(12, 12))
    template = make_rgb_page(tmp_path / "template" / "page_001.png", size=(20, 24))
    put_pixel(page, xy=(0, 0), color=(0, 0, 0))
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

    result = run_direct_cli(["scans", "canonicalize", "--options", json.dumps(payload)])
    assert result.returncode == 0
    assert result.stdout_lines[-1]["command"] == "scans.canonicalize"
    output_path = Path(
        result.stdout_lines[-1]["data"]["canonicalize_results"][0]["output_page"]["image_path"]
    )
    with Image.open(output_path) as image:
        assert image.size == (20, 24)
        assert image.getpixel((0, 0)) == (0, 0, 0)


def test_scans_crop_via_stdin_supports_progress_events(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(20, 20))
    output_dir = (tmp_path / "crop_out").resolve()

    result = run_direct_cli(
        ["scans", "crop", "--stdin", "--emit-events", "--request-id", "req_crop"],
        stdin_json={
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
        },
    )
    assert result.returncode == 0
    assert result.stdout_lines[0]["type"] == "event"
    assert result.stdout_lines[-1]["command"] == "scans.crop"
    assert result.stdout_lines[-1]["request_id"] == "req_crop"


def test_scans_ingest_via_options_returns_success_envelope(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "scan_001.pdf", page_texts=["first page", "second page"])
    output_dir = (tmp_path / "ingest_out").resolve()
    payload = {
        "pdf_targets": [{"student_ref": "scan_001", "pdf_path": str(pdf)}],
        "output_artifacts_dir": str(output_dir),
    }

    result = run_direct_cli(["scans", "ingest", "--options", json.dumps(payload)])
    assert result.returncode == 0
    assert result.stdout_lines[-1]["command"] == "scans.ingest"
    assert len(result.stdout_lines[-1]["data"]["pdf_results"][0]["pages"]) == 2


def test_exam_setup_via_options_returns_success_envelope(tmp_path: Path) -> None:
    template_pdf = make_template_pdf(
        tmp_path / "template.pdf",
        questions=[TemplateQuestionSpec(number=1, text="Compute 2 + 2.", points=4)],
    )
    output_dir = (tmp_path / "setup_out").resolve()
    payload = {
        "template_pdf_path": str(template_pdf),
        "output_artifacts_dir": str(output_dir),
    }

    result = run_direct_cli(["exam", "setup", "--options", json.dumps(payload)])
    assert result.returncode == 0
    assert result.stdout_lines[-1]["command"] == "exam.setup"
    assert result.stdout_lines[-1]["data"]["questions"][0]["question_id"] == "q1"


def test_direct_cli_malformed_json_returns_structured_validation_envelope() -> None:
    result = run_direct_cli(["scans", "transform", "--options", "{not-json"])
    assert result.returncode == 2
    assert result.stdout_lines[-1]["ok"] is False
    assert result.stdout_lines[-1]["error"]["code"] == "validation_failed"
    assert result.stdout_lines[-1]["error"]["category"] == "validation"


def test_direct_cli_non_object_json_reaches_runner_validation() -> None:
    result = run_direct_cli(["scans", "transform", "--options", "[]"])
    assert result.returncode == 2
    assert result.stdout_lines[-1]["ok"] is False
    assert result.stdout_lines[-1]["error"]["code"] == "validation_failed"
    assert result.stdout_lines[-1]["error"]["category"] == "validation"
