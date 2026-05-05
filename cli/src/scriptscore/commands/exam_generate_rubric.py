# SPDX-License-Identifier: AGPL-3.0-only
"""Implementation of `exam generate-rubric`."""

from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, ConfigDict

from scriptscore.commands.common import inventory_manifest_data, progress, warning
from scriptscore.commands.exam_shared import (
    RubricLintCandidate,
    find_rubric_lint_candidates,
    instructor_profile_xml,
    question_context_text,
)
from scriptscore.commands.llm import execute_prompt_step, prompt_trace_artifact
from scriptscore.contracts import (
    ArtifactReference,
    ErrorCategory,
    ExamGenerateRubricRequest,
    RubricCriterion,
    RubricDraft,
    ScriptscoreError,
    WarningObject,
)
from scriptscore.prompts import parse_json_model, render_xml, xml_node, xml_text
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec


class RubricGeneratePayload(BaseModel):
    """Strict schema for rubric generation output."""

    model_config = ConfigDict(extra="forbid")

    criteria: list[RubricCriterion]


class RubricSemanticPairReview(BaseModel):
    """One secondary semantic review decision for a flagged criterion pair."""

    model_config = ConfigDict(extra="forbid")

    left_index: int
    right_index: int
    classification: Literal["distinct", "overlap", "duplicate"]
    reason: str


class RubricSemanticReviewPayload(BaseModel):
    """Secondary semantic review payload for flagged rubric pairs."""

    model_config = ConfigDict(extra="forbid")

    pair_reviews: list[RubricSemanticPairReview]


def _rubric_criteria_xml(criteria: list[RubricCriterion], *, index_start: int = 1) -> str:
    return render_xml(
        xml_node(
            "rubric_criteria",
            *[
                xml_node(
                    "criterion",
                    xml_text("label", criterion.label),
                    xml_text("points", str(criterion.points)),
                    xml_text("partial_credit_guidance", criterion.partial_credit_guidance),
                    attrs={"index": index},
                )
                for index, criterion in enumerate(criteria, start=index_start)
            ],
        )
    )


def _candidate_pairs_xml(candidates: list[RubricLintCandidate]) -> str:
    return render_xml(
        xml_node(
            "candidate_pairs",
            *[
                xml_node(
                    "pair",
                    xml_text("code", candidate.code),
                    xml_text("message", candidate.message),
                    attrs={
                        "left_index": candidate.left_index,
                        "right_index": candidate.right_index,
                    },
                )
                for candidate in candidates
            ],
        )
    )


def _semantic_review_warnings(
    *,
    ctx: CommandContext,
    request: ExamGenerateRubricRequest,
    provider_name: str,
    criteria: list[RubricCriterion],
    scope: dict[str, object],
) -> tuple[list[dict[str, object]], list[WarningObject], list[ArtifactReference]]:
    candidates = find_rubric_lint_candidates(
        criteria,
        host_prepends_minimum_credit_criterion=request.host_prepends_minimum_credit_criterion,
    )
    if not candidates:
        return [], [], []

    candidate_payload = [
        {
            "code": candidate.code,
            "left_index": candidate.left_index,
            "right_index": candidate.right_index,
            "message": candidate.message,
        }
        for candidate in candidates
    ]
    displayed_index_start = 2 if request.host_prepends_minimum_credit_criterion else 1
    prompt_variables = {
        "question_text_clean": request.question_text_clean,
        "rubric_criteria_xml": _rubric_criteria_xml(criteria, index_start=displayed_index_start),
        "candidate_pairs_xml": _candidate_pairs_xml(candidates),
    }
    attempt = execute_prompt_step(
        ctx,
        provider_name=provider_name,
        prompt_id="rubric_semantic_review",
        command_inputs=prompt_variables,
        llm_config=request.llm_config,
    )
    if attempt.execution is None:
        return (
            candidate_payload,
            [
                warning(
                    code="rubric_semantic_review_failed",
                    message=str(attempt.error)
                    or "Secondary rubric semantic review failed; local warnings were kept.",
                    scope=scope,
                ),
                *[
                    warning(
                        code=candidate.code,
                        message=candidate.message,
                        scope={
                            **scope,
                            "criteria": [candidate.left_index, candidate.right_index],
                            "lint_source": "local",
                        },
                    )
                    for candidate in candidates
                ],
            ],
            [
                prompt_trace_artifact(
                    output_artifacts_dir=request.output_artifacts_dir,
                    ctx=ctx,
                    attempt=attempt,
                    step="review_rubric_semantics",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="rubric_semantic_review",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                )
            ],
        )

    payload_raw = attempt.execution.provider_response.raw_text
    try:
        payload = parse_json_model(payload_raw, RubricSemanticReviewPayload)
    except Exception as exc:
        return (
            candidate_payload,
            [
                warning(
                    code="rubric_semantic_review_failed",
                    message=str(exc)
                    or "Secondary rubric semantic review returned invalid output; local warnings were kept.",
                    scope=scope,
                ),
                *[
                    warning(
                        code=candidate.code,
                        message=candidate.message,
                        scope={
                            **scope,
                            "criteria": [candidate.left_index, candidate.right_index],
                            "lint_source": "local",
                        },
                    )
                    for candidate in candidates
                ],
            ],
            [
                prompt_trace_artifact(
                    output_artifacts_dir=request.output_artifacts_dir,
                    ctx=ctx,
                    attempt=attempt,
                    step="review_rubric_semantics",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="rubric_semantic_review",
                    prompt_variables=prompt_variables,
                    input_artifacts=[],
                    response_raw=payload_raw,
                )
            ],
        )

    reviews = {(review.left_index, review.right_index): review for review in payload.pair_reviews}
    warnings_out: list[WarningObject] = []
    for candidate in candidates:
        review = reviews.get((candidate.left_index, candidate.right_index))
        if review is None:
            warnings_out.append(
                warning(
                    code=candidate.code,
                    message=candidate.message,
                    scope={
                        **scope,
                        "criteria": [candidate.left_index, candidate.right_index],
                        "lint_source": "local",
                    },
                )
            )
            continue
        if review.classification == "distinct":
            continue
        warning_code = (
            "rubric_duplicate_criterion"
            if review.classification == "duplicate"
            else "rubric_potential_overlap"
        )
        warnings_out.append(
            warning(
                code=warning_code,
                message=f"{candidate.message} {review.reason}",
                scope={
                    **scope,
                    "criteria": [candidate.left_index, candidate.right_index],
                    "lint_source": "local+prompt",
                },
            )
        )

    return (
        candidate_payload,
        warnings_out,
        [
            prompt_trace_artifact(
                output_artifacts_dir=request.output_artifacts_dir,
                ctx=ctx,
                attempt=attempt,
                step="review_rubric_semantics",
                scope=scope,
                provider_name=provider_name,
                prompt_id="rubric_semantic_review",
                prompt_variables=prompt_variables,
                input_artifacts=[],
                response_parsed=payload.model_dump(mode="json"),
                response_raw=payload_raw,
            )
        ],
    )


def handle_exam_generate_rubric(
    ctx: CommandContext, request: ExamGenerateRubricRequest
) -> CommandOutcome:
    """Generate one rubric draft for a question."""

    provider_name = request.providers.llm_provider
    assert provider_name is not None
    ctx.provider_registry.resolve_llm(provider_name)
    scope: dict[str, object] = {"question_id": request.question_id}
    ctx.emit(
        event="started",
        progress=progress(completed=0, total=1),
        data={"total_stages": 1},
        scope=scope,
    )

    assets_xml = question_context_text(request.question_context)
    attempt = execute_prompt_step(
        ctx,
        provider_name=provider_name,
        prompt_id="rubric_generate",
        command_inputs={
            "max_points": str(request.max_points),
            "subject": request.subject,
            "question_text_clean": request.question_text_clean,
            "question_context_text": assets_xml,
            "instructor_profile_xml": instructor_profile_xml(request),
        },
        llm_config=request.llm_config,
    )
    if attempt.execution is None:
        raise ScriptscoreError(
            code="rubric_generate_failed",
            message=str(attempt.error) or "Rubric generation failed.",
            category=ErrorCategory.EXECUTION,
            retryable=True,
        ) from attempt.error

    raw_text = attempt.execution.provider_response.raw_text
    payload = parse_json_model(raw_text, RubricGeneratePayload)
    indexed_criteria = [
        criterion.model_copy(update={"criterion_index": index})
        for index, criterion in enumerate(payload.criteria)
    ]
    semantic_warnings = []
    total_points = sum(criterion.points for criterion in indexed_criteria)
    if total_points != request.max_points:
        semantic_warnings.append(
            warning(
                code="rubric_points_mismatch",
                message=f"Rubric criterion points summed to {total_points} instead of {request.max_points}.",
                scope=scope,
            )
        )
    local_lint_candidates, local_lint_warnings, review_artifacts = _semantic_review_warnings(
        ctx=ctx,
        request=request,
        provider_name=provider_name,
        criteria=indexed_criteria,
        scope=scope,
    )
    semantic_warnings.extend(local_lint_warnings)

    trace_artifact = prompt_trace_artifact(
        output_artifacts_dir=request.output_artifacts_dir,
        ctx=ctx,
        attempt=attempt,
        step="generate_rubric",
        scope=scope,
        provider_name=provider_name,
        prompt_id="rubric_generate",
        prompt_variables={
            "max_points": str(request.max_points),
            "subject": request.subject,
            "question_text_clean": request.question_text_clean,
            "question_context_text": assets_xml,
        },
        input_artifacts=[],
        response_parsed={
            "criteria": [criterion.model_dump(mode="json") for criterion in indexed_criteria]
        },
        response_raw=raw_text,
    )

    rubric = RubricDraft(
        question_id=request.question_id,
        model_output=(
            {
                "criteria": [criterion.model_dump(mode="json") for criterion in indexed_criteria],
                "rubric_lint_candidates": local_lint_candidates,
            }
            if semantic_warnings
            else None
        ),
        criteria=indexed_criteria,
        warnings=semantic_warnings,
    )
    ctx.emit(
        event="completed",
        progress=progress(completed=1, total=1),
        scope=scope,
    )
    return CommandOutcome(
        data={
            "rubric_draft": rubric.model_dump(mode="json", exclude_none=True),
            "output_metadata_path": str(
                (request.output_artifacts_dir / "output_metadata.json").resolve()
            ),
        },
        degraded=bool(semantic_warnings),
        warnings=semantic_warnings,
        artifacts=[trace_artifact, *review_artifacts],
        providers={"llm_provider": provider_name},
        output_artifacts_dir=request.output_artifacts_dir,
        manifest_data=inventory_manifest_data(
            result_row_count=1,
            written_artifact_count=1 + len(review_artifacts),
            failed_count=0,
        ),
    )


def exam_generate_rubric_spec() -> CommandSpec:
    return CommandSpec(
        name="exam.generate-rubric",
        request_model=ExamGenerateRubricRequest,
        handler=handle_exam_generate_rubric,
    )
