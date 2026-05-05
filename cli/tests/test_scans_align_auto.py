# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for `scans align-auto`."""

from __future__ import annotations

import json
from collections.abc import Callable
from pathlib import Path

import pytest
from PIL import Image, ImageDraw
from pydantic import ValidationError

from scriptscore.artifacts.images import apply_manual_transform, load_page_image
from scriptscore.commands import build_command_registry
from scriptscore.contracts import (
    CommandSuccessEnvelope,
    ErrorCategory,
    ScansAlignAutoRequest,
    ScriptscoreError,
    Transform,
)
from scriptscore.providers import (
    AlignmentRequest,
    AlignmentResponse,
    CoreTemplateMatchProvider,
    FakeAlignmentProvider,
    ProviderRegistry,
)
from scriptscore.providers.core_template_match import _detect_aruco_markers, _marker_centers
from scriptscore.runtime import CommandRunner
from tests.support.images import make_rgb_page


def _runner(*, provider_registry: ProviderRegistry | None = None) -> CommandRunner:
    return CommandRunner(
        registry=build_command_registry(),
        provider_registry=provider_registry or ProviderRegistry.with_builtin_fakes(),
    )


def _registry_with_alignment(
    responder: Callable[[AlignmentRequest], AlignmentResponse],
) -> ProviderRegistry:
    registry = ProviderRegistry.with_builtin_fakes()
    registry.register(FakeAlignmentProvider(responder=responder))
    return registry


def _request(*, template_page: Path, student_page: Path, output_dir: Path) -> dict[str, object]:
    return {
        "template_pages": [
            {
                "page_type": "template",
                "page_number": 1,
                "image_path": str(template_page),
            }
        ],
        "student_pages": [
            {
                "page_type": "student_scan",
                "page_number": 1,
                "image_path": str(student_page),
                "student_ref": "scan_001",
            }
        ],
        "output_artifacts_dir": str(output_dir),
        "providers": {"alignment_engine": "core_template_match"},
    }


def _make_alignment_template(path: Path) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    image = Image.new("RGB", (120, 120), (255, 255, 255))
    draw = ImageDraw.Draw(image)
    draw.rectangle((14, 2, 42, 13), fill=(0, 0, 0))
    draw.rectangle((54, 4, 95, 11), fill=(0, 0, 0))
    draw.rectangle((84, 0, 92, 7), fill=(64, 64, 64))
    draw.rectangle((46, 8, 52, 13), fill=(32, 32, 32))
    draw.rectangle((10, 70, 22, 82), fill=(0, 0, 0))
    image.save(path, format="PNG")
    return path


def _make_aruco_alignment_template(path: Path) -> Path:
    import cv2
    import numpy as np

    path.parent.mkdir(parents=True, exist_ok=True)
    canvas = np.full((720, 540), 255, dtype=np.uint8)
    dictionary = cv2.aruco.getPredefinedDictionary(cv2.aruco.DICT_4X4_100)
    marker_size = 72
    marker_positions = {
        0: (18, 18),
        1: (540 - 18 - marker_size, 18),
        2: (18, 720 - 18 - marker_size),
        3: (540 - 18 - marker_size, 720 - 18 - marker_size),
    }
    for marker_id, (left, top) in marker_positions.items():
        marker = cv2.aruco.generateImageMarker(dictionary, marker_id, marker_size)
        canvas[top : top + marker_size, left : left + marker_size] = marker
    image = Image.fromarray(canvas).convert("RGB")
    draw = ImageDraw.Draw(image)
    draw.rectangle((120, 90, 420, 120), fill=(0, 0, 0))
    draw.rectangle((140, 170, 400, 190), fill=(32, 32, 32))
    draw.rectangle((130, 250, 410, 270), fill=(64, 64, 64))
    image.save(path, format="PNG")
    return path


def test_scans_align_auto_request_rejects_duplicate_template_pages(tmp_path: Path) -> None:
    template = make_rgb_page(tmp_path / "template" / "page_001.png")
    student = make_rgb_page(tmp_path / "scan_001" / "page_001.png")
    with pytest.raises(ValidationError):
        ScansAlignAutoRequest.model_validate(
            {
                "template_pages": [
                    {"page_type": "template", "page_number": 1, "image_path": str(template)},
                    {"page_type": "template", "page_number": 1, "image_path": str(template)},
                ],
                "student_pages": [
                    {
                        "page_type": "student_scan",
                        "page_number": 1,
                        "image_path": str(student),
                        "student_ref": "scan_001",
                    }
                ],
                "output_artifacts_dir": str((tmp_path / "align_out").resolve()),
                "providers": {"alignment_engine": "core_template_match"},
            }
        )


def test_core_template_match_provider_returns_material_transform_proposal(tmp_path: Path) -> None:
    template_path = _make_alignment_template(tmp_path / "template" / "page_001.png")
    student_path = tmp_path / "scan_001" / "page_001.png"
    template_image = load_page_image(template_path)
    transformed = apply_manual_transform(
        template_image,
        Transform(rotation=0.0, scale=1.0, translate_x=7.0, translate_y=5.0),
    )
    student_path.parent.mkdir(parents=True, exist_ok=True)
    transformed.save(student_path, format="PNG")

    response = CoreTemplateMatchProvider().align(
        AlignmentRequest(
            template_page_path=str(template_path),
            student_page_path=str(student_path),
            mode="fast",
            marker_mode="ignore",
        )
    )

    assert response.status in {"ok", "low_confidence"}
    assert response.translate_x is not None
    assert response.translate_y is not None
    assert response.scale is not None
    assert response.rotation is not None
    assert response.translate_x < 0.0
    assert response.translate_y < 0.0
    assert abs(response.scale - 1.0) <= 0.05


def test_scans_align_auto_warns_about_content_clipping_without_forcing_review(
    tmp_path: Path,
) -> None:
    template_path = make_rgb_page(tmp_path / "template" / "page_001.png", size=(90, 120))
    student_path = make_rgb_page(tmp_path / "scan_001" / "page_001.png", size=(90, 120))
    with Image.open(student_path) as image:
        updated = image.copy()
    draw = ImageDraw.Draw(updated)
    draw.rectangle((10, 5, 70, 20), fill=(0, 0, 0))
    updated.save(student_path, format="PNG")

    registry = _registry_with_alignment(
        lambda _request: AlignmentResponse(
            status="ok",
            confidence=0.92,
            rotation=0.0,
            scale=1.0,
            translate_x=0.0,
            translate_y=-30.0,
            warnings=[],
        )
    )
    result = _runner(provider_registry=registry).run(
        "scans.align-auto",
        _request(
            template_page=template_path,
            student_page=student_path,
            output_dir=(tmp_path / "align_out").resolve(),
        ),
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["alignment_results"][0]
    assert row["status"] == "ok"
    assert row["confidence"] == 0.92
    assert row["transform"]["translate_y"] == -30.0
    assert row["warnings"][0]["code"] == "alignment_transform_clips_content"
    assert row["warnings"][0]["scope"]["top_px"] > 16


def test_core_template_match_provider_returns_identity_for_identical_pages(tmp_path: Path) -> None:
    template_path = _make_alignment_template(tmp_path / "template" / "page_001.png")

    response = CoreTemplateMatchProvider().align(
        AlignmentRequest(
            template_page_path=str(template_path),
            student_page_path=str(template_path),
            mode="fast",
            marker_mode="ignore",
        )
    )

    assert response.status == "ok"
    assert response.rotation == pytest.approx(0.0, abs=1e-6)
    assert response.scale == pytest.approx(1.0, abs=1e-6)
    assert response.translate_x == pytest.approx(0.0, abs=1e-6)
    assert response.translate_y == pytest.approx(0.0, abs=1e-6)


def test_core_template_match_provider_uses_aruco_before_template_matching(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    template_path = _make_aruco_alignment_template(tmp_path / "template" / "page_001.png")
    student_path = tmp_path / "scan_001" / "page_001.png"
    template_image = load_page_image(template_path)
    transformed = apply_manual_transform(
        template_image,
        Transform(rotation=0.0, scale=1.0, translate_x=18.0, translate_y=-12.0),
    )
    student_path.parent.mkdir(parents=True, exist_ok=True)
    transformed.save(student_path, format="PNG")

    def _unexpected_template_match(*_args: object, **_kwargs: object) -> tuple[object, int, int]:
        raise AssertionError("Template matching should not run when ArUco markers are usable.")

    monkeypatch.setattr(
        "scriptscore.providers.core_template_match._extract_reference_patch",
        _unexpected_template_match,
    )

    response = CoreTemplateMatchProvider().align(
        AlignmentRequest(
            template_page_path=str(template_path),
            student_page_path=str(student_path),
            mode="fast",
            marker_mode="prefer_aruco",
        )
    )

    assert response.status in {"ok", "low_confidence"}
    assert response.warnings == []
    assert response.rotation == pytest.approx(0.0, abs=0.5)
    assert response.scale == pytest.approx(1.0, abs=0.03)
    assert response.translate_x == pytest.approx(-18.0, abs=4.0)
    assert response.translate_y == pytest.approx(12.0, abs=4.0)


def test_core_template_match_provider_scores_minor_aruco_rotation_as_ok(tmp_path: Path) -> None:
    import cv2
    import numpy as np

    template_path = _make_aruco_alignment_template(tmp_path / "template" / "page_001.png")
    student_path = tmp_path / "scan_001" / "page_001.png"
    template_image = load_page_image(template_path)
    transformed = apply_manual_transform(
        template_image,
        Transform(rotation=0.8, scale=1.0, translate_x=18.0, translate_y=-12.0),
    )
    student_path.parent.mkdir(parents=True, exist_ok=True)
    transformed.save(student_path, format="PNG")

    response = CoreTemplateMatchProvider().align(
        AlignmentRequest(
            template_page_path=str(template_path),
            student_page_path=str(student_path),
            mode="fast",
            marker_mode="prefer_aruco",
        )
    )

    assert response.status == "ok"
    assert response.warnings == []
    assert response.rotation is not None
    assert response.scale is not None
    assert response.translate_x is not None
    assert response.translate_y is not None
    assert response.rotation == pytest.approx(-0.8, abs=0.5)
    assert response.scale == pytest.approx(1.0, abs=0.03)
    corrected = apply_manual_transform(
        load_page_image(student_path),
        Transform(
            rotation=response.rotation,
            scale=response.scale,
            translate_x=response.translate_x,
            translate_y=response.translate_y,
        ),
    )
    corrected_path = tmp_path / "corrected.png"
    corrected.save(corrected_path, format="PNG")
    template_markers = _detect_aruco_markers(
        cv2.imread(str(template_path), cv2.IMREAD_GRAYSCALE), cv2=cv2
    )
    corrected_markers = _detect_aruco_markers(
        cv2.imread(str(corrected_path), cv2.IMREAD_GRAYSCALE), cv2=cv2
    )
    shared_ids = sorted(set(template_markers) & set(corrected_markers))
    assert shared_ids
    template_centers = _marker_centers(template_markers, np=np)
    corrected_centers = _marker_centers(corrected_markers, np=np)
    mean_error = sum(
        (
            (float(template_centers[marker_id][0]) - float(corrected_centers[marker_id][0])) ** 2
            + (float(template_centers[marker_id][1]) - float(corrected_centers[marker_id][1])) ** 2
        )
        ** 0.5
        for marker_id in shared_ids
    ) / len(shared_ids)
    assert mean_error < 3.0


def test_scans_align_auto_returns_row_local_failed_result_for_missing_template_match(
    tmp_path: Path,
) -> None:
    template = make_rgb_page(tmp_path / "template" / "page_001.png")
    student = make_rgb_page(tmp_path / "scan_001" / "page_002.png")
    output_dir = (tmp_path / "align_out").resolve()

    result = _runner().run(
        "scans.align-auto",
        {
            "template_pages": [
                {"page_type": "template", "page_number": 1, "image_path": str(template)}
            ],
            "student_pages": [
                {
                    "page_type": "student_scan",
                    "page_number": 2,
                    "image_path": str(student),
                    "student_ref": "scan_001",
                }
            ],
            "output_artifacts_dir": str(output_dir),
            "providers": {"alignment_engine": "core_template_match"},
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    row = result.envelope.data["alignment_results"][0]
    assert row["status"] == "failed"
    assert row["warnings"][0]["code"] == "template_page_missing"


def test_scans_align_auto_degrades_one_row_for_typed_page_execution_failure(tmp_path: Path) -> None:
    template_one = _make_alignment_template(tmp_path / "template" / "page_001.png")
    template_two = _make_alignment_template(tmp_path / "template" / "page_002.png")
    student_one = tmp_path / "scan_001" / "page_001.png"
    student_one.parent.mkdir(parents=True, exist_ok=True)
    load_page_image(template_one).save(student_one, format="PNG")
    student_two = tmp_path / "scan_001" / "page_002.png"
    student_two.parent.mkdir(parents=True, exist_ok=True)
    student_two.write_text("not an image", encoding="utf-8")
    output_dir = (tmp_path / "align_out").resolve()

    result = CommandRunner(
        registry=build_command_registry(),
        provider_registry=ProviderRegistry.with_builtin_core(),
    ).run(
        "scans.align-auto",
        {
            "template_pages": [
                {"page_type": "template", "page_number": 1, "image_path": str(template_one)},
                {"page_type": "template", "page_number": 2, "image_path": str(template_two)},
            ],
            "student_pages": [
                {
                    "page_type": "student_scan",
                    "page_number": 1,
                    "image_path": str(student_one),
                    "student_ref": "scan_001",
                },
                {
                    "page_type": "student_scan",
                    "page_number": 2,
                    "image_path": str(student_two),
                    "student_ref": "scan_001",
                },
            ],
            "output_artifacts_dir": str(output_dir),
            "providers": {"alignment_engine": "core_template_match"},
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    rows = result.envelope.data["alignment_results"]
    assert len(rows) == 2
    assert rows[0]["status"] == "ok"
    assert rows[1]["status"] == "failed"
    assert rows[1]["warnings"][0]["code"] == "alignment_image_unreadable"


def test_scans_align_auto_preserves_provider_exit_code_for_global_provider_failure(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    template = _make_alignment_template(tmp_path / "template" / "page_001.png")
    student = tmp_path / "scan_001" / "page_001.png"
    student.parent.mkdir(parents=True, exist_ok=True)
    load_page_image(template).save(student, format="PNG")
    output_dir = (tmp_path / "align_out").resolve()

    def _raise_provider_error() -> tuple[object, object]:
        raise ScriptscoreError(
            code="alignment_dependency_unavailable",
            message="OpenCV alignment dependencies are not installed.",
            category=ErrorCategory.PROVIDER,
            retryable=False,
            details={"missing_module": "cv2"},
        )

    monkeypatch.setattr(
        "scriptscore.providers.core_template_match._load_cv_dependencies",
        _raise_provider_error,
    )

    result = CommandRunner(
        registry=build_command_registry(),
        provider_registry=ProviderRegistry.with_builtin_core(),
    ).run(
        "scans.align-auto",
        _request(template_page=template, student_page=student, output_dir=output_dir),
    )

    assert result.exit_code == 5


def test_scans_align_auto_writes_manifest_and_returns_provider_warning(tmp_path: Path) -> None:
    template = make_rgb_page(tmp_path / "template" / "page_001.png")
    student = make_rgb_page(tmp_path / "scan_001" / "page_001.png")
    output_dir = (tmp_path / "align_out").resolve()

    result = _runner().run(
        "scans.align-auto",
        _request(template_page=template, student_page=student, output_dir=output_dir),
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    assert result.envelope.warnings[0].code == "marker_guided_alignment_not_used"
    row = result.envelope.data["alignment_results"][0]
    assert row["status"] == "ok"
    assert row["transform"] == {
        "rotation": 0.0,
        "scale": 1.0,
        "translate_x": 0.0,
        "translate_y": 0.0,
    }
    assert row["warnings"][0]["code"] == "marker_guided_alignment_not_used"
    manifest = json.loads((output_dir / "output_metadata.json").read_text(encoding="utf-8"))
    assert manifest["data"] == {
        "failed_count": 0,
        "result_row_count": 1,
        "written_artifact_count": 0,
    }


def test_scans_align_auto_uses_custom_fake_alignment_provider(tmp_path: Path) -> None:
    template = make_rgb_page(tmp_path / "template" / "page_001.png")
    student = make_rgb_page(tmp_path / "scan_001" / "page_001.png")
    output_dir = (tmp_path / "align_out").resolve()

    def _responder(_request: AlignmentRequest) -> AlignmentResponse:
        return AlignmentResponse(
            status="low_confidence",
            confidence=0.41,
            rotation=1.25,
            scale=0.97,
            translate_x=-3.0,
            translate_y=4.0,
            warnings=[],
        )

    result = _runner(provider_registry=_registry_with_alignment(_responder)).run(
        "scans.align-auto",
        _request(template_page=template, student_page=student, output_dir=output_dir),
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["alignment_results"][0]
    assert row["status"] == "low_confidence"
    assert row["confidence"] == 0.41
    assert row["transform"]["rotation"] == 1.25
