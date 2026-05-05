# SPDX-License-Identifier: AGPL-3.0-only
"""Grading-domain contract models for Phase 4 commands."""

from __future__ import annotations

from pathlib import Path
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator

from scriptscore.contracts.common import (
    ConfidenceBucket,
    LlmConfig,
    ProviderSelections,
    WarningObject,
    validate_llm_provider_selection,
)
from scriptscore.contracts.exam import InstructorProfile, RubricCriterion
from scriptscore.paths import require_absolute_path, require_safe_path_component


def _require_indexed_rubric_criterion(criterion: RubricCriterion, *, field_name: str) -> None:
    if criterion.criterion_index is None:
        raise ValueError(f"{field_name}.criterion_index is required.")


def _question_row_context(*, question_id: str | None = None, student_ref: str | None = None) -> str:
    details: list[str] = []
    if question_id is not None:
        details.append(f"question_id={question_id!r}")
    if student_ref is not None:
        details.append(f"student_ref={student_ref!r}")
    return "" if not details else f" for {', '.join(details)}"


def _validate_question_grading_alignment(
    *,
    rubric_criteria: list[RubricCriterion],
    criterion_results: list[FeedbackCriterionResult],
    total_points_awarded: int,
    question_max_points: int,
    question_id: str | None = None,
    student_ref: str | None = None,
) -> None:
    context = _question_row_context(question_id=question_id, student_ref=student_ref)
    if total_points_awarded > question_max_points:
        raise ValueError(
            f"total_points_awarded={total_points_awarded} must be within [0, question_max_points={question_max_points}]{context}."
        )

    rubric_indexes: list[int] = []
    for criterion in rubric_criteria:
        _require_indexed_rubric_criterion(criterion, field_name="rubric_criteria[]")
        assert criterion.criterion_index is not None
        rubric_indexes.append(criterion.criterion_index)

    if len(set(rubric_indexes)) != len(rubric_indexes):
        raise ValueError(f"rubric_criteria[].criterion_index values must be unique{context}.")

    result_indexes = [result.criterion_index for result in criterion_results]
    if rubric_indexes != result_indexes:
        raise ValueError(
            f"criterion_results must cover rubric_criteria exactly once in the same order{context}."
        )

    for criterion, result in zip(rubric_criteria, criterion_results, strict=True):
        if result.points_awarded > criterion.points:
            raise ValueError(
                "criterion_results[*].points_awarded must be within "
                f"[0, matching rubric_criteria[{criterion.criterion_index}].points={criterion.points}]{context}."
            )

    if sum(result.points_awarded for result in criterion_results) != total_points_awarded:
        raise ValueError(
            f"criterion_results points must sum exactly to total_points_awarded{context}."
        )


class PreliminaryScoreRequest(BaseModel):
    """One criterion-scoped preliminary scoring request row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str = Field(min_length=1)
    question_id: str = Field(min_length=1)
    subject: str = Field(min_length=1)
    student_answer: str
    question_text_clean: str = Field(min_length=1)
    question_context: str
    rubric_criterion: RubricCriterion
    instructor_profile: InstructorProfile

    @model_validator(mode="after")
    def validate_fields(self) -> PreliminaryScoreRequest:
        require_safe_path_component(self.student_ref, field_name="student_ref")
        require_safe_path_component(self.question_id, field_name="question_id")
        _require_indexed_rubric_criterion(self.rubric_criterion, field_name="rubric_criterion")
        return self


class PreliminaryAnswerScoreRequest(BaseModel):
    """One answer-scoped preliminary scoring request row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str = Field(min_length=1)
    question_id: str = Field(min_length=1)
    subject: str = Field(min_length=1)
    student_answer: str
    question_text_clean: str = Field(min_length=1)
    question_context: str
    rubric_criteria: list[RubricCriterion] = Field(min_length=1)
    instructor_profile: InstructorProfile

    @model_validator(mode="after")
    def validate_fields(self) -> PreliminaryAnswerScoreRequest:
        require_safe_path_component(self.student_ref, field_name="student_ref")
        require_safe_path_component(self.question_id, field_name="question_id")
        seen_indexes: set[int] = set()
        for index, criterion in enumerate(self.rubric_criteria):
            _require_indexed_rubric_criterion(criterion, field_name=f"rubric_criteria[{index}]")
            assert criterion.criterion_index is not None
            if criterion.criterion_index in seen_indexes:
                raise ValueError(
                    "rubric_criteria must not contain duplicate criterion_index values."
                )
            seen_indexes.add(criterion.criterion_index)
        return self


class PreliminaryScoreResult(BaseModel):
    """One criterion-scoped preliminary scoring result row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str
    question_id: str
    criterion_index: int = Field(ge=0)
    blank: bool
    points_awarded: int = Field(ge=0)
    rationale: str
    status: Literal["ok", "degraded_parse_error", "error", "cancelled"]
    confidence: ConfidenceBucket | None = None
    confidence_reason: str | None = None
    warnings: list[WarningObject] = Field(default_factory=list)


class GradingRuntimeConfig(BaseModel):
    """Execution controls for preliminary grading runtime behavior."""

    model_config = ConfigDict(extra="forbid")

    max_workers: int = Field(default=1, ge=1, le=4)


class GradingScorePreliminaryRequest(BaseModel):
    """Command request for grading.score-preliminary."""

    model_config = ConfigDict(extra="forbid")

    score_requests: list[PreliminaryScoreRequest] | None = None
    answer_score_requests: list[PreliminaryAnswerScoreRequest] | None = None
    grading_runtime_config: GradingRuntimeConfig = Field(default_factory=GradingRuntimeConfig)
    output_artifacts_dir: Path
    providers: ProviderSelections
    llm_config: LlmConfig

    @model_validator(mode="after")
    def validate_request(self) -> GradingScorePreliminaryRequest:
        self.output_artifacts_dir = require_absolute_path(
            self.output_artifacts_dir,
            field_name="output_artifacts_dir",
        )
        if bool(self.score_requests) == bool(self.answer_score_requests):
            raise ValueError("Provide exactly one of score_requests or answer_score_requests.")
        validate_llm_provider_selection(providers=self.providers, llm_config=self.llm_config)
        return self


class ConsistencyStudentScore(BaseModel):
    """One student row inside a criterion-wide consistency review request."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str = Field(min_length=1)
    student_answer: str
    blank: bool
    preliminary_points_awarded: int = Field(ge=0)
    preliminary_rationale: str
    preliminary_status: Literal["ok", "degraded_parse_error", "error", "cancelled"]
    warnings: list[WarningObject] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_fields(self) -> ConsistencyStudentScore:
        require_safe_path_component(self.student_ref, field_name="student_ref")
        return self


class ConsistencyRequest(BaseModel):
    """One criterion-wide consistency review request row."""

    model_config = ConfigDict(extra="forbid")

    question_id: str = Field(min_length=1)
    subject: str = Field(min_length=1)
    question_text_clean: str = Field(min_length=1)
    question_context: str
    rubric_criterion: RubricCriterion
    instructor_profile: InstructorProfile
    student_scores: list[ConsistencyStudentScore] = Field(min_length=1)

    @model_validator(mode="after")
    def validate_fields(self) -> ConsistencyRequest:
        require_safe_path_component(self.question_id, field_name="question_id")
        _require_indexed_rubric_criterion(self.rubric_criterion, field_name="rubric_criterion")
        return self


class ConsistencyAdjustment(BaseModel):
    """One sparse criterion adjustment returned by consistency review."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str = Field(min_length=1)
    question_id: str = Field(min_length=1)
    criterion_index: int = Field(ge=0)
    points_awarded: int = Field(ge=0)
    adjustment_reason: str
    warnings: list[WarningObject] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_fields(self) -> ConsistencyAdjustment:
        require_safe_path_component(self.student_ref, field_name="student_ref")
        require_safe_path_component(self.question_id, field_name="question_id")
        return self


class ConsistencyReview(BaseModel):
    """One criterion-wide consistency review result row."""

    model_config = ConfigDict(extra="forbid")

    question_id: str
    criterion_index: int = Field(ge=0)
    status: Literal["ok", "error", "cancelled"]
    adjustments: list[ConsistencyAdjustment] = Field(default_factory=list)
    warnings: list[WarningObject] = Field(default_factory=list)


class GradingRunConsistencyRequest(BaseModel):
    """Command request for grading.run-consistency."""

    model_config = ConfigDict(extra="forbid")

    consistency_requests: list[ConsistencyRequest] = Field(min_length=1)
    output_artifacts_dir: Path
    providers: ProviderSelections
    llm_config: LlmConfig

    @model_validator(mode="after")
    def validate_request(self) -> GradingRunConsistencyRequest:
        self.output_artifacts_dir = require_absolute_path(
            self.output_artifacts_dir,
            field_name="output_artifacts_dir",
        )
        validate_llm_provider_selection(providers=self.providers, llm_config=self.llm_config)
        return self


class FeedbackCriterionResult(BaseModel):
    """One final criterion-level grading outcome row."""

    model_config = ConfigDict(extra="forbid")

    criterion_index: int = Field(ge=0)
    points_awarded: int = Field(ge=0)
    rationale: str


class FeedbackRequest(BaseModel):
    """One student/question feedback request row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str = Field(min_length=1)
    question_id: str = Field(min_length=1)
    subject: str = Field(min_length=1)
    total_points_awarded: int = Field(ge=0)
    question_max_points: int = Field(gt=0)
    student_answer: str
    question_text_clean: str = Field(min_length=1)
    question_context: str
    rubric_criteria: list[RubricCriterion] = Field(min_length=1)
    criterion_results: list[FeedbackCriterionResult] = Field(min_length=1)

    @model_validator(mode="after")
    def validate_fields(self) -> FeedbackRequest:
        require_safe_path_component(self.student_ref, field_name="student_ref")
        require_safe_path_component(self.question_id, field_name="question_id")
        _validate_question_grading_alignment(
            rubric_criteria=self.rubric_criteria,
            criterion_results=self.criterion_results,
            total_points_awarded=self.total_points_awarded,
            question_max_points=self.question_max_points,
            question_id=self.question_id,
            student_ref=self.student_ref,
        )
        return self


class FeedbackDraft(BaseModel):
    """One final feedback draft row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str
    question_id: str
    feedback_source: Literal["model", "blank_local", "default_fallback"]
    feedback_text: str
    warnings: list[WarningObject] = Field(default_factory=list)


class GradingDraftFeedbackRequest(BaseModel):
    """Command request for grading.draft-feedback."""

    model_config = ConfigDict(extra="forbid")

    feedback_requests: list[FeedbackRequest] = Field(min_length=1)
    output_artifacts_dir: Path
    providers: ProviderSelections
    llm_config: LlmConfig

    @model_validator(mode="after")
    def validate_request(self) -> GradingDraftFeedbackRequest:
        self.output_artifacts_dir = require_absolute_path(
            self.output_artifacts_dir,
            field_name="output_artifacts_dir",
        )
        validate_llm_provider_selection(providers=self.providers, llm_config=self.llm_config)
        return self


class HighlightSpan(BaseModel):
    """One parsed moderation-highlight span."""

    model_config = ConfigDict(extra="forbid")

    kind: Literal["correct", "incorrect", "neutral"]
    start_char: int = Field(ge=0)
    end_char: int = Field(gt=0)
    text: str

    @model_validator(mode="after")
    def validate_range(self) -> HighlightSpan:
        if self.end_char <= self.start_char:
            raise ValueError("highlight spans must have end_char > start_char.")
        if not self.text:
            raise ValueError("highlight spans must include non-empty text.")
        return self


class HighlightResult(BaseModel):
    """One final markup/highlight result row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str
    question_id: str
    status: Literal["ok", "fallback"]
    highlights: list[HighlightSpan] = Field(default_factory=list)
    warnings: list[WarningObject] = Field(default_factory=list)


class MarkupRequest(BaseModel):
    """One student/question markup request row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str = Field(min_length=1)
    question_id: str = Field(min_length=1)
    subject: str = Field(min_length=1)
    total_points_awarded: int = Field(ge=0)
    question_max_points: int = Field(gt=0)
    student_answer: str
    question_text_clean: str = Field(min_length=1)
    question_context: str
    rubric_criteria: list[RubricCriterion] = Field(min_length=1)
    criterion_results: list[FeedbackCriterionResult] = Field(min_length=1)

    @model_validator(mode="after")
    def validate_fields(self) -> MarkupRequest:
        require_safe_path_component(self.student_ref, field_name="student_ref")
        require_safe_path_component(self.question_id, field_name="question_id")
        _validate_question_grading_alignment(
            rubric_criteria=self.rubric_criteria,
            criterion_results=self.criterion_results,
            total_points_awarded=self.total_points_awarded,
            question_max_points=self.question_max_points,
            question_id=self.question_id,
            student_ref=self.student_ref,
        )
        return self


class GradingMarkupRequest(BaseModel):
    """Command request for grading.markup."""

    model_config = ConfigDict(extra="forbid")

    markup_requests: list[MarkupRequest] = Field(min_length=1)
    output_artifacts_dir: Path
    providers: ProviderSelections
    llm_config: LlmConfig

    @model_validator(mode="after")
    def validate_request(self) -> GradingMarkupRequest:
        self.output_artifacts_dir = require_absolute_path(
            self.output_artifacts_dir,
            field_name="output_artifacts_dir",
        )
        validate_llm_provider_selection(providers=self.providers, llm_config=self.llm_config)
        return self


class ExportQuestion(BaseModel):
    """One question block inside a per-student export request."""

    model_config = ConfigDict(extra="forbid")

    question_id: str = Field(min_length=1)
    question_max_points: int = Field(gt=0)
    total_points_awarded: int = Field(ge=0)
    question_text_clean: str = Field(min_length=1)
    student_answer: str
    question_crop_path: Path
    feedback_text: str | None = None
    highlights: list[HighlightSpan] | None = None

    @model_validator(mode="after")
    def validate_fields(self) -> ExportQuestion:
        require_safe_path_component(self.question_id, field_name="question_id")
        self.question_crop_path = require_absolute_path(
            self.question_crop_path,
            field_name="question_crop_path",
        )
        if self.total_points_awarded > self.question_max_points:
            raise ValueError(
                "total_points_awarded="
                f"{self.total_points_awarded} must be within [0, question_max_points={self.question_max_points}]"
                f"{_question_row_context(question_id=self.question_id)}."
            )
        if self.highlights is not None:
            last_end = 0
            for highlight in self.highlights:
                if highlight.end_char > len(self.student_answer):
                    raise ValueError("highlight spans must stay within student_answer.")
                if self.student_answer[highlight.start_char : highlight.end_char] != highlight.text:
                    raise ValueError(
                        "highlight span text must match the referenced student_answer substring."
                    )
                if highlight.start_char < last_end:
                    raise ValueError("highlight spans must not overlap.")
                last_end = highlight.end_char
        return self


class ExportRequest(BaseModel):
    """One per-student export request row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str = Field(min_length=1)
    student_display_name: str | None = None
    questions: list[ExportQuestion] = Field(min_length=1)

    @model_validator(mode="after")
    def validate_fields(self) -> ExportRequest:
        require_safe_path_component(self.student_ref, field_name="student_ref")
        question_ids = [question.question_id for question in self.questions]
        if len(set(question_ids)) != len(question_ids):
            raise ValueError("export_request.questions[*].question_id values must be unique.")
        return self


class ExportResult(BaseModel):
    """One per-student export result row."""

    model_config = ConfigDict(extra="forbid")

    student_ref: str
    html_path: Path
    warnings: list[WarningObject] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_paths(self) -> ExportResult:
        require_safe_path_component(self.student_ref, field_name="student_ref")
        self.html_path = require_absolute_path(self.html_path, field_name="html_path")
        return self


class GradingExportRequest(BaseModel):
    """Command request for grading.export."""

    model_config = ConfigDict(extra="forbid")

    export_requests: list[ExportRequest] = Field(min_length=1)
    output_artifacts_dir: Path

    @model_validator(mode="after")
    def validate_request(self) -> GradingExportRequest:
        self.output_artifacts_dir = require_absolute_path(
            self.output_artifacts_dir,
            field_name="output_artifacts_dir",
        )
        student_refs = [request.student_ref for request in self.export_requests]
        if len(set(student_refs)) != len(student_refs):
            raise ValueError(
                "export_requests[*].student_ref values must be unique within a grading.export request."
            )
        return self
