# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for runtime inspection commands."""

from __future__ import annotations

import io
import json
from dataclasses import dataclass
from typing import Any

import pytest

import scriptscore.cli as cli_module
from scriptscore.contracts import CommandErrorEnvelope, CommandSuccessEnvelope
from scriptscore.engine import create_engine
from scriptscore.transport import SidecarServer


@dataclass
class _FakeClient:
    host: str
    headers: dict[str, str] | None
    timeout: int


def _messages(raw: str) -> list[dict[str, Any]]:
    return [json.loads(line) for line in raw.splitlines() if line.strip()]


def _configure_fake_ollama(monkeypatch: pytest.MonkeyPatch) -> None:
    from scriptscore.commands import runtime_list_llm_models as runtime_module

    def fake_client_factory(
        *, host: str, headers: dict[str, str] | None, timeout_seconds: int | None
    ) -> _FakeClient:
        return _FakeClient(host=host, headers=headers, timeout=timeout_seconds or 0)

    def fake_client_list(client: _FakeClient) -> dict[str, object]:
        assert client.host == "http://127.0.0.1:11434"
        return {
            "models": [
                {"model": "qwen2.5vl:7b"},
                {"model": "qwen2.5:14b"},
                {"model": "llava:7b"},
            ]
        }

    def fake_client_show(client: _FakeClient, *, model: str) -> dict[str, object]:
        assert client.host == "http://127.0.0.1:11434"
        capabilities = {
            "qwen2.5vl:7b": ["vision", "completion"],
            "qwen2.5:14b": ["completion"],
            "llava:7b": ["vision", "completion"],
        }
        if model not in capabilities:
            raise RuntimeError(f"model {model} not found")
        return {"capabilities": capabilities[model]}

    monkeypatch.setattr(runtime_module, "_client_factory", fake_client_factory)
    monkeypatch.setattr(runtime_module, "_client_list", fake_client_list)
    monkeypatch.setattr(runtime_module, "_client_show", fake_client_show)


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


def test_runtime_list_llm_models_matches_between_direct_cli_and_sidecar(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    _configure_fake_ollama(monkeypatch)
    payload = {
        "providers": {"llm_provider": "ollama_native"},
        "llm_discovery_config": {"base_url": "http://127.0.0.1:11434", "timeout_seconds": 5},
        "required_capabilities": ["vision"],
    }

    monkeypatch.setattr("sys.stdin", io.StringIO(json.dumps(payload)))
    direct_exit_code = cli_module.main(
        [
            "runtime",
            "list-llm-models",
            "--stdin",
            "--emit-events",
            "--request-id",
            "req_models",
        ]
    )
    direct_messages = _messages(capsys.readouterr().out)
    direct_events = [line for line in direct_messages if line.get("type") == "event"]
    direct_terminal = next(line for line in reversed(direct_messages) if "ok" in line)

    sidecar_stdout = io.StringIO()
    SidecarServer(engine=create_engine(include_builtin_fakes=True)).serve(
        stdin=io.StringIO(
            json.dumps(
                {
                    "jsonrpc": "2.0",
                    "id": "req_models",
                    "method": "runtime.list-llm-models",
                    "params": payload,
                }
            )
            + "\n"
        ),
        stdout=sidecar_stdout,
    )
    sidecar_messages = _messages(sidecar_stdout.getvalue())
    sidecar_events = [
        message["params"]
        for message in sidecar_messages
        if message.get("method") == "scriptscore.progress"
    ]
    sidecar_terminal = next(
        message["result"] for message in sidecar_messages if message.get("id") == "req_models"
    )

    assert direct_exit_code == 0
    assert [_normalize_event(event) for event in direct_events] == [
        _normalize_event(event) for event in sidecar_events
    ]
    assert _normalize_terminal(direct_terminal) == _normalize_terminal(sidecar_terminal)
    assert direct_terminal["data"]["models"] == [
        {
            "model": "qwen2.5vl:7b",
            "display_name": "qwen2.5vl:7b",
            "capabilities": ["vision", "completion"],
        },
        {
            "model": "llava:7b",
            "display_name": "llava:7b",
            "capabilities": ["vision", "completion"],
        },
    ]


def test_runtime_list_llm_models_requires_provider_selection() -> None:
    engine = create_engine(include_builtin_fakes=True)

    result = engine.run(
        "runtime.list-llm-models",
        {"llm_discovery_config": {"base_url": "http://127.0.0.1:11434"}},
    )

    assert result.exit_code == 2
    assert result.envelope.ok is False
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.code == "validation_failed"


def test_runtime_validate_llm_model_matches_between_direct_cli_and_sidecar(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    _configure_fake_ollama(monkeypatch)
    payload = {
        "providers": {"llm_provider": "ollama_native"},
        "llm_discovery_config": {"base_url": "http://127.0.0.1:11434", "timeout_seconds": 5},
        "model": "qwen2.5vl:7b",
        "required_capabilities": ["vision"],
    }

    monkeypatch.setattr("sys.stdin", io.StringIO(json.dumps(payload)))
    direct_exit_code = cli_module.main(
        [
            "runtime",
            "validate-llm-model",
            "--stdin",
            "--emit-events",
            "--request-id",
            "req_validate_model",
        ]
    )
    direct_messages = _messages(capsys.readouterr().out)
    direct_events = [line for line in direct_messages if line.get("type") == "event"]
    direct_terminal = next(line for line in reversed(direct_messages) if "ok" in line)

    sidecar_stdout = io.StringIO()
    SidecarServer(engine=create_engine(include_builtin_fakes=True)).serve(
        stdin=io.StringIO(
            json.dumps(
                {
                    "jsonrpc": "2.0",
                    "id": "req_validate_model",
                    "method": "runtime.validate-llm-model",
                    "params": payload,
                }
            )
            + "\n"
        ),
        stdout=sidecar_stdout,
    )
    sidecar_messages = _messages(sidecar_stdout.getvalue())
    sidecar_events = [
        message["params"]
        for message in sidecar_messages
        if message.get("method") == "scriptscore.progress"
    ]
    sidecar_terminal = next(
        message["result"]
        for message in sidecar_messages
        if message.get("id") == "req_validate_model"
    )

    assert direct_exit_code == 0
    assert [_normalize_event(event) for event in direct_events] == [
        _normalize_event(event) for event in sidecar_events
    ]
    assert _normalize_terminal(direct_terminal) == _normalize_terminal(sidecar_terminal)
    assert direct_terminal["data"] == {
        "model": "qwen2.5vl:7b",
        "display_name": "qwen2.5vl:7b",
        "capabilities": ["vision", "completion"],
        "valid": True,
        "reason": None,
        "missing_capabilities": [],
    }


def test_runtime_validate_llm_model_reports_missing_capabilities(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _configure_fake_ollama(monkeypatch)
    engine = create_engine(include_builtin_fakes=True)

    result = engine.run(
        "runtime.validate-llm-model",
        {
            "providers": {"llm_provider": "ollama_native"},
            "llm_discovery_config": {"base_url": "http://127.0.0.1:11434"},
            "model": "qwen2.5:14b",
            "required_capabilities": ["vision"],
        },
    )

    assert result.exit_code == 0
    assert result.envelope.ok is True
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.data == {
        "model": "qwen2.5:14b",
        "display_name": "qwen2.5:14b",
        "capabilities": ["completion"],
        "valid": False,
        "reason": "missing_capabilities",
        "missing_capabilities": ["vision"],
    }


def test_runtime_validate_llm_model_accepts_documented_ollama_cloud_api_root(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from scriptscore.commands import runtime_list_llm_models as runtime_module

    captured: dict[str, Any] = {}

    def fake_client_factory(
        *, host: str, headers: dict[str, str] | None, timeout_seconds: int | None
    ) -> _FakeClient:
        captured["host"] = host
        captured["headers"] = headers
        return _FakeClient(host=host, headers=headers, timeout=timeout_seconds or 0)

    def fake_client_show(client: _FakeClient, *, model: str) -> dict[str, object]:
        assert client.host == "https://ollama.com"
        assert client.headers == {"Authorization": "Bearer cloud-token"}
        assert model == "gpt-oss:120b"
        return {"capabilities": ["completion", "vision"]}

    monkeypatch.setattr(runtime_module, "_client_factory", fake_client_factory)
    monkeypatch.setattr(runtime_module, "_client_show", fake_client_show)
    engine = create_engine(include_builtin_fakes=True)

    result = engine.run(
        "runtime.validate-llm-model",
        {
            "providers": {"llm_provider": "ollama_cloud"},
            "llm_discovery_config": {
                "base_url": "https://ollama.com/api",
                "api_key": "cloud-token",
            },
            "model": "gpt-oss:120b",
            "required_capabilities": ["vision"],
        },
    )

    assert result.exit_code == 0
    assert result.envelope.ok is True
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.data["valid"] is True
    assert captured == {
        "host": "https://ollama.com",
        "headers": {"Authorization": "Bearer cloud-token"},
    }


def test_runtime_validate_llm_model_reports_unavailable_model(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _configure_fake_ollama(monkeypatch)
    engine = create_engine(include_builtin_fakes=True)

    result = engine.run(
        "runtime.validate-llm-model",
        {
            "providers": {"llm_provider": "ollama_native"},
            "llm_discovery_config": {"base_url": "http://127.0.0.1:11434"},
            "model": "missing-model:1b",
        },
    )

    assert result.exit_code == 0
    assert result.envelope.ok is True
    assert isinstance(result.envelope, CommandSuccessEnvelope)
    assert result.envelope.data == {
        "model": "missing-model:1b",
        "display_name": "missing-model:1b",
        "capabilities": [],
        "valid": False,
        "reason": "model_unavailable",
        "missing_capabilities": [],
    }


def test_runtime_validate_llm_model_requires_provider_selection() -> None:
    engine = create_engine(include_builtin_fakes=True)

    result = engine.run(
        "runtime.validate-llm-model",
        {"model": "qwen2.5vl:7b"},
    )

    assert result.exit_code == 2
    assert result.envelope.ok is False
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.code == "validation_failed"
