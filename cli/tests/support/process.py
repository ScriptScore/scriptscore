# SPDX-License-Identifier: AGPL-3.0-only
"""Subprocess helpers for CLI and sidecar tests."""

from __future__ import annotations

import json
import os
import queue
import socket
import subprocess
import sys
import tempfile
import threading
from contextlib import suppress
from dataclasses import dataclass
from pathlib import Path
from typing import Any, cast

from scriptscore.transport.desktop_worker import decode_frame, encode_frame

PROJECT_ROOT = Path(__file__).resolve().parents[2]
SRC_ROOT = PROJECT_ROOT / "src"


@dataclass(frozen=True)
class DirectCliResult:
    """Captured direct CLI result."""

    returncode: int
    stdout_lines: list[dict[str, Any]]
    stderr: str


def _base_env(*, overrides: dict[str, str] | None = None) -> dict[str, str]:
    env = os.environ.copy()
    env["PYTHONPATH"] = str(SRC_ROOT)
    env["SCRIPTSCORE_INCLUDE_BUILTIN_FAKES"] = "1"
    if overrides:
        env.update(overrides)
    return env


def run_direct_cli(
    args: list[str],
    *,
    stdin_json: dict[str, Any] | None = None,
    env_overrides: dict[str, str] | None = None,
) -> DirectCliResult:
    """Run the CLI as a subprocess and parse NDJSON stdout."""

    completed = subprocess.run(
        [sys.executable, "-m", "scriptscore", *args],
        cwd=PROJECT_ROOT,
        env=_base_env(overrides=env_overrides),
        input=None if stdin_json is None else json.dumps(stdin_json),
        capture_output=True,
        text=True,
        check=False,
    )
    stdout_lines = [json.loads(line) for line in completed.stdout.splitlines() if line.strip()]
    return DirectCliResult(
        returncode=completed.returncode,
        stdout_lines=stdout_lines,
        stderr=completed.stderr,
    )


class SidecarSession:
    """JSON-RPC sidecar subprocess with queued stdout decoding."""

    def __init__(self, *, env_overrides: dict[str, str] | None = None) -> None:
        self._proc = subprocess.Popen(
            [sys.executable, "-m", "scriptscore", "sidecar", "rpc"],
            cwd=PROJECT_ROOT,
            env=_base_env(overrides=env_overrides),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        self._queue: queue.Queue[dict[str, Any]] = queue.Queue()
        self._stderr_lines: list[str] = []
        self._reader = threading.Thread(target=self._pump_stdout, daemon=True)
        self._stderr_reader = threading.Thread(target=self._pump_stderr, daemon=True)
        self._reader.start()
        self._stderr_reader.start()

    def _pump_stdout(self) -> None:
        assert self._proc.stdout is not None
        for line in self._proc.stdout:
            raw = line.strip()
            if raw:
                self._queue.put(json.loads(raw))

    def _pump_stderr(self) -> None:
        assert self._proc.stderr is not None
        for line in self._proc.stderr:
            raw = line.strip()
            if raw:
                self._stderr_lines.append(raw)

    def send(self, payload: dict[str, Any]) -> None:
        """Send a JSON-RPC request or notification."""

        assert self._proc.stdin is not None
        self._proc.stdin.write(json.dumps(payload))
        self._proc.stdin.write("\n")
        self._proc.stdin.flush()

    def next_message(self, *, timeout: float = 5.0) -> dict[str, Any]:
        """Return the next decoded sidecar message."""

        try:
            return self._queue.get(timeout=timeout)
        except queue.Empty as exc:
            stderr_tail = "\n".join(self._stderr_lines[-20:])
            if self._proc.poll() is not None:
                detail = (
                    f"sidecar exited with code {self._proc.returncode} before producing a message"
                )
            else:
                detail = f"timed out waiting {timeout:.1f}s for sidecar response"
            if stderr_tail:
                detail = f"{detail}\nCaptured stderr:\n{stderr_tail}"
            raise AssertionError(detail) from exc

    def close(self) -> None:
        """Terminate the sidecar process and wait for shutdown."""

        if self._proc.stdin is not None:
            self._proc.stdin.close()
        self._proc.terminate()
        self._proc.wait(timeout=5)

    def __enter__(self) -> SidecarSession:
        return self

    def __exit__(self, *_args: object) -> None:
        self.close()


class DesktopWorkerSession:
    """Desktop worker subprocess connected over a Unix domain socket."""

    def __init__(self, *, env_overrides: dict[str, str] | None = None) -> None:
        self._tmpdir = tempfile.TemporaryDirectory()
        self._socket_path = Path(self._tmpdir.name) / "desktop-worker.sock"
        self._listener = socket.socket(
            socket.AddressFamily(cast(Any, socket).AF_UNIX), socket.SOCK_STREAM
        )
        try:
            self._listener.bind(str(self._socket_path))
        except PermissionError as exc:
            self._listener.close()
            self._tmpdir.cleanup()
            raise RuntimeError("Unix domain sockets are unavailable in this environment.") from exc
        self._listener.listen(1)
        self._listener.settimeout(5)
        self._proc = subprocess.Popen(
            [
                sys.executable,
                "-m",
                "scriptscore.transport.desktop_worker",
                "--socket-path",
                str(self._socket_path),
            ],
            cwd=PROJECT_ROOT,
            env=_base_env(overrides=env_overrides),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self._conn, _ = self._listener.accept()
        self._stream = self._conn.makefile("rwb", buffering=0)
        self._queue: queue.Queue[dict[str, Any]] = queue.Queue()
        self._closing = threading.Event()
        self._reader = threading.Thread(target=self._pump_stream, daemon=True)
        self._reader.start()

    def _pump_stream(self) -> None:
        while True:
            try:
                message = decode_frame(self._stream)
            except OSError:
                if self._closing.is_set():
                    return
                raise
            if message is None:
                return
            self._queue.put(message)

    def send(self, payload: dict[str, Any]) -> None:
        """Send one framed worker request."""

        self._stream.write(encode_frame(payload))
        self._stream.flush()

    def next_message(self, *, timeout: float = 2.0) -> dict[str, Any]:
        """Return the next decoded worker message."""

        return self._queue.get(timeout=timeout)

    def close(self) -> None:
        """Close the worker session and wait for shutdown."""

        if self._proc.poll() is None:
            with suppress(BrokenPipeError, OSError):
                self.send({"type": "shutdown", "request_id": "req_shutdown", "payload": {}})
        self._closing.set()
        with suppress(OSError):
            self._conn.shutdown(socket.SHUT_RDWR)
        self._stream.close()
        self._conn.close()
        self._listener.close()
        self._tmpdir.cleanup()
        self._reader.join(timeout=2)
        if self._proc.poll() is None:
            self._proc.terminate()
        self._proc.wait(timeout=5)

    def __enter__(self) -> DesktopWorkerSession:
        return self

    def __exit__(self, *_args: object) -> None:
        self.close()
