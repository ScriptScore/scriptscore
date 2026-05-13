# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for Phase 4 grading commands."""

from __future__ import annotations

import json
import time
from collections.abc import Callable
from pathlib import Path
from threading import Lock

import pytest
from pydantic import ValidationError

from scriptscore.commands import build_command_registry
from scriptscore.contracts import (
    CommandErrorEnvelope,
    CommandSuccessEnvelope,
    FeedbackRequest,
    GradingExportRequest,
    MarkupRequest,
    ProgressEvent,
)
from scriptscore.providers import FakeLlmProvider, LlmRequest, LlmResponse, ProviderRegistry
from scriptscore.runtime import CancellationToken, CommandRunner
from tests.support.images import make_rgb_page
from tests.support.llm import llm_request_fields


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


def _instructor_profile() -> dict[str, object]:
    return {
        "grading_strictness": "balanced",
        "syntax_leniency": "medium",
        "ocr_tolerance": "medium",
        "partial_credit_style": "balanced",
        "feedback_style": "brief",
        "additional_guidance": None,
    }


def _rubric_criterion(*, criterion_index: int = 0, points: int = 1) -> dict[str, object]:
    return {
        "criterion_index": criterion_index,
        "label": f"Criterion {criterion_index}",
        "points": points,
        "partial_credit_guidance": f"Award between 0 and {points} points.",
    }


def _answer_score_request(
    *,
    student_ref: str = "scan_001",
    question_id: str = "q1",
    student_answer: str = "my answer",
    rubric_criteria: list[dict[str, object]] | None = None,
) -> dict[str, object]:
    return {
        "student_ref": student_ref,
        "question_id": question_id,
        "subject": "Python",
        "student_answer": student_answer,
        "question_text_clean": "Explain list slicing.",
        "question_context": "",
        "rubric_criteria": rubric_criteria or [_rubric_criterion()],
        "instructor_profile": _instructor_profile(),
    }


def test_grading_score_preliminary_blank_answer_scores_locally(tmp_path: Path) -> None:
    output_dir = (tmp_path / "preliminary_out").resolve()

    result = _runner().run(
        "grading.score-preliminary",
        {
            "score_requests": [
                {
                    "student_ref": "scan_001",
                    "question_id": "q1",
                    "subject": "Python",
                    "student_answer": "   ",
                    "question_text_clean": "Explain list slicing.",
                    "question_context": "",
                    "rubric_criterion": _rubric_criterion(),
                    "instructor_profile": _instructor_profile(),
                }
            ],
            "output_artifacts_dir": str(output_dir),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["preliminary_scores"][0]
    assert row["blank"] is True
    assert row["points_awarded"] == 0
    assert row["status"] == "ok"
    assert row["confidence"] == "high"
    assert row["confidence_reason"] == "Blank local scoring required no model judgment."
    trace_path = (
        output_dir
        / "traces"
        / "preliminary_score__criterion_index-0__question_id-q1__student_ref-scan_001.json"
    )
    assert trace_path.exists()


def test_grading_score_preliminary_retry_exhaustion_degrades_to_zero(tmp_path: Path) -> None:
    def responder(request: LlmRequest) -> LlmResponse:
        assert request.prompt_id == "preliminary_score"
        return LlmResponse(raw_text="not json")

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "grading.score-preliminary",
        {
            "score_requests": [
                {
                    "student_ref": "scan_001",
                    "question_id": "q1",
                    "subject": "Python",
                    "student_answer": "my answer",
                    "question_text_clean": "Explain list slicing.",
                    "question_context": "",
                    "rubric_criterion": _rubric_criterion(),
                    "instructor_profile": _instructor_profile(),
                }
            ],
            "output_artifacts_dir": str((tmp_path / "preliminary_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    row = result.envelope.data["preliminary_scores"][0]
    assert row["status"] == "degraded_parse_error"
    assert row["points_awarded"] == 0
    assert row["confidence"] == "low"
    assert (
        row["confidence_reason"]
        == "Criterion response parsing failed after retries and fell back to zero."
    )
    assert row["warnings"][0]["code"] == "preliminary_score_parse_failed"


def test_grading_score_preliminary_retry_success_sets_medium_confidence(tmp_path: Path) -> None:
    call_count = 0

    def responder(request: LlmRequest) -> LlmResponse:
        nonlocal call_count
        call_count += 1
        assert request.prompt_id == "preliminary_score"
        if call_count == 1:
            return LlmResponse(raw_text="not json")
        return LlmResponse(raw_text='{"points_awarded": 1, "rationale": "Recovered on retry."}')

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "grading.score-preliminary",
        {
            "score_requests": [
                {
                    "student_ref": "scan_001",
                    "question_id": "q1",
                    "subject": "Python",
                    "student_answer": "my answer",
                    "question_text_clean": "Explain list slicing.",
                    "question_context": "",
                    "rubric_criterion": _rubric_criterion(),
                    "instructor_profile": _instructor_profile(),
                }
            ],
            "output_artifacts_dir": str((tmp_path / "preliminary_retry_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["preliminary_scores"][0]
    assert row["status"] == "ok"
    assert row["points_awarded"] == 1
    assert row["confidence"] == "medium"
    assert (
        row["confidence_reason"]
        == "Preliminary scoring succeeded after at least one response retry."
    )


def test_grading_score_preliminary_answer_scoped_scores_all_criteria(
    tmp_path: Path,
) -> None:
    captured: dict[str, object] = {}

    def responder(request: LlmRequest) -> LlmResponse:
        captured["prompt_id"] = request.prompt_id
        captured["rendered_text"] = request.rendered_text
        return LlmResponse(
            raw_text=(
                '{"scores": ['
                '{"criterion_index": 0, "points_awarded": 1, "rationale": "Shows the main idea."},'
                '{"criterion_index": 1, "points_awarded": 2, "rationale": "Includes the example."}'
                "]}"
            )
        )

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "grading.score-preliminary",
        {
            "answer_score_requests": [
                _answer_score_request(
                    rubric_criteria=[
                        _rubric_criterion(criterion_index=0, points=1),
                        _rubric_criterion(criterion_index=1, points=2),
                    ],
                )
            ],
            "output_artifacts_dir": str((tmp_path / "preliminary_answer_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert captured["prompt_id"] == "preliminary_score_multi_criterion"
    rows = result.envelope.data["preliminary_scores"]
    assert [row["criterion_index"] for row in rows] == [0, 1]
    assert [row["points_awarded"] for row in rows] == [1, 2]
    assert "<question_rubric>" in str(captured["rendered_text"])


def test_grading_score_preliminary_omits_disabled_instructor_profile_tags(
    tmp_path: Path,
) -> None:
    captured: dict[str, object] = {}

    def responder(request: LlmRequest) -> LlmResponse:
        captured["rendered_text"] = request.rendered_text
        return LlmResponse(
            raw_text=(
                '{"scores": ['
                '{"criterion_index": 0, "points_awarded": 1, "rationale": "Shows the main idea."}'
                "]}"
            )
        )

    request = _answer_score_request()
    request["instructor_profile"] = {
        "grading_strictness": "balanced",
        "feedback_style": "brief",
        "additional_guidance": None,
    }
    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "grading.score-preliminary",
        {
            "answer_score_requests": [request],
            "output_artifacts_dir": str((tmp_path / "preliminary_profile_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    rendered_text = str(captured["rendered_text"])
    assert "<grading_strictness>balanced</grading_strictness>" in rendered_text
    assert "<feedback_style>brief</feedback_style>" in rendered_text
    assert "<additional_guidance></additional_guidance>" in rendered_text
    assert "<syntax_leniency>" not in rendered_text
    assert "<ocr_tolerance>" not in rendered_text
    assert "<partial_credit_style>" not in rendered_text


def test_grading_score_preliminary_answer_scoped_blank_scores_all_criteria_locally(
    tmp_path: Path,
) -> None:
    result = _runner().run(
        "grading.score-preliminary",
        {
            "answer_score_requests": [
                _answer_score_request(
                    student_answer=" [blank] ",
                    rubric_criteria=[
                        _rubric_criterion(criterion_index=0, points=1),
                        _rubric_criterion(criterion_index=1, points=2),
                    ],
                )
            ],
            "output_artifacts_dir": str((tmp_path / "preliminary_answer_blank_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    rows = result.envelope.data["preliminary_scores"]
    assert [row["criterion_index"] for row in rows] == [0, 1]
    assert all(row["blank"] is True for row in rows)
    assert all(row["points_awarded"] == 0 for row in rows)


@pytest.mark.parametrize("max_workers", [0, 5])
def test_grading_score_preliminary_rejects_invalid_max_workers(
    tmp_path: Path,
    max_workers: int,
) -> None:
    result = _runner().run(
        "grading.score-preliminary",
        {
            "answer_score_requests": [_answer_score_request()],
            "grading_runtime_config": {"max_workers": max_workers},
            "output_artifacts_dir": str((tmp_path / "preliminary_invalid_workers_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code != 0
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert "grading_runtime_config" in json.dumps(result.envelope.error.details)
    assert "max_workers" in json.dumps(result.envelope.error.details)


def test_grading_score_preliminary_defaults_answer_scoped_runtime_to_serial(
    tmp_path: Path,
) -> None:
    lock = Lock()
    active = 0
    max_active = 0
    events: list[ProgressEvent] = []

    def responder(request: LlmRequest) -> LlmResponse:
        nonlocal active, max_active
        assert request.prompt_id == "preliminary_score_multi_criterion"
        with lock:
            active += 1
            max_active = max(max_active, active)
        time.sleep(0.02)
        with lock:
            active -= 1
        return LlmResponse(
            raw_text='{"scores":[{"criterion_index":0,"points_awarded":1,"rationale":"Shows the main idea."}]}'
        )

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "grading.score-preliminary",
        {
            "answer_score_requests": [
                _answer_score_request(student_ref="scan_001", question_id="q1"),
                _answer_score_request(student_ref="scan_002", question_id="q2"),
            ],
            "output_artifacts_dir": str((tmp_path / "preliminary_default_workers_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
        event_sink=events.append,
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert max_active == 1
    assert events[0].data["max_workers"] == 1


def test_grading_score_preliminary_answer_scoped_concurrency_preserves_output_order(
    tmp_path: Path,
) -> None:
    events: list[ProgressEvent] = []

    def responder(request: LlmRequest) -> LlmResponse:
        assert request.prompt_id == "preliminary_score_multi_criterion"
        if "slow answer" in request.rendered_text:
            time.sleep(0.08)
        return LlmResponse(
            raw_text='{"scores":[{"criterion_index":0,"points_awarded":1,"rationale":"Shows the main idea."}]}'
        )

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "grading.score-preliminary",
        {
            "answer_score_requests": [
                _answer_score_request(
                    student_ref="scan_slow",
                    question_id="q1",
                    student_answer="slow answer",
                ),
                _answer_score_request(
                    student_ref="scan_fast",
                    question_id="q2",
                    student_answer="fast answer",
                ),
            ],
            "grading_runtime_config": {"max_workers": 2},
            "output_artifacts_dir": str((tmp_path / "preliminary_concurrent_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
        event_sink=events.append,
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    rows = result.envelope.data["preliminary_scores"]
    assert [row["student_ref"] for row in rows] == ["scan_slow", "scan_fast"]
    assert [event.event for event in events] == [
        "started",
        "item_started",
        "item_started",
        "item_completed",
        "item_completed",
        "completed",
    ]
    assert events[0].data["max_workers"] == 2
    assert [event.event for event in events].count("item_started") == 2
    assert [event.event for event in events].count("item_completed") == 2
    completed_scopes = [event.scope for event in events if event.event == "item_completed"]
    assert completed_scopes[0] == {"student_ref": "scan_fast", "question_id": "q2"}


def test_grading_score_preliminary_answer_scoped_concurrency_cancels_queued_work(
    tmp_path: Path,
) -> None:
    token = CancellationToken()
    calls: list[str] = []
    events: list[ProgressEvent] = []
    lock = Lock()

    def responder(request: LlmRequest) -> LlmResponse:
        assert request.prompt_id == "preliminary_score_multi_criterion"
        with lock:
            calls.append(request.rendered_text)
        time.sleep(0.02)
        return LlmResponse(
            raw_text='{"scores":[{"criterion_index":0,"points_awarded":1,"rationale":"Shows the main idea."}]}'
        )

    def event_sink(event: ProgressEvent) -> None:
        events.append(event)
        if event.event == "item_completed":
            token.cancel()

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "grading.score-preliminary",
        {
            "answer_score_requests": [
                _answer_score_request(student_ref=f"scan_{index:03}", question_id=f"q{index}")
                for index in range(1, 7)
            ],
            "grading_runtime_config": {"max_workers": 2},
            "output_artifacts_dir": str((tmp_path / "preliminary_cancel_workers_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
        event_sink=event_sink,
        cancellation_token=token,
    )

    assert result.exit_code == 130
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.code == "cancelled"
    assert len(calls) <= 2
    assert [event.event for event in events].count("item_started") <= 2
    assert [event.event for event in events].count("item_completed") == 1


def test_grading_score_preliminary_requires_one_request_shape(tmp_path: Path) -> None:
    result = _runner().run(
        "grading.score-preliminary",
        {
            "score_requests": [
                {
                    "student_ref": "scan_001",
                    "question_id": "q1",
                    "subject": "Python",
                    "student_answer": "answer",
                    "question_text_clean": "Explain list slicing.",
                    "question_context": "",
                    "rubric_criterion": _rubric_criterion(),
                    "instructor_profile": _instructor_profile(),
                }
            ],
            "answer_score_requests": [
                {
                    "student_ref": "scan_001",
                    "question_id": "q1",
                    "subject": "Python",
                    "student_answer": "answer",
                    "question_text_clean": "Explain list slicing.",
                    "question_context": "",
                    "rubric_criteria": [_rubric_criterion()],
                    "instructor_profile": _instructor_profile(),
                }
            ],
            "output_artifacts_dir": str((tmp_path / "preliminary_invalid_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code != 0
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert "exactly one" in json.dumps(result.envelope.error.details)


def test_grading_run_consistency_deidentifies_prompt_and_repairs_adjustments(
    tmp_path: Path,
) -> None:
    captured: dict[str, str] = {}

    def responder(request: LlmRequest) -> LlmResponse:
        captured["rendered_text"] = request.rendered_text
        return LlmResponse(
            raw_text=(
                '{"adjustments": ['
                '{"student_ref": "student_001", "points_awarded": 1, "adjustment_reason": "same"},'
                '{"student_ref": "student_002", "points_awarded": 1, "adjustment_reason": "first pass"},'
                '{"student_ref": "student_002", "points_awarded": 1, "adjustment_reason": ""}'
                "]}"
            )
        )

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "grading.run-consistency",
        {
            "consistency_requests": [
                {
                    "question_id": "q1",
                    "subject": "Python",
                    "question_text_clean": "Explain list slicing.",
                    "question_context": "",
                    "rubric_criterion": _rubric_criterion(),
                    "instructor_profile": _instructor_profile(),
                    "student_scores": [
                        {
                            "student_ref": "scan_001",
                            "student_answer": "first answer",
                            "blank": False,
                            "preliminary_points_awarded": 1,
                            "preliminary_rationale": "Looks acceptable.",
                            "preliminary_status": "ok",
                            "warnings": [],
                        },
                        {
                            "student_ref": "scan_002",
                            "student_answer": "second answer",
                            "blank": False,
                            "preliminary_points_awarded": 0,
                            "preliminary_rationale": "Missing criterion evidence.",
                            "preliminary_status": "ok",
                            "warnings": [],
                        },
                    ],
                }
            ],
            "output_artifacts_dir": str((tmp_path / "consistency_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    review = result.envelope.data["consistency_reviews"][0]
    assert review["status"] == "ok"
    assert review["adjustments"] == [
        {
            "student_ref": "scan_002",
            "question_id": "q1",
            "criterion_index": 0,
            "points_awarded": 1,
            "adjustment_reason": "Adjusted for rubric consistency.",
            "warnings": [],
        }
    ]
    warning_codes = {item["code"] for item in review["warnings"]}
    assert warning_codes == {
        "consistency_adjustment_unchanged",
        "consistency_adjustment_reason_repaired",
        "consistency_adjustment_duplicate",
    }
    assert "scan_001" not in captured["rendered_text"]
    assert "scan_002" not in captured["rendered_text"]
    assert "student_001" in captured["rendered_text"]
    assert "student_002" in captured["rendered_text"]


def test_grading_run_consistency_invalid_output_is_row_local_error(tmp_path: Path) -> None:
    def responder(request: LlmRequest) -> LlmResponse:
        if "Question one" in request.rendered_text:
            return LlmResponse(
                raw_text='{"adjustments":[{"student_ref":"student_999","points_awarded":1,"adjustment_reason":"bad"}]}'
            )
        return LlmResponse(raw_text='{"adjustments":[]}')

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "grading.run-consistency",
        {
            "consistency_requests": [
                {
                    "question_id": "q1",
                    "subject": "Python",
                    "question_text_clean": "Question one",
                    "question_context": "",
                    "rubric_criterion": _rubric_criterion(),
                    "instructor_profile": _instructor_profile(),
                    "student_scores": [
                        {
                            "student_ref": "scan_001",
                            "student_answer": "first answer",
                            "blank": False,
                            "preliminary_points_awarded": 0,
                            "preliminary_rationale": "Needs review.",
                            "preliminary_status": "ok",
                            "warnings": [],
                        }
                    ],
                },
                {
                    "question_id": "q2",
                    "subject": "Python",
                    "question_text_clean": "Question two",
                    "question_context": "",
                    "rubric_criterion": _rubric_criterion(),
                    "instructor_profile": _instructor_profile(),
                    "student_scores": [
                        {
                            "student_ref": "scan_002",
                            "student_answer": "second answer",
                            "blank": False,
                            "preliminary_points_awarded": 0,
                            "preliminary_rationale": "Needs review.",
                            "preliminary_status": "ok",
                            "warnings": [],
                        }
                    ],
                },
            ],
            "output_artifacts_dir": str((tmp_path / "consistency_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    rows = result.envelope.data["consistency_reviews"]
    assert [row["status"] for row in rows] == ["error", "ok"]
    assert rows[0]["warnings"][0]["code"] == "consistency_review_invalid_output"


def test_grading_draft_feedback_blank_and_fallback_paths(tmp_path: Path) -> None:
    attempts = {"count": 0}

    def responder(request: LlmRequest) -> LlmResponse:
        attempts["count"] += 1
        raise RuntimeError("provider temporarily failed")

    output_dir = (tmp_path / "feedback_out").resolve()
    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "grading.draft-feedback",
        {
            "feedback_requests": [
                {
                    "student_ref": "scan_001",
                    "question_id": "q1",
                    "subject": "Python",
                    "total_points_awarded": 0,
                    "question_max_points": 5,
                    "student_answer": " ",
                    "question_text_clean": "Explain list slicing.",
                    "question_context": "",
                    "rubric_criteria": [_rubric_criterion(points=5)],
                    "criterion_results": [
                        {"criterion_index": 0, "points_awarded": 0, "rationale": "Blank."}
                    ],
                },
                {
                    "student_ref": "scan_002",
                    "question_id": "q1",
                    "subject": "Python",
                    "total_points_awarded": 1,
                    "question_max_points": 5,
                    "student_answer": "some answer",
                    "question_text_clean": "Explain list slicing.",
                    "question_context": "",
                    "rubric_criteria": [_rubric_criterion(points=5)],
                    "criterion_results": [
                        {"criterion_index": 0, "points_awarded": 1, "rationale": "Partial credit."}
                    ],
                },
            ],
            "output_artifacts_dir": str(output_dir),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    rows = result.envelope.data["feedback_drafts"]
    assert rows[0]["feedback_source"] == "blank_local"
    assert rows[0]["feedback_text"] == "No relevant answer provided."
    assert rows[1]["feedback_source"] == "default_fallback"
    assert rows[1]["feedback_text"] == "Needs instructor review."
    assert rows[1]["warnings"][0]["code"] == "feedback_default_fallback"
    assert attempts["count"] == 2
    trace_dir = output_dir / "traces"
    assert sorted(path.name for path in trace_dir.glob("*.json")) == [
        "feedback_draft__question_id-q1__student_ref-scan_001.json",
        "feedback_draft__question_id-q1__student_ref-scan_002.json",
        "feedback_draft__question_id-q1__student_ref-scan_002__attempt_02.json",
    ]
    manifest = json.loads(
        Path(result.envelope.data["output_metadata_path"]).read_text(encoding="utf-8")
    )
    manifest_trace_names = sorted(Path(artifact["path"]).name for artifact in manifest["artifacts"])
    assert manifest_trace_names == sorted(path.name for path in trace_dir.glob("*.json"))
    assert manifest["data"]["written_artifact_count"] == len(manifest["artifacts"]) == 3


def test_grading_markup_blank_and_fallback_paths(tmp_path: Path) -> None:
    call_count = 0

    def responder(request: LlmRequest) -> LlmResponse:
        nonlocal call_count
        call_count += 1
        assert request.prompt_id == "markup"
        assert request.response_mode == "json_schema"
        return LlmResponse(raw_text="this is not json")

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "grading.markup",
        {
            "markup_requests": [
                {
                    "student_ref": "scan_001",
                    "question_id": "q1",
                    "subject": "Python",
                    "total_points_awarded": 0,
                    "question_max_points": 5,
                    "student_answer": "",
                    "question_text_clean": "Explain list slicing.",
                    "question_context": "",
                    "rubric_criteria": [_rubric_criterion(points=5)],
                    "criterion_results": [
                        {"criterion_index": 0, "points_awarded": 0, "rationale": "Blank."}
                    ],
                },
                {
                    "student_ref": "scan_002",
                    "question_id": "q1",
                    "subject": "Python",
                    "total_points_awarded": 1,
                    "question_max_points": 5,
                    "student_answer": "abc def ghi",
                    "question_text_clean": "Explain list slicing.",
                    "question_context": "",
                    "rubric_criteria": [_rubric_criterion(points=5)],
                    "criterion_results": [
                        {"criterion_index": 0, "points_awarded": 1, "rationale": "Partial credit."}
                    ],
                },
            ],
            "output_artifacts_dir": str((tmp_path / "markup_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    rows = result.envelope.data["highlight_results"]
    assert rows[0]["status"] == "ok"
    assert rows[0]["highlights"] == []
    assert rows[1]["status"] == "fallback"
    assert rows[1]["highlights"] == []
    assert rows[1]["warnings"][0]["code"] == "markup_fallback"
    assert call_count == 1


def test_grading_markup_exact_incorrect_substrings_return_highlights(tmp_path: Path) -> None:
    def responder(request: LlmRequest) -> LlmResponse:
        assert request.prompt_id == "markup"
        assert request.response_mode == "json_schema"
        assert request.response_contract is not None
        assert request.execution_options["temperature"] == 0.0
        assert request.execution_options["top_p"] == 1
        assert request.execution_options["top_k"] == 1
        assert request.execution_options["repeat_penalty"] == 1.05
        assert request.execution_options["num_predict"] == 512
        return LlmResponse(
            raw_text=json.dumps(
                {
                    "incorrect_segments": [
                        "xs[1:]",
                    ]
                }
            )
        )

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "grading.markup",
        {
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
                    "rubric_criteria": [_rubric_criterion(points=1)],
                    "criterion_results": [
                        {"criterion_index": 0, "points_awarded": 1, "rationale": "Looks fine."}
                    ],
                }
            ],
            "output_artifacts_dir": str((tmp_path / "markup_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["highlight_results"][0]
    assert row["status"] == "ok"
    assert row["warnings"] == []
    assert row["highlights"] == [
        {"kind": "incorrect", "start_char": 7, "end_char": 13, "text": "xs[1:]"},
    ]


def test_grading_markup_repeated_substrings_use_first_unused_occurrence(tmp_path: Path) -> None:
    def responder(request: LlmRequest) -> LlmResponse:
        return LlmResponse(
            raw_text=json.dumps(
                {
                    "incorrect_segments": [
                        "return",
                        "return",
                    ]
                }
            )
        )

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "grading.markup",
        {
            "markup_requests": [
                {
                    "student_ref": "scan_001",
                    "question_id": "q1",
                    "subject": "Python",
                    "total_points_awarded": 1,
                    "question_max_points": 1,
                    "student_answer": "return xs then return ys",
                    "question_text_clean": "Explain list slicing.",
                    "question_context": "",
                    "rubric_criteria": [_rubric_criterion(points=1)],
                    "criterion_results": [
                        {"criterion_index": 0, "points_awarded": 1, "rationale": "Looks fine."}
                    ],
                }
            ],
            "output_artifacts_dir": str((tmp_path / "markup_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["highlight_results"][0]
    assert row["status"] == "ok"
    assert row["highlights"] == [
        {"kind": "incorrect", "start_char": 0, "end_char": 6, "text": "return"},
        {"kind": "incorrect", "start_char": 15, "end_char": 21, "text": "return"},
    ]


def test_grading_markup_drops_missing_substrings_without_retry(tmp_path: Path) -> None:
    call_count = 0

    def responder(request: LlmRequest) -> LlmResponse:
        nonlocal call_count
        call_count += 1
        return LlmResponse(
            raw_text=json.dumps(
                {
                    "incorrect_segments": [
                        "xs[1:]",
                        "not copied from answer",
                    ]
                }
            )
        )

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "grading.markup",
        {
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
                    "rubric_criteria": [_rubric_criterion(points=1)],
                    "criterion_results": [
                        {"criterion_index": 0, "points_awarded": 1, "rationale": "Looks fine."}
                    ],
                }
            ],
            "output_artifacts_dir": str((tmp_path / "markup_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["highlight_results"][0]
    assert row["status"] == "ok"
    assert row["highlights"] == [
        {"kind": "incorrect", "start_char": 7, "end_char": 13, "text": "xs[1:]"}
    ]
    assert call_count == 1


def test_grading_markup_caps_incorrect_segments_at_four(tmp_path: Path) -> None:
    def responder(request: LlmRequest) -> LlmResponse:
        return LlmResponse(
            raw_text=json.dumps(
                {
                    "incorrect_segments": [
                        "one",
                        "two",
                        "three",
                        "four",
                        "five",
                    ]
                }
            )
        )

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "grading.markup",
        {
            "markup_requests": [
                {
                    "student_ref": "scan_001",
                    "question_id": "q1",
                    "subject": "Python",
                    "total_points_awarded": 1,
                    "question_max_points": 1,
                    "student_answer": "one two three four five",
                    "question_text_clean": "Explain list slicing.",
                    "question_context": "",
                    "rubric_criteria": [_rubric_criterion(points=1)],
                    "criterion_results": [
                        {"criterion_index": 0, "points_awarded": 1, "rationale": "Looks fine."}
                    ],
                }
            ],
            "output_artifacts_dir": str((tmp_path / "markup_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["highlight_results"][0]
    assert row["status"] == "ok"
    assert [highlight["text"] for highlight in row["highlights"]] == [
        "one",
        "two",
        "three",
        "four",
    ]


@pytest.mark.parametrize(
    "raw_text",
    [
        json.dumps({"incorrect_segments": ["not copied from answer"]}),
        json.dumps({"incorrect_segments": ["missing", "also missing"]}),
        json.dumps({"incorrect_segments": []}),
    ],
)
def test_grading_markup_no_valid_incorrect_segments_returns_empty_highlights(
    tmp_path: Path, raw_text: str
) -> None:
    call_count = 0

    def responder(request: LlmRequest) -> LlmResponse:
        nonlocal call_count
        call_count += 1
        return LlmResponse(raw_text=raw_text)

    result = _runner(provider_registry=_registry_with_llm(responder)).run(
        "grading.markup",
        {
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
                    "rubric_criteria": [_rubric_criterion(points=1)],
                    "criterion_results": [
                        {"criterion_index": 0, "points_awarded": 1, "rationale": "Looks fine."}
                    ],
                }
            ],
            "output_artifacts_dir": str((tmp_path / "markup_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["highlight_results"][0]
    assert row["status"] == "ok"
    assert row["highlights"] == []
    assert row["warnings"] == []
    assert call_count == 1


def test_feedback_and_markup_reject_points_above_matching_rubric_max() -> None:
    invalid_payload = {
        "student_ref": "scan_001",
        "question_id": "q1",
        "subject": "Python",
        "total_points_awarded": 2,
        "question_max_points": 2,
        "student_answer": "answer",
        "question_text_clean": "Explain list slicing.",
        "question_context": "",
        "rubric_criteria": [
            _rubric_criterion(criterion_index=0, points=1),
            _rubric_criterion(criterion_index=1, points=1),
        ],
        "criterion_results": [
            {"criterion_index": 0, "points_awarded": 2, "rationale": "Too high."},
            {"criterion_index": 1, "points_awarded": 0, "rationale": "Zero."},
        ],
    }

    with pytest.raises(ValidationError) as feedback_excinfo:
        FeedbackRequest.model_validate(invalid_payload)
    assert "question_id='q1'" in str(feedback_excinfo.value)
    assert "student_ref='scan_001'" in str(feedback_excinfo.value)

    with pytest.raises(ValidationError) as markup_excinfo:
        MarkupRequest.model_validate(invalid_payload)
    assert "question_id='q1'" in str(markup_excinfo.value)
    assert "student_ref='scan_001'" in str(markup_excinfo.value)


def test_grading_draft_feedback_validation_error_names_question_context(tmp_path: Path) -> None:
    output_dir = (tmp_path / "feedback_out").resolve()

    result = _runner().run(
        "grading.draft-feedback",
        {
            "feedback_requests": [
                {
                    "student_ref": "scan_001",
                    "question_id": "q14",
                    "subject": "Python",
                    "total_points_awarded": 6,
                    "question_max_points": 5,
                    "student_answer": "return xs[1:]",
                    "question_text_clean": "Explain list slicing.",
                    "question_context": "",
                    "rubric_criteria": [_rubric_criterion(points=5)],
                    "criterion_results": [
                        {"criterion_index": 0, "points_awarded": 5, "rationale": "Mostly correct."}
                    ],
                }
            ],
            "output_artifacts_dir": str(output_dir),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 2
    assert isinstance(result.envelope, CommandErrorEnvelope)
    issues = result.envelope.error.details["issues"]
    assert issues[0]["path"] == ["feedback_requests", 0]
    assert "question_id='q14'" in issues[0]["message"]
    assert "student_ref='scan_001'" in issues[0]["message"]


def test_grading_export_request_rejects_duplicate_student_refs(tmp_path: Path) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png", size=(20, 20))

    with pytest.raises(ValidationError):
        GradingExportRequest.model_validate(
            {
                "export_requests": [
                    {
                        "student_ref": "scan_001",
                        "questions": [
                            {
                                "question_id": "q1",
                                "question_max_points": 1,
                                "total_points_awarded": 1,
                                "question_text_clean": "Explain list slicing.",
                                "student_answer": "return xs[1:]",
                                "question_crop_path": str(crop),
                            }
                        ],
                    },
                    {
                        "student_ref": "scan_001",
                        "questions": [
                            {
                                "question_id": "q2",
                                "question_max_points": 1,
                                "total_points_awarded": 1,
                                "question_text_clean": "Explain loops.",
                                "student_answer": "for i in xs",
                                "question_crop_path": str(crop),
                            }
                        ],
                    },
                ],
                "output_artifacts_dir": str((tmp_path / "export_out").resolve()),
            }
        )


def test_grading_export_writes_self_contained_html(tmp_path: Path) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png", size=(20, 20))
    output_dir = (tmp_path / "export_out").resolve()

    result = _runner().run(
        "grading.export",
        {
            "export_requests": [
                {
                    "student_ref": "scan_001",
                    "student_display_name": "Ada Lovelace",
                    "questions": [
                        {
                            "question_id": "q1",
                            "question_max_points": 5,
                            "total_points_awarded": 4,
                            "question_text_clean": "Explain list slicing.",
                            "student_answer": "return xs[1:]",
                            "question_crop_path": str(crop),
                            "feedback_text": "Show one more detail next time.",
                            "highlights": [
                                {
                                    "kind": "correct",
                                    "start_char": 0,
                                    "end_char": 6,
                                    "text": "return",
                                }
                            ],
                        }
                    ],
                }
            ],
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    export_row = result.envelope.data["exports"][0]
    html_path = Path(export_row["html_path"])
    html = html_path.read_text(encoding="utf-8")
    assert "data:image/png;base64," in html
    assert "Ada Lovelace" in html
    assert "xs[1:]" in html
    assert "highlight-correct" in html
    assert "white-space: pre-wrap;" in html


def test_grading_export_preserves_answer_whitespace_in_html(tmp_path: Path) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png", size=(20, 20))
    output_dir = (tmp_path / "export_out").resolve()
    student_answer = "if x:\n    return 1"

    result = _runner().run(
        "grading.export",
        {
            "export_requests": [
                {
                    "student_ref": "scan_001",
                    "questions": [
                        {
                            "question_id": "q1",
                            "question_max_points": 5,
                            "total_points_awarded": 4,
                            "question_text_clean": "Write a simple branch.",
                            "student_answer": student_answer,
                            "question_crop_path": str(crop),
                        }
                    ],
                }
            ],
            "output_artifacts_dir": str(output_dir),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    html_path = Path(result.envelope.data["exports"][0]["html_path"])
    html = html_path.read_text(encoding="utf-8")
    assert "white-space: pre-wrap;" in html
    assert "if x:<br>" in html
    assert "    return 1" in html
