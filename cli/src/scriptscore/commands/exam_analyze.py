# SPDX-License-Identifier: AGPL-3.0-only
"""Implementation of `exam analyze`."""

from __future__ import annotations

from scriptscore.commands.common import batch_outcome, progress, warning
from scriptscore.commands.exam_shared import normalize_question_text
from scriptscore.commands.llm import execute_prompt_step, prompt_trace_artifact
from scriptscore.contracts import (
    ArtifactReference,
    ErrorCategory,
    ExamAnalyzeRequest,
    QuestionAnalysisResult,
    ScriptscoreError,
    WarningObject,
    WriteState,
)
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec


def handle_exam_analyze(ctx: CommandContext, request: ExamAnalyzeRequest) -> CommandOutcome:
    """Run per-question clean-text and context extraction."""

    provider_name = request.providers.llm_provider
    assert provider_name is not None
    ctx.provider_registry.resolve_llm(provider_name)
    for target in request.question_targets:
        if not target.template_question_png_path.exists():
            raise ScriptscoreError(
                code="template_question_not_found",
                message="One or more template question PNG artifacts were not found.",
                category=ErrorCategory.NOT_FOUND,
                retryable=True,
                details={"missing_path": str(target.template_question_png_path)},
                write_state=WriteState.NO_WRITE,
            )

    total = len(request.question_targets)
    ctx.emit(
        event="started",
        progress=progress(completed=0, total=total),
        data={"target_count": total, "total_stages": 1},
    )

    results: list[QuestionAnalysisResult] = []
    artifacts: list[ArtifactReference] = []
    failed_count = 0
    for index, target in enumerate(request.question_targets, start=1):
        ctx.check_cancelled()
        scope: dict[str, object] = {"question_id": target.question_id}
        ctx.emit(
            event="item_started",
            progress=progress(completed=index - 1, total=total),
            scope=scope,
        )

        row_warnings: list[WarningObject] = []
        if not target.baseline_pdf_text:
            row_warnings.append(
                warning(
                    code="baseline_pdf_text_missing",
                    message="Baseline PDF text was empty; question_text used the PNG as the primary source.",
                    scope=scope,
                )
            )

        question_text_attempt = execute_prompt_step(
            ctx,
            provider_name=provider_name,
            prompt_id="question_text",
            command_inputs={
                "template_question_png": str(target.template_question_png_path),
                "baseline_pdf_text": target.baseline_pdf_text,
            },
            llm_config=request.llm_config,
        )
        question_text_raw: str | None = None
        question_text_clean: str | None = None
        if question_text_attempt.execution is None:
            failed_count += 1
            row = QuestionAnalysisResult(
                question_id=target.question_id,
                status="error",
                baseline_pdf_text=target.baseline_pdf_text,
                warnings=[
                    *row_warnings,
                    warning(
                        code="question_text_failed",
                        message=str(question_text_attempt.error)
                        or "Question text extraction failed.",
                        scope=scope,
                    ),
                ],
            )
            artifacts.append(
                prompt_trace_artifact(
                    output_artifacts_dir=request.output_artifacts_dir,
                    ctx=ctx,
                    attempt=question_text_attempt,
                    step="analyze_question_text",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="question_text",
                    prompt_variables={"baseline_pdf_text": target.baseline_pdf_text},
                    input_artifacts=[str(target.template_question_png_path)],
                    response_parsed=None,
                )
            )
            results.append(row)
            ctx.emit(
                event="item_completed",
                progress=progress(completed=index, total=total),
                scope=scope,
                data={"status": "error"},
            )
            continue

        question_text_raw = question_text_attempt.execution.provider_response.raw_text
        question_text_clean = normalize_question_text(question_text_raw)
        artifacts.append(
            prompt_trace_artifact(
                output_artifacts_dir=request.output_artifacts_dir,
                ctx=ctx,
                attempt=question_text_attempt,
                step="analyze_question_text",
                scope=scope,
                provider_name=provider_name,
                prompt_id="question_text",
                prompt_variables={"baseline_pdf_text": target.baseline_pdf_text},
                input_artifacts=[str(target.template_question_png_path)],
                response_parsed={"question_text_clean": question_text_clean},
                response_raw=question_text_raw,
            )
        )

        question_context_attempt = execute_prompt_step(
            ctx,
            provider_name=provider_name,
            prompt_id="question_context",
            command_inputs={
                "template_question_png": str(target.template_question_png_path),
                "question_text_clean": question_text_clean,
            },
            llm_config=request.llm_config,
        )
        if question_context_attempt.execution is None:
            failed_count += 1
            row = QuestionAnalysisResult(
                question_id=target.question_id,
                status="error",
                baseline_pdf_text=target.baseline_pdf_text,
                question_text_clean=question_text_clean,
                warnings=[
                    *row_warnings,
                    warning(
                        code="question_context_failed",
                        message=str(question_context_attempt.error)
                        or "Question context extraction failed.",
                        scope=scope,
                    ),
                ],
            )
            artifacts.append(
                prompt_trace_artifact(
                    output_artifacts_dir=request.output_artifacts_dir,
                    ctx=ctx,
                    attempt=question_context_attempt,
                    step="analyze_question_context",
                    scope=scope,
                    provider_name=provider_name,
                    prompt_id="question_context",
                    prompt_variables={"question_text_clean": question_text_clean},
                    input_artifacts=[str(target.template_question_png_path)],
                    response_parsed=None,
                )
            )
            results.append(row)
            ctx.emit(
                event="item_completed",
                progress=progress(completed=index, total=total),
                scope=scope,
                data={"status": "error"},
            )
            continue

        question_context_raw = question_context_attempt.execution.provider_response.raw_text
        stripped = question_context_raw.strip()
        question_context = "" if stripped.lower() == "none" else stripped

        artifacts.append(
            prompt_trace_artifact(
                output_artifacts_dir=request.output_artifacts_dir,
                ctx=ctx,
                attempt=question_context_attempt,
                step="analyze_question_context",
                scope=scope,
                provider_name=provider_name,
                prompt_id="question_context",
                prompt_variables={"question_text_clean": question_text_clean},
                input_artifacts=[str(target.template_question_png_path)],
                response_parsed={"question_context": question_context},
                response_raw=question_context_raw,
            )
        )

        row = QuestionAnalysisResult(
            question_id=target.question_id,
            status="ok",
            baseline_pdf_text=target.baseline_pdf_text,
            question_text_clean=question_text_clean,
            question_context=question_context,
            warnings=row_warnings,
        )
        results.append(row)
        ctx.emit(
            event="item_completed",
            progress=progress(completed=index, total=total),
            scope=scope,
            data={"status": "ok"},
        )

    ctx.emit(
        event="completed",
        progress=progress(completed=total, total=total),
        data={"target_count": total, "failed_count": failed_count},
    )
    return batch_outcome(
        data={
            "question_results": [
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
        command_label="Question analysis",
        providers={"llm_provider": provider_name},
    )


def exam_analyze_spec() -> CommandSpec:
    return CommandSpec(
        name="exam.analyze", request_model=ExamAnalyzeRequest, handler=handle_exam_analyze
    )
