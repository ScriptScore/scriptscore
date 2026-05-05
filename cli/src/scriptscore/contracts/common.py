# SPDX-License-Identifier: AGPL-3.0-only
"""Shared contract models used across commands and transports."""

from __future__ import annotations

from typing import Any, Literal
from urllib.parse import urlparse

from pydantic import BaseModel, ConfigDict, Field, model_validator

AlignmentEngineName = Literal["core_template_match"]
LlmProviderName = Literal["ollama_native", "ollama_cloud"]
ConfidenceBucket = Literal["high", "medium", "low"]


class ProviderSelections(BaseModel):
    """Explicit capability-scoped provider selections."""

    model_config = ConfigDict(extra="forbid")

    alignment_engine: AlignmentEngineName | None = None
    llm_provider: LlmProviderName | None = None


class LlmConfig(BaseModel):
    """Shared typed LLM provider config."""

    model_config = ConfigDict(extra="forbid")

    base_url: str | None = None
    model: str = Field(min_length=1)
    api_key: str | None = None
    timeout_seconds: int | None = Field(default=None, gt=0)
    keep_alive: str | None = None
    options: dict[str, Any] = Field(default_factory=dict)

    @model_validator(mode="after")
    def validate_fields(self) -> LlmConfig:
        if self.base_url is not None:
            parsed = urlparse(self.base_url)
            if parsed.scheme not in {"http", "https"} or not parsed.netloc:
                raise ValueError("llm_config.base_url must be an absolute http(s) URL.")
        if self.api_key is not None and not self.api_key.strip():
            raise ValueError("llm_config.api_key must be non-empty when provided.")
        if self.keep_alive is not None and not self.keep_alive.strip():
            raise ValueError("llm_config.keep_alive must be non-empty when provided.")
        return self


class LlmDiscoveryConfig(BaseModel):
    """Shared typed config for provider-backed model discovery."""

    model_config = ConfigDict(extra="forbid")

    base_url: str | None = None
    api_key: str | None = None
    timeout_seconds: int | None = Field(default=None, gt=0)

    @model_validator(mode="after")
    def validate_fields(self) -> LlmDiscoveryConfig:
        if self.base_url is not None:
            parsed = urlparse(self.base_url)
            if parsed.scheme not in {"http", "https"} or not parsed.netloc:
                raise ValueError("llm_discovery_config.base_url must be an absolute http(s) URL.")
        if self.api_key is not None and not self.api_key.strip():
            raise ValueError("llm_discovery_config.api_key must be non-empty when provided.")
        return self


def validate_llm_provider_selection(
    *, providers: ProviderSelections, llm_config: LlmConfig
) -> None:
    """Validate shared built-in LLM provider selection and config rules."""

    provider_name = providers.llm_provider
    if provider_name is None:
        raise ValueError("providers.llm_provider is required.")
    if provider_name == "ollama_cloud" and llm_config.api_key is None:
        raise ValueError(
            "llm_config.api_key is required when providers.llm_provider is 'ollama_cloud'."
        )
    if provider_name == "ollama_native" and llm_config.api_key is not None:
        raise ValueError(
            "llm_config.api_key is only allowed when providers.llm_provider is 'ollama_cloud'."
        )


def validate_llm_discovery_provider_selection(
    *, providers: ProviderSelections, llm_discovery_config: LlmDiscoveryConfig
) -> None:
    """Validate shared built-in LLM provider selection and discovery config rules."""

    provider_name = providers.llm_provider
    if provider_name is None:
        raise ValueError("providers.llm_provider is required.")
    if provider_name == "ollama_cloud" and llm_discovery_config.api_key is None:
        raise ValueError(
            "llm_discovery_config.api_key is required when providers.llm_provider is 'ollama_cloud'."
        )
    if provider_name == "ollama_native" and llm_discovery_config.api_key is not None:
        raise ValueError(
            "llm_discovery_config.api_key is only allowed when providers.llm_provider is 'ollama_cloud'."
        )


def validate_alignment_provider_selection(*, providers: ProviderSelections) -> None:
    """Validate shared built-in alignment provider selection rules."""

    if providers.alignment_engine is None:
        raise ValueError("providers.alignment_engine is required.")


class WarningObject(BaseModel):
    """Structured warning object used in success envelopes."""

    model_config = ConfigDict(extra="forbid")

    code: str
    message: str
    scope: dict[str, Any] | None = None


class ArtifactReference(BaseModel):
    """Shared artifact reference shape."""

    model_config = ConfigDict(extra="forbid")

    kind: str
    role: str
    label: str
    path: str
    format: str
    schema_version: str | None = None
    entity_scope: dict[str, Any] | None = None
    fingerprint: str | None = None


class TimingInfo(BaseModel):
    """Shared timing metadata."""

    model_config = ConfigDict(extra="forbid")

    started_at: str
    finished_at: str
    duration_ms: int


class LlmTokenUsage(BaseModel):
    """Token usage for one LLM generation call."""

    model_config = ConfigDict(extra="forbid")

    input_tokens: int = Field(ge=0)
    output_tokens: int = Field(ge=0)
    total_tokens: int = Field(ge=0)
    count_source: Literal["exact", "estimated"]
    estimation_method: str | None = None
    image_input_count: int = Field(default=0, ge=0)


class LlmTokenUsageCall(LlmTokenUsage):
    """Token usage for one traced LLM generation call."""

    provider: str
    model: str
    prompt_id: str | None = None
    step: str | None = None
    scope: dict[str, Any] | None = None


class LlmTokenUsageSummary(BaseModel):
    """Aggregate LLM token usage for a command invocation."""

    model_config = ConfigDict(extra="forbid")

    input_tokens: int = Field(ge=0)
    output_tokens: int = Field(ge=0)
    total_tokens: int = Field(ge=0)
    count_source: Literal["exact", "estimated", "mixed"]
    calls: list[LlmTokenUsageCall] = Field(default_factory=list)


class ProgressSummary(BaseModel):
    """Optional normalized progress payload."""

    model_config = ConfigDict(extra="forbid")

    completed: int
    total: int
    percent: int
