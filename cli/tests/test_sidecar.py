# SPDX-License-Identifier: AGPL-3.0-only
"""JSON-RPC sidecar smoke-path tests."""

from __future__ import annotations

from tests.support.process import SidecarSession


def test_sidecar_smoke_request_returns_result_envelope() -> None:
    with SidecarSession() as session:
        session.send(
            {
                "jsonrpc": "2.0",
                "id": "req_1",
                "method": "smoke.ping",
                "params": {"message": "hello", "steps": 1},
            }
        )
        result = session.next_message()
        while result.get("method") == "scriptscore.progress":
            result = session.next_message()
        assert result["id"] == "req_1"
        assert result["result"]["ok"] is True
        assert result["result"]["command"] == "smoke.ping"


def test_sidecar_rejects_second_in_flight_request() -> None:
    with SidecarSession() as session:
        session.send(
            {
                "jsonrpc": "2.0",
                "id": "req_1",
                "method": "smoke.ping",
                "params": {"message": "slow", "steps": 3, "sleep_ms": 300},
            }
        )
        session.send(
            {
                "jsonrpc": "2.0",
                "id": "req_2",
                "method": "smoke.ping",
                "params": {"message": "fast", "steps": 1},
            }
        )
        seen_conflict = False
        while not seen_conflict:
            message = session.next_message(timeout=5)
            if message.get("id") == "req_2":
                seen_conflict = True
                assert message["error"]["data"]["error"]["category"] == "conflict"
                assert message["error"]["data"]["error"]["write_state"] == "no_write"


def test_sidecar_cancel_request_maps_to_cancelled_error() -> None:
    with SidecarSession() as session:
        session.send(
            {
                "jsonrpc": "2.0",
                "id": "req_1",
                "method": "smoke.ping",
                "params": {"message": "slow", "steps": 10, "sleep_ms": 1000},
            }
        )
        while True:
            message = session.next_message(timeout=5)
            if message.get("method") == "scriptscore.progress":
                session.send(
                    {
                        "jsonrpc": "2.0",
                        "method": "$/cancelRequest",
                        "params": {"id": "req_1"},
                    }
                )
                break
        while True:
            message = session.next_message(timeout=5)
            if message.get("id") == "req_1":
                assert message["error"]["data"]["error"]["category"] == "cancelled"
                break
