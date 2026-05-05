# SPDX-License-Identifier: AGPL-3.0-only
"""In-process CLI entrypoint tests for coverage of direct dispatch logic."""

from __future__ import annotations

import io
import json

import pytest

import scriptscore.cli as cli_module
from scriptscore.commands import build_command_registry


def test_main_runs_hidden_smoke_command_with_options(capsys: pytest.CaptureFixture[str]) -> None:
    exit_code = cli_module.main(["_smoke", "ping", "--options", '{"message":"hello","steps":1}'])
    out = capsys.readouterr().out.strip().splitlines()
    assert exit_code == 0
    assert json.loads(out[-1])["data"]["message"] == "hello"


def test_main_rejects_both_stdin_and_options(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("sys.stdin", io.StringIO("{}"))
    with pytest.raises(SystemExit):
        cli_module.main(["_smoke", "ping", "--stdin", "--options", "{}"])


def test_main_delegates_sidecar_rpc(monkeypatch: pytest.MonkeyPatch) -> None:
    seen: dict[str, object] = {}

    class DummySidecar:
        def __init__(self, *, engine: object) -> None:
            seen["engine"] = engine

        def serve(self) -> int:
            seen["served"] = True
            return 7

    monkeypatch.setattr(cli_module, "SidecarServer", DummySidecar)
    assert cli_module.main(["sidecar", "rpc"]) == 7
    assert seen["served"] is True


def test_main_without_args_prints_help(capsys: pytest.CaptureFixture[str]) -> None:
    assert cli_module.main([]) == 0
    assert "usage: scriptscore" in capsys.readouterr().out


def test_registry_includes_phase_three_commands() -> None:
    registry = build_command_registry()
    assert registry.has("runtime.list-llm-models") is True
    assert registry.has("runtime.validate-llm-model") is True
    assert registry.has("exam.setup") is True
    assert registry.has("exam.analyze") is True
    assert registry.has("exam.generate-rubric") is True
    assert registry.has("scans.ingest") is True
    assert registry.has("scans.transform") is True
    assert registry.has("scans.canonicalize") is True
    assert registry.has("scans.align-auto") is True
    assert registry.has("scans.detect") is True
    assert registry.has("scans.crop") is True
    assert registry.has("scans.pii") is True
    assert registry.has("scans.parse") is True
    assert registry.has("scans.pdf-render-page") is True
    assert registry.has("scans.pdf-create-redacted") is True
    assert registry.has("scans.pdf-detect-aruco") is True
    assert registry.has("scans.pdf-stamp-aruco") is True
    assert registry.has("scans.pdf-clip-rects") is True
    assert registry.has("scans.pdf-extract-text") is True
    assert registry.has("scans.pdf-map-template-regions") is True
