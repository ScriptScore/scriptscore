# SPDX-License-Identifier: AGPL-3.0-only
"""Runtime command for discovering available LLM models."""

from __future__ import annotations

from typing import Any

from pydantic import BaseModel, ConfigDict, Field, model_validator

from scriptscore.commands.common import progress, warning
from scriptscore.contracts import (
    LlmDiscoveryConfig,
    ProviderSelections,
    validate_llm_discovery_provider_selection,
)
from scriptscore.providers.ollama import (
    _client_factory,
    _client_show,
    _external_error,
    _raise_ollama_error,
    _resolve_host,
    _response_dict,
)
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec


class RuntimeListLlmModelsRequest(BaseModel):
    """Request schema for runtime LLM model discovery."""

    model_config = ConfigDict(extra="forbid")

    providers: ProviderSelections
    llm_discovery_config: LlmDiscoveryConfig = Field(default_factory=LlmDiscoveryConfig)
    required_capabilities: list[str] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_fields(self) -> RuntimeListLlmModelsRequest:
        validate_llm_discovery_provider_selection(
            providers=self.providers,
            llm_discovery_config=self.llm_discovery_config,
        )
        normalized: list[str] = []
        seen: set[str] = set()
        for capability in self.required_capabilities:
            cleaned = capability.strip().lower()
            if not cleaned:
                raise ValueError("required_capabilities entries must be non-empty strings.")
            if cleaned not in seen:
                seen.add(cleaned)
                normalized.append(cleaned)
        self.required_capabilities = normalized
        return self


def _client_list(client: Any) -> Any:
    return client.list()


def _build_discovery_client(
    *, provider_name: str, discovery_config: LlmDiscoveryConfig
) -> tuple[Any, str]:
    host = _resolve_host(provider_name=provider_name, base_url=discovery_config.base_url)
    headers: dict[str, str] | None = None
    if provider_name == "ollama_cloud":
        assert discovery_config.api_key is not None
        headers = {"Authorization": f"Bearer {discovery_config.api_key}"}
    client = _client_factory(
        host=host,
        headers=headers,
        timeout_seconds=discovery_config.timeout_seconds,
    )
    return client, host


def _extract_model_name(model_payload: dict[str, Any]) -> str | None:
    for key in ("name", "model"):
        raw_value = model_payload.get(key)
        if isinstance(raw_value, str) and raw_value.strip():
            return raw_value.strip()
    return None


def _normalize_capabilities(raw_capabilities: Any) -> list[str]:
    if not isinstance(raw_capabilities, list):
        return []
    seen: set[str] = set()
    normalized: list[str] = []
    for capability in raw_capabilities:
        cleaned = str(capability).strip().lower()
        if cleaned and cleaned not in seen:
            seen.add(cleaned)
            normalized.append(cleaned)
    return normalized


def handle_runtime_list_llm_models(
    ctx: CommandContext, request: RuntimeListLlmModelsRequest
) -> CommandOutcome:
    """List available built-in Ollama models and their capabilities."""

    provider_name = request.providers.llm_provider
    assert provider_name is not None
    ctx.provider_registry.resolve_llm(provider_name)
    client, host = _build_discovery_client(
        provider_name=provider_name,
        discovery_config=request.llm_discovery_config,
    )

    ctx.emit(
        event="started",
        progress=progress(completed=0, total=1),
        data={"provider": provider_name, "host": host},
    )

    try:
        raw_list = _client_list(client)
    except Exception as exc:  # pragma: no cover - concrete library exception types vary.
        _raise_ollama_error(
            exc=exc,
            provider_name=provider_name,
            base_url=host,
            model="*",
            operation="list",
        )

    list_response = _response_dict(
        response=raw_list,
        provider_name=provider_name,
        base_url=host,
        model="*",
        operation="list",
    )
    raw_models = list_response.get("models")
    if not isinstance(raw_models, list):
        raise _external_error(
            code="ollama_response_invalid",
            message="Ollama list response did not include a models array.",
            details={
                "provider": provider_name,
                "base_url": host,
                "operation": "list",
                "response": list_response,
            },
            retryable=False,
        )

    discovered: list[dict[str, Any]] = []
    warnings = []
    for raw_model in raw_models:
        if not isinstance(raw_model, dict):
            warnings.append(
                warning(
                    code="llm_model_discovery_skipped",
                    message="Skipped an Ollama model entry with an unsupported payload shape.",
                )
            )
            continue
        model_name = _extract_model_name(raw_model)
        if model_name is None:
            warnings.append(
                warning(
                    code="llm_model_discovery_skipped",
                    message="Skipped an Ollama model entry that did not expose a usable name.",
                )
            )
            continue
        try:
            raw_show = _client_show(client, model=model_name)
            show_response = _response_dict(
                response=raw_show,
                provider_name=provider_name,
                base_url=host,
                model=model_name,
                operation="show",
            )
        except Exception as exc:  # pragma: no cover - concrete library exception types vary.
            warnings.append(
                warning(
                    code="llm_model_inspection_skipped",
                    message=f"Skipped Ollama model '{model_name}' because its metadata could not be inspected.",
                    scope={"model": model_name, "error": str(exc)},
                )
            )
            continue

        capabilities = _normalize_capabilities(show_response.get("capabilities"))
        if request.required_capabilities and not set(request.required_capabilities).issubset(
            capabilities
        ):
            continue
        discovered.append(
            {
                "model": model_name,
                "display_name": model_name,
                "capabilities": capabilities,
            }
        )

    ctx.emit(
        event="completed",
        progress=progress(completed=1, total=1),
        data={"provider": provider_name, "host": host, "model_count": len(discovered)},
    )

    return CommandOutcome(
        data={"models": discovered},
        degraded=bool(warnings),
        warnings=warnings,
        providers={"llm_provider": provider_name},
    )


def runtime_list_llm_models_spec() -> CommandSpec:
    """Return the registered runtime list-llm-models command spec."""

    return CommandSpec(
        name="runtime.list-llm-models",
        request_model=RuntimeListLlmModelsRequest,
        handler=handle_runtime_list_llm_models,
    )
