# SPDX-License-Identifier: AGPL-3.0-only
"""Runtime command for validating one candidate LLM model."""

from __future__ import annotations

from pydantic import BaseModel, ConfigDict, Field, model_validator

import scriptscore.commands.runtime_list_llm_models as runtime_models
from scriptscore.commands.common import progress
from scriptscore.contracts import (
    LlmDiscoveryConfig,
    ProviderSelections,
    ScriptscoreError,
    validate_llm_discovery_provider_selection,
)
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec


class RuntimeValidateLlmModelRequest(BaseModel):
    """Request schema for runtime validation of one LLM model."""

    model_config = ConfigDict(extra="forbid")

    providers: ProviderSelections
    llm_discovery_config: LlmDiscoveryConfig = Field(default_factory=LlmDiscoveryConfig)
    model: str = Field(min_length=1)
    required_capabilities: list[str] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_fields(self) -> RuntimeValidateLlmModelRequest:
        validate_llm_discovery_provider_selection(
            providers=self.providers,
            llm_discovery_config=self.llm_discovery_config,
        )
        self.model = self.model.strip()
        if not self.model:
            raise ValueError("model must be a non-empty string.")
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


def _result_data(
    *,
    model: str,
    capabilities: list[str],
    valid: bool,
    reason: str | None = None,
    missing_capabilities: list[str] | None = None,
) -> dict[str, object]:
    return {
        "model": model,
        "display_name": model,
        "capabilities": capabilities,
        "valid": valid,
        "reason": reason,
        "missing_capabilities": missing_capabilities or [],
    }


def handle_runtime_validate_llm_model(
    ctx: CommandContext, request: RuntimeValidateLlmModelRequest
) -> CommandOutcome:
    """Validate whether one candidate model exists and satisfies required capabilities."""

    provider_name = request.providers.llm_provider
    assert provider_name is not None
    ctx.provider_registry.resolve_llm(provider_name)
    client, host = runtime_models._build_discovery_client(
        provider_name=provider_name,
        discovery_config=request.llm_discovery_config,
    )

    ctx.emit(
        event="started",
        progress=progress(completed=0, total=1),
        data={"provider": provider_name, "host": host, "model": request.model},
    )

    try:
        raw_show = runtime_models._client_show(client, model=request.model)  # type: ignore[attr-defined]
        show_response = runtime_models._response_dict(  # type: ignore[attr-defined]
            response=raw_show,
            provider_name=provider_name,
            base_url=host,
            model=request.model,
            operation="show",
        )
    except Exception as exc:  # pragma: no cover - concrete library exception types vary.
        try:
            runtime_models._raise_ollama_error(  # type: ignore[attr-defined]
                exc=exc,
                provider_name=provider_name,
                base_url=host,
                model=request.model,
                operation="show",
            )
        except ScriptscoreError as error:
            if error.code != "llm_model_unavailable":
                raise
            data = _result_data(
                model=request.model,
                capabilities=[],
                valid=False,
                reason="model_unavailable",
            )
            ctx.emit(
                event="completed",
                progress=progress(completed=1, total=1),
                data={**data, "provider": provider_name, "host": host},
            )
            return CommandOutcome(data=data, providers={"llm_provider": provider_name})
        raise AssertionError("unreachable") from exc

    capabilities = runtime_models._normalize_capabilities(show_response.get("capabilities"))
    missing_capabilities = [
        capability for capability in request.required_capabilities if capability not in capabilities
    ]
    data = _result_data(
        model=request.model,
        capabilities=capabilities,
        valid=not missing_capabilities,
        reason=None if not missing_capabilities else "missing_capabilities",
        missing_capabilities=missing_capabilities,
    )
    ctx.emit(
        event="completed",
        progress=progress(completed=1, total=1),
        data={**data, "provider": provider_name, "host": host},
    )
    return CommandOutcome(data=data, providers={"llm_provider": provider_name})


def runtime_validate_llm_model_spec() -> CommandSpec:
    """Return the registered runtime validate-llm-model command spec."""

    return CommandSpec(
        name="runtime.validate-llm-model",
        request_model=RuntimeValidateLlmModelRequest,
        handler=handle_runtime_validate_llm_model,
    )
