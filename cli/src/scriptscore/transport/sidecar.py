# SPDX-License-Identifier: AGPL-3.0-only
"""JSON-RPC sidecar transport."""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass
from threading import Lock, Thread
from typing import Any, TextIO

from scriptscore.contracts import ConflictError
from scriptscore.engine import ScriptScoreEngine
from scriptscore.runtime import (
    CancellationToken,
    make_error_envelope,
    new_operation_id,
)
from scriptscore.runtime.context import utc_now_iso


@dataclass
class ActiveRequest:
    """State for the single active request per sidecar process."""

    request_id: str | int | None
    cancellation_token: CancellationToken
    thread: Thread


class SidecarServer:
    """Single-flight JSON-RPC sidecar server."""

    def __init__(self, *, engine: ScriptScoreEngine) -> None:
        self.engine = engine
        self._write_lock = Lock()
        self._state_lock = Lock()
        self._active: ActiveRequest | None = None

    def _write_message(self, payload: dict[str, Any], *, stdout: TextIO) -> None:
        with self._write_lock:
            stdout.write(json.dumps(payload, separators=(",", ":"), sort_keys=True))
            stdout.write("\n")
            stdout.flush()

    def _emit_event(self, event: Any, *, stdout: TextIO) -> None:
        self._write_message(
            {
                "jsonrpc": "2.0",
                "method": "scriptscore.progress",
                "params": event.model_dump(mode="json", exclude_none=True),
            },
            stdout=stdout,
        )

    def _clear_active(self, request_id: str | int | None) -> None:
        with self._state_lock:
            if self._active is not None and self._active.request_id == request_id:
                self._active = None

    def _command_worker(
        self,
        *,
        request_id: str | int | None,
        method: str,
        params: dict[str, Any],
        stdout: TextIO,
        cancellation_token: CancellationToken,
    ) -> None:
        try:
            result = self.engine.run(
                method,
                params,
                request_id=None if request_id is None else str(request_id),
                event_sink=lambda event: self._emit_event(event, stdout=stdout),
                cancellation_token=cancellation_token,
            )
            if request_id is None:
                return
            envelope = result.envelope.model_dump(mode="json", exclude_none=True)
            if result.exit_code == 0:
                payload = {"jsonrpc": "2.0", "id": request_id, "result": envelope}
            else:
                payload = {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "error": {
                        "code": -32000,
                        "message": envelope["error"]["message"],
                        "data": envelope,
                    },
                }
            self._write_message(payload, stdout=stdout)
        finally:
            self._clear_active(request_id)

    def _handle_cancel(self, message: dict[str, Any]) -> None:
        params = message.get("params") or {}
        requested_id = params.get("id")
        with self._state_lock:
            active = self._active
        if active is not None and active.request_id == requested_id:
            active.cancellation_token.cancel()

    def _conflict_response(
        self, *, request_id: str | int | None, method: str, stdout: TextIO
    ) -> None:
        if request_id is None:
            return
        now = utc_now_iso().replace("Z", "+00:00")
        conflict = ConflictError(
            code="request_conflict",
            message="A command request is already active in this sidecar process.",
        )
        envelope = make_error_envelope(
            command=method,
            operation_id=new_operation_id(),
            request_id=None if request_id is None else str(request_id),
            error=conflict,
            started=datetime_from_iso(now),
            finished=datetime_from_iso(now),
        )
        self._write_message(
            {
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {
                    "code": -32004,
                    "message": conflict.message,
                    "data": envelope.model_dump(mode="json", exclude_none=True),
                },
            },
            stdout=stdout,
        )

    def serve(self, *, stdin: TextIO | None = None, stdout: TextIO | None = None) -> int:
        """Serve JSON-RPC over stdin/stdout until EOF."""

        in_stream = stdin or sys.stdin
        out_stream = stdout or sys.stdout

        for line in in_stream:
            raw = line.strip()
            if not raw:
                continue
            try:
                message = json.loads(raw)
            except json.JSONDecodeError:
                self._write_message(
                    {
                        "jsonrpc": "2.0",
                        "id": None,
                        "error": {"code": -32700, "message": "Parse error"},
                    },
                    stdout=out_stream,
                )
                continue
            if isinstance(message, list) or not isinstance(message, dict):
                self._write_message(
                    {
                        "jsonrpc": "2.0",
                        "id": None,
                        "error": {"code": -32600, "message": "Invalid Request"},
                    },
                    stdout=out_stream,
                )
                continue
            method = message.get("method")
            request_id = message.get("id")
            if message.get("jsonrpc") != "2.0":
                self._write_message(
                    {
                        "jsonrpc": "2.0",
                        "id": request_id,
                        "error": {"code": -32600, "message": "Invalid Request"},
                    },
                    stdout=out_stream,
                )
                continue
            if method == "$/cancelRequest":
                self._handle_cancel(message)
                continue
            if not isinstance(method, str):
                self._write_message(
                    {
                        "jsonrpc": "2.0",
                        "id": request_id,
                        "error": {"code": -32600, "message": "Invalid Request"},
                    },
                    stdout=out_stream,
                )
                continue
            params = message.get("params") or {}
            if not isinstance(params, dict):
                self._write_message(
                    {
                        "jsonrpc": "2.0",
                        "id": request_id,
                        "error": {"code": -32602, "message": "Invalid params"},
                    },
                    stdout=out_stream,
                )
                continue
            with self._state_lock:
                active = self._active
                if active is not None:
                    self._conflict_response(request_id=request_id, method=method, stdout=out_stream)
                    continue
                token = CancellationToken()
                worker = Thread(
                    target=self._command_worker,
                    kwargs={
                        "request_id": request_id,
                        "method": method,
                        "params": params,
                        "stdout": out_stream,
                        "cancellation_token": token,
                    },
                    daemon=True,
                )
                self._active = ActiveRequest(
                    request_id=request_id, cancellation_token=token, thread=worker
                )
                worker.start()

        with self._state_lock:
            active = self._active
        if active is not None:
            active.thread.join()
        return 0


def datetime_from_iso(value: str) -> Any:
    """Parse a UTC ISO timestamp into a datetime."""

    from datetime import datetime

    return datetime.fromisoformat(value)
