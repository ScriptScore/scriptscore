# SPDX-License-Identifier: AGPL-3.0-only
"""Provider protocol definitions."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Literal, Protocol, runtime_checkable

from scriptscore.contracts.common import LlmTokenUsage, WarningObject

ProviderCapability = Literal["alignment_engine", "llm_provider"]


@runtime_checkable
class Provider(Protocol):
    """Minimal provider protocol frozen for the Phase 1 scaffold."""

    @property
    def capability(self) -> ProviderCapability: ...

    @property
    def provider_name(self) -> str: ...

    @property
    def interface_version(self) -> str: ...


@dataclass(frozen=True)
class ProviderDescriptor:
    """Resolved provider description used in tests and diagnostics."""

    capability: ProviderCapability
    provider_name: str
    interface_version: str


@dataclass(frozen=True)
class LlmProviderConfig:
    """Typed provider-local config supplied by the CLI."""

    model: str
    base_url: str | None = None
    api_key: str | None = None
    timeout_seconds: int | None = None
    keep_alive: str | None = None
    options: dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class LlmRequest:
    """Normalized LLM provider request owned by the CLI."""

    prompt_id: str
    response_mode: str
    rendered_text: str
    provider_config: LlmProviderConfig
    file_inputs: dict[str, str] = field(default_factory=dict)
    execution_options: dict[str, Any] = field(default_factory=dict)
    response_contract: dict[str, Any] | None = None


@dataclass(frozen=True)
class LlmResponse:
    """Normalized LLM provider response returned to the CLI."""

    raw_text: str
    provider_response: dict[str, Any] = field(default_factory=dict)
    usage: LlmTokenUsage | None = None


@dataclass(frozen=True)
class AlignmentRequest:
    """Normalized alignment provider request owned by the CLI."""

    template_page_path: str
    student_page_path: str
    mode: Literal["fast", "precise"]
    marker_mode: Literal["ignore", "prefer_aruco"]


@dataclass(frozen=True)
class AlignmentResponse:
    """Normalized alignment provider response returned to the CLI."""

    status: Literal["ok", "low_confidence", "failed"]
    confidence: float | None = None
    rotation: float | None = None
    scale: float | None = None
    translate_x: float | None = None
    translate_y: float | None = None
    warnings: list[WarningObject] = field(default_factory=list)


@runtime_checkable
class LlmProvider(Provider, Protocol):
    """LLM provider protocol for CLI-owned prompt execution."""

    @property
    def capability(self) -> Literal["llm_provider"]: ...

    def generate(self, request: LlmRequest) -> LlmResponse: ...


@runtime_checkable
class AlignmentProvider(Provider, Protocol):
    """Alignment provider protocol for auto-alignment proposal execution."""

    @property
    def capability(self) -> Literal["alignment_engine"]: ...

    def align(self, request: AlignmentRequest) -> AlignmentResponse: ...
