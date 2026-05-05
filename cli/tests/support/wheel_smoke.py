# SPDX-License-Identifier: AGPL-3.0-only
"""Smoke-test an installed scriptscore wheel via its console entrypoint."""

from __future__ import annotations

import argparse
import json
import os
import queue
import subprocess
import threading
from pathlib import Path
from typing import Any


def _clean_env() -> dict[str, str]:
    env = os.environ.copy()
    env.pop("PYTHONPATH", None)
    return env


def _run_command(command: list[str], *, cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=cwd,
        env=_clean_env(),
        capture_output=True,
        text=True,
        check=False,
    )


def _assert_success(command: list[str], *, cwd: Path) -> subprocess.CompletedProcess[str]:
    completed = _run_command(command, cwd=cwd)
    if completed.returncode != 0:
        raise SystemExit(
            "Command failed with exit code "
            f"{completed.returncode}: {' '.join(command)}\n"
            f"stdout:\n{completed.stdout}\n"
            f"stderr:\n{completed.stderr}"
        )
    return completed


def _assert_help(scriptscore_bin: Path, *, cwd: Path) -> None:
    completed = _assert_success([str(scriptscore_bin), "--help"], cwd=cwd)
    if "scriptscore" not in completed.stdout:
        raise SystemExit("--help output did not contain the scriptscore command name.")


def _assert_builtin_prompt_resource(python_bin: Path, *, cwd: Path) -> None:
    command = [
        str(python_bin),
        "-c",
        (
            "from scriptscore.prompts import get_builtin_prompt; "
            "prompt = get_builtin_prompt('question_text'); "
            "assert prompt.id == 'question_text'; "
            "assert 'Baseline text from `pdftotext`.' in prompt.template_text"
        ),
    ]
    _assert_success(command, cwd=cwd)


def _assert_engine_smoke(python_bin: Path, *, cwd: Path) -> None:
    command = [
        str(python_bin),
        "-c",
        (
            "from scriptscore.engine import ScriptScoreEngine, create_engine; "
            "engine = create_engine(); "
            "assert isinstance(engine, ScriptScoreEngine); "
            "result = engine.run('smoke.ping', {'message': 'wheel', 'steps': 1}); "
            "assert result.exit_code == 0; "
            "assert result.envelope.command == 'smoke.ping'; "
            "assert result.envelope.data['message'] == 'wheel'"
        ),
    ]
    _assert_success(command, cwd=cwd)


def _assert_direct_smoke(scriptscore_bin: Path, *, cwd: Path) -> None:
    completed = _assert_success(
        [
            str(scriptscore_bin),
            "_smoke",
            "ping",
            "--options",
            '{"message":"wheel","steps":1}',
        ],
        cwd=cwd,
    )
    lines = [json.loads(line) for line in completed.stdout.splitlines() if line.strip()]
    if not lines:
        raise SystemExit("Direct smoke command produced no JSON output.")
    envelope = lines[-1]
    if envelope.get("ok") is not True or envelope.get("command") != "smoke.ping":
        raise SystemExit(f"Unexpected direct smoke envelope: {json.dumps(envelope)}")


def _assert_sidecar_smoke(scriptscore_bin: Path, *, cwd: Path) -> None:
    process = subprocess.Popen(
        [str(scriptscore_bin), "sidecar", "rpc"],
        cwd=cwd,
        env=_clean_env(),
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )
    messages: queue.Queue[dict[str, Any]] = queue.Queue()

    def pump_stdout() -> None:
        assert process.stdout is not None
        for line in process.stdout:
            raw = line.strip()
            if raw:
                messages.put(json.loads(raw))

    reader = threading.Thread(target=pump_stdout, daemon=True)
    reader.start()
    try:
        assert process.stdin is not None
        process.stdin.write(
            json.dumps(
                {
                    "jsonrpc": "2.0",
                    "id": "wheel_smoke",
                    "method": "smoke.ping",
                    "params": {"message": "wheel", "steps": 1},
                }
            )
        )
        process.stdin.write("\n")
        process.stdin.flush()
        while True:
            try:
                message = messages.get(timeout=5.0)
            except queue.Empty as exc:
                raise SystemExit(
                    "Timed out waiting for sidecar response from the installed sidecar."
                ) from exc
            if message.get("method") == "scriptscore.progress":
                continue
            if message.get("id") != "wheel_smoke":
                raise SystemExit(f"Unexpected sidecar message: {json.dumps(message)}")
            result = message.get("result")
            if not isinstance(result, dict):
                raise SystemExit(f"Sidecar returned unexpected payload: {json.dumps(message)}")
            if result.get("ok") is not True or result.get("command") != "smoke.ping":
                raise SystemExit(f"Unexpected sidecar result: {json.dumps(message)}")
            return
    finally:
        if process.stdin is not None:
            process.stdin.close()
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=5)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--scriptscore-bin",
        required=True,
        type=Path,
        help="Path to the installed scriptscore console entrypoint.",
    )
    parser.add_argument(
        "--cwd",
        type=Path,
        default=Path.cwd(),
        help="Working directory used for smoke commands.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    scriptscore_bin = args.scriptscore_bin.resolve()
    python_bin = scriptscore_bin.parent / "python"
    cwd = args.cwd.resolve()
    _assert_help(scriptscore_bin, cwd=cwd)
    _assert_builtin_prompt_resource(python_bin, cwd=cwd)
    _assert_engine_smoke(python_bin, cwd=cwd)
    _assert_direct_smoke(scriptscore_bin, cwd=cwd)
    _assert_sidecar_smoke(scriptscore_bin, cwd=cwd)
    print("wheel smoke passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
