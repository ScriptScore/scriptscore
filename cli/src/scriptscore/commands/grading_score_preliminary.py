# SPDX-License-Identifier: AGPL-3.0-only
"""Implementation of `grading score-preliminary`."""

from __future__ import annotations

from concurrent.futures import FIRST_COMPLETED, Future, ThreadPoolExecutor, wait
from dataclasses import dataclass
from pathlib import Path

from pydantic import BaseModel, ConfigDict, Field

from scriptscore.commands.common import batch_outcome, progress, warning
from scriptscore.commands.grading_shared import (
    answer_is_effectively_blank,
    instructor_profile_xml,
    question_context_xml,
    question_rubric_xml,
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
    ConfidenceBucket,
    GradingScorePreliminaryRequest,
    LlmConfig,
    PreliminaryAnswerScoreRequest,
    PreliminaryScoreRequest,
    PreliminaryScoreResult,
    ScriptscoreError,
    WarningObject,
)
from scriptscore.prompts import PromptResponseError, parse_json_model
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec


class PreliminaryScorePayload(BaseModel):
    """Strict provider response payload for criterion scoring."""

    model_config = ConfigDict(extra="forbid")

    points_awarded: int = Field(ge=0)
    rationale: str


class PreliminaryCriterionPayload(BaseModel):
    """Strict provider response payload for one criterion in answer scoring."""

    model_config = ConfigDict(extra="forbid")

    criterion_index: int = Field(ge=0)
    points_awarded: int = Field(ge=0)
    rationale: str


class PreliminaryAnswerScorePayload(BaseModel):
    """Strict provider response payload for answer-scoped scoring."""

    model_config = ConfigDict(extra="forbid")

    scores: list[PreliminaryCriterionPayload] = Field(min_length=1)


@dataclass(frozen=True)
class PreliminaryPromptRun:
    """Result of the retrying preliminary-score prompt execution."""

    payload: PreliminaryScorePayload | None
    payload_raw: str | None
    successful_attempt: PromptStepAttempt | None
    successful_filename_suffix: str | None
    failure_message: str | None
    parse_error_message: str | None
    artifacts: list[ArtifactReference]


@dataclass(frozen=True)
class PreliminaryAnswerPromptRun:
    """Result of the retrying answer-scoped preliminary-score prompt execution."""

    payload: PreliminaryAnswerScorePayload | None
    payload_raw: str | None
    successful_attempt: PromptStepAttempt | None
    successful_filename_suffix: str | None
    failure_message: str | None
    parse_error_message: str | None
    artifacts: list[ArtifactReference]


@dataclass(frozen=True)
class PreliminaryRowRun:
    """One completed preliminary-score row with accounting data."""

    row: PreliminaryScoreResult
    artifacts: list[ArtifactReference]
    failed_count: int
    degraded_parse_count: int


@dataclass(frozen=True)
class PreliminaryItemRun:
    """One completed preliminary work item with accounting data."""

    rows: list[PreliminaryScoreResult]
    artifacts: list[ArtifactReference]
    failed_count: int
    degraded_parse_count: int


def _preliminary_confidence(
    *,
    blank_local: bool = False,
    successful_retry: bool = False,
    degraded_parse_error: bool = False,
) -> tuple[ConfidenceBucket | None, str | None]:
    """Derive explicit preliminary-scoring confidence metadata."""

    if blank_local:
        return "high", "Blank local scoring required no model judgment."
    if degraded_parse_error:
        return "low", "Criterion response parsing failed after retries and fell back to zero."
    if successful_retry:
        return "medium", "Preliminary scoring succeeded after at least one response retry."
    return "high", None


def _command_inputs(request_row: PreliminaryScoreRequest) -> dict[str, str]:
    return {
        "subject": request_row.subject,
        "question_text_clean": request_row.question_text_clean,
        "question_context_xml": question_context_xml(request_row.question_context),
        "rubric_criterion_xml": rubric_criterion_xml(request_row.rubric_criterion),
        "instructor_profile_xml": instructor_profile_xml(request_row.instructor_profile),
        "student_answer": request_row.student_answer,
    }


def _answer_command_inputs(request_row: PreliminaryAnswerScoreRequest) -> dict[str, str]:
    return {
        "subject": request_row.subject,
        "question_text_clean": request_row.question_text_clean,
        "question_context_xml": question_context_xml(request_row.question_context),
        "question_rubric_xml": question_rubric_xml(request_row.rubric_criteria),
        "instructor_profile_xml": instructor_profile_xml(request_row.instructor_profile),
        "student_answer": request_row.student_answer,
    }


def _blank_local_row(
    *,
    output_artifacts_dir: Path,
    ctx: CommandContext,
    request_row: PreliminaryScoreRequest,
    criterion_index: int,
    scope: dict[str, object],
    prompt_variables: dict[str, str],
) -> PreliminaryRowRun:
    row = PreliminaryScoreResult(
        student_ref=request_row.student_ref,
        question_id=request_row.question_id,
        criterion_index=criterion_index,
        blank=True,
        points_awarded=0,
        rationale="Blank answer scored zero.",
        status="ok",
        confidence="high",
        confidence_reason="Blank local scoring required no model judgment.",
        warnings=[],
    )
    return PreliminaryRowRun(
        row=row,
        artifacts=[
            synthetic_llm_trace_artifact(
                output_artifacts_dir=output_artifacts_dir,
                ctx=ctx,
                step="preliminary_score",
                scope=scope,
                prompt_id="preliminary_score",
                prompt_variables=prompt_variables,
                response_raw=None,
                response_parsed=row.model_dump(mode="json"),
            )
        ],
        failed_count=0,
        degraded_parse_count=0,
    )


def _blank_local_answer_rows(
    *,
    output_artifacts_dir: Path,
    ctx: CommandContext,
    request_row: PreliminaryAnswerScoreRequest,
    scope: dict[str, object],
    prompt_variables: dict[str, str],
) -> PreliminaryItemRun:
    rows = [
        PreliminaryScoreResult(
            student_ref=request_row.student_ref,
            question_id=request_row.question_id,
            criterion_index=criterion.criterion_index or 0,
            blank=True,
            points_awarded=0,
            rationale="Blank answer scored zero.",
            status="ok",
            confidence="high",
            confidence_reason="Blank local scoring required no model judgment.",
            warnings=[],
        )
        for criterion in request_row.rubric_criteria
    ]
    return PreliminaryItemRun(
        rows=rows,
        artifacts=[
            synthetic_llm_trace_artifact(
                output_artifacts_dir=output_artifacts_dir,
                ctx=ctx,
                step="preliminary_score",
                scope=scope,
                prompt_id="preliminary_score_multi_criterion",
                prompt_variables=prompt_variables,
                response_raw=None,
                response_parsed={
                    "scores": [row.model_dump(mode="json") for row in rows],
                },
            )
        ],
        failed_count=0,
        degraded_parse_count=0,
    )


def _run_preliminary_prompt(
    *,
    output_artifacts_dir: Path,
    ctx: CommandContext,
    provider_name: str,
    llm_config: LlmConfig,
    scope: dict[str, object],
    prompt_variables: dict[str, str],
    criterion_points: int,
) -> PreliminaryPromptRun:
    artifacts: list[ArtifactReference] = []
    parse_error_message: str | None = None
    for attempt_index in range(1, 4):
        filename_suffix = None if attempt_index == 1 else f"attempt_{attempt_index:02d}"
        try:
            attempt = execute_prompt_step(
                ctx,
                provider_name=provider_name,
                prompt_id="preliminary_score",
                command_inputs=prompt_variables,
                llm_config=llm_config,
            )
        except ScriptscoreError as exc:
            artifacts.append(
                prompt_error_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    step="preliminary_score",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="preliminary_score",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                    error=exc,
                    filename_suffix=filename_suffix,
                )
            )
            return PreliminaryPromptRun(
                payload=None,
                payload_raw=None,
                successful_attempt=None,
                successful_filename_suffix=None,
                failure_message=exc.message,
                parse_error_message=None,
                artifacts=artifacts,
            )

        if attempt.execution is None:
            artifacts.append(
                prompt_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    attempt=attempt,
                    step="preliminary_score",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="preliminary_score",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                    response_parsed=None,
                    filename_suffix=filename_suffix,
                )
            )
            return PreliminaryPromptRun(
                payload=None,
                payload_raw=None,
                successful_attempt=None,
                successful_filename_suffix=None,
                failure_message=str(attempt.error) or "Preliminary scoring failed.",
                parse_error_message=None,
                artifacts=artifacts,
            )

        payload_raw = attempt.execution.provider_response.raw_text
        try:
            payload = parse_json_model(payload_raw, PreliminaryScorePayload)
            if payload.points_awarded > criterion_points:
                raise PromptResponseError(
                    code="prompt_response_schema_invalid",
                    message="Prompt response awarded points outside the criterion range.",
                )
            return PreliminaryPromptRun(
                payload=payload,
                payload_raw=payload_raw,
                successful_attempt=attempt,
                successful_filename_suffix=filename_suffix,
                failure_message=None,
                parse_error_message=None,
                artifacts=artifacts,
            )
        except PromptResponseError as exc:
            parse_error_message = exc.message
            artifacts.append(
                prompt_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    attempt=attempt,
                    step="preliminary_score",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="preliminary_score",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                    response_parsed=None,
                    response_raw=payload_raw,
                    filename_suffix=filename_suffix,
                )
            )

    return PreliminaryPromptRun(
        payload=None,
        payload_raw=None,
        successful_attempt=None,
        successful_filename_suffix=None,
        failure_message=None,
        parse_error_message=parse_error_message,
        artifacts=artifacts,
    )


def _validate_answer_payload(
    payload: PreliminaryAnswerScorePayload,
    criteria_by_index: dict[int, int],
) -> None:
    seen: set[int] = set()
    for score in payload.scores:
        if score.criterion_index not in criteria_by_index:
            raise PromptResponseError(
                code="prompt_response_schema_invalid",
                message="Prompt response included an unknown criterion_index.",
            )
        if score.criterion_index in seen:
            raise PromptResponseError(
                code="prompt_response_schema_invalid",
                message="Prompt response included a duplicate criterion_index.",
            )
        seen.add(score.criterion_index)
        if score.points_awarded > criteria_by_index[score.criterion_index]:
            raise PromptResponseError(
                code="prompt_response_schema_invalid",
                message="Prompt response awarded points outside a criterion range.",
            )
    if seen != set(criteria_by_index):
        raise PromptResponseError(
            code="prompt_response_schema_invalid",
            message="Prompt response did not include exactly one score for each criterion.",
        )


def _run_preliminary_answer_prompt(
    *,
    output_artifacts_dir: Path,
    ctx: CommandContext,
    provider_name: str,
    llm_config: LlmConfig,
    scope: dict[str, object],
    prompt_variables: dict[str, str],
    criteria_by_index: dict[int, int],
) -> PreliminaryAnswerPromptRun:
    artifacts: list[ArtifactReference] = []
    parse_error_message: str | None = None
    for attempt_index in range(1, 4):
        filename_suffix = None if attempt_index == 1 else f"attempt_{attempt_index:02d}"
        try:
            attempt = execute_prompt_step(
                ctx,
                provider_name=provider_name,
                prompt_id="preliminary_score_multi_criterion",
                command_inputs=prompt_variables,
                llm_config=llm_config,
            )
        except ScriptscoreError as exc:
            artifacts.append(
                prompt_error_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    step="preliminary_score",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="preliminary_score_multi_criterion",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                    error=exc,
                    filename_suffix=filename_suffix,
                )
            )
            return PreliminaryAnswerPromptRun(
                payload=None,
                payload_raw=None,
                successful_attempt=None,
                successful_filename_suffix=None,
                failure_message=exc.message,
                parse_error_message=None,
                artifacts=artifacts,
            )

        if attempt.execution is None:
            artifacts.append(
                prompt_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    attempt=attempt,
                    step="preliminary_score",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="preliminary_score_multi_criterion",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                    response_parsed=None,
                    filename_suffix=filename_suffix,
                )
            )
            return PreliminaryAnswerPromptRun(
                payload=None,
                payload_raw=None,
                successful_attempt=None,
                successful_filename_suffix=None,
                failure_message=str(attempt.error) or "Preliminary scoring failed.",
                parse_error_message=None,
                artifacts=artifacts,
            )

        payload_raw = attempt.execution.provider_response.raw_text
        try:
            payload = parse_json_model(payload_raw, PreliminaryAnswerScorePayload)
            _validate_answer_payload(payload, criteria_by_index)
            return PreliminaryAnswerPromptRun(
                payload=payload,
                payload_raw=payload_raw,
                successful_attempt=attempt,
                successful_filename_suffix=filename_suffix,
                failure_message=None,
                parse_error_message=None,
                artifacts=artifacts,
            )
        except PromptResponseError as exc:
            parse_error_message = exc.message
            artifacts.append(
                prompt_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    attempt=attempt,
                    step="preliminary_score",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="preliminary_score_multi_criterion",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                    response_parsed=None,
                    response_raw=payload_raw,
                    filename_suffix=filename_suffix,
                )
            )

    return PreliminaryAnswerPromptRun(
        payload=None,
        payload_raw=None,
        successful_attempt=None,
        successful_filename_suffix=None,
        failure_message=None,
        parse_error_message=parse_error_message,
        artifacts=artifacts,
    )


def _execute_preliminary_row(
    *,
    output_artifacts_dir: Path,
    ctx: CommandContext,
    request_row: PreliminaryScoreRequest,
    provider_name: str,
    llm_config: LlmConfig,
    criterion_index: int,
    scope: dict[str, object],
) -> PreliminaryRowRun:
    prompt_variables = _command_inputs(request_row)
    if answer_is_effectively_blank(request_row.student_answer):
        return _blank_local_row(
            output_artifacts_dir=output_artifacts_dir,
            ctx=ctx,
            request_row=request_row,
            criterion_index=criterion_index,
            scope=scope,
            prompt_variables=prompt_variables,
        )

    prompt_run = _run_preliminary_prompt(
        output_artifacts_dir=output_artifacts_dir,
        ctx=ctx,
        provider_name=provider_name,
        llm_config=llm_config,
        scope=scope,
        prompt_variables=prompt_variables,
        criterion_points=request_row.rubric_criterion.points,
    )

    if (
        prompt_run.payload is not None
        and prompt_run.successful_attempt is not None
        and prompt_run.payload_raw is not None
    ):
        confidence, confidence_reason = _preliminary_confidence(
            successful_retry=prompt_run.successful_filename_suffix is not None
        )
        row = PreliminaryScoreResult(
            student_ref=request_row.student_ref,
            question_id=request_row.question_id,
            criterion_index=criterion_index,
            blank=False,
            points_awarded=prompt_run.payload.points_awarded,
            rationale=prompt_run.payload.rationale,
            status="ok",
            confidence=confidence,
            confidence_reason=confidence_reason,
            warnings=[],
        )
        return PreliminaryRowRun(
            row=row,
            artifacts=[
                *prompt_run.artifacts,
                prompt_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    attempt=prompt_run.successful_attempt,
                    step="preliminary_score",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="preliminary_score",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                    response_parsed=prompt_run.payload.model_dump(mode="json"),
                    response_raw=prompt_run.payload_raw,
                    filename_suffix=prompt_run.successful_filename_suffix,
                ),
            ],
            failed_count=0,
            degraded_parse_count=0,
        )

    if prompt_run.failure_message is not None:
        row = PreliminaryScoreResult(
            student_ref=request_row.student_ref,
            question_id=request_row.question_id,
            criterion_index=criterion_index,
            blank=False,
            points_awarded=0,
            rationale="Preliminary scoring failed for this criterion.",
            status="error",
            confidence=None,
            confidence_reason=None,
            warnings=[
                warning(
                    code="preliminary_score_failed",
                    message=prompt_run.failure_message,
                    scope=scope,
                )
            ],
        )
        return PreliminaryRowRun(
            row=row,
            artifacts=prompt_run.artifacts,
            failed_count=1,
            degraded_parse_count=0,
        )

    confidence, confidence_reason = _preliminary_confidence(degraded_parse_error=True)
    row = PreliminaryScoreResult(
        student_ref=request_row.student_ref,
        question_id=request_row.question_id,
        criterion_index=criterion_index,
        blank=False,
        points_awarded=0,
        rationale="Scored zero because criterion response parsing failed after retries.",
        status="degraded_parse_error",
        confidence=confidence,
        confidence_reason=confidence_reason,
        warnings=[
            warning(
                code="preliminary_score_parse_failed",
                message=prompt_run.parse_error_message
                or "Criterion response parsing failed after retries.",
                scope=scope,
            )
        ],
    )
    return PreliminaryRowRun(
        row=row,
        artifacts=prompt_run.artifacts,
        failed_count=0,
        degraded_parse_count=1,
    )


def _execute_preliminary_answer(
    *,
    output_artifacts_dir: Path,
    ctx: CommandContext,
    request_row: PreliminaryAnswerScoreRequest,
    provider_name: str,
    llm_config: LlmConfig,
    scope: dict[str, object],
) -> PreliminaryItemRun:
    prompt_variables = _answer_command_inputs(request_row)
    if answer_is_effectively_blank(request_row.student_answer):
        return _blank_local_answer_rows(
            output_artifacts_dir=output_artifacts_dir,
            ctx=ctx,
            request_row=request_row,
            scope=scope,
            prompt_variables=prompt_variables,
        )

    criteria_by_index = {
        criterion.criterion_index or 0: criterion.points
        for criterion in request_row.rubric_criteria
    }
    prompt_run = _run_preliminary_answer_prompt(
        output_artifacts_dir=output_artifacts_dir,
        ctx=ctx,
        provider_name=provider_name,
        llm_config=llm_config,
        scope=scope,
        prompt_variables=prompt_variables,
        criteria_by_index=criteria_by_index,
    )

    if (
        prompt_run.payload is not None
        and prompt_run.successful_attempt is not None
        and prompt_run.payload_raw is not None
    ):
        confidence, confidence_reason = _preliminary_confidence(
            successful_retry=prompt_run.successful_filename_suffix is not None
        )
        rows = [
            PreliminaryScoreResult(
                student_ref=request_row.student_ref,
                question_id=request_row.question_id,
                criterion_index=score.criterion_index,
                blank=False,
                points_awarded=score.points_awarded,
                rationale=score.rationale,
                status="ok",
                confidence=confidence,
                confidence_reason=confidence_reason,
                warnings=[],
            )
            for score in sorted(prompt_run.payload.scores, key=lambda item: item.criterion_index)
        ]
        return PreliminaryItemRun(
            rows=rows,
            artifacts=[
                *prompt_run.artifacts,
                prompt_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    attempt=prompt_run.successful_attempt,
                    step="preliminary_score",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="preliminary_score_multi_criterion",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                    response_parsed=prompt_run.payload.model_dump(mode="json"),
                    response_raw=prompt_run.payload_raw,
                    filename_suffix=prompt_run.successful_filename_suffix,
                ),
            ],
            failed_count=0,
            degraded_parse_count=0,
        )

    if prompt_run.failure_message is not None:
        rows = [
            PreliminaryScoreResult(
                student_ref=request_row.student_ref,
                question_id=request_row.question_id,
                criterion_index=criterion.criterion_index or 0,
                blank=False,
                points_awarded=0,
                rationale="Preliminary scoring failed for this answer.",
                status="error",
                confidence=None,
                confidence_reason=None,
                warnings=[
                    warning(
                        code="preliminary_score_failed",
                        message=prompt_run.failure_message,
                        scope={**scope, "criterion_index": criterion.criterion_index or 0},
                    )
                ],
            )
            for criterion in request_row.rubric_criteria
        ]
        return PreliminaryItemRun(
            rows=rows,
            artifacts=prompt_run.artifacts,
            failed_count=len(rows),
            degraded_parse_count=0,
        )

    confidence, confidence_reason = _preliminary_confidence(degraded_parse_error=True)
    rows = [
        PreliminaryScoreResult(
            student_ref=request_row.student_ref,
            question_id=request_row.question_id,
            criterion_index=criterion.criterion_index or 0,
            blank=False,
            points_awarded=0,
            rationale="Scored zero because answer response parsing failed after retries.",
            status="degraded_parse_error",
            confidence=confidence,
            confidence_reason=confidence_reason,
            warnings=[
                warning(
                    code="preliminary_score_parse_failed",
                    message=prompt_run.parse_error_message
                    or "Answer response parsing failed after retries.",
                    scope={**scope, "criterion_index": criterion.criterion_index or 0},
                )
            ],
        )
        for criterion in request_row.rubric_criteria
    ]
    return PreliminaryItemRun(
        rows=rows,
        artifacts=prompt_run.artifacts,
        failed_count=0,
        degraded_parse_count=len(rows),
    )


def _preliminary_item_status(item_run: PreliminaryItemRun) -> str:
    if any(row.status == "error" for row in item_run.rows):
        return "error"
    if any(row.status == "degraded_parse_error" for row in item_run.rows):
        return "degraded_parse_error"
    if any(row.status == "cancelled" for row in item_run.rows):
        return "cancelled"
    return "ok"


def _answer_scope(answer_request: PreliminaryAnswerScoreRequest) -> dict[str, object]:
    return {
        "student_ref": answer_request.student_ref,
        "question_id": answer_request.question_id,
    }


def _run_answer_requests_serial(
    *,
    ctx: CommandContext,
    request: GradingScorePreliminaryRequest,
    answer_score_requests: list[PreliminaryAnswerScoreRequest],
    provider_name: str,
    total: int,
    completed: int,
) -> tuple[list[PreliminaryItemRun], int]:
    item_runs: list[PreliminaryItemRun] = []
    for answer_request in answer_score_requests:
        ctx.check_cancelled()
        scope = _answer_scope(answer_request)
        ctx.emit(
            event="item_started",
            progress=progress(completed=completed, total=total),
            scope=scope,
        )
        item_run = _execute_preliminary_answer(
            output_artifacts_dir=request.output_artifacts_dir,
            ctx=ctx,
            request_row=answer_request,
            provider_name=provider_name,
            llm_config=request.llm_config,
            scope=scope,
        )
        item_runs.append(item_run)
        completed += 1
        ctx.emit(
            event="item_completed",
            progress=progress(completed=completed, total=total),
            scope=scope,
            data={
                "status": _preliminary_item_status(item_run),
                "result_row_count": len(item_run.rows),
            },
        )
    return item_runs, completed


def _run_answer_requests_concurrent(
    *,
    ctx: CommandContext,
    request: GradingScorePreliminaryRequest,
    answer_score_requests: list[PreliminaryAnswerScoreRequest],
    provider_name: str,
    total: int,
    completed: int,
    max_workers: int,
) -> tuple[list[PreliminaryItemRun], int]:
    item_runs: list[PreliminaryItemRun | None] = [None] * len(answer_score_requests)
    futures: dict[Future[PreliminaryItemRun], tuple[int, dict[str, object]]] = {}
    next_index = 0

    def submit_next(executor: ThreadPoolExecutor) -> None:
        nonlocal next_index
        ctx.check_cancelled()
        index = next_index
        answer_request = answer_score_requests[index]
        scope = _answer_scope(answer_request)
        ctx.emit(
            event="item_started",
            progress=progress(completed=completed, total=total),
            scope=scope,
        )
        ctx.check_cancelled()
        futures[
            executor.submit(
                _execute_preliminary_answer,
                output_artifacts_dir=request.output_artifacts_dir,
                ctx=ctx,
                request_row=answer_request,
                provider_name=provider_name,
                llm_config=request.llm_config,
                scope=scope,
            )
        ] = (index, scope)
        next_index += 1

    with ThreadPoolExecutor(max_workers=max_workers) as executor:
        try:
            while next_index < len(answer_score_requests) and len(futures) < max_workers:
                submit_next(executor)

            while futures:
                done, _pending = wait(futures, return_when=FIRST_COMPLETED)
                for future in done:
                    ctx.check_cancelled()
                    index, scope = futures.pop(future)
                    item_run = future.result()
                    item_runs[index] = item_run
                    completed += 1
                    ctx.emit(
                        event="item_completed",
                        progress=progress(completed=completed, total=total),
                        scope=scope,
                        data={
                            "status": _preliminary_item_status(item_run),
                            "result_row_count": len(item_run.rows),
                        },
                    )
                    if next_index < len(answer_score_requests):
                        submit_next(executor)
        except Exception:
            for future in futures:
                future.cancel()
            executor.shutdown(wait=False, cancel_futures=True)
            raise

    return [item_run for item_run in item_runs if item_run is not None], completed


def handle_grading_score_preliminary(
    ctx: CommandContext,
    request: GradingScorePreliminaryRequest,
) -> CommandOutcome:
    """Score one rubric criterion per request row."""

    provider_name = request.providers.llm_provider
    assert provider_name is not None
    ctx.provider_registry.resolve_llm(provider_name)

    score_requests = request.score_requests or []
    answer_score_requests = request.answer_score_requests or []
    total = len(score_requests) + len(answer_score_requests)
    ctx.emit(
        event="started",
        progress=progress(completed=0, total=total),
        data={
            "target_count": total,
            "total_stages": 1,
            "max_workers": request.grading_runtime_config.max_workers,
        },
    )

    results: list[PreliminaryScoreResult] = []
    artifacts: list[ArtifactReference] = []
    failed_count = 0
    degraded_parse_count = 0
    completed = 0
    for score_request in score_requests:
        ctx.check_cancelled()
        criterion_index = score_request.rubric_criterion.criterion_index
        assert criterion_index is not None
        scope: dict[str, object] = {
            "student_ref": score_request.student_ref,
            "question_id": score_request.question_id,
            "criterion_index": criterion_index,
        }
        ctx.emit(
            event="item_started",
            progress=progress(completed=completed, total=total),
            scope=scope,
        )
        row_run = _execute_preliminary_row(
            output_artifacts_dir=request.output_artifacts_dir,
            ctx=ctx,
            request_row=score_request,
            provider_name=provider_name,
            llm_config=request.llm_config,
            criterion_index=criterion_index,
            scope=scope,
        )
        results.append(row_run.row)
        artifacts.extend(row_run.artifacts)
        failed_count += row_run.failed_count
        degraded_parse_count += row_run.degraded_parse_count
        completed += 1
        ctx.emit(
            event="item_completed",
            progress=progress(completed=completed, total=total),
            scope=scope,
            data={"status": row_run.row.status},
        )
    max_workers = request.grading_runtime_config.max_workers
    if answer_score_requests:
        if max_workers == 1:
            answer_item_runs, completed = _run_answer_requests_serial(
                ctx=ctx,
                request=request,
                answer_score_requests=answer_score_requests,
                provider_name=provider_name,
                total=total,
                completed=completed,
            )
        else:
            answer_item_runs, completed = _run_answer_requests_concurrent(
                ctx=ctx,
                request=request,
                answer_score_requests=answer_score_requests,
                provider_name=provider_name,
                total=total,
                completed=completed,
                max_workers=max_workers,
            )
    else:
        answer_item_runs = []

    for item_run in answer_item_runs:
        results.extend(item_run.rows)
        artifacts.extend(item_run.artifacts)
        failed_count += item_run.failed_count
        degraded_parse_count += item_run.degraded_parse_count

    extra_warnings: list[WarningObject] = []
    if degraded_parse_count:
        extra_warnings.append(
            warning(
                code="preliminary_score_parse_failed",
                message=(
                    "Preliminary scoring degraded to zero after response parsing failed for "
                    f"{degraded_parse_count} row{'s' if degraded_parse_count != 1 else ''}."
                ),
                scope={"row_count": degraded_parse_count},
            )
        )

    ctx.emit(
        event="completed",
        progress=progress(completed=total, total=total),
        data={
            "target_count": total,
            "failed_count": failed_count,
            "degraded_parse_count": degraded_parse_count,
        },
    )
    return batch_outcome(
        data={
            "preliminary_scores": [
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
        command_label="Preliminary scoring",
        providers={"llm_provider": provider_name},
        extra_warnings=extra_warnings or None,
    )


def grading_score_preliminary_spec() -> CommandSpec:
    return CommandSpec(
        name="grading.score-preliminary",
        request_model=GradingScorePreliminaryRequest,
        handler=handle_grading_score_preliminary,
    )
