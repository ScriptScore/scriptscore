# SPDX-License-Identifier: AGPL-3.0-only
"""In-process engine tests."""

from __future__ import annotations

from pathlib import Path
from threading import Thread
from time import monotonic, sleep
from typing import Any

from scriptscore.contracts import (
    CommandErrorEnvelope,
    CommandSuccessEnvelope,
    ErrorCategory,
    ProgressEvent,
)
from scriptscore.engine import ScriptScoreEngine, create_engine
from scriptscore.runtime import CancellationToken
from tests.support.process import run_direct_cli


def _normalize_success_envelope(raw: dict[str, Any]) -> dict[str, Any]:
    return {
        "ok": raw["ok"],
        "command": raw["command"],
        "degraded": raw["degraded"],
        "warnings": raw.get("warnings", []),
        "providers": raw.get("providers"),
        "artifacts": raw.get("artifacts", []),
        "data": raw.get("data", {}),
    }


def test_create_engine_returns_scriptscore_engine() -> None:
    engine = create_engine(include_builtin_fakes=True)

    assert isinstance(engine, ScriptScoreEngine)


def test_engine_runs_smoke_ping_successfully() -> None:
    engine = create_engine(include_builtin_fakes=True)

    result = engine.run("smoke.ping", {"message": "hello", "steps": 1}, request_id="req_engine")

    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.ok is True
    assert result.envelope.command == "smoke.ping"
    assert result.envelope.request_id == "req_engine"
    assert result.envelope.data["message"] == "hello"


def test_engine_smoke_ping_waits_for_release_marker(tmp_path: Path) -> None:
    engine = create_engine(include_builtin_fakes=True)
    marker = tmp_path / "release-smoke-ping"
    seen: list[ProgressEvent] = []
    result_holder: dict[str, Any] = {}

    def run_smoke() -> None:
        result_holder["result"] = engine.run(
            "smoke.ping",
            {"message": "hello", "steps": 1, "wait_for_file": str(marker)},
            event_sink=seen.append,
        )

    thread = Thread(target=run_smoke)
    thread.start()
    deadline = monotonic() + 5
    while not seen and monotonic() < deadline:
        sleep(0.01)

    assert [event.event for event in seen] == ["started"]
    assert thread.is_alive()

    marker.write_text("release", encoding="utf-8")
    thread.join(timeout=5)

    assert not thread.is_alive()
    result = result_holder["result"]
    assert result.exit_code == 0
    assert isinstance(result.envelope, CommandSuccessEnvelope)


def test_engine_emits_progress_events_via_callback() -> None:
    engine = create_engine(include_builtin_fakes=True)
    seen: list[ProgressEvent] = []

    result = engine.run(
        "smoke.ping",
        {"message": "hello", "steps": 2},
        event_sink=seen.append,
    )

    assert result.exit_code == 0
    assert len(seen) == 6
    assert all(isinstance(event, ProgressEvent) for event in seen)
    assert [event.sequence for event in seen] == [1, 2, 3, 4, 5, 6]
    assert [event.event for event in seen] == [
        "started",
        "item_started",
        "item_completed",
        "item_started",
        "item_completed",
        "completed",
    ]
    assert all(event.command == "smoke.ping" for event in seen)


def test_engine_honors_cancellation_token() -> None:
    engine = create_engine(include_builtin_fakes=True)
    token = CancellationToken()
    token.cancel()

    result = engine.run(
        "smoke.ping",
        {"message": "hello", "steps": 1},
        cancellation_token=token,
    )

    assert result.exit_code == 130
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.category is ErrorCategory.CANCELLED


def test_engine_smoke_matches_direct_cli_result() -> None:
    engine = create_engine(include_builtin_fakes=True)

    engine_result = engine.run("smoke.ping", {"message": "hello", "steps": 1})
    direct_result = run_direct_cli(
        ["_smoke", "ping", "--stdin"],
        stdin_json={"message": "hello", "steps": 1},
    )

    assert engine_result.exit_code == direct_result.returncode
    assert _normalize_success_envelope(
        engine_result.envelope.model_dump(mode="json", exclude_none=True)
    ) == _normalize_success_envelope(direct_result.stdout_lines[-1])
