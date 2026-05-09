# SPDX-License-Identifier: AGPL-3.0-only
"""Smoke tests for desktop development launchers."""

from __future__ import annotations

import errno
import json
import os
import select
import shutil
import subprocess
import time
import urllib.error
import urllib.request
from contextlib import suppress
from pathlib import Path
from typing import cast

import pytest

PROJECT_ROOT = Path(__file__).resolve().parents[2]
DESKTOP_ROOT = PROJECT_ROOT / "desktop"
FRONTEND_DIR = DESKTOP_ROOT / "frontend"
SCRIPTS_DIR = DESKTOP_ROOT / "scripts"
HOST_START_MARKER = "scriptscore-desktop-host:startup-complete"
HOST_START_TIMEOUT_ENV = "DESKTOP_HOST_START_TIMEOUT_SECONDS"


def _frontend_env(*, port: int, extra: dict[str, str] | None = None) -> dict[str, str]:
    env = os.environ.copy()
    env["VITE_HOST"] = "127.0.0.1"
    env["VITE_PORT"] = str(port)
    env["DESKTOP_FRONTEND_URL"] = f"http://127.0.0.1:{port}"
    env["DESKTOP_OPEN_BROWSER"] = "0"
    env["DESKTOP_FRONTEND_WAIT_SECONDS"] = "20"
    if extra:
        env.update(extra)
    return env


def _timeout_from_env(name: str, default: float) -> float:
    raw = os.environ.get(name)
    if raw is None:
        return default
    try:
        value = float(raw)
    except ValueError:
        return default
    return value if value > 0 else default


def _wait_for_url(url: str, *, timeout: float = 20.0) -> str:
    deadline = time.monotonic() + timeout
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=2) as response:
                return cast(bytes, response.read()).decode("utf-8", errors="replace")
        except Exception as exc:  # pragma: no cover - exercised in polling loop
            if _is_restricted_loopback_error(exc):
                pytest.skip("local loopback connections are blocked in this environment")
            last_error = exc
            time.sleep(0.25)
    raise AssertionError(f"Timed out waiting for {url}: {last_error}")


def _is_restricted_loopback_error(exc: Exception) -> bool:
    current: BaseException | None = exc
    while current is not None:
        if isinstance(current, urllib.error.URLError):
            current = current.reason if isinstance(current.reason, BaseException) else None
            continue
        if isinstance(current, OSError) and current.errno in {errno.EPERM, errno.EACCES}:
            return True
        current = current.__cause__ or current.__context__
    return False


def _terminate(proc: subprocess.Popen[bytes]) -> None:
    if proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:  # pragma: no cover - defensive cleanup
            proc.kill()
            proc.wait(timeout=10)


def _wait_for_output_marker(
    proc: subprocess.Popen[bytes], marker: str, *, timeout: float = 180.0
) -> None:
    if proc.stdout is None:  # pragma: no cover - defensive guard
        raise AssertionError("process stdout is not captured")

    deadline = time.monotonic() + timeout
    buffer = bytearray()
    marker_bytes = marker.encode("utf-8")

    while time.monotonic() < deadline:
        if proc.poll() is not None:
            buffer.extend(proc.stdout.read() or b"")
            raise AssertionError(
                f"Process exited before emitting marker {marker!r}.\n"
                f"Captured output:\n{buffer.decode('utf-8', errors='replace')}"
            )

        ready, _, _ = select.select([proc.stdout], [], [], 0.25)
        if not ready:
            continue

        chunk = os.read(proc.stdout.fileno(), 4096)
        if not chunk:
            continue
        buffer.extend(chunk)
        if marker_bytes in buffer:
            return

    raise AssertionError(
        f"Timed out waiting for marker {marker!r}.\n"
        f"Captured output:\n{buffer.decode('utf-8', errors='replace')}"
    )


def _launcher_process(script_name: str, *, port: int) -> subprocess.Popen[bytes]:
    bash = shutil.which("bash")
    if bash is None and Path("/bin/bash").is_file():
        bash = "/bin/bash"
    if bash is None:
        pytest.skip("bash is required for shell launcher smoke tests")
    return subprocess.Popen(
        [bash, str(SCRIPTS_DIR / script_name)],
        cwd=PROJECT_ROOT,
        env=_frontend_env(port=port),
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )


def test_dev_vite_launcher_serves_the_frontend() -> None:
    if not (FRONTEND_DIR / "node_modules").is_dir():
        pytest.skip("desktop frontend dependencies are not installed")

    proc = _launcher_process("dev-vite.sh", port=4173)
    try:
        html = _wait_for_url("http://127.0.0.1:4173")
        assert "<!doctype html>" in html.lower()
        assert "data-sveltekit-preload-data" in html
    finally:
        _terminate(proc)


def test_dev_frontend_launcher_serves_browser_preview_without_opening_browser() -> None:
    if not (FRONTEND_DIR / "node_modules").is_dir():
        pytest.skip("desktop frontend dependencies are not installed")

    proc = _launcher_process("dev-frontend.sh", port=4174)
    try:
        html = _wait_for_url("http://127.0.0.1:4174")
        assert "<!doctype html>" in html.lower()
        assert "data-sveltekit-preload-data" in html
    finally:
        _terminate(proc)


def test_dev_desktop_launcher_override_keeps_frontend_dev_command(tmp_path: Path) -> None:
    capture_path = tmp_path / "tauri-config.json"
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    cargo = bin_dir / "cargo"
    cargo.write_text(
        "\n".join(
            [
                "#!/usr/bin/env bash",
                "set -euo pipefail",
                'if [[ "$1" == "tauri" && "$2" == "--help" ]]; then',
                "  exit 0",
                "fi",
                'if [[ "$1" == "tauri" && "$2" == "dev" && "$3" == "--config" ]]; then',
                '  printf "%s" "$4" > "${SCRIPTSCORE_TEST_TAURI_CONFIG_CAPTURE}"',
                "  exit 0",
                "fi",
                'echo "unexpected cargo invocation: $*" >&2',
                "exit 2",
            ]
        )
        + "\n",
        encoding="utf-8",
    )
    cargo.chmod(0o755)

    env = _frontend_env(
        port=4175,
        extra={
            "PATH": f"{bin_dir}{os.pathsep}{os.environ.get('PATH', '')}",
            "SCRIPTSCORE_TEST_TAURI_CONFIG_CAPTURE": str(capture_path),
        },
    )

    subprocess.run(
        ["/bin/bash", str(SCRIPTS_DIR / "dev-desktop.sh")],
        cwd=PROJECT_ROOT,
        env=env,
        check=True,
    )

    config = json.loads(capture_path.read_text(encoding="utf-8"))
    assert config["build"]["devUrl"] == "http://127.0.0.1:4175"
    assert config["build"]["beforeDevCommand"] == "bash ../scripts/dev-vite.sh"


@pytest.mark.skipif(shutil.which("xvfb-run") is None, reason="xvfb-run is required")
def test_dev_desktop_launcher_reaches_host_startup() -> None:
    if shutil.which("cargo") is None:
        pytest.skip("cargo is required")
    if (
        subprocess.run(
            ["cargo", "tauri", "--help"],
            cwd=PROJECT_ROOT,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        ).returncode
        != 0
    ):
        pytest.skip("cargo-tauri is required")
    if not (FRONTEND_DIR / "node_modules").is_dir():
        pytest.skip("desktop frontend dependencies are not installed")

    proc = subprocess.Popen(
        ["xvfb-run", "-a", "/bin/bash", str(SCRIPTS_DIR / "dev-desktop.sh")],
        cwd=PROJECT_ROOT,
        env=_frontend_env(
            port=4175,
            extra={
                "WEBKIT_DISABLE_DMABUF_RENDERER": "1",
            },
        ),
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    try:
        html = _wait_for_url("http://127.0.0.1:4175", timeout=60.0)
        assert "<!doctype html>" in html.lower()
        assert "data-sveltekit-preload-data" in html
        assert "scriptscore-app-icon.png" in html
        _wait_for_output_marker(
            proc,
            HOST_START_MARKER,
            timeout=_timeout_from_env(HOST_START_TIMEOUT_ENV, 180.0),
        )
        assert proc.poll() is None
    finally:
        _terminate(proc)
        with suppress(Exception):
            if proc.stdout is not None:
                proc.stdout.close()
