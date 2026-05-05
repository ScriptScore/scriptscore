# SPDX-License-Identifier: AGPL-3.0-only
"""Reusable direct-vs-sidecar parity harness."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from tests.support.process import SidecarSession, run_direct_cli


@dataclass(frozen=True)
class DirectInvocation:
    """Direct CLI invocation details for parity tests."""

    args: list[str]
    stdin_json: dict[str, Any] | None = None


@dataclass(frozen=True)
class ParityInvocation:
    """Combined direct and sidecar invocation for the same logical command."""

    direct: DirectInvocation
    method: str
    params: dict[str, Any]
    request_id: str = "req_parity"
    env_overrides: dict[str, str] | None = None


@dataclass(frozen=True)
class ParityResult:
    """Normalized parity result for one command invocation."""

    direct_exit_code: int
    direct_events: list[dict[str, Any]]
    direct_terminal: dict[str, Any]
    sidecar_events: list[dict[str, Any]]
    sidecar_terminal: dict[str, Any]


def _normalize_event(event: dict[str, Any]) -> dict[str, Any]:
    normalized = {
        key: value for key, value in event.items() if key not in {"operation_id", "timestamp"}
    }
    normalized.pop("request_id", None)
    return normalized


def _normalize_terminal(envelope: dict[str, Any]) -> dict[str, Any]:
    normalized = {
        key: value for key, value in envelope.items() if key not in {"operation_id", "timing"}
    }
    normalized.pop("request_id", None)
    return normalized


def run_parity(invocation: ParityInvocation) -> ParityResult:
    """Execute the same logical command through direct CLI and sidecar."""

    direct_result = run_direct_cli(
        invocation.direct.args,
        stdin_json=invocation.direct.stdin_json,
        env_overrides=invocation.env_overrides,
    )
    direct_events = [line for line in direct_result.stdout_lines if line.get("type") == "event"]
    direct_terminal = next(line for line in reversed(direct_result.stdout_lines) if "ok" in line)

    with SidecarSession(env_overrides=invocation.env_overrides) as session:
        session.send(
            {
                "jsonrpc": "2.0",
                "id": invocation.request_id,
                "method": invocation.method,
                "params": invocation.params,
            }
        )
        sidecar_events: list[dict[str, Any]] = []
        while True:
            message = session.next_message(timeout=5)
            if message.get("method") == "scriptscore.progress":
                sidecar_events.append(message["params"])
                continue
            if message.get("id") == invocation.request_id:
                sidecar_terminal = message.get("result") or message["error"]["data"]
                break

    return ParityResult(
        direct_exit_code=direct_result.returncode,
        direct_events=direct_events,
        direct_terminal=direct_terminal,
        sidecar_events=sidecar_events,
        sidecar_terminal=sidecar_terminal,
    )


def assert_parity(invocation: ParityInvocation) -> ParityResult:
    """Assert parity between direct CLI and sidecar execution."""

    result = run_parity(invocation)
    assert [_normalize_event(event) for event in result.direct_events] == [
        _normalize_event(event) for event in result.sidecar_events
    ]
    assert _normalize_terminal(result.direct_terminal) == _normalize_terminal(
        result.sidecar_terminal
    )
    return result
