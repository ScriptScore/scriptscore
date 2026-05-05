# SPDX-License-Identifier: AGPL-3.0-only
"""In-process tests for the desktop worker protocol."""

from __future__ import annotations

import io
import json
import sys
from pathlib import Path
from typing import Any, cast

import pytest

from scriptscore.contracts import (
    CommandErrorBody,
    CommandErrorEnvelope,
    CommandSuccessEnvelope,
    ErrorCategory,
    TimingInfo,
    WriteState,
)
from scriptscore.runtime.runner import CommandRunner
from scriptscore.transport import desktop_worker as desktop_worker_module

encode_frame = desktop_worker_module.encode_frame


def _server() -> desktop_worker_module.DesktopWorkerServer:
    return desktop_worker_module.DesktopWorkerServer(
        runner=desktop_worker_module.create_worker_runner()
    )


def _messages(raw: bytes) -> list[dict[str, Any]]:
    stream = io.BytesIO(raw)
    messages: list[dict[str, Any]] = []
    while True:
        message = desktop_worker_module.decode_frame(stream)
        if message is None:
            return messages
        messages.append(message)


def test_frame_round_trip() -> None:
    payload = {"type": "hello", "request_id": "req_1", "payload": {"protocol_version": "v1"}}

    message = desktop_worker_module.decode_frame(
        io.BytesIO(desktop_worker_module.encode_frame(payload))
    )

    assert message == payload


def test_decode_frame_rejects_truncated_payload() -> None:
    with pytest.raises(desktop_worker_module.ProtocolError, match="ended mid-frame") as exc_info:
        desktop_worker_module.decode_frame(io.BytesIO((2).to_bytes(4, byteorder="big") + b"{"))

    assert exc_info.value.code == "truncated_frame"


def test_decode_frame_rejects_non_object_payload() -> None:
    payload = json.dumps(["not", "an", "object"]).encode("utf-8")

    with pytest.raises(desktop_worker_module.ProtocolError, match="JSON object") as exc_info:
        desktop_worker_module.decode_frame(
            io.BytesIO(len(payload).to_bytes(4, byteorder="big") + payload)
        )

    assert exc_info.value.code == "invalid_request"


def test_normalize_request_payload_preserves_non_mapping_and_existing_output_dir() -> None:
    existing = desktop_worker_module.RunJobPayload(
        command_name="smoke.ping",
        request={"message": "ok", "output_artifacts_dir": "/tmp/existing"},
        output_artifacts_dir="/tmp/new",
    )
    raw = desktop_worker_module.RunJobPayload(
        command_name="smoke.ping",
        request="raw-payload",
        output_artifacts_dir="/tmp/new",
    )

    assert desktop_worker_module._normalize_request_payload(existing) == {
        "message": "ok",
        "output_artifacts_dir": "/tmp/existing",
    }
    assert desktop_worker_module._normalize_request_payload(raw) == "raw-payload"


def test_desktop_worker_requires_hello_before_run_job() -> None:
    stdout = io.BytesIO()
    stdin = io.BytesIO(
        encode_frame(
            {
                "type": "run_job",
                "request_id": "req_run",
                "job_id": "job_1",
                "payload": {"command_name": "smoke.ping", "request": {"message": "hello"}},
            }
        )
    )

    _server().serve(stream=_DuplexBytesIO(stdin, stdout))

    messages = _messages(stdout.getvalue())
    assert messages[0]["type"] == "error"
    assert messages[0]["payload"]["code"] == "handshake_required"


def test_desktop_worker_success_and_progress_use_framed_messages() -> None:
    stdout = io.BytesIO()
    stdin = io.BytesIO(
        b"".join(
            [
                encode_frame(
                    {
                        "type": "hello",
                        "request_id": "req_hello",
                        "payload": {"protocol_version": "desktop.sidecar.v1"},
                    }
                ),
                encode_frame(
                    {
                        "type": "run_job",
                        "request_id": "req_run",
                        "job_id": "job_1",
                        "payload": {
                            "command_name": "smoke.ping",
                            "request": {"message": "hello", "steps": 2},
                        },
                    }
                ),
            ]
        )
    )

    _server().serve(stream=_DuplexBytesIO(stdin, stdout))

    messages = _messages(stdout.getvalue())
    assert messages[0]["type"] == "hello_ok"
    assert messages[0]["payload"]["transport"] in {"unix_socket", "named_pipe"}
    assert any(message["type"] == "job_progress" for message in messages)
    terminal = next(message for message in messages if message["type"] == "job_finished")
    assert terminal["payload"]["exit_code"] == 0
    assert terminal["payload"]["envelope"]["command"] == "smoke.ping"


def test_desktop_worker_rejects_unsupported_protocol_version() -> None:
    stdout = io.BytesIO()
    stdin = io.BytesIO(
        encode_frame(
            {
                "type": "hello",
                "request_id": "req_hello",
                "payload": {"protocol_version": "desktop.sidecar.v0"},
            }
        )
    )

    _server().serve(stream=_DuplexBytesIO(stdin, stdout))

    messages = _messages(stdout.getvalue())
    assert messages[0]["type"] == "error"
    assert messages[0]["payload"]["code"] == "unsupported_protocol"
    assert messages[0]["payload"]["details"]["expected_protocol_version"] == "desktop.sidecar.v1"


def test_desktop_worker_validates_hello_payload() -> None:
    stdout = io.BytesIO()
    stdin = io.BytesIO(
        encode_frame(
            {
                "type": "hello",
                "request_id": "req_hello",
                "payload": {},
            }
        )
    )

    _server().serve(stream=_DuplexBytesIO(stdin, stdout))

    messages = _messages(stdout.getvalue())
    assert messages[0]["type"] == "error"
    assert messages[0]["payload"]["code"] == "invalid_request"


def test_desktop_worker_rejects_second_active_job() -> None:
    stdout = io.BytesIO()
    stdin = io.BytesIO(
        b"".join(
            [
                encode_frame(
                    {
                        "type": "hello",
                        "request_id": "req_hello",
                        "payload": {"protocol_version": "desktop.sidecar.v1"},
                    }
                ),
                encode_frame(
                    {
                        "type": "run_job",
                        "request_id": "req_slow",
                        "job_id": "job_slow",
                        "payload": {
                            "command_name": "smoke.ping",
                            "request": {"message": "slow", "steps": 3, "sleep_ms": 300},
                        },
                    }
                ),
                encode_frame(
                    {
                        "type": "run_job",
                        "request_id": "req_fast",
                        "job_id": "job_fast",
                        "payload": {
                            "command_name": "smoke.ping",
                            "request": {"message": "fast", "steps": 1},
                        },
                    }
                ),
            ]
        )
    )

    _server().serve(stream=_DuplexBytesIO(stdin, stdout))

    messages = _messages(stdout.getvalue())
    conflict = next(
        message
        for message in messages
        if message["type"] == "error" and message.get("request_id") == "req_fast"
    )
    assert conflict["payload"]["code"] == "request_conflict"


def test_desktop_worker_reports_supported_commands_for_unsupported_command() -> None:
    stdout = io.BytesIO()
    stdin = io.BytesIO(
        b"".join(
            [
                encode_frame(
                    {
                        "type": "hello",
                        "request_id": "req_hello",
                        "payload": {"protocol_version": "desktop.sidecar.v1"},
                    }
                ),
                encode_frame(
                    {
                        "type": "run_job",
                        "request_id": "req_run",
                        "job_id": "job_1",
                        "payload": {
                            "command_name": "students.parse",
                            "request": {},
                        },
                    }
                ),
            ]
        )
    )

    _server().serve(stream=_DuplexBytesIO(stdin, stdout))

    messages = _messages(stdout.getvalue())
    error = next(message for message in messages if message["type"] == "error")
    assert error["payload"]["code"] == "unsupported_command"
    assert error["payload"]["details"]["supported_commands"] == sorted(
        desktop_worker_module.SUPPORTED_COMMANDS
    )


def test_desktop_worker_rejects_invalid_run_job_payload() -> None:
    stdout = io.BytesIO()
    stdin = io.BytesIO(
        b"".join(
            [
                encode_frame(
                    {
                        "type": "hello",
                        "request_id": "req_hello",
                        "payload": {"protocol_version": "desktop.sidecar.v1"},
                    }
                ),
                encode_frame(
                    {
                        "type": "run_job",
                        "request_id": "req_run",
                        "job_id": "job_1",
                        "payload": {"request": {"message": "hello"}},
                    }
                ),
            ]
        )
    )

    _server().serve(stream=_DuplexBytesIO(stdin, stdout))

    messages = _messages(stdout.getvalue())
    error = next(
        message
        for message in messages
        if message["type"] == "error" and message.get("request_id") == "req_run"
    )
    assert error["payload"]["code"] == "invalid_request"
    assert error["payload"]["message"] == "run_job payload is invalid."


def test_desktop_worker_cancel_job_rejects_unknown_job_id() -> None:
    stdout = io.BytesIO()
    stdin = io.BytesIO(
        b"".join(
            [
                encode_frame(
                    {
                        "type": "hello",
                        "request_id": "req_hello",
                        "payload": {"protocol_version": "desktop.sidecar.v1"},
                    }
                ),
                encode_frame(
                    {
                        "type": "cancel_job",
                        "request_id": "req_cancel",
                        "job_id": "job_missing",
                        "payload": {},
                    }
                ),
            ]
        )
    )

    _server().serve(stream=_DuplexBytesIO(stdin, stdout))

    messages = _messages(stdout.getvalue())
    error = next(message for message in messages if message["type"] == "error")
    assert error["payload"]["code"] == "job_not_found"
    assert error["job_id"] == "job_missing"


def test_desktop_worker_cancel_job_yields_cancelled_terminal_event() -> None:
    stdout = io.BytesIO()
    stdin = io.BytesIO(
        b"".join(
            [
                encode_frame(
                    {
                        "type": "hello",
                        "request_id": "req_hello",
                        "payload": {"protocol_version": "desktop.sidecar.v1"},
                    }
                ),
                encode_frame(
                    {
                        "type": "run_job",
                        "request_id": "req_run",
                        "job_id": "job_1",
                        "payload": {
                            "command_name": "smoke.ping",
                            "request": {"message": "slow", "steps": 10, "sleep_ms": 1000},
                        },
                    }
                ),
                encode_frame(
                    {
                        "type": "cancel_job",
                        "request_id": "req_cancel",
                        "job_id": "job_1",
                        "payload": {},
                    }
                ),
            ]
        )
    )

    _server().serve(stream=_DuplexBytesIO(stdin, stdout))

    messages = _messages(stdout.getvalue())
    cancelled = next(message for message in messages if message["type"] == "job_cancelled")
    assert cancelled["payload"]["envelope"]["error"]["category"] == "cancelled"


def test_desktop_worker_shutdown_cancels_the_active_job_before_exit() -> None:
    stdout = io.BytesIO()
    stdin = io.BytesIO(
        b"".join(
            [
                encode_frame(
                    {
                        "type": "hello",
                        "request_id": "req_hello",
                        "payload": {"protocol_version": "desktop.sidecar.v1"},
                    }
                ),
                encode_frame(
                    {
                        "type": "run_job",
                        "request_id": "req_run",
                        "job_id": "job_1",
                        "payload": {
                            "command_name": "smoke.ping",
                            "request": {"message": "slow", "steps": 10, "sleep_ms": 1000},
                        },
                    }
                ),
                encode_frame(
                    {
                        "type": "shutdown",
                        "request_id": "req_shutdown",
                        "payload": {},
                    }
                ),
            ]
        )
    )

    exit_code = _server().serve(stream=_DuplexBytesIO(stdin, stdout))

    messages = _messages(stdout.getvalue())
    assert exit_code == 0
    cancelled = next(message for message in messages if message["type"] == "job_cancelled")
    assert cancelled["job_id"] == "job_1"


def test_desktop_worker_forwards_output_artifacts_dir_into_request_payload() -> None:
    capture: dict[str, Any] = {}

    class CapturingRunner:
        def run(
            self,
            command_name: str,
            request_payload: Any,
            *,
            request_id: str,
            event_sink: Any,
            cancellation_token: Any,
        ) -> Any:
            del event_sink, cancellation_token
            capture["command_name"] = command_name
            capture["request_payload"] = request_payload
            capture["request_id"] = request_id
            return type(
                "RunResultStub",
                (),
                {
                    "exit_code": 0,
                    "envelope": CommandSuccessEnvelope(
                        command="smoke.ping",
                        operation_id="op_1",
                        request_id=request_id,
                        data={"message": "ok", "steps": 1},
                        timing=TimingInfo(started_at="1", finished_at="2", duration_ms=1),
                    ),
                },
            )()

    stdout = io.BytesIO()
    stdin = io.BytesIO(
        b"".join(
            [
                encode_frame(
                    {
                        "type": "hello",
                        "request_id": "req_hello",
                        "payload": {"protocol_version": "desktop.sidecar.v1"},
                    }
                ),
                encode_frame(
                    {
                        "type": "run_job",
                        "request_id": "req_run",
                        "job_id": "job_1",
                        "payload": {
                            "command_name": "smoke.ping",
                            "request": {"message": "ok", "steps": 1},
                            "output_artifacts_dir": "/tmp/out",
                        },
                    }
                ),
            ]
        )
    )

    desktop_worker_module.DesktopWorkerServer(runner=cast(CommandRunner, CapturingRunner())).serve(
        stream=_DuplexBytesIO(stdin, stdout)
    )

    assert capture == {
        "command_name": "smoke.ping",
        "request_payload": {"message": "ok", "steps": 1, "output_artifacts_dir": "/tmp/out"},
        "request_id": "req_run",
    }


def test_desktop_worker_reports_protocol_and_request_shape_errors() -> None:
    invalid_json = (1).to_bytes(4, byteorder="big") + b"{"
    stdout = io.BytesIO()
    stdin = io.BytesIO(
        b"".join(
            [
                invalid_json,
                encode_frame({"request_id": "req_invalid_type"}),
                encode_frame(
                    {
                        "type": "unknown",
                        "request_id": "req_unknown",
                        "job_id": "job_unknown",
                        "payload": {},
                    }
                ),
            ]
        )
    )

    _server().serve(stream=_DuplexBytesIO(stdin, stdout))

    messages = _messages(stdout.getvalue())
    assert [message["payload"]["code"] for message in messages] == [
        "invalid_json",
        "invalid_request",
        "unknown_method",
    ]
    assert messages[2]["request_id"] == "req_unknown"
    assert messages[2]["job_id"] == "job_unknown"


def test_terminal_type_for_non_cancelled_failures() -> None:
    server = _server()

    success_envelope = CommandSuccessEnvelope(
        command="smoke.ping",
        operation_id="op_success",
        request_id="req_success",
        data={"message": "ok", "steps": 1},
        timing=TimingInfo(started_at="1", finished_at="2", duration_ms=1),
    )
    error_envelope = CommandErrorEnvelope(
        command="smoke.ping",
        operation_id="op_error",
        request_id="req_error",
        error=CommandErrorBody(
            code="execution_failed",
            message="boom",
            category=ErrorCategory.EXECUTION,
            retryable=False,
            details={},
            write_state=WriteState.NO_WRITE,
        ),
        timing=TimingInfo(started_at="1", finished_at="2", duration_ms=1),
    )

    non_error_result = type(
        "RunResultStub",
        (),
        {"exit_code": 8, "envelope": success_envelope},
    )()
    execution_error_result = type(
        "RunResultStub",
        (),
        {"exit_code": 8, "envelope": error_envelope},
    )()

    assert server._terminal_type_for_result(non_error_result) == "job_failed"
    assert server._terminal_type_for_result(execution_error_result) == "job_failed"


def test_main_rejects_pipe_name_on_non_windows(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setattr(
        desktop_worker_module, "create_worker_runner", lambda: cast(CommandRunner, object())
    )

    class DummyServer:
        def __init__(self, *, runner: CommandRunner) -> None:
            self.runner = runner

    monkeypatch.setattr(desktop_worker_module, "DesktopWorkerServer", DummyServer)

    with pytest.raises(SystemExit) as exc_info:
        desktop_worker_module.main(["--pipe-name", "worker"])

    assert exc_info.value.code == 2


def test_main_requires_socket_path_on_non_windows(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setattr(
        desktop_worker_module, "create_worker_runner", lambda: cast(CommandRunner, object())
    )

    class DummyServer:
        def __init__(self, *, runner: CommandRunner) -> None:
            self.runner = runner

    monkeypatch.setattr(desktop_worker_module, "DesktopWorkerServer", DummyServer)

    with pytest.raises(SystemExit) as exc_info:
        desktop_worker_module.main([])

    assert exc_info.value.code == 2


def test_main_serves_over_unix_socket_and_closes_handles(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setattr(
        desktop_worker_module, "create_worker_runner", lambda: cast(CommandRunner, object())
    )

    calls: dict[str, Any] = {"closed": []}

    class DummyStream:
        def close(self) -> None:
            calls["closed"].append("stream")

    class DummyClient:
        def close(self) -> None:
            calls["closed"].append("client")

    class DummyServer:
        def __init__(self, *, runner: CommandRunner) -> None:
            self.runner = runner

        def serve(self, *, stream: Any) -> int:
            calls["served_stream"] = stream
            return 17

    monkeypatch.setattr(desktop_worker_module, "DesktopWorkerServer", DummyServer)
    monkeypatch.setattr(
        desktop_worker_module,
        "_connect_unix_socket",
        lambda socket_path: (DummyClient(), DummyStream()),
    )

    exit_code = desktop_worker_module.main(["--socket-path", str(Path("/tmp/worker.sock"))])

    assert exit_code == 17
    assert calls["closed"] == ["stream", "client"]


class _DuplexBytesIO:
    """Minimal test double for a duplex binary stream."""

    def __init__(self, reader: io.BytesIO, writer: io.BytesIO) -> None:
        self._reader = reader
        self._writer = writer

    def read(self, size: int = -1) -> bytes:
        return self._reader.read(size)

    def write(self, data: bytes) -> int:
        return self._writer.write(data)

    def flush(self) -> None:
        return None

    def close(self) -> None:
        self._reader.close()
        self._writer.close()
