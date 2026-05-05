# SPDX-License-Identifier: AGPL-3.0-only
"""Implementation of `grading run-consistency`."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from pydantic import BaseModel, ConfigDict, Field

from scriptscore.commands.common import batch_outcome, progress, warning
from scriptscore.commands.grading_shared import (
    DeidentifiedConsistencyInput,
    consistency_error_warning,
    deidentify_consistency_student_answers,
    instructor_profile_xml,
    question_context_xml,
    rubric_criterion_xml,
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
    ConsistencyAdjustment,
    ConsistencyRequest,
    ConsistencyReview,
    GradingRunConsistencyRequest,
    LlmConfig,
    ScriptscoreError,
    WarningObject,
)
from scriptscore.prompts import PromptResponseError, parse_json_model
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec

_DEFAULT_ADJUSTMENT_REASON = "Adjusted for rubric consistency."


class ConsistencyAdjustmentPayload(BaseModel):
    """Provider-returned sparse adjustment row keyed by student alias."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str
    points_awarded: int = Field(ge=0)
    adjustment_reason: str


class ConsistencyReviewPayload(BaseModel):
    """Provider-returned sparse consistency review payload."""

    model_config = ConfigDict(extra="forbid")

    adjustments: list[ConsistencyAdjustmentPayload]


@dataclass(frozen=True)
class ConsistencyPromptRun:
    """Result of the retrying consistency-review prompt execution."""

    payload: ConsistencyReviewPayload | None
    payload_raw: str | None
    successful_attempt: PromptStepAttempt | None
    successful_filename_suffix: str | None
    failure_message: str | None
    invalid_output_message: str | None
    artifacts: list[ArtifactReference]


@dataclass(frozen=True)
class ConsistencyRowRun:
    """One completed consistency-review row with accounting data."""

    row: ConsistencyReview
    artifacts: list[ArtifactReference]
    failed_count: int


def _prompt_variables(
    request_row: ConsistencyRequest,
    *,
    deidentified: DeidentifiedConsistencyInput,
) -> dict[str, str]:
    return {
        "subject": request_row.subject,
        "question_text_clean": request_row.question_text_clean,
        "question_context_xml": question_context_xml(request_row.question_context),
        "rubric_criterion_xml": rubric_criterion_xml(request_row.rubric_criterion),
        "instructor_profile_xml": instructor_profile_xml(request_row.instructor_profile),
        "consistency_student_answers_xml": deidentified.rendered_xml,
    }


def _blank_local_review(
    *,
    output_artifacts_dir: Path,
    ctx: CommandContext,
    request_row: ConsistencyRequest,
    criterion_index: int,
    scope: dict[str, object],
    prompt_variables: dict[str, str],
) -> ConsistencyRowRun:
    row = ConsistencyReview(
        question_id=request_row.question_id,
        criterion_index=criterion_index,
        status="ok",
        adjustments=[],
        warnings=[],
    )
    return ConsistencyRowRun(
        row=row,
        artifacts=[
            synthetic_llm_trace_artifact(
                output_artifacts_dir=output_artifacts_dir,
                ctx=ctx,
                step="consistency_review",
                scope=scope,
                prompt_id="consistency_review",
                prompt_variables=prompt_variables,
                response_raw=None,
                response_parsed={"adjustments": [], "blank_local": True},
            )
        ],
        failed_count=0,
    )


def _run_consistency_prompt(
    *,
    output_artifacts_dir: Path,
    ctx: CommandContext,
    provider_name: str,
    llm_config: LlmConfig,
    request_row: ConsistencyRequest,
    scope: dict[str, object],
    prompt_variables: dict[str, str],
    deidentified: DeidentifiedConsistencyInput,
) -> ConsistencyPromptRun:
    artifacts: list[ArtifactReference] = []
    invalid_output_message: str | None = None
    for attempt_index in range(1, 3):
        filename_suffix = None if attempt_index == 1 else f"attempt_{attempt_index:02d}"
        try:
            attempt = execute_prompt_step(
                ctx,
                provider_name=provider_name,
                prompt_id="consistency_review",
                command_inputs=prompt_variables,
                llm_config=llm_config,
            )
        except ScriptscoreError as exc:
            artifacts.append(
                prompt_error_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    step="consistency_review",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="consistency_review",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                    error=exc,
                    filename_suffix=filename_suffix,
                )
            )
            return ConsistencyPromptRun(
                payload=None,
                payload_raw=None,
                successful_attempt=None,
                successful_filename_suffix=None,
                failure_message=exc.message,
                invalid_output_message=None,
                artifacts=artifacts,
            )

        if attempt.execution is None:
            artifacts.append(
                prompt_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    attempt=attempt,
                    step="consistency_review",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="consistency_review",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                    response_parsed=None,
                    filename_suffix=filename_suffix,
                )
            )
            return ConsistencyPromptRun(
                payload=None,
                payload_raw=None,
                successful_attempt=None,
                successful_filename_suffix=None,
                failure_message=str(attempt.error) or "Consistency review execution failed.",
                invalid_output_message=None,
                artifacts=artifacts,
            )

        payload_raw = attempt.execution.provider_response.raw_text
        try:
            payload = parse_json_model(payload_raw, ConsistencyReviewPayload)
            for adjustment in payload.adjustments:
                if adjustment.student_ref not in deidentified.alias_to_student_ref:
                    raise PromptResponseError(
                        code="prompt_response_schema_invalid",
                        message="Consistency review returned an unknown student alias.",
                    )
                if adjustment.points_awarded > request_row.rubric_criterion.points:
                    raise PromptResponseError(
                        code="prompt_response_schema_invalid",
                        message="Consistency review awarded points outside the criterion range.",
                    )
            return ConsistencyPromptRun(
                payload=payload,
                payload_raw=payload_raw,
                successful_attempt=attempt,
                successful_filename_suffix=filename_suffix,
                failure_message=None,
                invalid_output_message=None,
                artifacts=artifacts,
            )
        except PromptResponseError as exc:
            invalid_output_message = exc.message
            artifacts.append(
                prompt_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    attempt=attempt,
                    step="consistency_review",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="consistency_review",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                    response_parsed=None,
                    response_raw=payload_raw,
                    filename_suffix=filename_suffix,
                )
            )

    return ConsistencyPromptRun(
        payload=None,
        payload_raw=None,
        successful_attempt=None,
        successful_filename_suffix=None,
        failure_message=None,
        invalid_output_message=invalid_output_message,
        artifacts=artifacts,
    )


def _normalize_adjustments(
    *,
    request_row: ConsistencyRequest,
    criterion_index: int,
    payload: ConsistencyReviewPayload,
    scope: dict[str, object],
    deidentified: DeidentifiedConsistencyInput,
) -> tuple[list[ConsistencyAdjustment], list[WarningObject]]:
    preliminary_by_ref = {score.student_ref: score for score in request_row.student_scores}
    latest_adjustment_by_ref: dict[str, ConsistencyAdjustment] = {}
    last_position_by_ref: dict[str, int] = {}
    row_warnings: list[WarningObject] = []
    for position, adjustment in enumerate(payload.adjustments):
        raw_student_ref = deidentified.alias_to_student_ref[adjustment.student_ref]
        preliminary = preliminary_by_ref[raw_student_ref]
        reason = adjustment.adjustment_reason.strip()
        if adjustment.points_awarded == preliminary.preliminary_points_awarded:
            row_warnings.append(
                warning(
                    code="consistency_adjustment_unchanged",
                    message=f"Dropped unchanged consistency adjustment for {adjustment.student_ref}.",
                    scope={**scope, "student_alias": adjustment.student_ref},
                )
            )
            continue
        if not reason:
            reason = _DEFAULT_ADJUSTMENT_REASON
            row_warnings.append(
                warning(
                    code="consistency_adjustment_reason_repaired",
                    message=f"Repaired missing adjustment_reason for {adjustment.student_ref}.",
                    scope={**scope, "student_alias": adjustment.student_ref},
                )
            )
        if raw_student_ref in latest_adjustment_by_ref:
            row_warnings.append(
                warning(
                    code="consistency_adjustment_duplicate",
                    message=f"Collapsed duplicate consistency adjustment for {adjustment.student_ref} to the last row.",
                    scope={**scope, "student_alias": adjustment.student_ref},
                )
            )
        latest_adjustment_by_ref[raw_student_ref] = ConsistencyAdjustment(
            student_ref=raw_student_ref,
            question_id=request_row.question_id,
            criterion_index=criterion_index,
            points_awarded=adjustment.points_awarded,
            adjustment_reason=reason,
            warnings=[],
        )
        last_position_by_ref[raw_student_ref] = position

    return (
        [
            latest_adjustment_by_ref[student_ref]
            for student_ref in sorted(
                last_position_by_ref, key=lambda ref: last_position_by_ref[ref]
            )
        ],
        row_warnings,
    )


def _execute_consistency_row(
    *,
    output_artifacts_dir: Path,
    ctx: CommandContext,
    request_row: ConsistencyRequest,
    provider_name: str,
    llm_config: LlmConfig,
    criterion_index: int,
    scope: dict[str, object],
) -> ConsistencyRowRun:
    deidentified = deidentify_consistency_student_answers(request_row.student_scores)
    prompt_variables = _prompt_variables(request_row, deidentified=deidentified)
    if not deidentified.alias_to_student_ref:
        return _blank_local_review(
            output_artifacts_dir=output_artifacts_dir,
            ctx=ctx,
            request_row=request_row,
            criterion_index=criterion_index,
            scope=scope,
            prompt_variables=prompt_variables,
        )

    prompt_run = _run_consistency_prompt(
        output_artifacts_dir=output_artifacts_dir,
        ctx=ctx,
        provider_name=provider_name,
        llm_config=llm_config,
        request_row=request_row,
        scope=scope,
        prompt_variables=prompt_variables,
        deidentified=deidentified,
    )

    if (
        prompt_run.payload is not None
        and prompt_run.successful_attempt is not None
        and prompt_run.payload_raw is not None
    ):
        final_adjustments, row_warnings = _normalize_adjustments(
            request_row=request_row,
            criterion_index=criterion_index,
            payload=prompt_run.payload,
            scope=scope,
            deidentified=deidentified,
        )
        row = ConsistencyReview(
            question_id=request_row.question_id,
            criterion_index=criterion_index,
            status="ok",
            adjustments=final_adjustments,
            warnings=row_warnings,
        )
        return ConsistencyRowRun(
            row=row,
            artifacts=[
                *prompt_run.artifacts,
                prompt_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    attempt=prompt_run.successful_attempt,
                    step="consistency_review",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="consistency_review",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                    response_parsed=prompt_run.payload.model_dump(mode="json"),
                    response_raw=prompt_run.payload_raw,
                    filename_suffix=prompt_run.successful_filename_suffix,
                ),
            ],
            failed_count=0,
        )

    message = (
        prompt_run.failure_message
        or prompt_run.invalid_output_message
        or "Consistency review output was invalid after retry."
    )
    row = ConsistencyReview(
        question_id=request_row.question_id,
        criterion_index=criterion_index,
        status="error",
        adjustments=[],
        warnings=[
            warning(
                code="consistency_review_failed",
                message=message,
                scope=scope,
            )
            if prompt_run.failure_message is not None
            else consistency_error_warning(scope=scope, message=message)
        ],
    )
    return ConsistencyRowRun(
        row=row,
        artifacts=prompt_run.artifacts,
        failed_count=1,
    )


def handle_grading_run_consistency(
    ctx: CommandContext,
    request: GradingRunConsistencyRequest,
) -> CommandOutcome:
    """Run criterion-scoped consistency review across student rows."""

    provider_name = request.providers.llm_provider
    assert provider_name is not None
    ctx.provider_registry.resolve_llm(provider_name)

    total = len(request.consistency_requests)
    ctx.emit(
        event="started",
        progress=progress(completed=0, total=total),
        data={"target_count": total, "total_stages": 1},
    )

    results: list[ConsistencyReview] = []
    artifacts: list[ArtifactReference] = []
    failed_count = 0
    for index, request_row in enumerate(request.consistency_requests, start=1):
        ctx.check_cancelled()
        criterion_index = request_row.rubric_criterion.criterion_index
        assert criterion_index is not None
        scope: dict[str, object] = {
            "question_id": request_row.question_id,
            "criterion_index": criterion_index,
        }
        ctx.emit(
            event="item_started",
            progress=progress(completed=index - 1, total=total),
            scope=scope,
        )
        row_run = _execute_consistency_row(
            output_artifacts_dir=request.output_artifacts_dir,
            ctx=ctx,
            request_row=request_row,
            provider_name=provider_name,
            llm_config=request.llm_config,
            criterion_index=criterion_index,
            scope=scope,
        )
        results.append(row_run.row)
        artifacts.extend(row_run.artifacts)
        failed_count += row_run.failed_count
        ctx.emit(
            event="item_completed",
            progress=progress(completed=index, total=total),
            scope=scope,
            data={"status": row_run.row.status},
        )

    ctx.emit(
        event="completed",
        progress=progress(completed=total, total=total),
        data={"target_count": total, "failed_count": failed_count},
    )
    return batch_outcome(
        data={
            "consistency_reviews": [
                result.model_dump(mode="json", exclude_none=True) for result in results
            ],
            "output_metadata_path": str(
                (request.output_artifacts_dir / "output_metadata.json").resolve()
            ),
        },
        output_artifacts_dir=request.output_artifacts_dir,
        artifacts=artifacts,
        result_row_count=len(results),
        failed_count=failed_count,
        command_label="Consistency review",
        providers={"llm_provider": provider_name},
    )


def grading_run_consistency_spec() -> CommandSpec:
    return CommandSpec(
        name="grading.run-consistency",
        request_model=GradingRunConsistencyRequest,
        handler=handle_grading_run_consistency,
    )
