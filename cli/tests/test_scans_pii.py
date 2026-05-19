# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for scans.pii and parse prescreen reuse."""

from __future__ import annotations

import os
import time
from collections.abc import Callable
from pathlib import Path
from threading import Event, Lock

import pytest
from pydantic import ValidationError

from scriptscore.commands import build_command_registry
from scriptscore.contracts import (
    CommandErrorEnvelope,
    CommandSuccessEnvelope,
    ParseDraft,
    ProgressEvent,
    ScansParseRequest,
    ScansPiiRequest,
)
from scriptscore.pii_scan import ScanFinding, ScanRuntimeOptions, inspect_student_crop
from scriptscore.providers import FakeLlmProvider, LlmRequest, LlmResponse, ProviderRegistry
from scriptscore.runtime import CommandRunner
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


def _fake_model_root(root: Path) -> Path:
    model_root = (root / "models" / "paddle").resolve()
    for leaf in ("det", "rec"):
        target = model_root / leaf
        target.mkdir(parents=True, exist_ok=True)
        (target / "inference.yml").write_text("test", encoding="utf-8")
        (target / "inference.json").write_text("{}", encoding="utf-8")
    return model_root


def test_scans_pii_request_rejects_duplicate_question_ids(tmp_path: Path) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png")
    with pytest.raises(ValidationError):
        ScansPiiRequest.model_validate(
            {
                "students": [
                    {
                        "student_ref": "scan_001",
                        "pii_trigger_words": ["Alice Smith"],
                        "pii_targets": [
                            {"question_id": "q1", "question_crop_path": str(crop)},
                            {"question_id": "q1", "question_crop_path": str(crop)},
                        ],
                    }
                ],
                "output_artifacts_dir": str((tmp_path / "out").resolve()),
                "pii_runtime_config": {
                    "paddle_model_dir": str(_fake_model_root(tmp_path)),
                },
            }
        )


def test_parse_models_accept_pii_prescreen_extensions(tmp_path: Path) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png")
    template = make_rgb_page(tmp_path / "template" / "q1.png")

    request = ScansParseRequest.model_validate(
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
                    "pii_prescreen": {
                        "source_command": "scans.pii",
                        "status": "ok",
                        "contains_handwriting": "true",
                        "contains_pii": False,
                    },
                }
            ],
            "output_artifacts_dir": str((tmp_path / "out").resolve()),
            **llm_request_fields("ollama_native"),
        }
    )

    draft = ParseDraft(
        student_ref="scan_001",
        question_id="q1",
        status="blank",
        parsed_text="",
        blank=True,
        confidence="high",
        confidence_source="pii_prescreen",
        warnings=[],
    )

    assert request.parse_targets[0].pii_prescreen is not None
    assert draft.confidence_source == "pii_prescreen"


def test_scans_pii_happy_path_writes_manifest_and_redacts_trigger_strings(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png")
    output_dir = (tmp_path / "pii_out").resolve()
    trigger_words = ["Alice Smith", "alice@example.edu", "asmith42"]

    monkeypatch.setattr("scriptscore.commands.scans_pii.verify_model_root", lambda _path: None)
    monkeypatch.setattr(
        "scriptscore.commands.scans_pii.create_reader",
        lambda _path: object(),
    )
    monkeypatch.setattr(
        "scriptscore.commands.scans_pii.inspect_student_crop",
        lambda *_args, **_kwargs: ScanFinding(
            image_path=str(crop),
            handwriting_state="true",
            handwriting_score=0.91,
            pii_present=True,
            pii_kinds=["email", "name"],
            reasons=["synthetic test result"],
            duration_seconds=0.01,
            metrics={"backend_name": "paddleocr_test_override"},
        ),
    )

    result = _runner().run(
        "scans.pii",
        {
            "students": [
                {
                    "student_ref": "scan_001",
                    "pii_trigger_words": trigger_words,
                    "pii_targets": [
                        {"question_id": "q1", "question_crop_path": str(crop)},
                    ],
                }
            ],
            "output_artifacts_dir": str(output_dir),
            "pii_runtime_config": {
                "paddle_model_dir": str((tmp_path / "models").resolve()),
            },
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["pii_results"][0]
    assert row["status"] == "ok"
    assert row["contains_handwriting"] == "true"
    assert row["contains_pii"] is True
    assert row["pii_types_detected"] == ["email", "name"]

    trace_dir = output_dir / "traces"
    trace_text = next(trace_dir.glob("*.json")).read_text(encoding="utf-8")
    manifest_text = (output_dir / "output_metadata.json").read_text(encoding="utf-8")
    envelope_text = result.envelope.model_dump_json(exclude_none=True)
    for trigger in trigger_words:
        assert trigger not in trace_text
        assert trigger not in manifest_text
        assert trigger not in envelope_text


def test_scans_pii_parallel_workers_preserve_result_order(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    q1 = make_rgb_page(tmp_path / "scan_001" / "q1.png")
    q2 = make_rgb_page(tmp_path / "scan_001" / "q2.png")

    monkeypatch.setattr("scriptscore.commands.scans_pii.verify_model_root", lambda _path: None)
    monkeypatch.setattr("scriptscore.commands.scans_pii.create_reader", lambda _path: object())

    def inspect(path: Path, *_args: object, **_kwargs: object) -> ScanFinding:
        if path.name == "q1.png":
            time.sleep(0.03)
        return ScanFinding(
            image_path=str(path),
            handwriting_state="true",
            handwriting_score=0.91,
            pii_present=False,
            pii_kinds=[],
            reasons=["synthetic test result"],
            duration_seconds=0.01,
            metrics={"backend_name": "paddleocr_test_override"},
        )

    monkeypatch.setattr("scriptscore.commands.scans_pii.inspect_student_crop", inspect)

    result = _runner().run(
        "scans.pii",
        {
            "students": [
                {
                    "student_ref": "scan_001",
                    "pii_trigger_words": ["Alice Smith"],
                    "pii_targets": [
                        {"question_id": "q1", "question_crop_path": str(q1)},
                        {"question_id": "q2", "question_crop_path": str(q2)},
                    ],
                }
            ],
            "output_artifacts_dir": str((tmp_path / "pii_parallel_out").resolve()),
            "pii_runtime_config": {
                "paddle_model_dir": str((tmp_path / "models").resolve()),
                "max_workers": 2,
            },
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    rows = result.envelope.data["pii_results"]
    assert [row["question_id"] for row in rows] == ["q1", "q2"]


def test_scans_pii_progress_starts_items_when_analysis_reaches_them(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    q1 = make_rgb_page(tmp_path / "scan_001" / "q1.png")
    q2 = make_rgb_page(tmp_path / "scan_001" / "q2.png")
    events: list[ProgressEvent] = []

    monkeypatch.setattr("scriptscore.commands.scans_pii.verify_model_root", lambda _path: None)
    monkeypatch.setattr("scriptscore.commands.scans_pii.create_reader", lambda _path: object())
    monkeypatch.setattr(
        "scriptscore.commands.scans_pii.inspect_student_crop",
        lambda path, *_args, **_kwargs: ScanFinding(
            image_path=str(path),
            handwriting_state="true",
            handwriting_score=0.91,
            pii_present=False,
            pii_kinds=[],
            reasons=["synthetic test result"],
            duration_seconds=0.01,
            metrics={"backend_name": "paddleocr_test_override"},
        ),
    )

    result = _runner().run(
        "scans.pii",
        {
            "students": [
                {
                    "student_ref": "scan_001",
                    "pii_trigger_words": ["Alice Smith"],
                    "pii_targets": [
                        {"question_id": "q1", "question_crop_path": str(q1)},
                        {"question_id": "q2", "question_crop_path": str(q2)},
                    ],
                }
            ],
            "output_artifacts_dir": str((tmp_path / "pii_progress_out").resolve()),
            "pii_runtime_config": {
                "paddle_model_dir": str((tmp_path / "models").resolve()),
            },
        },
        event_sink=events.append,
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    item_events = [event for event in events if event.event in {"item_started", "item_completed"}]
    assert [
        (
            event.event,
            event.scope,
            None if event.progress is None else event.progress.completed,
            None if event.progress is None else event.progress.total,
        )
        for event in item_events
    ] == [
        ("item_started", {"student_ref": "scan_001", "question_id": "q1"}, 0, 2),
        ("item_completed", {"student_ref": "scan_001", "question_id": "q1"}, 1, 2),
        ("item_started", {"student_ref": "scan_001", "question_id": "q2"}, 1, 2),
        ("item_completed", {"student_ref": "scan_001", "question_id": "q2"}, 2, 2),
    ]


def test_scans_pii_parallel_progress_is_not_front_loaded(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    crops = [make_rgb_page(tmp_path / "scan_001" / f"q{index}.png") for index in range(1, 5)]
    events: list[ProgressEvent] = []
    first_wave_started = Event()
    release_slow_worker = Event()
    inspected_initial: set[str] = set()
    lock = Lock()

    monkeypatch.setattr("scriptscore.commands.scans_pii.verify_model_root", lambda _path: None)
    monkeypatch.setattr("scriptscore.commands.scans_pii.create_reader", lambda _path: object())

    def inspect(path: Path, *_args: object, **_kwargs: object) -> ScanFinding:
        question = path.stem
        if question in {"q1", "q2"}:
            with lock:
                inspected_initial.add(question)
                if inspected_initial == {"q1", "q2"}:
                    first_wave_started.set()
            assert first_wave_started.wait(timeout=1.0)
        if question == "q1":
            assert release_slow_worker.wait(timeout=1.0)
        elif question == "q4":
            release_slow_worker.set()
        return ScanFinding(
            image_path=str(path),
            handwriting_state="true",
            handwriting_score=0.91,
            pii_present=False,
            pii_kinds=[],
            reasons=["synthetic test result"],
            duration_seconds=0.01,
            metrics={"backend_name": "paddleocr_test_override"},
        )

    monkeypatch.setattr("scriptscore.commands.scans_pii.inspect_student_crop", inspect)

    result = _runner().run(
        "scans.pii",
        {
            "students": [
                {
                    "student_ref": "scan_001",
                    "pii_trigger_words": ["Alice Smith"],
                    "pii_targets": [
                        {"question_id": f"q{index}", "question_crop_path": str(crop)}
                        for index, crop in enumerate(crops, start=1)
                    ],
                }
            ],
            "output_artifacts_dir": str((tmp_path / "pii_parallel_progress_out").resolve()),
            "pii_runtime_config": {
                "paddle_model_dir": str((tmp_path / "models").resolve()),
                "max_workers": 2,
            },
        },
        event_sink=events.append,
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    item_events = [event for event in events if event.event in {"item_started", "item_completed"}]
    first_completed_index = next(
        index for index, event in enumerate(item_events) if event.event == "item_completed"
    )
    starts_before_first_completion = [
        event for event in item_events[:first_completed_index] if event.event == "item_started"
    ]
    assert len(starts_before_first_completion) <= 2
    assert any(
        event.event == "item_started"
        and event.scope == {"student_ref": "scan_001", "question_id": "q4"}
        for event in item_events[first_completed_index + 1 :]
    )
    completed_counts = []
    for event in item_events:
        if event.event == "item_completed":
            assert event.progress is not None
            completed_counts.append(event.progress.completed)
    assert completed_counts == [1, 2, 3, 4]
    assert [row["question_id"] for row in result.envelope.data["pii_results"]] == [
        "q1",
        "q2",
        "q3",
        "q4",
    ]


def test_scans_pii_request_rejects_too_many_workers(tmp_path: Path) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png")
    with pytest.raises(ValidationError):
        ScansPiiRequest.model_validate(
            {
                "students": [
                    {
                        "student_ref": "scan_001",
                        "pii_trigger_words": ["Alice Smith"],
                        "pii_targets": [{"question_id": "q1", "question_crop_path": str(crop)}],
                    }
                ],
                "output_artifacts_dir": str((tmp_path / "out").resolve()),
                "pii_runtime_config": {
                    "paddle_model_dir": str(_fake_model_root(tmp_path)),
                    "max_workers": 5,
                },
            }
        )


def test_scans_pii_request_rejects_zero_targets(tmp_path: Path) -> None:
    with pytest.raises(ValidationError):
        ScansPiiRequest.model_validate(
            {
                "students": [
                    {
                        "student_ref": "scan_001",
                        "pii_trigger_words": ["Alice Smith"],
                        "pii_targets": [],
                    }
                ],
                "output_artifacts_dir": str((tmp_path / "out").resolve()),
                "pii_runtime_config": {
                    "paddle_model_dir": str(_fake_model_root(tmp_path)),
                },
            }
        )


def test_scans_pii_missing_crop_hard_fails_without_writes(tmp_path: Path) -> None:
    missing_crop = (tmp_path / "scan_001" / "q1.png").resolve()
    output_dir = (tmp_path / "pii_out").resolve()

    result = _runner().run(
        "scans.pii",
        {
            "students": [
                {
                    "student_ref": "scan_001",
                    "pii_trigger_words": ["Alice Smith"],
                    "pii_targets": [
                        {"question_id": "q1", "question_crop_path": str(missing_crop)},
                    ],
                }
            ],
            "output_artifacts_dir": str(output_dir),
            "pii_runtime_config": {
                "paddle_model_dir": str(_fake_model_root(tmp_path)),
            },
        },
    )

    assert result.exit_code == 3
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert not (output_dir / "output_metadata.json").exists()


def test_scans_pii_invalid_model_dir_fails_as_prerequisite(tmp_path: Path) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png")
    output_dir = (tmp_path / "pii_out").resolve()

    result = _runner().run(
        "scans.pii",
        {
            "students": [
                {
                    "student_ref": "scan_001",
                    "pii_trigger_words": ["Alice Smith"],
                    "pii_targets": [
                        {"question_id": "q1", "question_crop_path": str(crop)},
                    ],
                }
            ],
            "output_artifacts_dir": str(output_dir),
            "pii_runtime_config": {
                "paddle_model_dir": str((tmp_path / "missing_models").resolve()),
            },
        },
    )

    assert result.exit_code == 2
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.code == "pii_runtime_invalid"


def test_scans_pii_reader_creation_failure_hard_fails_without_writes(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png")
    output_dir = (tmp_path / "pii_out").resolve()

    monkeypatch.setattr("scriptscore.commands.scans_pii.verify_model_root", lambda _path: None)

    def create_reader(_path: Path) -> object:
        raise RuntimeError("runtime exploded")

    monkeypatch.setattr("scriptscore.commands.scans_pii.create_reader", create_reader)

    result = _runner().run(
        "scans.pii",
        {
            "students": [
                {
                    "student_ref": "scan_001",
                    "pii_trigger_words": ["Alice Smith"],
                    "pii_targets": [{"question_id": "q1", "question_crop_path": str(crop)}],
                }
            ],
            "output_artifacts_dir": str(output_dir),
            "pii_runtime_config": {
                "paddle_model_dir": str((tmp_path / "models").resolve()),
            },
        },
    )

    assert result.exit_code == 6
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.code == "pii_runtime_unavailable"
    assert result.envelope.error.category == "external_dependency"
    assert result.envelope.error.write_state == "no_write"
    assert not (output_dir / "output_metadata.json").exists()


def test_scans_pii_warning_rows_are_degraded_and_scrub_backend_warnings(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png")
    output_dir = (tmp_path / "pii_out").resolve()
    trigger_words = ["Alice Smith"]

    monkeypatch.setattr("scriptscore.commands.scans_pii.verify_model_root", lambda _path: None)
    monkeypatch.setattr("scriptscore.commands.scans_pii.create_reader", lambda _path: object())
    monkeypatch.setattr(
        "scriptscore.commands.scans_pii.inspect_student_crop",
        lambda *_args, **_kwargs: ScanFinding(
            image_path=str(crop),
            handwriting_state="unknown",
            handwriting_score=0.22,
            pii_present=False,
            pii_kinds=[],
            reasons=["synthetic warning"],
            duration_seconds=0.01,
            metrics={"backend_name": "paddleocr_test_override"},
            backend_warnings=["low confidence near Alice Smith"],
        ),
    )

    result = _runner().run(
        "scans.pii",
        {
            "students": [
                {
                    "student_ref": "scan_001",
                    "pii_trigger_words": trigger_words,
                    "pii_targets": [{"question_id": "q1", "question_crop_path": str(crop)}],
                }
            ],
            "output_artifacts_dir": str(output_dir),
            "pii_runtime_config": {
                "paddle_model_dir": str((tmp_path / "models").resolve()),
            },
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    assert result.envelope.warnings[0].code == "pii_analysis_warning_rows"
    row = result.envelope.data["pii_results"][0]
    assert row["status"] == "warning"
    assert row["contains_handwriting"] == "unknown"
    assert [item["code"] for item in row["warnings"]] == [
        "pii_handwriting_unknown",
        "pii_analysis_degraded",
    ]
    assert "[redacted-trigger]" in row["warnings"][1]["message"]
    assert "Alice Smith" not in result.envelope.model_dump_json(exclude_none=True)


def test_scans_pii_fatal_rows_scrub_triggers_from_outputs_and_count_failures(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png")
    output_dir = (tmp_path / "pii_out").resolve()
    trigger_words = ["Alice Smith", "alice@example.edu"]

    monkeypatch.setattr("scriptscore.commands.scans_pii.verify_model_root", lambda _path: None)
    monkeypatch.setattr("scriptscore.commands.scans_pii.create_reader", lambda _path: object())
    monkeypatch.setattr(
        "scriptscore.commands.scans_pii.inspect_student_crop",
        lambda *_args, **_kwargs: ScanFinding(
            image_path=str(crop),
            handwriting_state="unknown",
            handwriting_score=0.0,
            pii_present=False,
            pii_kinds=[],
            reasons=["synthetic fatal"],
            duration_seconds=0.01,
            metrics={"backend_name": "paddleocr_test_override"},
            fatal_error="failed while checking Alice Smith and alice@example.edu",
        ),
    )

    result = _runner().run(
        "scans.pii",
        {
            "students": [
                {
                    "student_ref": "scan_001",
                    "pii_trigger_words": trigger_words,
                    "pii_targets": [{"question_id": "q1", "question_crop_path": str(crop)}],
                }
            ],
            "output_artifacts_dir": str(output_dir),
            "pii_runtime_config": {
                "paddle_model_dir": str((tmp_path / "models").resolve()),
            },
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.degraded is True
    assert [warning.code for warning in result.envelope.warnings] == ["partial_failure"]
    row = result.envelope.data["pii_results"][0]
    assert row["status"] == "error"
    assert row["contains_pii"] is False
    assert row["pii_types_detected"] == []
    assert row["warnings"][0]["message"] == (
        "failed while checking [redacted-trigger] and [redacted-trigger]"
    )

    trace_text = next((output_dir / "traces").glob("*.json")).read_text(encoding="utf-8")
    manifest_text = (output_dir / "output_metadata.json").read_text(encoding="utf-8")
    envelope_text = result.envelope.model_dump_json(exclude_none=True)
    assert '"failed_count":1' in manifest_text.replace(" ", "")
    for trigger in trigger_words:
        assert trigger not in trace_text
        assert trigger not in manifest_text
        assert trigger not in envelope_text


def test_scans_parse_reuses_clean_pii_prescreen_and_skips_handwriting_prompt(
    tmp_path: Path,
) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png")
    template = make_rgb_page(tmp_path / "template" / "q1.png")
    call_sequence: list[str] = []

    def responder(request: LlmRequest) -> LlmResponse:
        call_sequence.append(request.prompt_id)
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
                    "pii_prescreen": {
                        "source_command": "scans.pii",
                        "status": "ok",
                        "contains_handwriting": "true",
                        "contains_pii": False,
                    },
                }
            ],
            "output_artifacts_dir": str((tmp_path / "parse_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert call_sequence == ["parse_ocr"]
    row = result.envelope.data["parse_results"][0]
    assert row["status"] == "ok"
    assert row["confidence_source"] == "combined"
    trace_dir = Path(result.envelope.data["output_metadata_path"]).parent / "traces"
    assert sorted(path.name for path in trace_dir.glob("*.json")) == [
        "parse_ocr__question_id-q1__student_ref-scan_001.json",
        "pii_prescreen__question_id-q1__student_ref-scan_001.json",
    ]


def test_scans_parse_blank_short_circuits_from_clean_pii_prescreen(tmp_path: Path) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png")
    template = make_rgb_page(tmp_path / "template" / "q1.png")

    def responder(request: LlmRequest) -> LlmResponse:
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
                    "pii_prescreen": {
                        "source_command": "scans.pii",
                        "status": "ok",
                        "contains_handwriting": "false",
                        "contains_pii": False,
                    },
                }
            ],
            "output_artifacts_dir": str((tmp_path / "parse_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    row = result.envelope.data["parse_results"][0]
    assert row["status"] == "blank"
    assert row["blank"] is True
    assert row["confidence_source"] == "pii_prescreen"
    trace_dir = Path(result.envelope.data["output_metadata_path"]).parent / "traces"
    assert [path.name for path in trace_dir.glob("*.json")] == [
        "pii_prescreen__question_id-q1__student_ref-scan_001.json"
    ]


def test_scans_parse_nonclean_pii_prescreen_falls_back_to_handwriting_prompt(
    tmp_path: Path,
) -> None:
    crop = make_rgb_page(tmp_path / "scan_001" / "q1.png")
    template = make_rgb_page(tmp_path / "template" / "q1.png")
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
                        "question_text_clean": "Explain the result.",
                    },
                    "question_crop_path": str(crop),
                    "template_question_png_path": str(template),
                    "pii_prescreen": {
                        "source_command": "scans.pii",
                        "status": "warning",
                        "contains_handwriting": "unknown",
                        "contains_pii": False,
                    },
                }
            ],
            "output_artifacts_dir": str((tmp_path / "parse_out").resolve()),
            **llm_request_fields("ollama_native"),
        },
    )

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert call_sequence == ["handwriting_verify", "parse_ocr"]
    trace_dir = Path(result.envelope.data["output_metadata_path"]).parent / "traces"
    assert sorted(path.name for path in trace_dir.glob("*.json")) == [
        "handwriting_verify__question_id-q1__student_ref-scan_001.json",
        "parse_ocr__question_id-q1__student_ref-scan_001.json",
    ]


def _fixture_model_root() -> Path | None:
    raw = os.environ.get("SCRIPTSCORE_TEST_PII_PADDLE_MODEL_DIR")
    if not raw:
        return None
    model_root = Path(raw).resolve()
    return model_root if model_root.is_dir() else None


@pytest.mark.parametrize(
    ("relative_path", "trigger_words", "expect_pii", "expect_handwriting"),
    [
        (
            "positive/pii-email.png",
            ["student@example.edu"],
            True,
            "true",
        ),
        (
            "positive/pii-synthetic-name.png",
            ["Harper", "Rivera"],
            True,
            "true",
        ),
        (
            "negative/handwritten-no-pii.png",
            ["Alice Smith", "alice@example.edu", "asmith42"],
            False,
            "true",
        ),
        (
            "negative/student_1_q1.png",
            ["Alice Smith", "alice@example.edu", "asmith42"],
            False,
            None,
        ),
        (
            "negative/student_2_q1.png",
            ["Bob Jones", "bob@example.edu", "bjones77"],
            False,
            None,
        ),
    ],
)
def test_local_scan_fixture_subset(
    relative_path: str,
    trigger_words: list[str],
    expect_pii: bool,
    expect_handwriting: str | None,
) -> None:
    model_root = _fixture_model_root()
    if model_root is None:
        pytest.skip("SCRIPTSCORE_TEST_PII_PADDLE_MODEL_DIR is not configured")
    pytest.importorskip("paddle")
    pytest.importorskip("paddleocr")

    fixture_path = (
        Path(__file__).resolve().parent / "fixtures" / "scans_pii" / relative_path
    ).resolve()
    finding = inspect_student_crop(
        fixture_path,
        trigger_words=trigger_words,
        options=ScanRuntimeOptions(model_root=model_root),
    )

    assert finding.handwriting_state in {"true", "false", "unknown"}
    if expect_handwriting is not None:
        assert finding.handwriting_state == expect_handwriting
    assert finding.pii_present is expect_pii
    if expect_pii:
        assert finding.handwriting_state == "true"
        assert finding.pii_kinds
    else:
        assert not finding.pii_kinds
