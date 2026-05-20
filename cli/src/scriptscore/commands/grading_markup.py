# SPDX-License-Identifier: AGPL-3.0-only
"""Implementation of `grading markup`."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from scriptscore.commands.common import batch_outcome, progress, warning
from scriptscore.commands.grading_shared import (
    answer_is_effectively_blank,
    assessment_xml,
    parse_tagged_highlights,
    question_context_xml,
    synthetic_llm_trace_artifact,
)
from scriptscore.commands.llm import (
    PromptStepAttempt,
    execute_prompt_step,
    prompt_error_trace_artifact,
    prompt_trace_artifact,
)
from scriptscore.contracts import (
    ArtifactReference,
    GradingMarkupRequest,
    HighlightResult,
    HighlightSpan,
    LlmConfig,
    MarkupRequest,
    ScriptscoreError,
)
from scriptscore.prompts import PromptResponseError
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec


def _row_count_label(count: int) -> str:
    return f"{count} row{'s' if count != 1 else ''}"


@dataclass(frozen=True)
class MarkupPromptRun:
    """One non-retrying markup prompt execution."""

    highlights: list[HighlightSpan] | None
    raw_response: str | None
    attempt: PromptStepAttempt | None
    fallback_message: str
    artifacts: list[ArtifactReference]


@dataclass(frozen=True)
class MarkupRowRun:
    """One completed markup row with fallback accounting."""

    row: HighlightResult
    artifacts: list[ArtifactReference]
    fallback_count: int


def _prompt_variables(request_row: MarkupRequest) -> dict[str, str]:
    return {
        "subject": request_row.subject,
        "question_text_clean": request_row.question_text_clean,
        "question_context_xml": question_context_xml(request_row.question_context),
        "student_answer": request_row.student_answer,
        "assessment_xml": assessment_xml(
            rubric_criteria=request_row.rubric_criteria,
            criterion_results=request_row.criterion_results,
            total_points_awarded=request_row.total_points_awarded,
            question_max_points=request_row.question_max_points,
        ),
    }


def _run_markup_prompt(
    *,
    output_artifacts_dir: Path,
    ctx: CommandContext,
    provider_name: str,
    llm_config: LlmConfig,
    scope: dict[str, object],
    prompt_variables: dict[str, str],
    student_answer: str,
) -> MarkupPromptRun:
    try:
        attempt = execute_prompt_step(
            ctx,
            provider_name=provider_name,
            prompt_id="markup",
            command_inputs=prompt_variables,
            llm_config=llm_config,
        )
    except ScriptscoreError as exc:
        return MarkupPromptRun(
            highlights=None,
            raw_response=None,
            attempt=None,
            fallback_message=exc.message,
            artifacts=[
                prompt_error_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    step="markup",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="markup",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                    error=exc,
                )
            ],
        )

    if attempt.execution is None:
        fallback_message = (
            "Markup generation failed."
            if attempt.error is None
            else f"Markup generation failed: {attempt.error}"
        )
        return MarkupPromptRun(
            highlights=None,
            raw_response=None,
            attempt=attempt,
            fallback_message=fallback_message,
            artifacts=[
                prompt_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    attempt=attempt,
                    step="markup",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="markup",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                    response_parsed=None,
                    filename_suffix="attempt-1",
                )
            ],
        )

    raw_response = attempt.execution.provider_response.raw_text
    try:
        highlights = parse_tagged_highlights(raw_response, student_answer=student_answer)
    except PromptResponseError as exc:
        return MarkupPromptRun(
            highlights=None,
            raw_response=raw_response,
            attempt=attempt,
            fallback_message=exc.message,
            artifacts=[
                prompt_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    attempt=attempt,
                    step="markup",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="markup",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                    response_parsed=None,
                    response_raw=raw_response,
                    filename_suffix="attempt-1",
                )
            ],
        )
    return MarkupPromptRun(
        highlights=highlights,
        raw_response=raw_response,
        attempt=attempt,
        fallback_message="",
        artifacts=[],
    )


def _blank_local_markup_row(
    *,
    output_artifacts_dir: Path,
    ctx: CommandContext,
    request_row: MarkupRequest,
    scope: dict[str, object],
    prompt_variables: dict[str, str],
) -> MarkupRowRun:
    row = HighlightResult(
        student_ref=request_row.student_ref,
        question_id=request_row.question_id,
        status="ok",
        highlights=[],
        warnings=[],
    )
    return MarkupRowRun(
        row=row,
        artifacts=[
            synthetic_llm_trace_artifact(
                output_artifacts_dir=output_artifacts_dir,
                ctx=ctx,
                step="markup",
                scope=scope,
                prompt_id="markup",
                prompt_variables=prompt_variables,
                response_raw=None,
                response_parsed=row.model_dump(mode="json"),
            )
        ],
        fallback_count=0,
    )


def _execute_markup_row(
    *,
    output_artifacts_dir: Path,
    ctx: CommandContext,
    request_row: MarkupRequest,
    llm_config: LlmConfig,
    provider_name: str,
    scope: dict[str, object],
) -> MarkupRowRun:
    prompt_variables = _prompt_variables(request_row)
    student_answer = request_row.student_answer

    if answer_is_effectively_blank(student_answer):
        return _blank_local_markup_row(
            output_artifacts_dir=output_artifacts_dir,
            ctx=ctx,
            request_row=request_row,
            scope=scope,
            prompt_variables=prompt_variables,
        )

    prompt_run = _run_markup_prompt(
        output_artifacts_dir=output_artifacts_dir,
        ctx=ctx,
        provider_name=provider_name,
        llm_config=llm_config,
        scope=scope,
        prompt_variables=prompt_variables,
        student_answer=student_answer,
    )

    if (
        prompt_run.highlights is not None
        and prompt_run.attempt is not None
        and prompt_run.raw_response is not None
    ):
        row = HighlightResult(
            student_ref=request_row.student_ref,
            question_id=request_row.question_id,
            status="ok",
            highlights=prompt_run.highlights,
            warnings=[],
        )
        return MarkupRowRun(
            row=row,
            artifacts=[
                *prompt_run.artifacts,
                prompt_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    attempt=prompt_run.attempt,
                    step="markup",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="markup",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                    response_parsed={
                        "status": "ok",
                        "highlights": [
                            highlight.model_dump(mode="json") for highlight in prompt_run.highlights
                        ],
                    },
                    response_raw=prompt_run.raw_response,
                    filename_suffix="attempt-1",
                ),
            ],
            fallback_count=0,
        )

    row = HighlightResult(
        student_ref=request_row.student_ref,
        question_id=request_row.question_id,
        status="fallback",
        highlights=[],
        warnings=[
            warning(
                code="markup_fallback",
                message=prompt_run.fallback_message,
                scope=scope,
            )
        ],
    )
    return MarkupRowRun(
        row=row,
        artifacts=prompt_run.artifacts,
        fallback_count=1,
    )


def handle_grading_markup(ctx: CommandContext, request: GradingMarkupRequest) -> CommandOutcome:
    """Compute moderation highlight spans for explicit grading rows."""

    provider_name = request.providers.llm_provider
    assert provider_name is not None
    ctx.provider_registry.resolve_llm(provider_name)

    total = len(request.markup_requests)
    ctx.emit(
        event="started",
        progress=progress(completed=0, total=total),
        data={"target_count": total, "total_stages": 1},
    )

    results: list[HighlightResult] = []
    artifacts: list[ArtifactReference] = []
    fallback_count = 0
    for index, request_row in enumerate(request.markup_requests, start=1):
        ctx.check_cancelled()
        scope: dict[str, object] = {
            "student_ref": request_row.student_ref,
            "question_id": request_row.question_id,
        }
        ctx.emit(
            event="item_started",
            progress=progress(completed=index - 1, total=total),
            scope=scope,
        )
        row_run = _execute_markup_row(
            output_artifacts_dir=request.output_artifacts_dir,
            ctx=ctx,
            request_row=request_row,
            llm_config=request.llm_config,
            provider_name=provider_name,
            scope=scope,
        )
        results.append(row_run.row)
        artifacts.extend(row_run.artifacts)
        fallback_count += row_run.fallback_count
        ctx.emit(
            event="item_completed",
            progress=progress(completed=index, total=total),
            scope=scope,
            data={"status": row_run.row.status},
        )

    ctx.emit(
        event="completed",
        progress=progress(completed=total, total=total),
        data={"target_count": total, "fallback_count": fallback_count},
    )
    return batch_outcome(
        data={
            "highlight_results": [
                result.model_dump(mode="json", exclude_none=True) for result in results
            ],
            "output_metadata_path": str(
                (request.output_artifacts_dir / "output_metadata.json").resolve()
            ),
        },
        output_artifacts_dir=request.output_artifacts_dir,
        artifacts=artifacts,
        result_row_count=len(results),
        failed_count=0,
        command_label="Markup",
        providers={"llm_provider": provider_name},
        extra_warnings=(
            [
                warning(
                    code="markup_fallback",
                    message=(
                        "Markup returned no instructor highlights for "
                        f"{_row_count_label(fallback_count)}."
                    ),
                    scope={"row_count": fallback_count},
                )
            ]
            if fallback_count
            else None
        ),
    )


def grading_markup_spec() -> CommandSpec:
    return CommandSpec(
        name="grading.markup", request_model=GradingMarkupRequest, handler=handle_grading_markup
    )
