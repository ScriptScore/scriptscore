# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for trace artifact writing helpers."""

from __future__ import annotations

import json
from pathlib import Path

from scriptscore.artifacts import write_trace_artifact
from scriptscore.contracts import LlmTokenUsage, TimingInfo


def test_write_trace_artifact_avoids_scope_slug_collisions(tmp_path: Path) -> None:
    output_dir = (tmp_path / "artifacts").resolve()
    timing = TimingInfo(
        started_at="2026-03-22T00:00:00Z",
        finished_at="2026-03-22T00:00:01Z",
        duration_ms=1000,
    )

    first = write_trace_artifact(
        output_artifacts_dir=output_dir,
        command="scans.parse",
        operation_id="op-1",
        request_id="req-1",
        step="parse_ocr",
        scope={"student_ref": "scan 001"},
        provider_capability="llm_provider",
        provider_name="ollama_native",
        prompt_id="parse_ocr",
        timing=timing,
    )
    second = write_trace_artifact(
        output_artifacts_dir=output_dir,
        command="scans.parse",
        operation_id="op-1",
        request_id="req-1",
        step="parse_ocr",
        scope={"student_ref": "scan_001"},
        provider_capability="llm_provider",
        provider_name="ollama_native",
        prompt_id="parse_ocr",
        timing=timing,
    )

    assert first.path != second.path
    first_payload = json.loads(Path(first.path).read_text(encoding="utf-8"))
    second_payload = json.loads(Path(second.path).read_text(encoding="utf-8"))
    assert first_payload["scope"] == {"student_ref": "scan 001"}
    assert second_payload["scope"] == {"student_ref": "scan_001"}


def test_write_trace_artifact_allows_distinct_retry_suffixes(tmp_path: Path) -> None:
    output_dir = (tmp_path / "artifacts").resolve()
    timing = TimingInfo(
        started_at="2026-03-22T00:00:00Z",
        finished_at="2026-03-22T00:00:01Z",
        duration_ms=1000,
    )

    first = write_trace_artifact(
        output_artifacts_dir=output_dir,
        command="grading.draft-feedback",
        operation_id="op-1",
        request_id="req-1",
        step="feedback_draft",
        scope={"student_ref": "scan_001", "question_id": "q1"},
        provider_capability="llm_provider",
        provider_name="ollama_native",
        prompt_id="feedback_draft",
        timing=timing,
    )
    second = write_trace_artifact(
        output_artifacts_dir=output_dir,
        command="grading.draft-feedback",
        operation_id="op-1",
        request_id="req-1",
        step="feedback_draft",
        scope={"student_ref": "scan_001", "question_id": "q1"},
        filename_suffix="attempt_02",
        provider_capability="llm_provider",
        provider_name="ollama_native",
        prompt_id="feedback_draft",
        timing=timing,
    )

    assert first.path != second.path
    assert Path(first.path).name == "feedback_draft__question_id-q1__student_ref-scan_001.json"
    assert (
        Path(second.path).name
        == "feedback_draft__question_id-q1__student_ref-scan_001__attempt_02.json"
    )


def test_write_trace_artifact_includes_response_usage(tmp_path: Path) -> None:
    output_dir = (tmp_path / "artifacts").resolve()
    timing = TimingInfo(
        started_at="2026-03-22T00:00:00Z",
        finished_at="2026-03-22T00:00:01Z",
        duration_ms=1000,
    )

    artifact = write_trace_artifact(
        output_artifacts_dir=output_dir,
        command="exam.analyze",
        operation_id="op-1",
        request_id="req-1",
        step="analyze_question_text",
        scope={"question_id": "q1"},
        provider_capability="llm_provider",
        provider_name="ollama_native",
        prompt_id="question_text",
        response_raw="clean text",
        response_usage=LlmTokenUsage(
            input_tokens=12,
            output_tokens=3,
            total_tokens=15,
            count_source="exact",
        ),
        timing=timing,
    )

    payload = json.loads(Path(artifact.path).read_text(encoding="utf-8"))
    assert payload["response"]["usage"] == {
        "count_source": "exact",
        "image_input_count": 0,
        "input_tokens": 12,
        "output_tokens": 3,
        "total_tokens": 15,
    }
