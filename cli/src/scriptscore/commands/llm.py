# SPDX-License-Identifier: AGPL-3.0-only
"""Shared helpers for prompt-backed command steps."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from scriptscore.artifacts import write_trace_artifact
from scriptscore.commands.common import timing_info
from scriptscore.contracts import ArtifactReference, LlmConfig, ScriptscoreError
from scriptscore.prompts import PromptExecution, execute_builtin_prompt, get_builtin_prompt
from scriptscore.runtime import CommandContext


@dataclass(frozen=True)
class PromptStepAttempt:
    """One prompt execution attempt with timing and optional error."""

    execution: PromptExecution | None
    error: Exception | None
    started: datetime
    finished: datetime


def execute_prompt_step(
    ctx: CommandContext,
    *,
    provider_name: str,
    prompt_id: str,
    command_inputs: dict[str, str],
    llm_config: LlmConfig,
) -> PromptStepAttempt:
    """Execute one prompt step while preserving timing on both success and failure."""

    started = datetime.now(UTC)
    try:
        execution = execute_builtin_prompt(
            ctx,
            provider_name=provider_name,
            prompt_id=prompt_id,
            command_inputs=command_inputs,
            llm_config=llm_config,
        )
        finished = datetime.now(UTC)
        return PromptStepAttempt(
            execution=execution,
            error=None,
            started=started,
            finished=finished,
        )
    except ScriptscoreError:
        raise
    except Exception as exc:
        finished = datetime.now(UTC)
        return PromptStepAttempt(
            execution=None,
            error=exc,
            started=started,
            finished=finished,
        )


def prompt_trace_artifact(
    *,
    output_artifacts_dir: Path,
    ctx: CommandContext,
    attempt: PromptStepAttempt,
    step: str,
    scope: dict[str, object] | None,
    provider_name: str,
    prompt_id: str,
    prompt_variables: dict[str, str],
    input_artifacts: list[str] | None = None,
    response_parsed: dict[str, Any] | None = None,
    request_options: dict[str, object] | None = None,
    response_raw: str | None = None,
    filename_suffix: str | None = None,
) -> ArtifactReference:
    """Write a standard trace artifact for one prompt attempt."""

    execution = attempt.execution
    raw_text = (
        response_raw
        if response_raw is not None
        else (None if execution is None else execution.provider_response.raw_text)
    )
    resolved_request_options = request_options
    if resolved_request_options is None:
        resolved_request_options = (
            dict(get_builtin_prompt(prompt_id).execution_options)
            if execution is None
            else dict(execution.request_options)
        )
    response_usage = None if execution is None else execution.provider_response.usage
    if execution is not None and response_usage is not None:
        model = execution.provider_response.provider_response.get("model")
        ctx.record_llm_usage(
            provider=provider_name,
            model=str(model) if model else "unknown",
            prompt_id=prompt_id,
            step=step,
            scope=scope,
            usage=response_usage,
        )
    return write_trace_artifact(
        output_artifacts_dir=output_artifacts_dir,
        command=ctx.command,
        operation_id=ctx.operation_id,
        request_id=ctx.request_id,
        step=step,
        scope=scope,
        filename_suffix=filename_suffix,
        provider_capability="llm_provider",
        provider_name=provider_name,
        prompt_id=prompt_id,
        prompt_variables=prompt_variables,
        prompt_rendered=None if execution is None else execution.rendered_prompt.rendered_text,
        request_options=resolved_request_options,
        input_artifacts=input_artifacts or [],
        response_raw=raw_text,
        response_parsed=response_parsed,
        response_usage=response_usage,
        timing=timing_info(started=attempt.started, finished=attempt.finished),
    )


def prompt_error_trace_artifact(
    *,
    output_artifacts_dir: Path,
    ctx: CommandContext,
    step: str,
    scope: dict[str, object] | None,
    provider_name: str,
    prompt_id: str,
    prompt_variables: dict[str, str],
    input_artifacts: list[str] | None,
    error: ScriptscoreError,
    filename_suffix: str | None = None,
) -> ArtifactReference:
    """Write a trace artifact for a typed prompt-step failure without a normal execution payload."""

    now = datetime.now(UTC)
    return write_trace_artifact(
        output_artifacts_dir=output_artifacts_dir,
        command=ctx.command,
        operation_id=ctx.operation_id,
        request_id=ctx.request_id,
        step=step,
        scope=scope,
        filename_suffix=filename_suffix,
        provider_capability="llm_provider",
        provider_name=provider_name,
        prompt_id=prompt_id,
        prompt_variables=prompt_variables,
        prompt_rendered=None,
        request_options=dict(get_builtin_prompt(prompt_id).execution_options),
        input_artifacts=input_artifacts or [],
        response_raw=None,
        response_parsed={
            "error": {
                "code": error.code,
                "message": error.message,
                "category": error.category,
            }
        },
        timing=timing_info(started=now, finished=now),
    )
