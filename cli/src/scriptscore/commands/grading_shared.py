# SPDX-License-Identifier: AGPL-3.0-only
"""Shared helpers for grading command implementations."""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Literal

from scriptscore.artifacts import write_trace_artifact
from scriptscore.commands.common import timing_info, warning
from scriptscore.commands.llm import (
    PromptStepAttempt,
    execute_prompt_step,
    prompt_error_trace_artifact,
    prompt_trace_artifact,
)
from scriptscore.contracts import (
    ArtifactReference,
    ConsistencyStudentScore,
    FeedbackCriterionResult,
    HighlightSpan,
    InstructorProfile,
    LlmConfig,
    RubricCriterion,
    ScriptscoreError,
    WarningObject,
)
from scriptscore.prompts import PromptResponseError, render_xml, xml_node, xml_text
from scriptscore.runtime import CommandContext


def answer_is_effectively_blank(text: str) -> bool:
    """Return whether a student answer should be treated as blank locally."""

    normalized = text.strip().lower()
    return normalized in {"", "[blank]"}


def question_context_xml(question_context: str) -> str:
    """Render the shared escaped question-context XML projection."""

    return render_xml(xml_text("question_context", question_context))


def instructor_profile_xml(profile: InstructorProfile) -> str:
    """Render the shared instructor-profile XML projection."""

    children = [
        xml_text(tag, value)
        for tag, value in [
            ("grading_strictness", profile.grading_strictness),
            ("syntax_leniency", profile.syntax_leniency),
            ("ocr_tolerance", profile.ocr_tolerance),
            ("partial_credit_style", profile.partial_credit_style),
            ("feedback_style", profile.feedback_style),
        ]
        if value is not None
    ]
    children.append(xml_text("additional_guidance", profile.additional_guidance or ""))
    return render_xml(xml_node("instructor_profile", *children))


def rubric_criterion_xml(criterion: RubricCriterion) -> str:
    """Render one criterion-scoped XML block."""

    if criterion.criterion_index is None:
        raise ValueError("rubric_criterion_xml requires criterion_index.")
    return render_xml(
        xml_node(
            "rubric_criterion",
            xml_text("criterion_index", criterion.criterion_index),
            xml_text("label", criterion.label),
            xml_text("points", criterion.points),
            xml_text("partial_credit_guidance", criterion.partial_credit_guidance),
        )
    )


def question_rubric_xml(criteria: list[RubricCriterion]) -> str:
    """Render the shared ordered full-rubric XML block."""

    children = []
    for criterion in criteria:
        if criterion.criterion_index is None:
            raise ValueError(
                "question_rubric_xml requires criterion_index on every rubric criterion."
            )
        children.append(
            xml_node(
                "criterion",
                xml_text("criterion_index", criterion.criterion_index),
                xml_text("label", criterion.label),
                xml_text("points", criterion.points),
                xml_text("partial_credit_guidance", criterion.partial_credit_guidance),
            )
        )
    return render_xml(xml_node("question_rubric", *children))


def assessment_xml(
    *,
    rubric_criteria: list[RubricCriterion],
    criterion_results: list[FeedbackCriterionResult],
    total_points_awarded: int,
    question_max_points: int,
) -> str:
    """Render the merged assessment XML block used by feedback and markup prompts."""

    by_index = {result.criterion_index: result for result in criterion_results}
    children = []
    for criterion in rubric_criteria:
        if criterion.criterion_index is None:
            raise ValueError("assessment_xml requires criterion_index on every rubric criterion.")
        result = by_index[criterion.criterion_index]
        children.append(
            xml_node(
                "criterion",
                xml_text("criterion_index", criterion.criterion_index),
                xml_text("label", criterion.label),
                xml_text("partial_credit_guidance", criterion.partial_credit_guidance),
                xml_text("rationale", result.rationale),
                attrs={"points_awarded": result.points_awarded},
            )
        )
    return render_xml(
        xml_node(
            "assessment",
            *children,
            attrs={
                "total_points_awarded": total_points_awarded,
                "question_max_points": question_max_points,
            },
        )
    )


@dataclass(frozen=True)
class DeidentifiedConsistencyInput:
    """Prompt-facing consistency input with alias mapping."""

    alias_to_student_ref: dict[str, str]
    rendered_xml: str


@dataclass(frozen=True)
class PlainTextPromptRun[T]:
    """Result of a retrying plain-text prompt execution."""

    parsed: T | None
    raw_response: str | None
    successful_attempt: PromptStepAttempt | None
    successful_filename_suffix: str | None
    fallback_message: str
    artifacts: list[ArtifactReference]


def deidentify_consistency_student_answers(
    student_scores: list[ConsistencyStudentScore],
) -> DeidentifiedConsistencyInput:
    """Render deidentified student-answer XML and return the alias mapping."""

    alias_to_student_ref: dict[str, str] = {}
    children = []
    visible_index = 0
    for score in student_scores:
        if score.blank:
            continue
        visible_index += 1
        alias = f"student_{visible_index:03d}"
        alias_to_student_ref[alias] = score.student_ref
        children.append(
            xml_node(
                "student",
                xml_text("answer", score.student_answer),
                xml_text("rationale", score.preliminary_rationale),
                attrs={"id": alias, "score": score.preliminary_points_awarded},
            )
        )
    return DeidentifiedConsistencyInput(
        alias_to_student_ref=alias_to_student_ref,
        rendered_xml=render_xml(xml_node("student_answers", *children)),
    )


def run_plain_text_prompt[T](
    ctx: CommandContext,
    *,
    output_artifacts_dir: Path,
    provider_name: str,
    llm_config: LlmConfig,
    prompt_id: str,
    step: str,
    scope: dict[str, object],
    prompt_variables: dict[str, str],
    parse_response: Callable[[str], T],
    max_attempts: int,
    default_failure_message: str,
    input_artifacts: list[str] | None = None,
) -> PlainTextPromptRun[T]:
    """Run a plain-text prompt with retry, trace writing, and parse validation."""

    artifacts: list[ArtifactReference] = []
    fallback_message = default_failure_message
    for attempt_index in range(1, max_attempts + 1):
        filename_suffix = None if attempt_index == 1 else f"attempt_{attempt_index:02d}"
        try:
            attempt = execute_prompt_step(
                ctx,
                provider_name=provider_name,
                prompt_id=prompt_id,
                command_inputs=prompt_variables,
                llm_config=llm_config,
            )
        except ScriptscoreError as exc:
            fallback_message = exc.message
            artifacts.append(
                prompt_error_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    step=step,
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id=prompt_id,
                    prompt_variables=prompt_variables,
                    input_artifacts=input_artifacts,
                    error=exc,
                    filename_suffix=filename_suffix,
                )
            )
            continue

        if attempt.execution is None:
            fallback_message = str(attempt.error) or default_failure_message
            artifacts.append(
                prompt_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    attempt=attempt,
                    step=step,
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id=prompt_id,
                    prompt_variables=prompt_variables,
                    input_artifacts=input_artifacts,
                    response_parsed=None,
                    filename_suffix=filename_suffix,
                )
            )
            continue

        raw_text = attempt.execution.provider_response.raw_text
        try:
            parsed = parse_response(raw_text)
        except PromptResponseError as exc:
            fallback_message = exc.message
            artifacts.append(
                prompt_trace_artifact(
                    output_artifacts_dir=output_artifacts_dir,
                    ctx=ctx,
                    attempt=attempt,
                    step=step,
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id=prompt_id,
                    prompt_variables=prompt_variables,
                    input_artifacts=input_artifacts,
                    response_parsed=None,
                    response_raw=raw_text,
                    filename_suffix=filename_suffix,
                )
            )
            continue

        return PlainTextPromptRun(
            parsed=parsed,
            raw_response=raw_text,
            successful_attempt=attempt,
            successful_filename_suffix=filename_suffix,
            fallback_message=fallback_message,
            artifacts=artifacts,
        )

    return PlainTextPromptRun(
        parsed=None,
        raw_response=None,
        successful_attempt=None,
        successful_filename_suffix=None,
        fallback_message=fallback_message,
        artifacts=artifacts,
    )


def parse_tagged_highlights(tagged_text: str, *, student_answer: str) -> list[HighlightSpan]:
    """Parse allowlisted span-tagged markup output into validated highlight spans."""

    highlights: list[HighlightSpan] = []
    rebuilt: list[str] = []
    cursor = 0
    source_offset = 0
    while cursor < len(tagged_text):
        if tagged_text.startswith('<span data-kind="correct">', cursor):
            kind: Literal["correct", "incorrect"] = "correct"
            open_tag = '<span data-kind="correct">'
        elif tagged_text.startswith('<span data-kind="incorrect">', cursor):
            kind = "incorrect"
            open_tag = '<span data-kind="incorrect">'
        else:
            if tagged_text[cursor] == "<":
                rebuilt.append("<")
                source_offset += 1
                cursor += 1
                continue
            next_tag = tagged_text.find("<", cursor)
            if next_tag == -1:
                plain = tagged_text[cursor:]
                rebuilt.append(plain)
                source_offset += len(plain)
                break
            plain = tagged_text[cursor:next_tag]
            rebuilt.append(plain)
            source_offset += len(plain)
            cursor = next_tag
            continue

        content_start = cursor + len(open_tag)
        close_tag = "</span>"
        content_end = tagged_text.find(close_tag, content_start)
        if content_end == -1:
            raise PromptResponseError(
                code="markup_parse_failed",
                message="Markup output contained an unterminated highlight tag.",
            )
        content = tagged_text[content_start:content_end]
        if not content:
            raise PromptResponseError(
                code="markup_parse_failed",
                message="Markup output contained an empty highlight span.",
            )
        if '<span data-kind="correct">' in content or '<span data-kind="incorrect">' in content:
            raise PromptResponseError(
                code="markup_parse_failed",
                message="Markup output contained nested highlight tags.",
            )
        rebuilt.append(content)
        highlights.append(
            HighlightSpan(
                kind=kind,
                start_char=source_offset,
                end_char=source_offset + len(content),
                text=content,
            )
        )
        source_offset += len(content)
        cursor = content_end + len(close_tag)

    if "".join(rebuilt) != student_answer:
        raise PromptResponseError(
            code="markup_parse_failed",
            message="Markup output did not reconstruct the original student answer exactly.",
        )
    if student_answer and highlights == []:
        raise PromptResponseError(
            code="markup_parse_failed",
            message="Markup output did not contain any allowed highlight spans.",
        )
    return highlights


def neutral_highlight_fallback(student_answer: str) -> list[HighlightSpan]:
    """Return the documented neutral full-answer fallback span."""

    if not student_answer:
        return []
    return [
        HighlightSpan(
            kind="neutral",
            start_char=0,
            end_char=len(student_answer),
            text=student_answer,
        )
    ]


def synthetic_llm_trace_artifact(
    *,
    output_artifacts_dir: Path,
    ctx: CommandContext,
    step: str,
    scope: dict[str, object],
    prompt_id: str,
    prompt_variables: dict[str, str],
    response_raw: str | None,
    response_parsed: dict[str, object] | None,
) -> ArtifactReference:
    """Write a synthetic local trace artifact for model-free grading paths."""

    now = datetime.now(UTC)
    return write_trace_artifact(
        output_artifacts_dir=output_artifacts_dir,
        command=ctx.command,
        operation_id=ctx.operation_id,
        request_id=ctx.request_id,
        step=step,
        scope=scope,
        provider_capability="llm_provider",
        provider_name="local_only",
        prompt_id=prompt_id,
        prompt_variables=prompt_variables,
        prompt_rendered=None,
        request_options={},
        input_artifacts=[],
        response_raw=response_raw,
        response_parsed=response_parsed,
        timing=timing_info(started=now, finished=now),
    )


def consistency_error_warning(*, scope: dict[str, object], message: str) -> WarningObject:
    """Build the shared consistency invalid-output warning."""

    return warning(code="consistency_review_invalid_output", message=message, scope=scope)
