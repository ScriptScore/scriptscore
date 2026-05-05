# SPDX-License-Identifier: AGPL-3.0-only
"""Shared contract and model tests."""

from __future__ import annotations

from pathlib import Path

import pytest
from pydantic import ValidationError

from scriptscore.artifacts import write_output_metadata
from scriptscore.contracts import (
    ArtifactReference,
    CommandSuccessEnvelope,
    CommandUsage,
    ErrorCategory,
    ExamAnalyzeRequest,
    ExamConfig,
    LlmConfig,
    LlmTokenUsageSummary,
    ProviderSelections,
    TimingInfo,
    exit_code_for_category,
)


def test_exam_config_round_trip_supports_question_shape() -> None:
    raw = {
        "exam_name": "Midterm 1",
        "template_dir": "template",
        "template_pdf": "template.pdf",
        "questions": {
            1: {
                "page": 2,
                "max_points": 10,
                "question_text": "Q1",
            }
        },
    }
    cfg = ExamConfig.model_validate(raw)
    assert cfg.exam_name == "Midterm 1"
    assert cfg.template_dir == "template"
    assert cfg.questions[1].page == 2


def test_exit_code_mapping_matches_spec() -> None:
    assert exit_code_for_category(ErrorCategory.VALIDATION) == 2
    assert exit_code_for_category(ErrorCategory.PROVIDER) == 5
    assert exit_code_for_category(ErrorCategory.CANCELLED) == 130


def test_output_metadata_manifest_write(tmp_path: Path) -> None:
    artifact_path = tmp_path / "artifact.txt"
    artifact_path.write_text("ok", encoding="utf-8")
    manifest_path = write_output_metadata(
        command="smoke.ping",
        operation_id="op_test",
        request_id="req_1",
        output_artifacts_dir=tmp_path,
        artifacts=[
            ArtifactReference(
                kind="file",
                role="output",
                label="artifact",
                path=str(artifact_path.resolve()),
                format="txt",
            )
        ],
        data={"artifact_count": 1},
    )
    assert manifest_path.exists()


def test_success_envelope_rejects_unknown_usage_keys() -> None:
    with pytest.raises(ValidationError):
        CommandSuccessEnvelope(
            command="exam.analyze",
            operation_id="op_1",
            usage=CommandUsage.model_validate(
                {
                    "llm": LlmTokenUsageSummary(
                        input_tokens=1,
                        output_tokens=2,
                        total_tokens=3,
                        count_source="exact",
                        calls=[],
                    ),
                    "other": {},
                }
            ),
            timing=TimingInfo(
                started_at="2026-03-22T00:00:00Z",
                finished_at="2026-03-22T00:00:01Z",
                duration_ms=1000,
            ),
        )


def test_llm_config_defaults_to_provider_specific_base_url_resolution() -> None:
    cfg = LlmConfig.model_validate({"model": "qwen2.5:14b"})
    assert cfg.base_url is None
    assert cfg.model == "qwen2.5:14b"


def test_llm_config_rejects_non_http_base_url() -> None:
    with pytest.raises(ValidationError):
        LlmConfig.model_validate({"model": "qwen2.5:14b", "base_url": "ftp://localhost"})


def test_llm_config_rejects_empty_api_key() -> None:
    with pytest.raises(ValidationError):
        LlmConfig.model_validate({"model": "qwen2.5:14b", "api_key": "   "})


def test_provider_selections_reject_unknown_llm_provider() -> None:
    with pytest.raises(ValidationError):
        ProviderSelections.model_validate({"llm_provider": "unsupported_provider"})


def test_exam_analyze_requires_api_key_for_ollama_cloud(tmp_path: Path) -> None:
    image_path = tmp_path / "q1.png"
    image_path.write_bytes(b"png")
    output_dir = tmp_path / "out"
    output_dir.mkdir()

    with pytest.raises(ValidationError):
        ExamAnalyzeRequest.model_validate(
            {
                "question_targets": [
                    {
                        "question_id": "q1",
                        "template_question_png_path": str(image_path.resolve()),
                        "baseline_pdf_text": "Question text",
                    }
                ],
                "output_artifacts_dir": str(output_dir.resolve()),
                "providers": {"llm_provider": "ollama_cloud"},
                "llm_config": {"model": "qwen2.5vl:7b"},
            }
        )


def test_exam_analyze_rejects_api_key_for_ollama_native(tmp_path: Path) -> None:
    image_path = tmp_path / "q1.png"
    image_path.write_bytes(b"png")
    output_dir = tmp_path / "out"
    output_dir.mkdir()

    with pytest.raises(ValidationError):
        ExamAnalyzeRequest.model_validate(
            {
                "question_targets": [
                    {
                        "question_id": "q1",
                        "template_question_png_path": str(image_path.resolve()),
                        "baseline_pdf_text": "Question text",
                    }
                ],
                "output_artifacts_dir": str(output_dir.resolve()),
                "providers": {"llm_provider": "ollama_native"},
                "llm_config": {
                    "model": "qwen2.5vl:7b",
                    "api_key": "secret",
                },
            }
        )
