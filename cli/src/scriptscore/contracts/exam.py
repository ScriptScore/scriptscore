# SPDX-License-Identifier: AGPL-3.0-only
"""Exam-domain and Phase 3 command contract models."""

from __future__ import annotations

from pathlib import Path
from typing import Annotated, Any, Literal

from pydantic import AliasChoices, BaseModel, ConfigDict, Field, model_validator

from scriptscore.contracts.common import (
    LlmConfig,
    ProviderSelections,
    WarningObject,
    validate_llm_provider_selection,
)
from scriptscore.contracts.scans import Region
from scriptscore.paths import require_absolute_path, require_safe_path_component


class ExamQuestionConfig(BaseModel):
    """Question config row in exam.yaml."""

    model_config = ConfigDict(extra="allow")

    page: Annotated[int, Field(validation_alias=AliasChoices("page", "page_number"))]
    max_points: int
    question_text: str | None = Field(
        default=None,
        validation_alias=AliasChoices("question_text", "question_text_clean"),
    )
    question_id: str | None = None


class ExamConfig(BaseModel):
    """Frozen Phase 1 exam.yaml model."""

    model_config = ConfigDict(extra="allow")

    schema_version: str = "scriptscore.exam_config.v1"
    exam_name: str | None = None
    exam_title: str | None = None
    template_dir: str | None = None
    template_pdf: str | None = None
    questions: dict[int, ExamQuestionConfig] = Field(default_factory=dict)


class SetupQuestion(BaseModel):
    """One setup-derived template question row."""

    model_config = ConfigDict(extra="forbid")

    question_id: str
    question_number: int = Field(ge=1)
    page_number: int = Field(ge=1)
    baseline_pdf_text: str
    max_points: int = Field(gt=0)
    region: Region
    template_question_png_path: Path

    @model_validator(mode="after")
    def validate_fields(self) -> SetupQuestion:
        require_safe_path_component(self.question_id, field_name="question_id")
        self.template_question_png_path = require_absolute_path(
            self.template_question_png_path,
            field_name="template_question_png_path",
        )
        return self


class ExamSetupRequest(BaseModel):
    """Command request for exam.setup."""

    model_config = ConfigDict(extra="forbid")

    template_pdf_path: Path
    output_artifacts_dir: Path

    @model_validator(mode="after")
    def validate_paths(self) -> ExamSetupRequest:
        self.template_pdf_path = require_absolute_path(
            self.template_pdf_path, field_name="template_pdf_path"
        )
        self.output_artifacts_dir = require_absolute_path(
            self.output_artifacts_dir,
            field_name="output_artifacts_dir",
        )
        return self


class QuestionTarget(BaseModel):
    """One question-analysis input row."""

    model_config = ConfigDict(extra="forbid")

    question_id: str
    template_question_png_path: Path
    baseline_pdf_text: str

    @model_validator(mode="after")
    def validate_paths(self) -> QuestionTarget:
        require_safe_path_component(self.question_id, field_name="question_id")
        self.template_question_png_path = require_absolute_path(
            self.template_question_png_path,
            field_name="template_question_png_path",
        )
        return self


class QuestionAnalysisResult(BaseModel):
    """One question-analysis output row."""

    model_config = ConfigDict(extra="forbid")

    question_id: str
    status: Literal["ok", "error", "cancelled"]
    baseline_pdf_text: str
    question_text_clean: str | None = None
    question_context: str | None = None
    warnings: list[WarningObject] = Field(default_factory=list)


class ExamAnalyzeRequest(BaseModel):
    """Command request for exam.analyze."""

    model_config = ConfigDict(extra="forbid")

    question_targets: list[QuestionTarget] = Field(min_length=1)
    output_artifacts_dir: Path
    providers: ProviderSelections
    llm_config: LlmConfig

    @model_validator(mode="after")
    def validate_request(self) -> ExamAnalyzeRequest:
        self.output_artifacts_dir = require_absolute_path(
            self.output_artifacts_dir,
            field_name="output_artifacts_dir",
        )
        validate_llm_provider_selection(providers=self.providers, llm_config=self.llm_config)
        return self


class InstructorProfile(BaseModel):
    """Shared structured instructor-guidance object."""

    model_config = ConfigDict(extra="forbid")

    grading_strictness: Literal["strict", "balanced", "generous"] | None = None
    syntax_leniency: Literal["low", "medium", "high"] | None = None
    ocr_tolerance: Literal["low", "medium", "high"] | None = None
    partial_credit_style: Literal["strict", "balanced", "generous"] | None = None
    feedback_style: Literal["brief", "balanced", "detailed"] | None = None
    additional_guidance: str | None = None


class RubricCriterion(BaseModel):
    """One rubric criterion."""

    model_config = ConfigDict(extra="forbid")

    criterion_index: int | None = Field(default=None, ge=0)
    label: str
    points: int = Field(ge=0)
    partial_credit_guidance: str


class RubricDraft(BaseModel):
    """Typed rubric result returned on stdout."""

    model_config = ConfigDict(extra="forbid")

    question_id: str
    model_output: dict[str, Any] | None = None
    criteria: list[RubricCriterion]
    warnings: list[WarningObject] = Field(default_factory=list)


class ExamGenerateRubricRequest(BaseModel):
    """Command request for exam.generate-rubric."""

    model_config = ConfigDict(extra="forbid")

    question_id: str
    max_points: int = Field(gt=0)
    subject: str = Field(min_length=1)
    question_text_clean: str = Field(min_length=1)
    question_context: str
    instructor_profile: InstructorProfile
    # When true, the host adds a deterministic minimum-credit row after generation; skip local lint on LLM rows that echo that slice.
    host_prepends_minimum_credit_criterion: bool = False
    output_artifacts_dir: Path
    providers: ProviderSelections
    llm_config: LlmConfig

    @model_validator(mode="after")
    def validate_request(self) -> ExamGenerateRubricRequest:
        require_safe_path_component(self.question_id, field_name="question_id")
        self.output_artifacts_dir = require_absolute_path(
            self.output_artifacts_dir,
            field_name="output_artifacts_dir",
        )
        validate_llm_provider_selection(providers=self.providers, llm_config=self.llm_config)
        return self
