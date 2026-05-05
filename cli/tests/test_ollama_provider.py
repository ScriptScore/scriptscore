# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for the built-in Ollama LLM providers."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

import pytest

from scriptscore.contracts import ErrorCategory, ScriptscoreError
from scriptscore.providers import LlmProviderConfig, LlmRequest
from scriptscore.providers.ollama import OllamaCloudProvider, OllamaNativeProvider
from tests.support.images import make_rgb_page


def _request(
    *,
    with_image: bool = False,
    image_path: str | None = None,
    provider_config: LlmProviderConfig | None = None,
    response_mode: str = "plain_text",
    response_contract: dict[str, object] | None = None,
    execution_options: dict[str, object] | None = None,
) -> LlmRequest:
    return LlmRequest(
        prompt_id="question_text",
        response_mode=response_mode,
        rendered_text="Explain the answer.",
        provider_config=provider_config
        or LlmProviderConfig(
            model="qwen2.5vl:7b" if with_image else "qwen2.5:14b",
            timeout_seconds=30,
            keep_alive="15m",
            options={"num_ctx": 8192},
        ),
        file_inputs={} if not with_image else {"question_crop_png": image_path or ""},
        execution_options={"temperature": 0.0} if execution_options is None else execution_options,
        response_contract=response_contract,
    )


@dataclass
class _FakeClient:
    host: str
    headers: dict[str, str] | None
    timeout: int


class _FakeResponseError(Exception):
    def __init__(self, status_code: int, error: str) -> None:
        super().__init__(error)
        self.status_code = status_code
        self.error = error


class ConnectError(Exception):
    pass


def test_ollama_native_generate_text_only_skips_capability_probe(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    calls: list[tuple[str, dict[str, Any]]] = []

    def fake_client_factory(
        *, host: str, headers: dict[str, str] | None, timeout_seconds: int | None
    ) -> _FakeClient:
        assert host == "http://127.0.0.1:11434"
        assert headers is None
        assert timeout_seconds == 30
        return _FakeClient(host=host, headers=headers, timeout=timeout_seconds or 0)

    def fake_generate(client: _FakeClient, **payload: object) -> dict[str, object]:
        calls.append(("generate", payload))
        assert client.host == "http://127.0.0.1:11434"
        return {"response": "ok"}

    monkeypatch.setattr("scriptscore.providers.ollama._client_factory", fake_client_factory)
    monkeypatch.setattr("scriptscore.providers.ollama._client_generate", fake_generate)

    response = OllamaNativeProvider().generate(_request())

    assert response.raw_text == "ok"
    assert [operation for operation, _payload in calls] == ["generate"]
    assert calls[0][1]["model"] == "qwen2.5:14b"
    assert calls[0][1]["options"] == {"temperature": 0.0}
    assert response.provider_response["host"] == "http://127.0.0.1:11434"


def test_ollama_native_generate_records_exact_token_usage(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    def fake_client_factory(
        *, host: str, headers: dict[str, str] | None, timeout_seconds: int | None
    ) -> _FakeClient:
        return _FakeClient(host=host, headers=headers, timeout=timeout_seconds or 0)

    def fake_generate(client: _FakeClient, **payload: object) -> dict[str, object]:
        return {
            "response": "ok",
            "prompt_eval_count": 11,
            "eval_count": 7,
        }

    monkeypatch.setattr("scriptscore.providers.ollama._client_factory", fake_client_factory)
    monkeypatch.setattr("scriptscore.providers.ollama._client_generate", fake_generate)

    response = OllamaNativeProvider().generate(_request())

    assert response.usage is not None
    assert response.usage.input_tokens == 11
    assert response.usage.output_tokens == 7
    assert response.usage.total_tokens == 18
    assert response.usage.count_source == "exact"


def test_ollama_native_generate_estimates_token_usage_when_counts_are_missing(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    def fake_client_factory(
        *, host: str, headers: dict[str, str] | None, timeout_seconds: int | None
    ) -> _FakeClient:
        return _FakeClient(host=host, headers=headers, timeout=timeout_seconds or 0)

    def fake_generate(client: _FakeClient, **payload: object) -> dict[str, object]:
        return {"response": "estimated output"}

    monkeypatch.setattr("scriptscore.providers.ollama._client_factory", fake_client_factory)
    monkeypatch.setattr("scriptscore.providers.ollama._client_generate", fake_generate)

    response = OllamaNativeProvider().generate(_request())

    assert response.usage is not None
    assert response.usage.input_tokens == 5
    assert response.usage.output_tokens == 4
    assert response.usage.total_tokens == 9
    assert response.usage.count_source == "estimated"
    assert response.usage.estimation_method == "chars_div_4"


def test_ollama_native_generate_image_prompt_checks_vision_capability(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    image_path = make_rgb_page(tmp_path / "q1.png")
    calls: list[tuple[str, dict[str, Any]]] = []

    def fake_client_factory(
        *, host: str, headers: dict[str, str] | None, timeout_seconds: int | None
    ) -> _FakeClient:
        assert host == "http://127.0.0.1:11434"
        assert headers is None
        assert timeout_seconds == 30
        return _FakeClient(host=host, headers=headers, timeout=timeout_seconds or 0)

    def fake_show(client: _FakeClient, *, model: str) -> dict[str, object]:
        calls.append(("show", {"model": model, "host": client.host}))
        return {"capabilities": ["vision"]}

    def fake_generate(client: _FakeClient, **payload: object) -> dict[str, object]:
        calls.append(("generate", payload))
        return {"response": "parsed"}

    monkeypatch.setattr("scriptscore.providers.ollama._client_factory", fake_client_factory)
    monkeypatch.setattr("scriptscore.providers.ollama._client_show", fake_show)
    monkeypatch.setattr("scriptscore.providers.ollama._client_generate", fake_generate)

    response = OllamaNativeProvider().generate(
        _request(
            with_image=True,
            image_path=str(image_path),
            response_mode="json_schema",
            execution_options={
                "temperature": 0.0,
                "top_p": 1,
                "top_k": 1,
                "seed": 42,
                "num_ctx": 16384,
                "keep_alive": "30m",
                "think": False,
            },
            response_contract={
                "type": "json_schema",
                "additionalProperties": False,
                "properties": {"tables": {"type": "array", "items": {"type": "string"}}},
                "required": ["tables"],
            },
        )
    )

    assert response.raw_text == "parsed"
    assert [operation for operation, _payload in calls] == ["show", "generate"]
    assert calls[0][1] == {"model": "qwen2.5vl:7b", "host": "http://127.0.0.1:11434"}
    assert "images" in calls[1][1]
    assert calls[1][1]["options"] == {
        "temperature": 0.0,
        "top_p": 1,
        "top_k": 1,
        "seed": 42,
        "num_ctx": 16384,
    }
    assert calls[1][1]["keep_alive"] == "15m"
    assert calls[1][1]["think"] is False
    assert calls[1][1]["format"] == {
        "type": "object",
        "additionalProperties": False,
        "properties": {"tables": {"type": "array", "items": {"type": "string"}}},
        "required": ["tables"],
    }


def test_ollama_native_generate_rejects_non_vision_model_for_image_prompt(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    image_path = make_rgb_page(tmp_path / "q1.png")

    def fake_client_factory(
        *, host: str, headers: dict[str, str] | None, timeout_seconds: int | None
    ) -> _FakeClient:
        return _FakeClient(host=host, headers=headers, timeout=timeout_seconds or 0)

    def fake_show(client: _FakeClient, *, model: str) -> dict[str, object]:
        return {"capabilities": ["completion"]}

    monkeypatch.setattr("scriptscore.providers.ollama._client_factory", fake_client_factory)
    monkeypatch.setattr("scriptscore.providers.ollama._client_show", fake_show)

    with pytest.raises(ScriptscoreError) as exc_info:
        OllamaNativeProvider().generate(_request(with_image=True, image_path=str(image_path)))

    assert exc_info.value.code == "llm_model_lacks_vision"
    assert exc_info.value.category == ErrorCategory.PROVIDER


def test_ollama_generate_rejects_invalid_generate_payload(monkeypatch: pytest.MonkeyPatch) -> None:
    def fake_client_factory(
        *, host: str, headers: dict[str, str] | None, timeout_seconds: int | None
    ) -> _FakeClient:
        return _FakeClient(host=host, headers=headers, timeout=timeout_seconds or 0)

    def fake_generate(client: _FakeClient, **payload: object) -> dict[str, object]:
        return {"done": True}

    monkeypatch.setattr("scriptscore.providers.ollama._client_factory", fake_client_factory)
    monkeypatch.setattr("scriptscore.providers.ollama._client_generate", fake_generate)

    with pytest.raises(ScriptscoreError) as exc_info:
        OllamaNativeProvider().generate(_request())

    assert exc_info.value.code == "ollama_response_invalid"
    assert exc_info.value.category == ErrorCategory.EXTERNAL_DEPENDENCY


def test_ollama_cloud_provider_injects_bearer_auth(monkeypatch: pytest.MonkeyPatch) -> None:
    observed_clients: list[_FakeClient] = []

    def fake_client_factory(
        *, host: str, headers: dict[str, str] | None, timeout_seconds: int | None
    ) -> _FakeClient:
        client = _FakeClient(host=host, headers=headers, timeout=timeout_seconds or 0)
        observed_clients.append(client)
        return client

    def fake_generate(client: _FakeClient, **payload: object) -> dict[str, object]:
        return {"response": "cloud ok"}

    monkeypatch.setattr("scriptscore.providers.ollama._client_factory", fake_client_factory)
    monkeypatch.setattr("scriptscore.providers.ollama._client_generate", fake_generate)

    response = OllamaCloudProvider().generate(
        _request(
            provider_config=LlmProviderConfig(
                model="qwen3:32b",
                api_key="secret-token",
                timeout_seconds=45,
            )
        )
    )

    assert response.raw_text == "cloud ok"
    assert observed_clients == [
        _FakeClient(
            host="https://ollama.com",
            headers={"Authorization": "Bearer secret-token"},
            timeout=45,
        )
    ]
    assert response.provider_response["host"] == "https://ollama.com"


def test_ollama_cloud_provider_honors_base_url_override(monkeypatch: pytest.MonkeyPatch) -> None:
    observed_clients: list[_FakeClient] = []

    def fake_client_factory(
        *, host: str, headers: dict[str, str] | None, timeout_seconds: int | None
    ) -> _FakeClient:
        client = _FakeClient(host=host, headers=headers, timeout=timeout_seconds or 0)
        observed_clients.append(client)
        return client

    def fake_generate(client: _FakeClient, **payload: object) -> dict[str, object]:
        return {"response": "override ok"}

    monkeypatch.setattr("scriptscore.providers.ollama._client_factory", fake_client_factory)
    monkeypatch.setattr("scriptscore.providers.ollama._client_generate", fake_generate)

    response = OllamaCloudProvider().generate(
        _request(
            provider_config=LlmProviderConfig(
                model="qwen3:32b",
                base_url="https://remote.example",
                api_key="secret-token",
                timeout_seconds=15,
            )
        )
    )

    assert response.raw_text == "override ok"
    assert observed_clients[0].host == "https://remote.example"


def test_ollama_cloud_provider_maps_auth_failure(monkeypatch: pytest.MonkeyPatch) -> None:
    def fake_client_factory(
        *, host: str, headers: dict[str, str] | None, timeout_seconds: int | None
    ) -> _FakeClient:
        return _FakeClient(host=host, headers=headers, timeout=timeout_seconds or 0)

    def fake_generate(client: _FakeClient, **payload: object) -> dict[str, object]:
        raise _FakeResponseError(401, "unauthorized")

    monkeypatch.setattr("scriptscore.providers.ollama._client_factory", fake_client_factory)
    monkeypatch.setattr("scriptscore.providers.ollama._client_generate", fake_generate)

    with pytest.raises(ScriptscoreError) as exc_info:
        OllamaCloudProvider().generate(
            _request(
                provider_config=LlmProviderConfig(
                    model="qwen3:32b",
                    api_key="secret-token",
                )
            )
        )

    assert exc_info.value.code == "ollama_auth_failed"
    assert exc_info.value.category == ErrorCategory.EXTERNAL_DEPENDENCY


def test_ollama_native_provider_maps_connection_failure(monkeypatch: pytest.MonkeyPatch) -> None:
    def fake_client_factory(
        *, host: str, headers: dict[str, str] | None, timeout_seconds: int | None
    ) -> _FakeClient:
        return _FakeClient(host=host, headers=headers, timeout=timeout_seconds or 0)

    def fake_generate(client: _FakeClient, **payload: object) -> dict[str, object]:
        raise ConnectError("connection refused")

    monkeypatch.setattr("scriptscore.providers.ollama._client_factory", fake_client_factory)
    monkeypatch.setattr("scriptscore.providers.ollama._client_generate", fake_generate)

    with pytest.raises(ScriptscoreError) as exc_info:
        OllamaNativeProvider().generate(_request())

    assert exc_info.value.code == "ollama_unreachable"
    assert exc_info.value.category == ErrorCategory.EXTERNAL_DEPENDENCY
