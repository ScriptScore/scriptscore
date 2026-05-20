# SPDX-License-Identifier: AGPL-3.0-only
"""Unit and integration-style tests for Phase 2 scan commands."""

from __future__ import annotations

import json
import math
from collections.abc import Callable
from pathlib import Path

import pytest
from PIL import Image, ImageChops, ImageDraw
from pydantic import ValidationError

from scriptscore.artifacts import images as image_artifacts
from scriptscore.artifacts.images import apply_manual_transform as base_apply_manual_transform
from scriptscore.commands import build_command_registry
from scriptscore.contracts import (
    CommandErrorEnvelope,
    CommandSuccessEnvelope,
    ParseQuestionContext,
    ScansCanonicalizeRequest,
    ScansCropRequest,
    ScansIngestRequest,
    ScansParseRequest,
    ScansTransformRequest,
    Transform,
)
from scriptscore.providers import FakeLlmProvider, LlmRequest, LlmResponse, ProviderRegistry
from scriptscore.runtime import CommandRunner
from tests.support.images import make_rgb_page, put_pixel
from tests.support.llm import llm_request_fields
from tests.support.pdfs import TemplateQuestionSpec, make_student_pdf, make_template_pdf


def _runner(*, provider_registry: ProviderRegistry | None = None) -> CommandRunner:
    return CommandRunner(
        registry=build_command_registry(),
        provider_registry=provider_registry or ProviderRegistry.with_builtin_fakes(),
    )


def _registry_with_llm(
    responder: Callable[[LlmRequest], LlmResponse],
    *,
    provider_name: str = "ollama_native",
) -> ProviderRegistry:
    registry = ProviderRegistry.with_builtin_fakes()
    registry.register(FakeLlmProvider(provider_name=provider_name, responder=responder))
    return registry


def test_transformed_image_treats_near_identity_scale_as_identity(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    image = Image.new("RGB", (12, 10), (255, 255, 255))

    def _unexpected_resize(*_args: object, **_kwargs: object) -> Image.Image:
        raise AssertionError("near-identity scale should not resize the transformed image")

    monkeypatch.setattr(Image.Image, "resize", _unexpected_resize)

    transformed, _mode, _fill = image_artifacts._transformed_image(
        image,
        Transform(
            rotation=0.0,
            scale=math.nextafter(1.0, 2.0),
            translate_x=0.0,
            translate_y=0.0,
        ),
    )

    assert transformed.size == image.size


def test_scans_transform_request_rejects_duplicate_page_targets(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png")
    with pytest.raises(ValidationError):
        ScansTransformRequest.model_validate(
            {
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
                    },
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
                    },
                ],
                "output_artifacts_dir": str((tmp_path / "out").resolve()),
            }
        )


def test_scans_crop_request_rejects_duplicate_student_page_numbers(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png")
    with pytest.raises(ValidationError):
        ScansCropRequest.model_validate(
            {
                "pages": [
                    {
                        "page_type": "student_scan",
                        "page_number": 1,
                        "image_path": str(page),
                        "student_ref": "scan_001",
                    },
                    {
                        "page_type": "student_scan",
                        "page_number": 1,
                        "image_path": str(page),
                        "student_ref": "scan_001",
                    },
                ],
                "question_crop_targets": [
                    {
                        "question_id": "q1",
                        "page_number": 1,
                        "region": {
                            "x": 0,
                            "y": 0,
                            "width": 5,
                            "height": 5,
                            "units": "rendered_page_pixels",
                        },
                    }
                ],
                "output_artifacts_dir": str((tmp_path / "out").resolve()),
            }
        )


def test_scans_transform_request_rejects_student_ref_path_traversal(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png")
    with pytest.raises(ValidationError):
        ScansTransformRequest.model_validate(
            {
                "transform_targets": [
                    {
                        "page": {
                            "page_type": "student_scan",
                            "page_number": 1,
                            "image_path": str(page),
                            "student_ref": "../outside",
                        },
                        "transform": {
                            "rotation": 0.0,
                            "scale": 1.0,
                            "translate_x": 0.0,
                            "translate_y": 0.0,
                        },
                    }
                ],
                "output_artifacts_dir": str((tmp_path / "out").resolve()),
            }
        )


def test_scans_transform_request_rejects_empty_student_ref(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png")
    with pytest.raises(ValidationError):
        ScansTransformRequest.model_validate(
            {
                "transform_targets": [
                    {
                        "page": {
                            "page_type": "student_scan",
                            "page_number": 1,
                            "image_path": str(page),
                            "student_ref": "",
                        },
                        "transform": {
                            "rotation": 0.0,
                            "scale": 1.0,
                            "translate_x": 0.0,
                            "translate_y": 0.0,
                        },
                    }
                ],
                "output_artifacts_dir": str((tmp_path / "out").resolve()),
            }
        )


def test_scans_canonicalize_request_rejects_duplicate_page_targets(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png")
    template = make_rgb_page(tmp_path / "template" / "page_001.png")
    with pytest.raises(ValidationError):
        ScansCanonicalizeRequest.model_validate(
            {
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
                    },
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
                            "translate_x": 1.0,
                            "translate_y": 0.0,
                        },
                    },
                ],
                "output_artifacts_dir": str((tmp_path / "out").resolve()),
            }
        )


def test_scans_crop_request_rejects_question_id_path_traversal(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png")
    with pytest.raises(ValidationError):
        ScansCropRequest.model_validate(
            {
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
                        "question_id": "../../escape",
                        "page_number": 1,
                        "region": {
                            "x": 0,
                            "y": 0,
                            "width": 5,
                            "height": 5,
                            "units": "rendered_page_pixels",
                        },
                    }
                ],
                "output_artifacts_dir": str((tmp_path / "out").resolve()),
            }
        )


def test_scans_crop_request_rejects_empty_student_ref(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png")
    with pytest.raises(ValidationError):
        ScansCropRequest.model_validate(
            {
                "pages": [
                    {
                        "page_type": "student_scan",
                        "page_number": 1,
                        "image_path": str(page),
                        "student_ref": "",
                    }
                ],
                "question_crop_targets": [
                    {
                        "question_id": "q1",
                        "page_number": 1,
                        "region": {
                            "x": 0,
                            "y": 0,
                            "width": 5,
                            "height": 5,
                            "units": "rendered_page_pixels",
                        },
                    }
                ],
                "output_artifacts_dir": str((tmp_path / "out").resolve()),
            }
        )


def test_scans_transform_shifts_pixel_and_writes_manifest(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png")
    put_pixel(page, xy=(0, 0), color=(0, 0, 0))
    output_dir = (tmp_path / "transform_out").resolve()

    result = _runner().run(
        "scans.transform",
        {
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
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["transform_results"][0]
    assert row["status"] == "ok"
    output_page = Path(row["output_page"]["image_path"])
    assert output_page.exists()
    assert not row["output_page"].get("student_ref")
    with Image.open(output_page) as image:
        assert image.getpixel((0, 0)) == (255, 255, 255)
        assert image.getpixel((1, 0)) == (0, 0, 0)
    manifest = json.loads((output_dir / "output_metadata.json").read_text(encoding="utf-8"))
    assert "transform_results" not in manifest["data"]
    assert manifest["data"] == {
        "failed_count": 0,
        "result_row_count": 1,
        "written_artifact_count": 1,
    }


def test_scans_transform_runtime_failure_is_row_local_degraded(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    page_one = make_rgb_page(tmp_path / "scan_001" / "page_001.png")
    page_two = make_rgb_page(tmp_path / "scan_002" / "page_001.png")
    output_dir = (tmp_path / "transform_out").resolve()

    call_state = {"count": 0}

    def _failing_transform(
        image: Image.Image,
        transform: Transform,
    ) -> Image.Image:
        if image.size == (12, 12):
            call_state["count"] += 1
        if call_state["count"] == 2:
            raise RuntimeError("synthetic transform failure")
        return base_apply_manual_transform(image, transform)

    monkeypatch.setattr(
        "scriptscore.commands.scans_transform.apply_manual_transform", _failing_transform
    )

    result = _runner().run(
        "scans.transform",
        {
            "transform_targets": [
                {
                    "page": {
                        "page_type": "student_scan",
                        "page_number": 1,
                        "image_path": str(page_one),
                        "student_ref": "scan_001",
                    },
                    "transform": {
                        "rotation": 0.0,
                        "scale": 1.0,
                        "translate_x": 0.0,
                        "translate_y": 0.0,
                    },
                },
                {
                    "page": {
                        "page_type": "student_scan",
                        "page_number": 1,
                        "image_path": str(page_two),
                        "student_ref": "scan_002",
                    },
                    "transform": {
                        "rotation": 0.0,
                        "scale": 1.0,
                        "translate_x": 0.0,
                        "translate_y": 0.0,
                    },
                },
            ],
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    rows = result.envelope.data["transform_results"]
    assert [row["status"] for row in rows] == ["ok", "error"]
    assert rows[1]["warnings"][0]["code"] == "transform_failed"
    assert Path(rows[0]["output_page"]["image_path"]).exists()
    assert not Path(rows[1]["output_page"]["image_path"]).exists()


def test_scans_transform_missing_source_page_hard_fails_without_writes(tmp_path: Path) -> None:
    output_dir = (tmp_path / "transform_out").resolve()
    missing = (tmp_path / "scan_001" / "page_001.png").resolve()

    result = _runner().run(
        "scans.transform",
        {
            "transform_targets": [
                {
                    "page": {
                        "page_type": "student_scan",
                        "page_number": 1,
                        "image_path": str(missing),
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
        },
    )

    assert result.exit_code == 3
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert not (output_dir / "output_metadata.json").exists()


def test_scans_canonicalize_writes_template_sized_output_and_manifest(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(12, 12))
    template = make_rgb_page(tmp_path / "template" / "page_001.png", size=(20, 24))
    put_pixel(page, xy=(0, 0), color=(0, 0, 0))
    output_dir = (tmp_path / "canonicalize_out").resolve()

    result = _runner().run(
        "scans.canonicalize",
        {
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
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["canonicalize_results"][0]
    assert row["status"] == "ok"
    output_page = Path(row["output_page"]["image_path"])
    assert output_page.exists()
    assert not row["output_page"].get("student_ref")
    with Image.open(output_page) as image:
        assert image.size == (20, 24)
        assert image.getpixel((0, 0)) == (0, 0, 0)
    manifest = json.loads((output_dir / "output_metadata.json").read_text(encoding="utf-8"))
    assert manifest["data"] == {
        "failed_count": 0,
        "result_row_count": 1,
        "written_artifact_count": 1,
    }


def test_scans_canonicalize_adjusts_transform_that_clips_visible_content(
    tmp_path: Path,
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(90, 120))
    with Image.open(page) as image:
        updated = image.copy()
    draw = ImageDraw.Draw(updated)
    draw.rectangle((10, 5, 70, 20), fill=(0, 0, 0))
    updated.save(page, format="PNG")
    template = make_rgb_page(tmp_path / "template" / "page_001.png", size=(90, 120))
    output_dir = (tmp_path / "canonicalize_out").resolve()

    result = _runner().run(
        "scans.canonicalize",
        {
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
                        "translate_y": -30.0,
                    },
                }
            ],
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is False
    row = result.envelope.data["canonicalize_results"][0]
    assert row["status"] == "ok"
    assert row["transform"]["translate_y"] == -5.0
    assert row["warnings"][0]["code"] == "canonicalize_transform_clips_content"
    assert row["warnings"][0]["scope"]["top_px"] > 16
    assert row["warnings"][0]["scope"]["adjusted_translate_y"] == -5.0
    output_path = Path(row["output_page"]["image_path"])
    assert output_path.exists()
    with Image.open(output_path) as image:
        assert image.getbbox() is not None


def test_scans_canonicalize_allows_transform_that_only_discards_blank_margin(
    tmp_path: Path,
) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(90, 120))
    with Image.open(page) as image:
        updated = image.copy()
    draw = ImageDraw.Draw(updated)
    draw.rectangle((10, 40, 70, 55), fill=(0, 0, 0))
    updated.save(page, format="PNG")
    template = make_rgb_page(tmp_path / "template" / "page_001.png", size=(90, 120))
    output_dir = (tmp_path / "canonicalize_out").resolve()

    result = _runner().run(
        "scans.canonicalize",
        {
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
                        "translate_y": -20.0,
                    },
                }
            ],
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["canonicalize_results"][0]
    assert row["status"] == "ok"
    assert Path(row["output_page"]["image_path"]).exists()


def test_scans_canonicalize_runtime_failure_is_row_local_degraded(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    page_one = make_rgb_page(tmp_path / "scan_001" / "page_001.png")
    page_two = make_rgb_page(tmp_path / "scan_002" / "page_001.png")
    template = make_rgb_page(tmp_path / "template" / "page_001.png")
    output_dir = (tmp_path / "canonicalize_out").resolve()

    call_state = {"count": 0}

    def _failing_canonicalize(
        image: Image.Image,
        transform: Transform,
        *,
        output_width: int,
        output_height: int,
    ) -> Image.Image:
        call_state["count"] += 1
        if call_state["count"] == 2:
            raise RuntimeError("synthetic canonicalize failure")
        from scriptscore.artifacts import apply_canonical_transform as real_canonicalize

        return real_canonicalize(
            image,
            transform,
            output_width=output_width,
            output_height=output_height,
        )

    monkeypatch.setattr(
        "scriptscore.commands.scans_canonicalize.apply_canonical_transform",
        _failing_canonicalize,
    )

    result = _runner().run(
        "scans.canonicalize",
        {
            "canonicalize_targets": [
                {
                    "page": {
                        "page_type": "student_scan",
                        "page_number": 1,
                        "image_path": str(page_one),
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
                },
                {
                    "page": {
                        "page_type": "student_scan",
                        "page_number": 1,
                        "image_path": str(page_two),
                        "student_ref": "scan_002",
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
                },
            ],
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    rows = result.envelope.data["canonicalize_results"]
    assert [row["status"] for row in rows] == ["ok", "error"]
    assert rows[1]["warnings"][0]["code"] == "canonicalize_failed"
    assert Path(rows[0]["output_page"]["image_path"]).exists()
    assert not Path(rows[1]["output_page"]["image_path"]).exists()


def test_scans_crop_matches_targets_by_page_number_per_student(tmp_path: Path) -> None:
    page_one = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(20, 20))
    page_two = make_rgb_page(tmp_path / "scan_002" / "page_001.png", size=(20, 20))
    output_dir = (tmp_path / "crop_out").resolve()

    result = _runner().run(
        "scans.crop",
        {
            "pages": [
                {
                    "page_type": "student_scan",
                    "page_number": 1,
                    "image_path": str(page_one),
                    "student_ref": "scan_001",
                },
                {
                    "page_type": "student_scan",
                    "page_number": 1,
                    "image_path": str(page_two),
                    "student_ref": "scan_002",
                },
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

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    rows = result.envelope.data["crop_results"]
    assert len(rows) == 2
    assert {row["student_ref"] for row in rows} == {"scan_001", "scan_002"}
    for row in rows:
        crop_path = Path(row["question_crop_path"])
        assert crop_path.exists()
        with Image.open(crop_path) as image:
            assert image.size == (10, 8)
    manifest = json.loads((output_dir / "output_metadata.json").read_text(encoding="utf-8"))
    assert "crop_results" not in manifest["data"]
    assert manifest["data"] == {
        "failed_count": 0,
        "result_row_count": 2,
        "written_artifact_count": 2,
    }


def test_scans_crop_can_scope_targets_to_one_student(tmp_path: Path) -> None:
    page_one = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(20, 20))
    page_two = make_rgb_page(tmp_path / "scan_002" / "page_001.png", size=(20, 20))
    output_dir = (tmp_path / "crop_scoped").resolve()

    result = _runner().run(
        "scans.crop",
        {
            "pages": [
                {
                    "page_type": "student_scan",
                    "page_number": 1,
                    "image_path": str(page_one),
                    "student_ref": "scan_001",
                },
                {
                    "page_type": "student_scan",
                    "page_number": 1,
                    "image_path": str(page_two),
                    "student_ref": "scan_002",
                },
            ],
            "question_crop_targets": [
                {
                    "student_ref": "scan_002",
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

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    rows = result.envelope.data["crop_results"]
    assert len(rows) == 1
    assert rows[0]["student_ref"] == "scan_002"
    assert Path(rows[0]["question_crop_path"]).exists()


def test_scans_crop_repeated_question_ids_write_unique_artifacts(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(20, 20))
    output_dir = (tmp_path / "crop_out").resolve()

    result = _runner().run(
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
                },
                {
                    "question_id": "q1",
                    "page_number": 1,
                    "region": {
                        "x": 5,
                        "y": 5,
                        "width": 10,
                        "height": 8,
                        "units": "rendered_page_pixels",
                    },
                },
            ],
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    rows = result.envelope.data["crop_results"]
    assert len(rows) == 2
    first_path = Path(rows[0]["question_crop_path"])
    second_path = Path(rows[1]["question_crop_path"])
    assert first_path != second_path
    assert first_path.exists()
    assert second_path.exists()
    assert first_path.name == "q1__p001__r01.png"
    assert second_path.name == "q1__p001__r02.png"


def test_scans_crop_out_of_bounds_region_returns_row_error(tmp_path: Path) -> None:
    page = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(10, 10))
    output_dir = (tmp_path / "crop_out").resolve()

    result = _runner().run(
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
            "question_crop_targets": [
                {
                    "question_id": "q1",
                    "page_number": 1,
                    "region": {
                        "x": 0,
                        "y": 0,
                        "width": 20,
                        "height": 8,
                        "units": "rendered_page_pixels",
                    },
                }
            ],
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    row = result.envelope.data["crop_results"][0]
    assert row["status"] == "error"
    assert "question_crop_path" not in row
    assert row["warnings"][0]["code"] == "crop_failed"


def test_scans_crop_missing_page_hard_fails_without_writes(tmp_path: Path) -> None:
    output_dir = (tmp_path / "crop_out").resolve()
    missing = (tmp_path / "scan_001" / "page_001.png").resolve()

    result = _runner().run(
        "scans.crop",
        {
            "pages": [
                {
                    "page_type": "student_scan",
                    "page_number": 1,
                    "image_path": str(missing),
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
                        "width": 5,
                        "height": 5,
                        "units": "rendered_page_pixels",
                    },
                }
            ],
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 3
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert not (output_dir / "output_metadata.json").exists()


def test_scans_ingest_request_rejects_duplicate_student_refs(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "scan_001.pdf", page_texts=["hello"])
    with pytest.raises(ValidationError):
        ScansIngestRequest.model_validate(
            {
                "pdf_targets": [
                    {"student_ref": "scan_001", "pdf_path": str(pdf)},
                    {"student_ref": "scan_001", "pdf_path": str(pdf)},
                ],
                "output_artifacts_dir": str((tmp_path / "out").resolve()),
            }
        )


def test_scans_ingest_request_rejects_duplicate_page_order_entries(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "scan_001.pdf", page_texts=["hello", "world"])
    with pytest.raises(ValidationError):
        ScansIngestRequest.model_validate(
            {
                "pdf_targets": [
                    {
                        "student_ref": "scan_001",
                        "pdf_path": str(pdf),
                        "page_order": [1, 1],
                    }
                ],
                "output_artifacts_dir": str((tmp_path / "out").resolve()),
            }
        )


def test_scans_parse_requires_llm_provider(tmp_path: Path) -> None:
    crop = make_rgb_page(tmp_path / "crop.png")
    template = make_rgb_page(tmp_path / "template.png")
    with pytest.raises(ValidationError):
        ScansParseRequest.model_validate(
            {
                "parse_targets": [
                    {
                        "student_ref": "scan_001",
                        "question_id": "q1",
                        "parse_question_context": {
                            "question_number": 1,
                            "question_text_clean": "Explain the output.",
                        },
                        "question_crop_path": str(crop),
                        "template_question_png_path": str(template),
                    }
                ],
                "output_artifacts_dir": str((tmp_path / "out").resolve()),
                "providers": {},
                "llm_config": {"model": "test-model"},
            }
        )


def test_scans_ingest_renders_pdf_pages_and_manifest(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "scan_001.pdf", page_texts=["page 1", "page 2"])
    output_dir = (tmp_path / "ingest_out").resolve()

    result = _runner().run(
        "scans.ingest",
        {
            "pdf_targets": [{"student_ref": "scan_001", "pdf_path": str(pdf)}],
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["pdf_results"][0]
    assert row["status"] == "ok"
    assert len(row["pages"]) == 2
    assert not row["pages"][0].get("student_ref")
    assert Path(row["pages"][0]["image_path"]).exists()
    assert Path(row["pages"][1]["image_path"]).exists()
    manifest = json.loads((output_dir / "output_metadata.json").read_text(encoding="utf-8"))
    assert manifest["data"] == {
        "failed_count": 0,
        "result_row_count": 1,
        "written_artifact_count": 2,
    }


def test_scans_ingest_uses_the_same_render_normalization_as_exam_setup(tmp_path: Path) -> None:
    pdf = make_template_pdf(
        tmp_path / "template.pdf",
        questions=[TemplateQuestionSpec(number=1, text="Explain entropy.", points=5)],
    )
    setup_output_dir = (tmp_path / "setup_out").resolve()
    ingest_output_dir = (tmp_path / "ingest_out").resolve()

    setup_result = _runner().run(
        "exam.setup",
        {
            "template_pdf_path": str(pdf),
            "output_artifacts_dir": str(setup_output_dir),
        },
    )
    ingest_result = _runner().run(
        "scans.ingest",
        {
            "pdf_targets": [{"student_ref": "scan_001", "pdf_path": str(pdf)}],
            "output_artifacts_dir": str(ingest_output_dir),
        },
    )

    assert setup_result.exit_code == 0
    assert ingest_result.exit_code == 0
    assert isinstance(setup_result.envelope, CommandSuccessEnvelope)
    assert isinstance(ingest_result.envelope, CommandSuccessEnvelope)

    setup_page = Path(setup_result.envelope.data["template_pages"][0]["image_path"])
    ingest_page = Path(ingest_result.envelope.data["pdf_results"][0]["pages"][0]["image_path"])
    with Image.open(setup_page) as setup_image, Image.open(ingest_page) as ingest_image:
        assert setup_image.mode == "RGB"
        assert ingest_image.mode == "RGB"
        assert setup_image.size == ingest_image.size
        assert ImageChops.difference(setup_image, ingest_image).getbbox() is None


def test_scans_ingest_omits_page_order_analysis_and_ocr_trace_artifacts(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "scan_001.pdf", page_texts=["page 1", "page 2"])
    output_dir = (tmp_path / "ingest_out").resolve()

    result = _runner().run(
        "scans.ingest",
        {
            "pdf_targets": [{"student_ref": "scan_001", "pdf_path": str(pdf)}],
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["pdf_results"][0]
    assert row["status"] == "ok"
    assert row.get("page_order_analysis") is None
    assert row["warnings"] == []
    assert not (output_dir / "traces").exists()


def test_scans_ingest_honors_explicit_page_order(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "scan_001.pdf", page_texts=["page 1", "page 2", "page 3"])
    output_dir = (tmp_path / "ingest_out").resolve()

    result = _runner().run(
        "scans.ingest",
        {
            "pdf_targets": [
                {
                    "student_ref": "scan_001",
                    "pdf_path": str(pdf),
                    "page_order": [3, 1, 2],
                }
            ],
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["pdf_results"][0]
    assert [page["page_number"] for page in row["pages"]] == [3, 1, 2]
    assert [Path(page["image_path"]).name for page in row["pages"]] == [
        "page_001.png",
        "page_002.png",
        "page_003.png",
    ]


def test_scans_ingest_accepts_partial_page_order_after_render(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "scan_001.pdf", page_texts=["page 1", "page 2"])
    output_dir = (tmp_path / "ingest_out").resolve()

    result = _runner().run(
        "scans.ingest",
        {
            "pdf_targets": [
                {
                    "student_ref": "scan_001",
                    "pdf_path": str(pdf),
                    "page_order": [2],
                }
            ],
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["pdf_results"][0]
    assert row["status"] == "ok"
    assert [page["page_number"] for page in row["pages"]] == [2]
    assert [Path(page["image_path"]).name for page in row["pages"]] == ["page_001.png"]


def test_scans_ingest_rejects_out_of_range_page_order_after_render(tmp_path: Path) -> None:
    pdf = make_student_pdf(tmp_path / "scan_001.pdf", page_texts=["page 1", "page 2"])
    output_dir = (tmp_path / "ingest_out").resolve()

    result = _runner().run(
        "scans.ingest",
        {
            "pdf_targets": [
                {
                    "student_ref": "scan_001",
                    "pdf_path": str(pdf),
                    "page_order": [3],
                }
            ],
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    row = result.envelope.data["pdf_results"][0]
    assert row["status"] == "error"
    assert row["warnings"][0]["code"] == "pdf_render_failed"
    assert "page_order must contain valid source PDF page numbers." in row["warnings"][0]["message"]


def test_scans_pdf_create_redacted_keeps_selected_pages_in_order(tmp_path: Path) -> None:
    fitz = pytest.importorskip("fitz")
    pdf = make_student_pdf(tmp_path / "scan_001.pdf", page_texts=["page 1", "page 2", "page 3"])
    output_pdf = (tmp_path / "redacted_selected.pdf").resolve()

    result = _runner().run(
        "scans.pdf-create-redacted",
        {
            "input_pdf_path": str(pdf.resolve()),
            "output_pdf_path": str(output_pdf),
            "regions": [],
            "raster_sizes_by_page": {},
            "page_order": [3, 1],
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.data["page_count"] == 2
    document = fitz.open(output_pdf)
    try:
        assert document.page_count == 2
        assert "page 3" in document.load_page(0).get_text()
        assert "page 1" in document.load_page(1).get_text()
    finally:
        document.close()


def test_scans_ingest_runtime_failure_is_row_local_degraded(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    pdf_one = make_student_pdf(tmp_path / "scan_001.pdf", page_texts=["page 1"])
    pdf_two = make_student_pdf(tmp_path / "scan_002.pdf", page_texts=["page 2"])
    output_dir = (tmp_path / "ingest_out").resolve()

    original_render = __import__(
        "scriptscore.commands.scans_ingest", fromlist=["render_pdf_document"]
    ).render_pdf_document

    def _maybe_fail(path: Path) -> object:
        if path.name == "scan_002.pdf":
            raise RuntimeError("synthetic render failure")
        return original_render(path)

    monkeypatch.setattr("scriptscore.commands.scans_ingest.render_pdf_document", _maybe_fail)

    result = _runner().run(
        "scans.ingest",
        {
            "pdf_targets": [
                {"student_ref": "scan_001", "pdf_path": str(pdf_one)},
                {"student_ref": "scan_002", "pdf_path": str(pdf_two)},
            ],
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    rows = result.envelope.data["pdf_results"]
    assert [row["status"] for row in rows] == ["ok", "error"]
    assert rows[1]["warnings"][0]["code"] == "pdf_render_failed"


def test_scans_ingest_partial_write_preserves_written_pages_in_error_row(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    pdf = make_student_pdf(tmp_path / "scan_001.pdf", page_texts=["page 1", "page 2"])
    output_dir = (tmp_path / "ingest_out").resolve()

    original_save = __import__("scriptscore.commands.scans_ingest", fromlist=["save_png"]).save_png
    call_count = 0

    def _fail_on_second_save(image: Image.Image, path: Path) -> None:
        nonlocal call_count
        call_count += 1
        if call_count == 2:
            raise RuntimeError("synthetic save failure")
        original_save(image, path)

    monkeypatch.setattr("scriptscore.commands.scans_ingest.save_png", _fail_on_second_save)

    result = _runner().run(
        "scans.ingest",
        {
            "pdf_targets": [{"student_ref": "scan_001", "pdf_path": str(pdf)}],
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    row = result.envelope.data["pdf_results"][0]
    assert row["status"] == "error"
    assert len(row["pages"]) == 1
    assert Path(row["pages"][0]["image_path"]).exists()
    manifest = json.loads((output_dir / "output_metadata.json").read_text(encoding="utf-8"))
    assert manifest["data"] == {
        "failed_count": 1,
        "result_row_count": 1,
        "written_artifact_count": 1,
    }


def test_scans_ingest_invalid_pdf_magic_hard_fails_without_writes(tmp_path: Path) -> None:
    bad_pdf = (tmp_path / "bad.pdf").resolve()
    bad_pdf.write_bytes(b"not a pdf")
    output_dir = (tmp_path / "ingest_out").resolve()

    result = _runner().run(
        "scans.ingest",
        {
            "pdf_targets": [{"student_ref": "scan_001", "pdf_path": str(bad_pdf)}],
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 2
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.code == "invalid_pdf_magic"
    assert not (output_dir / "output_metadata.json").exists()


def test_scans_parse_blank_short_circuits_ocr_and_writes_only_prescreen_trace(
    tmp_path: Path,
) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png", size=(20, 20))
    template = make_rgb_page(tmp_path / "template" / "q1.png", size=(20, 20))
    output_dir = (tmp_path / "parse_out").resolve()

    result = _runner().run(
        "scans.parse",
        {
            "parse_targets": [
                {
                    "student_ref": "scan_001",
                    "question_id": "q1",
                    "parse_question_context": {
                        "question_number": 1,
                        "question_text_clean": "Explain the result.",
                    },
                    "question_crop_path": str(crop),
                    "template_question_png_path": str(template),
                }
            ],
            "output_artifacts_dir": str(output_dir),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["parse_results"][0]
    assert row["status"] == "blank"
    assert row["blank"] is True
    assert row["parsed_text"] == ""
    trace_dir = output_dir / "traces"
    assert [path.name for path in trace_dir.glob("*.json")] == [
        "handwriting_verify__question_id-q1__student_ref-scan_001.json"
    ]


def test_scans_parse_low_confidence_continues_to_ocr_and_normalizes_blank(tmp_path: Path) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png", size=(20, 20))
    template = make_rgb_page(tmp_path / "template" / "q1.png", size=(20, 20))

    def responder(request: LlmRequest) -> LlmResponse:
        if request.prompt_id == "handwriting_verify":
            return LlmResponse(
                raw_text='{"has_handwriting": false, "confidence": "low", "status": "complete"}'
            )
        if request.prompt_id == "parse_ocr":
            return LlmResponse(raw_text="[blank]")
        raise AssertionError(f"unexpected prompt id: {request.prompt_id}")

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "scans.parse",
        {
            "parse_targets": [
                {
                    "student_ref": "scan_001",
                    "question_id": "q1",
                    "parse_question_context": ParseQuestionContext(
                        question_number=1,
                        question_text_clean="Explain the result.",
                    ).model_dump(mode="json"),
                    "question_crop_path": str(crop),
                    "template_question_png_path": str(template),
                }
            ],
            "output_artifacts_dir": str((tmp_path / "parse_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    assert result.envelope.warnings[0].code == "handwriting_verify_low_confidence"
    row = result.envelope.data["parse_results"][0]
    assert row["status"] == "blank"
    assert row["confidence"] == "low"
    assert row["confidence_source"] == "combined"
    assert row["warnings"][0]["code"] == "handwriting_verify_low_confidence"
    trace_dir = Path(result.envelope.data["output_metadata_path"]).parent / "traces"
    assert sorted(path.name for path in trace_dir.glob("*.json")) == [
        "handwriting_verify__question_id-q1__student_ref-scan_001.json",
        "parse_ocr__question_id-q1__student_ref-scan_001.json",
    ]


def test_scans_parse_prescreens_entire_batch_before_any_ocr(tmp_path: Path) -> None:
    crop_one = make_rgb_page(tmp_path / "scan_001" / "q1.png", size=(20, 20))
    crop_two = make_rgb_page(tmp_path / "scan_002" / "q1.png", size=(20, 20))
    template_one = make_rgb_page(tmp_path / "template" / "q1a.png", size=(20, 20))
    template_two = make_rgb_page(tmp_path / "template" / "q1b.png", size=(20, 20))
    call_sequence: list[str] = []

    def responder(request: LlmRequest) -> LlmResponse:
        call_sequence.append(request.prompt_id)
        if request.prompt_id == "handwriting_verify":
            return LlmResponse(
                raw_text='{"has_handwriting": true, "confidence": "high", "status": "complete"}'
            )
        if request.prompt_id == "parse_ocr":
            return LlmResponse(raw_text="parsed answer")
        raise AssertionError(f"unexpected prompt id: {request.prompt_id}")

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "scans.parse",
        {
            "parse_targets": [
                {
                    "student_ref": "scan_001",
                    "question_id": "q1",
                    "parse_question_context": {
                        "question_number": 1,
                        "question_text_clean": "Explain the first result.",
                    },
                    "question_crop_path": str(crop_one),
                    "template_question_png_path": str(template_one),
                },
                {
                    "student_ref": "scan_002",
                    "question_id": "q1",
                    "parse_question_context": {
                        "question_number": 1,
                        "question_text_clean": "Explain the second result.",
                    },
                    "question_crop_path": str(crop_two),
                    "template_question_png_path": str(template_two),
                },
            ],
            "output_artifacts_dir": str((tmp_path / "parse_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert call_sequence == [
        "handwriting_verify",
        "handwriting_verify",
        "parse_ocr",
        "parse_ocr",
    ]


def test_scans_parse_error_status_continues_to_ocr(tmp_path: Path) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png", size=(20, 20))
    put_pixel(crop, xy=(0, 0), color=(0, 0, 0))
    template = make_rgb_page(tmp_path / "template" / "q1.png", size=(20, 20))

    def responder(request: LlmRequest) -> LlmResponse:
        if request.prompt_id == "handwriting_verify":
            return LlmResponse(
                raw_text='{"has_handwriting": false, "confidence": "high", "status": "error"}'
            )
        if request.prompt_id == "parse_ocr":
            return LlmResponse(raw_text="parsed answer")
        raise AssertionError(f"unexpected prompt id: {request.prompt_id}")

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "scans.parse",
        {
            "parse_targets": [
                {
                    "student_ref": "scan_001",
                    "question_id": "q1",
                    "parse_question_context": {
                        "question_number": 1,
                        "question_text_clean": "Explain the result.",
                    },
                    "question_crop_path": str(crop),
                    "template_question_png_path": str(template),
                }
            ],
            "output_artifacts_dir": str((tmp_path / "parse_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    assert result.envelope.warnings[0].code == "handwriting_verify_error_status"
    row = result.envelope.data["parse_results"][0]
    assert row["status"] == "ok"
    assert row["confidence"] == "low"
    assert row["confidence_source"] == "combined"
    assert row["parsed_text"] == "parsed answer"
    assert row["warnings"][0]["code"] == "handwriting_verify_error_status"


def test_scans_parse_ocr_failure_is_row_local_degraded(tmp_path: Path) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png", size=(20, 20))
    put_pixel(crop, xy=(0, 0), color=(0, 0, 0))
    template = make_rgb_page(tmp_path / "template" / "q1.png", size=(20, 20))

    def responder(request: LlmRequest) -> LlmResponse:
        if request.prompt_id == "handwriting_verify":
            return LlmResponse(
                raw_text='{"has_handwriting": true, "confidence": "high", "status": "complete"}'
            )
        if request.prompt_id == "parse_ocr":
            raise RuntimeError("synthetic ocr failure")
        raise AssertionError(f"unexpected prompt id: {request.prompt_id}")

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "scans.parse",
        {
            "parse_targets": [
                {
                    "student_ref": "scan_001",
                    "question_id": "q1",
                    "parse_question_context": {
                        "question_number": 1,
                        "question_text_clean": "Explain the result.",
                    },
                    "question_crop_path": str(crop),
                    "template_question_png_path": str(template),
                }
            ],
            "output_artifacts_dir": str((tmp_path / "parse_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    row = result.envelope.data["parse_results"][0]
    assert row["status"] == "error"
    assert row["warnings"][-1]["code"] == "parse_ocr_failed"


def test_scans_parse_escapes_question_context_xml(tmp_path: Path) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png", size=(20, 20))
    put_pixel(crop, xy=(0, 0), color=(0, 0, 0))
    template = make_rgb_page(tmp_path / "template" / "q1.png", size=(20, 20))
    captured: dict[str, str] = {}

    def responder(request: LlmRequest) -> LlmResponse:
        if request.prompt_id == "handwriting_verify":
            return LlmResponse(
                raw_text='{"has_handwriting": true, "confidence": "high", "status": "complete"}'
            )
        if request.prompt_id == "parse_ocr":
            captured["rendered_text"] = request.rendered_text
            return LlmResponse(raw_text="parsed answer")
        raise AssertionError(f"unexpected prompt id: {request.prompt_id}")

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "scans.parse",
        {
            "parse_targets": [
                {
                    "student_ref": "scan_001",
                    "question_id": "q1",
                    "parse_question_context": {
                        "question_number": 1,
                        "question_text_clean": "Explain if x < y && a > b.</tag>",
                    },
                    "question_crop_path": str(crop),
                    "template_question_png_path": str(template),
                }
            ],
            "output_artifacts_dir": str((tmp_path / "parse_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert (
        "<question_text_clean>Explain if x &lt; y &amp;&amp; a &gt; b.&lt;/tag&gt;</question_text_clean>"
        in captured["rendered_text"]
    )
