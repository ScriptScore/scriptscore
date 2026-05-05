# SPDX-License-Identifier: AGPL-3.0-only
"""In-process sidecar transport tests for coverage of transport logic."""

from __future__ import annotations

import io
import json
from typing import Any

from scriptscore.engine import create_engine
from scriptscore.transport import SidecarServer


def _server() -> SidecarServer:
    return SidecarServer(engine=create_engine(include_builtin_fakes=True))


def _messages(raw: str) -> list[dict[str, Any]]:
    return [json.loads(line) for line in raw.splitlines() if line.strip()]


def test_sidecar_invalid_json_returns_parse_error() -> None:
    stdout = io.StringIO()
    _server().serve(stdin=io.StringIO("{not-json}\n"), stdout=stdout)
    messages = _messages(stdout.getvalue())
    assert messages[0]["error"]["code"] == -32700


def test_sidecar_invalid_request_shape_returns_invalid_request() -> None:
    stdout = io.StringIO()
    _server().serve(stdin=io.StringIO('{"jsonrpc":"2.0"}\n'), stdout=stdout)
    messages = _messages(stdout.getvalue())
    assert messages[0]["error"]["code"] == -32600


def test_sidecar_rejects_non_2_0_request_version() -> None:
    stdout = io.StringIO()
    payload = '{"jsonrpc":"1.0","id":"req_1","method":"smoke.ping","params":{"message":"hello"}}\n'
    _server().serve(stdin=io.StringIO(payload), stdout=stdout)
    messages = _messages(stdout.getvalue())
    assert messages[0]["id"] == "req_1"
    assert messages[0]["error"]["code"] == -32600


def test_sidecar_success_and_progress_use_shared_mapping() -> None:
    stdout = io.StringIO()
    payload = json.dumps(
        {
            "jsonrpc": "2.0",
            "id": "req_1",
            "method": "smoke.ping",
            "params": {"message": "hello", "steps": 2},
        }
    )
    _server().serve(stdin=io.StringIO(payload + "\n"), stdout=stdout)
    messages = _messages(stdout.getvalue())
    assert any(message.get("method") == "scriptscore.progress" for message in messages)
    result = next(message for message in messages if message.get("id") == "req_1")
    assert result["result"]["ok"] is True


def test_sidecar_notification_emits_no_terminal_response() -> None:
    stdout = io.StringIO()
    payload = json.dumps(
        {
            "jsonrpc": "2.0",
            "method": "smoke.ping",
            "params": {"message": "hello", "steps": 2},
        }
    )
    _server().serve(stdin=io.StringIO(payload + "\n"), stdout=stdout)
    messages = _messages(stdout.getvalue())
    assert any(message.get("method") == "scriptscore.progress" for message in messages)
    assert not any("result" in message or "error" in message for message in messages)


def test_sidecar_conflict_response_uses_error_data_envelope() -> None:
    stdout = io.StringIO()
    payloads = [
        {
            "jsonrpc": "2.0",
            "id": "req_1",
            "method": "smoke.ping",
            "params": {"message": "slow", "steps": 3, "sleep_ms": 300},
        },
        {
            "jsonrpc": "2.0",
            "id": "req_2",
            "method": "smoke.ping",
            "params": {"message": "fast", "steps": 1},
        },
    ]
    stdin = io.StringIO("\n".join(json.dumps(item) for item in payloads) + "\n")
    _server().serve(stdin=stdin, stdout=stdout)
    messages = _messages(stdout.getvalue())
    conflict = next(message for message in messages if message.get("id") == "req_2")
    assert conflict["error"]["data"]["error"]["category"] == "conflict"


def test_sidecar_cancel_request_yields_cancelled_error() -> None:
    stdout = io.StringIO()
    payloads = [
        {
            "jsonrpc": "2.0",
            "id": "req_1",
            "method": "smoke.ping",
            "params": {"message": "slow", "steps": 10, "sleep_ms": 1000},
        },
        {
            "jsonrpc": "2.0",
            "method": "$/cancelRequest",
            "params": {"id": "req_1"},
        },
    ]
    stdin = io.StringIO("\n".join(json.dumps(item) for item in payloads) + "\n")
    _server().serve(stdin=stdin, stdout=stdout)
    messages = _messages(stdout.getvalue())
    cancelled = next(message for message in messages if message.get("id") == "req_1")
    assert cancelled["error"]["data"]["error"]["category"] == "cancelled"
