# SPDX-License-Identifier: AGPL-3.0-only
"""Shared trace-artifact contract models."""

from __future__ import annotations

from typing import Any

from pydantic import BaseModel, ConfigDict, Field

from scriptscore.contracts.common import LlmTokenUsage, TimingInfo


class TraceProviderInfo(BaseModel):
    """Provider info attached to a trace artifact."""

    model_config = ConfigDict(extra="forbid")

    capability: str
    provider: str


class TracePromptInfo(BaseModel):
    """Prompt metadata attached to a trace artifact."""

    model_config = ConfigDict(extra="forbid")

    id: str
    variables: dict[str, str] = Field(default_factory=dict)
    rendered: str | None = None


class TraceRequestInfo(BaseModel):
    """Request metadata attached to a trace artifact."""

    model_config = ConfigDict(extra="forbid")

    options: dict[str, Any] = Field(default_factory=dict)
    input_artifacts: list[str] = Field(default_factory=list)


class TraceResponseInfo(BaseModel):
    """Response metadata attached to a trace artifact."""

    model_config = ConfigDict(extra="forbid")

    raw: str | None = None
    parsed: dict[str, Any] | None = None
    usage: LlmTokenUsage | None = None


class TraceArtifactRecord(BaseModel):
    """Durable per-step trace artifact."""

    model_config = ConfigDict(extra="forbid")

    schema_version: str = "cli.trace.v1"
    command: str
    operation_id: str
    request_id: str | None = None
    step: str
    scope: dict[str, Any] | None = None
    provider: TraceProviderInfo
    prompt: TracePromptInfo | None = None
    request: TraceRequestInfo | None = None
    response: TraceResponseInfo | None = None
    timing: TimingInfo
