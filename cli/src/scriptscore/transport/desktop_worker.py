# SPDX-License-Identifier: AGPL-3.0-only
"""Desktop worker transport over a framed local duplex stream."""

from __future__ import annotations

import argparse
import base64
import json
import os
import socket
import sys
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from io import BytesIO
from pathlib import Path
from threading import Lock, Thread
from typing import Any, Protocol, cast

from pydantic import BaseModel, ConfigDict, Field, ValidationError

from scriptscore import __version__
from scriptscore.contracts import CommandErrorEnvelope, ConflictError, ErrorCategory, ProgressEvent
from scriptscore.runtime import CancellationToken, CommandRunner, RunResult, utc_now_iso

FRAME_LENGTH_BYTES = 4
PROTOCOL_VERSION = "desktop.sidecar.v1"
WINDOWS_PIPE_WRITE_CHUNK_BYTES = 16 * 1024
SUPPORTED_COMMANDS = frozenset(
    {
        "exam.analyze",
        "exam.generate-rubric",
        "exam.setup",
        "runtime.list-llm-models",
        "runtime.validate-llm-model",
        "scans.ingest",
        "scans.align-auto",
        "scans.canonicalize",
        "scans.transform",
        "scans.detect",
        "scans.crop",
        "scans.pii",
        "scans.parse",
        "scans.ocr",
        "scans.pdf-clip-rects",
        "scans.pdf-extract-text",
        "scans.pdf-map-template-regions",
        "scans.pdf-render-page",
        "scans.pdf-create-redacted",
        "scans.pdf-detect-aruco",
        "scans.pdf-stamp-aruco",
        "grading.score-preliminary",
        "grading.run-consistency",
        "grading.draft-feedback",
        "grading.markup",
        "smoke.ping",
    }
)


class DuplexStream(Protocol):
    """Minimal read/write/flush protocol for the worker transport."""

    def read(self, length: int = -1) -> bytes: ...

    def write(self, data: bytes) -> int | None: ...

    def flush(self) -> None: ...

    def close(self) -> None: ...


class WindowsNamedPipeStream:
    """Unbuffered Windows named-pipe stream backed by os.read/os.write."""

    def __init__(self, read_fd: int, write_fd: int) -> None:
        self._read_fd = read_fd
        self._write_fd = write_fd

    def read(self, length: int = -1) -> bytes:
        return os.read(self._read_fd, length)

    def write(self, data: bytes | memoryview) -> int:
        return os.write(self._write_fd, data)

    def flush(self) -> None:
        return None

    def close(self) -> None:
        os.close(self._read_fd)
        os.close(self._write_fd)

    def __enter__(self) -> WindowsNamedPipeStream:
        return self

    def __exit__(self, exc_type: Any, exc: Any, traceback: Any) -> None:
        del exc_type, exc, traceback
        self.close()


class HelloPayload(BaseModel):
    """Payload for the required hello handshake."""

    model_config = ConfigDict(extra="forbid")

    protocol_version: str = Field(min_length=1)


class RunJobPayload(BaseModel):
    """Payload for a worker run request."""

    model_config = ConfigDict(extra="forbid")

    command_name: str = Field(min_length=1)
    request: Any = Field(default_factory=dict)
    output_artifacts_dir: str | None = None
    stdin_b64: str | None = None


@dataclass(frozen=True)
class ActiveJob:
    """State for the single active job inside one worker process."""

    request_id: str
    job_id: str
    cancellation_token: CancellationToken
    thread: Thread


class ProtocolError(Exception):
    """Raised when a framed transport message is invalid."""

    def __init__(self, *, code: str, message: str, details: dict[str, Any] | None = None) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.details = details or {}


def _include_builtin_fakes_for_testing() -> bool:
    return bool(int(os.environ.get("SCRIPTSCORE_INCLUDE_BUILTIN_FAKES", "0")))


class _BinaryStdin:
    """Minimal stdin shim exposing `buffer.read()` for binary payloads."""

    def __init__(self, data: bytes) -> None:
        self.buffer = BytesIO(data)


@contextmanager
def _override_stdin(stdin_bytes: bytes | None) -> Iterator[None]:
    if stdin_bytes is None:
        yield
        return
    previous = sys.stdin
    sys.stdin = cast(Any, _BinaryStdin(stdin_bytes))
    try:
        yield
    finally:
        sys.stdin = previous


def create_worker_runner() -> CommandRunner:
    """Build the shared runner used by the desktop worker process."""

    from scriptscore.commands import build_command_registry
    from scriptscore.providers import ProviderRegistry

    return CommandRunner(
        registry=build_command_registry(),
        provider_registry=ProviderRegistry.for_runtime(
            include_builtin_fakes=_include_builtin_fakes_for_testing(),
        ),
    )


def encode_frame(payload: dict[str, Any]) -> bytes:
    """Serialize one framed protocol message."""

    message = json.dumps(payload, separators=(",", ":"), sort_keys=True).encode("utf-8")
    return len(message).to_bytes(FRAME_LENGTH_BYTES, byteorder="big") + message


def _write_all(stream: DuplexStream, data: bytes) -> None:
    view = memoryview(data)
    offset = 0
    while offset < len(view):
        chunk = view[offset : offset + WINDOWS_PIPE_WRITE_CHUNK_BYTES]
        written = stream.write(chunk.tobytes())
        if written is None:
            written = len(chunk)
        if written <= 0:
            raise ProtocolError(
                code="transport_write_failed", message="Transport write made no progress."
            )
        offset += written


def _write_frame(stream: DuplexStream, payload: dict[str, Any]) -> None:
    message = json.dumps(payload, separators=(",", ":"), sort_keys=True).encode("utf-8")
    _write_all(stream, len(message).to_bytes(FRAME_LENGTH_BYTES, byteorder="big"))
    _write_all(stream, message)


def _read_exact(stream: DuplexStream, length: int) -> bytes:
    chunks: list[bytes] = []
    remaining = length
    while remaining > 0:
        chunk = stream.read(remaining)
        if chunk == b"":
            if not chunks:
                return b""
            raise ProtocolError(code="truncated_frame", message="Transport stream ended mid-frame.")
        chunks.append(chunk)
        remaining -= len(chunk)
    return b"".join(chunks)


def decode_frame(stream: DuplexStream) -> dict[str, Any] | None:
    """Read and decode one framed protocol message."""

    header = _read_exact(stream, FRAME_LENGTH_BYTES)
    if header == b"":
        return None
    payload_length = int.from_bytes(header, byteorder="big", signed=False)
    payload_bytes = _read_exact(stream, payload_length)
    try:
        payload = json.loads(payload_bytes.decode("utf-8"))
    except json.JSONDecodeError as exc:  # pragma: no cover - exercised through serve()
        raise ProtocolError(code="invalid_json", message=str(exc)) from exc
    if not isinstance(payload, dict):
        raise ProtocolError(
            code="invalid_request",
            message="Protocol payload must decode to a JSON object.",
        )
    return payload


def _normalize_request_payload(payload: RunJobPayload) -> Any:
    request_payload = payload.request
    if (
        payload.output_artifacts_dir is not None
        and isinstance(request_payload, dict)
        and "output_artifacts_dir" not in request_payload
    ):
        return {**request_payload, "output_artifacts_dir": payload.output_artifacts_dir}
    return request_payload


class DesktopWorkerServer:
    """Single-flight desktop worker transport."""

    def __init__(self, *, runner: CommandRunner) -> None:
        self._runner = runner
        self._write_lock = Lock()
        self._state_lock = Lock()
        self._active: ActiveJob | None = None
        self._hello_complete = False

    def _write_message(self, payload: dict[str, Any], *, stream: DuplexStream) -> None:
        with self._write_lock:
            _write_frame(stream, payload)
            stream.flush()

    def _write_error(
        self,
        *,
        stream: DuplexStream,
        code: str,
        message: str,
        request_id: str | None = None,
        job_id: str | None = None,
        details: dict[str, Any] | None = None,
    ) -> None:
        frame: dict[str, Any] = {
            "type": "error",
            "payload": {
                "code": code,
                "message": message,
                "details": details or {},
            },
        }
        if request_id is not None:
            frame["request_id"] = request_id
        if job_id is not None:
            frame["job_id"] = job_id
        self._write_message(frame, stream=stream)

    def _emit_progress(
        self,
        event: ProgressEvent,
        *,
        request_id: str,
        job_id: str,
        stream: DuplexStream,
    ) -> None:
        self._write_message(
            {
                "type": "job_progress",
                "request_id": request_id,
                "job_id": job_id,
                "payload": event.model_dump(mode="json", exclude_none=True),
            },
            stream=stream,
        )

    def _clear_active(self, job_id: str) -> None:
        with self._state_lock:
            if self._active is not None and self._active.job_id == job_id:
                self._active = None

    def _run_job_worker(
        self,
        *,
        request_id: str,
        job_id: str,
        command_name: str,
        request_payload: Any,
        stdin_bytes: bytes | None,
        cancellation_token: CancellationToken,
        stream: DuplexStream,
    ) -> None:
        self._write_message(
            {
                "type": "job_started",
                "request_id": request_id,
                "job_id": job_id,
                "payload": {
                    "command_name": command_name,
                    "timestamp": utc_now_iso(),
                },
            },
            stream=stream,
        )
        try:
            with _override_stdin(stdin_bytes):
                result = self._runner.run(
                    command_name,
                    request_payload,
                    request_id=request_id,
                    event_sink=lambda event: self._emit_progress(
                        event,
                        request_id=request_id,
                        job_id=job_id,
                        stream=stream,
                    ),
                    cancellation_token=cancellation_token,
                )
            terminal_type = self._terminal_type_for_result(result)
            self._write_message(
                {
                    "type": terminal_type,
                    "request_id": request_id,
                    "job_id": job_id,
                    "payload": {
                        "exit_code": result.exit_code,
                        "envelope": result.envelope.model_dump(mode="json", exclude_none=True),
                    },
                },
                stream=stream,
            )
        finally:
            self._clear_active(job_id)

    def _terminal_type_for_result(self, result: RunResult) -> str:
        if result.exit_code == 0:
            return "job_finished"
        error_envelope = result.envelope
        if not isinstance(error_envelope, CommandErrorEnvelope):
            return "job_failed"
        if error_envelope.error.category is ErrorCategory.CANCELLED:
            return "job_cancelled"
        return "job_failed"

    def _ensure_request_id(self, message: dict[str, Any]) -> str:
        request_id = message.get("request_id")
        if not isinstance(request_id, str) or not request_id:
            raise ProtocolError(
                code="invalid_request",
                message="request_id is required and must be a non-empty string.",
            )
        return request_id

    def _ensure_job_id(self, message: dict[str, Any]) -> str:
        job_id = message.get("job_id")
        if not isinstance(job_id, str) or not job_id:
            raise ProtocolError(
                code="invalid_request",
                message="job_id is required and must be a non-empty string.",
            )
        return job_id

    def _hello(self, *, message: dict[str, Any], stream: DuplexStream) -> None:
        request_id = self._ensure_request_id(message)
        try:
            payload = HelloPayload.model_validate(message.get("payload") or {})
        except ValidationError as exc:
            raise ProtocolError(
                code="invalid_request",
                message="hello payload is invalid.",
                details={"issues": exc.errors()},
            ) from exc
        if payload.protocol_version != PROTOCOL_VERSION:
            self._write_error(
                stream=stream,
                code="unsupported_protocol",
                message=(
                    f"Unsupported protocol version '{payload.protocol_version}'. "
                    f"Expected '{PROTOCOL_VERSION}'."
                ),
                request_id=request_id,
                details={"expected_protocol_version": PROTOCOL_VERSION},
            )
            return
        self._hello_complete = True
        self._write_message(
            {
                "type": "hello_ok",
                "request_id": request_id,
                "payload": {
                    "protocol_version": PROTOCOL_VERSION,
                    "worker_version": __version__,
                    "supported_commands": sorted(SUPPORTED_COMMANDS),
                    "transport": "named_pipe" if sys.platform == "win32" else "unix_socket",
                },
            },
            stream=stream,
        )

    def _run_job(self, *, message: dict[str, Any], stream: DuplexStream) -> None:
        request_id = self._ensure_request_id(message)
        job_id = self._ensure_job_id(message)
        if not self._hello_complete:
            self._write_error(
                stream=stream,
                code="handshake_required",
                message="hello must complete before run_job.",
                request_id=request_id,
                job_id=job_id,
            )
            return
        try:
            payload = RunJobPayload.model_validate(message.get("payload") or {})
        except ValidationError as exc:
            self._write_error(
                stream=stream,
                code="invalid_request",
                message="run_job payload is invalid.",
                request_id=request_id,
                job_id=job_id,
                details={"issues": exc.errors()},
            )
            return
        if payload.command_name not in SUPPORTED_COMMANDS:
            self._write_error(
                stream=stream,
                code="unsupported_command",
                message=(
                    f"Command '{payload.command_name}' is not supported by the current "
                    "desktop runtime."
                ),
                request_id=request_id,
                job_id=job_id,
                details={"supported_commands": sorted(SUPPORTED_COMMANDS)},
            )
            return
        request_payload = _normalize_request_payload(payload)
        stdin_bytes: bytes | None = None
        if payload.stdin_b64 is not None:
            try:
                stdin_bytes = base64.b64decode(payload.stdin_b64, validate=True)
            except Exception:
                self._write_error(
                    stream=stream,
                    code="invalid_request",
                    message="stdin_b64 was not valid base64.",
                    request_id=request_id,
                    job_id=job_id,
                )
                return
        with self._state_lock:
            active = self._active
            if active is not None:
                conflict = ConflictError(
                    code="request_conflict",
                    message="A job request is already active in this worker process.",
                )
                self._write_error(
                    stream=stream,
                    code=conflict.code,
                    message=conflict.message,
                    request_id=request_id,
                    job_id=job_id,
                    details={"active_job_id": active.job_id},
                )
                return
            cancellation_token = CancellationToken()
            worker = Thread(
                target=self._run_job_worker,
                kwargs={
                    "request_id": request_id,
                    "job_id": job_id,
                    "command_name": payload.command_name,
                    "request_payload": request_payload,
                    "stdin_bytes": stdin_bytes,
                    "cancellation_token": cancellation_token,
                    "stream": stream,
                },
                daemon=True,
            )
            self._active = ActiveJob(
                request_id=request_id,
                job_id=job_id,
                cancellation_token=cancellation_token,
                thread=worker,
            )
            worker.start()

    def _cancel_job(self, *, message: dict[str, Any], stream: DuplexStream) -> None:
        request_id = self._ensure_request_id(message)
        job_id = self._ensure_job_id(message)
        with self._state_lock:
            active = self._active
        if active is None or active.job_id != job_id:
            self._write_error(
                stream=stream,
                code="job_not_found",
                message=f"No active job with id '{job_id}' exists.",
                request_id=request_id,
                job_id=job_id,
            )
            return
        active.cancellation_token.cancel()

    def _shutdown(self, *, message: dict[str, Any]) -> None:
        request_id = self._ensure_request_id(message)
        del request_id
        with self._state_lock:
            active = self._active
        if active is not None:
            active.cancellation_token.cancel()
            active.thread.join()

    def serve(self, *, stream: DuplexStream) -> int:
        """Serve framed messages until EOF or shutdown."""

        while True:
            try:
                message = decode_frame(stream)
            except ProtocolError as exc:
                self._write_error(
                    stream=stream,
                    code=exc.code,
                    message=exc.message,
                    details=exc.details,
                )
                continue
            if message is None:
                break
            message_type = message.get("type")
            if not isinstance(message_type, str):
                self._write_error(
                    stream=stream,
                    code="invalid_request",
                    message="type is required and must be a string.",
                )
                continue
            try:
                if message_type == "hello":
                    self._hello(message=message, stream=stream)
                    continue
                if message_type == "run_job":
                    self._run_job(message=message, stream=stream)
                    continue
                if message_type == "cancel_job":
                    self._cancel_job(message=message, stream=stream)
                    continue
                if message_type == "shutdown":
                    self._shutdown(message=message)
                    break
                self._write_error(
                    stream=stream,
                    code="unknown_method",
                    message=f"Unknown message type '{message_type}'.",
                    request_id=message.get("request_id")
                    if isinstance(message.get("request_id"), str)
                    else None,
                    job_id=message.get("job_id")
                    if isinstance(message.get("job_id"), str)
                    else None,
                )
            except ProtocolError as exc:
                self._write_error(
                    stream=stream,
                    code=exc.code,
                    message=exc.message,
                    request_id=message.get("request_id")
                    if isinstance(message.get("request_id"), str)
                    else None,
                    job_id=message.get("job_id")
                    if isinstance(message.get("job_id"), str)
                    else None,
                    details=exc.details,
                )
        with self._state_lock:
            active = self._active
        if active is not None:
            active.thread.join()
        return 0


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="python -m scriptscore.transport.desktop_worker")
    parser.add_argument("--socket-path", type=Path)
    parser.add_argument("--pipe-name", type=str)
    return parser


def _connect_unix_socket(socket_path: Path) -> tuple[socket.socket, DuplexStream]:
    client = socket.socket(socket.AddressFamily(cast(Any, socket).AF_UNIX), socket.SOCK_STREAM)
    client.connect(str(socket_path))
    return client, cast(DuplexStream, client.makefile("rwb", buffering=0))


def _connect_named_pipe(pipe_name: str) -> WindowsNamedPipeStream:
    request_pipe_path = Path(rf"\\.\pipe\{pipe_name}-request")
    response_pipe_path = Path(rf"\\.\pipe\{pipe_name}-response")
    binary_flag = getattr(os, "O_BINARY", 0)
    read_fd = os.open(str(request_pipe_path), os.O_RDONLY | binary_flag)
    try:
        write_fd = os.open(str(response_pipe_path), os.O_WRONLY | binary_flag)
    except OSError:
        os.close(read_fd)
        raise
    stream = WindowsNamedPipeStream(read_fd, write_fd)
    return stream


def main(argv: list[str] | None = None) -> int:
    parser = _build_parser()
    args = parser.parse_args(argv)
    server = DesktopWorkerServer(runner=create_worker_runner())

    if args.pipe_name:
        if sys.platform != "win32":
            parser.error("--pipe-name is only supported on Windows.")
        with _connect_named_pipe(args.pipe_name) as pipe_stream:
            return server.serve(stream=pipe_stream)
    if args.socket_path is None:
        parser.error("--socket-path is required on non-Windows platforms.")
    client, socket_stream = _connect_unix_socket(args.socket_path)
    try:
        return server.serve(stream=socket_stream)
    finally:
        socket_stream.close()
        client.close()


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
