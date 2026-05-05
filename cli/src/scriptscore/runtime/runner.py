# SPDX-License-Identifier: AGPL-3.0-only
"""Command runner shared by direct CLI and sidecar transport."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Any
from uuid import uuid4

from pydantic import ValidationError

from scriptscore.artifacts.manifest import write_output_metadata
from scriptscore.contracts import (
    CommandErrorBody,
    CommandErrorEnvelope,
    CommandSuccessEnvelope,
    CommandUsage,
    ErrorCategory,
    ScriptscoreError,
    TimingInfo,
    ValidationFailedError,
    exit_code_for_category,
    validation_issues_from_exception,
)
from scriptscore.providers import ProviderRegistry
from scriptscore.runtime.context import CancellationToken, CommandContext
from scriptscore.runtime.registry import CommandRegistry


@dataclass(frozen=True)
class RunResult:
    """Final command result for direct CLI or sidecar mapping."""

    exit_code: int
    envelope: CommandSuccessEnvelope | CommandErrorEnvelope


def new_operation_id() -> str:
    """Generate an opaque operation id."""

    return f"op_{uuid4().hex[:12]}"


def _timing_info(started: datetime, finished: datetime) -> TimingInfo:
    return TimingInfo(
        started_at=started.isoformat().replace("+00:00", "Z"),
        finished_at=finished.isoformat().replace("+00:00", "Z"),
        duration_ms=int((finished - started).total_seconds() * 1000),
    )


def make_error_envelope(
    *,
    command: str,
    operation_id: str,
    request_id: str | None,
    error: ScriptscoreError,
    started: datetime,
    finished: datetime,
) -> CommandErrorEnvelope:
    """Build a terminal error envelope from a typed exception."""

    return CommandErrorEnvelope(
        command=command,
        operation_id=operation_id,
        request_id=request_id,
        error=CommandErrorBody(
            code=error.code,
            message=error.message,
            category=error.category,
            retryable=error.retryable,
            details=error.details,
            write_state=error.write_state,
        ),
        timing=_timing_info(started, finished),
    )


class CommandRunner:
    """Shared command execution entrypoint."""

    def __init__(self, *, registry: CommandRegistry, provider_registry: ProviderRegistry) -> None:
        self.registry = registry
        self.provider_registry = provider_registry

    def run(
        self,
        command_name: str,
        request_payload: Any,
        *,
        request_id: str | None = None,
        event_sink: Any | None = None,
        cancellation_token: CancellationToken | None = None,
    ) -> RunResult:
        """Validate and execute a command request."""

        started = datetime.now(UTC)
        operation_id = new_operation_id()
        token = cancellation_token or CancellationToken()
        try:
            spec = self.registry.get(command_name)
        except KeyError:
            finished = datetime.now(UTC)
            error = ScriptscoreError(
                code="unknown_method",
                message=f"Unknown command '{command_name}'.",
                category=ErrorCategory.VALIDATION,
                retryable=False,
            )
            return RunResult(
                exit_code=exit_code_for_category(error.category),
                envelope=make_error_envelope(
                    command=command_name,
                    operation_id=operation_id,
                    request_id=request_id,
                    error=error,
                    started=started,
                    finished=finished,
                ),
            )

        try:
            request_model = spec.request_model.model_validate(request_payload)
            ctx = CommandContext(
                command=spec.name,
                operation_id=operation_id,
                request_id=request_id,
                provider_registry=self.provider_registry,
                event_sink=event_sink,
                cancellation_token=token,
            )
            outcome = spec.handler(ctx, request_model)
            if outcome.output_artifacts_dir is not None:
                write_output_metadata(
                    command=spec.name,
                    operation_id=operation_id,
                    request_id=request_id,
                    output_artifacts_dir=outcome.output_artifacts_dir,
                    artifacts=outcome.artifacts,
                    data=outcome.manifest_data,
                )
            finished = datetime.now(UTC)
            llm_usage = ctx.llm_usage_summary()
            envelope = CommandSuccessEnvelope(
                command=spec.name,
                operation_id=operation_id,
                request_id=request_id,
                degraded=outcome.degraded,
                warnings=outcome.warnings,
                providers=outcome.providers,
                artifacts=outcome.artifacts,
                usage=None if llm_usage is None else CommandUsage(llm=llm_usage),
                data=outcome.data,
                timing=_timing_info(started, finished),
            )
            return RunResult(exit_code=0, envelope=envelope)
        except ValidationError as exc:
            finished = datetime.now(UTC)
            error = ValidationFailedError(validation_issues_from_exception(exc))
        except ScriptscoreError as exc:
            finished = datetime.now(UTC)
            error = exc
        except Exception as exc:  # pragma: no cover - defensive fallback
            finished = datetime.now(UTC)
            error = ScriptscoreError(
                code="execution_failed",
                message=str(exc) or "Command execution failed.",
                category=ErrorCategory.EXECUTION,
                retryable=False,
            )

        return RunResult(
            exit_code=exit_code_for_category(error.category),
            envelope=make_error_envelope(
                command=command_name,
                operation_id=operation_id,
                request_id=request_id,
                error=error,
                started=started,
                finished=finished,
            ),
        )
