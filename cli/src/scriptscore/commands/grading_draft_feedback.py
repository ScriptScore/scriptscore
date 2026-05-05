# SPDX-License-Identifier: AGPL-3.0-only
"""Implementation of `grading draft-feedback`."""

from __future__ import annotations

from scriptscore.commands.common import batch_outcome, progress, warning
from scriptscore.commands.grading_shared import (
    answer_is_effectively_blank,
    assessment_xml,
    question_context_xml,
    run_plain_text_prompt,
    synthetic_llm_trace_artifact,
)
from scriptscore.commands.llm import prompt_trace_artifact
from scriptscore.contracts import (
    ArtifactReference,
    FeedbackDraft,
    FeedbackRequest,
    GradingDraftFeedbackRequest,
)
from scriptscore.prompts import PromptResponseError
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec

_BLANK_LOCAL_FEEDBACK = "No relevant answer provided."
_DEFAULT_FALLBACK_FEEDBACK = "Needs instructor review."


def _prompt_variables(request_row: FeedbackRequest) -> dict[str, str]:
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


def _parse_feedback_response(raw_text: str) -> str:
    response_text = raw_text.strip()
    if not response_text:
        raise PromptResponseError(
            code="prompt_response_schema_invalid",
            message="Feedback provider returned an empty response.",
        )
    return response_text


def handle_grading_draft_feedback(
    ctx: CommandContext,
    request: GradingDraftFeedbackRequest,
) -> CommandOutcome:
    """Draft student-facing feedback for explicit grading rows."""

    provider_name = request.providers.llm_provider
    assert provider_name is not None
    ctx.provider_registry.resolve_llm(provider_name)

    total = len(request.feedback_requests)
    ctx.emit(
        event="started",
        progress=progress(completed=0, total=total),
        data={"target_count": total, "total_stages": 1},
    )

    results: list[FeedbackDraft] = []
    artifacts: list[ArtifactReference] = []
    fallback_count = 0
    for index, request_row in enumerate(request.feedback_requests, start=1):
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
        prompt_variables = _prompt_variables(request_row)

        if answer_is_effectively_blank(request_row.student_answer):
            row = FeedbackDraft(
                student_ref=request_row.student_ref,
                question_id=request_row.question_id,
                feedback_source="blank_local",
                feedback_text=_BLANK_LOCAL_FEEDBACK,
                warnings=[],
            )
            artifacts.append(
                synthetic_llm_trace_artifact(
                    output_artifacts_dir=request.output_artifacts_dir,
                    ctx=ctx,
                    step="feedback_draft",
                    scope=scope,
                    prompt_id="feedback_draft",
                    prompt_variables=prompt_variables,
                    response_raw=None,
                    response_parsed=row.model_dump(mode="json"),
                )
            )
            results.append(row)
            ctx.emit(
                event="item_completed",
                progress=progress(completed=index, total=total),
                scope=scope,
                data={"feedback_source": row.feedback_source},
            )
            continue

        prompt_run = run_plain_text_prompt(
            ctx,
            output_artifacts_dir=request.output_artifacts_dir,
            provider_name=provider_name,
            llm_config=request.llm_config,
            prompt_id="feedback_draft",
            step="feedback_draft",
            scope=scope,
            prompt_variables=prompt_variables,
            parse_response=_parse_feedback_response,
            max_attempts=2,
            default_failure_message="Feedback drafting failed after retry.",
        )
        artifacts.extend(prompt_run.artifacts)

        if (
            prompt_run.parsed is not None
            and prompt_run.successful_attempt is not None
            and prompt_run.raw_response is not None
        ):
            row = FeedbackDraft(
                student_ref=request_row.student_ref,
                question_id=request_row.question_id,
                feedback_source="model",
                feedback_text=prompt_run.parsed,
                warnings=[],
            )
            artifacts.append(
                prompt_trace_artifact(
                    output_artifacts_dir=request.output_artifacts_dir,
                    ctx=ctx,
                    attempt=prompt_run.successful_attempt,
                    step="feedback_draft",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="feedback_draft",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                    response_parsed={
                        "feedback_text": prompt_run.parsed,
                        "feedback_source": "model",
                    },
                    response_raw=prompt_run.raw_response,
                    filename_suffix=prompt_run.successful_filename_suffix,
                )
            )
        else:
            fallback_count += 1
            row = FeedbackDraft(
                student_ref=request_row.student_ref,
                question_id=request_row.question_id,
                feedback_source="default_fallback",
                feedback_text=_DEFAULT_FALLBACK_FEEDBACK,
                warnings=[
                    warning(
                        code="feedback_default_fallback",
                        message=prompt_run.fallback_message,
                        scope=scope,
                    )
                ],
            )

        results.append(row)
        ctx.emit(
            event="item_completed",
            progress=progress(completed=index, total=total),
            scope=scope,
            data={"feedback_source": row.feedback_source},
        )

    ctx.emit(
        event="completed",
        progress=progress(completed=total, total=total),
        data={"target_count": total, "fallback_count": fallback_count},
    )
    return batch_outcome(
        data={
            "feedback_drafts": [
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
        command_label="Feedback drafting",
        providers={"llm_provider": provider_name},
        extra_warnings=(
            [
                warning(
                    code="feedback_default_fallback",
                    message=(
                        "Feedback drafting used the default fallback for "
                        f"{fallback_count} row{'s' if fallback_count != 1 else ''}."
                    ),
                    scope={"row_count": fallback_count},
                )
            ]
            if fallback_count
            else None
        ),
    )


def grading_draft_feedback_spec() -> CommandSpec:
    return CommandSpec(
        name="grading.draft-feedback",
        request_model=GradingDraftFeedbackRequest,
        handler=handle_grading_draft_feedback,
    )
