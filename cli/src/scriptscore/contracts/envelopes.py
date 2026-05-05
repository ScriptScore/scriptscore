# SPDX-License-Identifier: AGPL-3.0-only
"""Shared success, error, progress, and manifest envelopes."""

from __future__ import annotations

from typing import Any

from pydantic import BaseModel, ConfigDict, Field

from scriptscore.contracts.common import (
    ArtifactReference,
    LlmTokenUsageSummary,
    ProgressSummary,
    TimingInfo,
    WarningObject,
)
from scriptscore.contracts.errors import ErrorCategory, ValidationIssue, WriteState


class CommandErrorBody(BaseModel):
    """Structured error body shared by direct CLI and sidecar transport."""

    model_config = ConfigDict(extra="forbid")

    code: str
    message: str
    category: ErrorCategory
    retryable: bool
    details: dict[str, Any] = Field(default_factory=dict)
    write_state: WriteState


class CommandUsage(BaseModel):
    """Operational usage metadata attached to success envelopes."""

    model_config = ConfigDict(extra="forbid")

    llm: LlmTokenUsageSummary | None = None


class CommandSuccessEnvelope(BaseModel):
    """Terminal success envelope."""

    model_config = ConfigDict(extra="forbid")

    ok: bool = True
    command: str
    operation_id: str
    request_id: str | None = None
    degraded: bool = False
    warnings: list[WarningObject] = Field(default_factory=list)
    providers: dict[str, str] | None = None
    artifacts: list[ArtifactReference] = Field(default_factory=list)
    usage: CommandUsage | None = None
    data: dict[str, Any] = Field(default_factory=dict)
    timing: TimingInfo


class CommandErrorEnvelope(BaseModel):
    """Terminal error envelope."""

    model_config = ConfigDict(extra="forbid")

    ok: bool = False
    command: str
    operation_id: str
    request_id: str | None = None
    error: CommandErrorBody
    timing: TimingInfo


class ProgressEvent(BaseModel):
    """NDJSON progress event envelope."""

    model_config = ConfigDict(extra="forbid")

    type: str = "event"
    command: str
    operation_id: str
    request_id: str | None = None
    sequence: int
    event: str
    level: str
    timestamp: str
    progress: ProgressSummary | None = None
    scope: dict[str, Any] | None = None
    data: dict[str, Any] = Field(default_factory=dict)


class OutputMetadataManifest(BaseModel):
    """Durable inventory manifest written under output_artifacts_dir."""

    model_config = ConfigDict(extra="forbid")

    schema_version: str = "cli.output_metadata.v1"
    command: str
    operation_id: str
    request_id: str | None = None
    output_artifacts_dir: str
    artifacts: list[ArtifactReference] = Field(default_factory=list)
    data: dict[str, Any] = Field(default_factory=dict)


class ValidationFailureDetails(BaseModel):
    """Convenience typed model for validation failure details."""

    model_config = ConfigDict(extra="forbid")

    issues: list[ValidationIssue]
