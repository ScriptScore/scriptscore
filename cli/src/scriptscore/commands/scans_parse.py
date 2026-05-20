# SPDX-License-Identifier: AGPL-3.0-only
"""Implementation of `scans parse`."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Literal

from scriptscore.artifacts import write_trace_artifact
from scriptscore.commands.common import batch_outcome, progress, timing_info, warning
from scriptscore.commands.llm import execute_prompt_step, prompt_trace_artifact
from scriptscore.commands.scans_shared import (
    HandwritingVerifyPayload,
    ensure_paths_exist,
    render_parse_question_context_xml,
)
from scriptscore.contracts import (
    ArtifactReference,
    ConfidenceBucket,
    ParseDraft,
    ParseTarget,
    ScansParseRequest,
    WarningObject,
)
from scriptscore.prompts import parse_json_model
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec

COMMAND_NAME = "scans.parse"
PII_PROVIDER_NAME = "scans.pii"


@dataclass(frozen=True)
class PrescreenOutcome:
    """Normalized result of the handwriting-prescreen step."""

    payload: HandwritingVerifyPayload | None
    source: Literal["handwriting_verify", "pii_prescreen"]
    warnings: list[WarningObject]
    trace_artifact: ArtifactReference


@dataclass(frozen=True)
class OcrOutcome:
    """Normalized result of the OCR step."""

    result: ParseDraft
    status: Literal["ok", "error", "blank"]
    failed: bool
    trace_artifact: ArtifactReference


ParseConfidenceSource = Literal["handwriting_verify", "pii_prescreen", "ocr_parse", "combined"]


def _parse_confidence(
    payload: HandwritingVerifyPayload | None,
    *,
    prescreen_source: Literal["handwriting_verify", "pii_prescreen"],
    ocr_status: Literal["ok", "error", "blank"] | None,
    row_warnings: list[WarningObject],
) -> tuple[ConfidenceBucket | None, ParseConfidenceSource | None]:
    """Derive the row-level parse confidence and its source."""

    warning_codes = {item.code for item in row_warnings}
    if "handwriting_verify_low_confidence" in warning_codes:
        return "low", "combined"
    if "handwriting_verify_error_status" in warning_codes:
        return "low", "combined"
    if ocr_status == "error":
        return None, None
    if payload is not None and payload.status == "complete":
        if ocr_status == "blank" and not payload.has_handwriting:
            return payload.confidence, prescreen_source
        return payload.confidence, "combined"
    return "high", "ocr_parse"


def _run_handwriting_prescreen(
    ctx: CommandContext,
    *,
    request: ScansParseRequest,
    provider_name: str,
    question_crop_path: Path,
    scope: dict[str, object],
) -> PrescreenOutcome:
    """Run handwriting verification and normalize warnings and trace output."""

    attempt = execute_prompt_step(
        ctx,
        provider_name=provider_name,
        prompt_id="handwriting_verify",
        command_inputs={"question_crop_png": str(question_crop_path)},
        llm_config=request.llm_config,
    )
    response_raw: str | None = None
    response_parsed: dict[str, object] | None = None
    payload: HandwritingVerifyPayload | None = None
    warnings: list[WarningObject] = []
    if attempt.execution is None:
        warnings.append(
            warning(
                code="handwriting_verify_failed",
                message=str(attempt.error) or "Handwriting prescreen failed.",
                scope=scope,
            )
        )
    else:
        response_raw = attempt.execution.provider_response.raw_text
        try:
            payload = parse_json_model(response_raw, HandwritingVerifyPayload)
            response_parsed = payload.model_dump(mode="json")
        except Exception as exc:
            warnings.append(
                warning(
                    code="handwriting_verify_invalid",
                    message=str(exc) or "Handwriting prescreen response was invalid.",
                    scope=scope,
                )
            )

    return PrescreenOutcome(
        payload=payload,
        source="handwriting_verify",
        warnings=warnings,
        trace_artifact=prompt_trace_artifact(
            output_artifacts_dir=request.output_artifacts_dir,
            ctx=ctx,
            attempt=attempt,
            step="handwriting_verify",
            scope=scope,
            provider_name=provider_name,
            prompt_id="handwriting_verify",
            prompt_variables={},
            input_artifacts=[str(question_crop_path)],
            response_parsed=response_parsed,
            response_raw=response_raw,
        ),
    )


def _pii_prescreen_reuse(
    ctx: CommandContext,
    *,
    request: ScansParseRequest,
    target: ParseTarget,
) -> PrescreenOutcome | None:
    """Return a reusable upstream PII prescreen when it is explicit and clean."""

    prescreen = target.pii_prescreen
    if prescreen is None:
        return None
    if prescreen.status != "ok":
        return None
    if prescreen.contains_pii:
        return None
    if prescreen.contains_handwriting == "unknown":
        return None

    payload = HandwritingVerifyPayload(
        has_handwriting=prescreen.contains_handwriting == "true",
        confidence="high",
        status="complete",
    )
    finished = datetime.now(UTC)
    started = finished - timedelta(milliseconds=1)
    trace_artifact = write_trace_artifact(
        output_artifacts_dir=request.output_artifacts_dir,
        command=COMMAND_NAME,
        operation_id=ctx.operation_id,
        request_id=ctx.request_id,
        step="pii_prescreen",
        scope={"student_ref": target.student_ref, "question_id": target.question_id},
        provider_capability="local_runtime",
        provider_name=PII_PROVIDER_NAME,
        request_options={
            "source_command": prescreen.source_command,
            "contains_handwriting": prescreen.contains_handwriting,
            "contains_pii": prescreen.contains_pii,
        },
        input_artifacts=[str(target.question_crop_path)],
        response_parsed=prescreen.model_dump(mode="json", exclude_none=True),
        timing=timing_info(started=started, finished=finished),
    )
    return PrescreenOutcome(
        payload=payload,
        source="pii_prescreen",
        warnings=[],
        trace_artifact=trace_artifact,
    )


def _prescreen_implies_blank(payload: HandwritingVerifyPayload | None) -> bool:
    """Return whether OCR should be skipped and the row treated as blank."""

    return (
        payload is not None
        and payload.status == "complete"
        and not payload.has_handwriting
        and payload.confidence in {"medium", "high"}
    )


def _prescreen_followup_warnings(
    payload: HandwritingVerifyPayload | None,
    *,
    scope: dict[str, object],
) -> list[WarningObject]:
    """Return follow-up OCR warnings implied by the prescreen result."""

    warnings: list[WarningObject] = []
    if payload is not None and payload.confidence == "low":
        warnings.append(
            warning(
                code="handwriting_verify_low_confidence",
                message="Handwriting prescreen returned low confidence; OCR proceeded anyway.",
                scope=scope,
            )
        )
    if payload is not None and payload.status == "error":
        warnings.append(
            warning(
                code="handwriting_verify_error_status",
                message="Handwriting prescreen returned status=error; OCR proceeded anyway.",
                scope=scope,
            )
        )
    return warnings


def _prescreen_soft_degrade_warnings(
    *,
    low_confidence_count: int,
    error_status_count: int,
) -> list[WarningObject]:
    """Return top-level warnings for documented soft-degraded prescreen outcomes."""

    warnings: list[WarningObject] = []
    if low_confidence_count:
        noun = "row" if low_confidence_count == 1 else "rows"
        warnings.append(
            warning(
                code="handwriting_verify_low_confidence",
                message=(
                    "Handwriting prescreen returned low confidence for "
                    f"{low_confidence_count} parse {noun}; OCR still ran."
                ),
                scope={"row_count": low_confidence_count},
            )
        )
    if error_status_count:
        noun = "row" if error_status_count == 1 else "rows"
        warnings.append(
            warning(
                code="handwriting_verify_error_status",
                message=(
                    "Handwriting prescreen returned status=error for "
                    f"{error_status_count} parse {noun}; OCR still ran."
                ),
                scope={"row_count": error_status_count},
            )
        )
    return warnings


def _run_parse_ocr(
    ctx: CommandContext,
    *,
    request: ScansParseRequest,
    provider_name: str,
    target: ParseTarget,
    scope: dict[str, object],
    prescreen_source: Literal["handwriting_verify", "pii_prescreen"],
    row_warnings: list[WarningObject],
) -> OcrOutcome:
    """Run OCR and normalize the row result and trace output."""

    attempt = execute_prompt_step(
        ctx,
        provider_name=provider_name,
        prompt_id="parse_ocr",
        command_inputs={
            "parse_question_context_xml": render_parse_question_context_xml(
                target.parse_question_context
            ),
            "question_crop_png": str(target.question_crop_path),
            "template_question_png": str(target.template_question_png_path),
        },
        llm_config=request.llm_config,
    )
    response_raw: str | None = None
    response_parsed: dict[str, object] | None = None
    if attempt.execution is None:
        confidence, confidence_source = _parse_confidence(
            None,
            prescreen_source=prescreen_source,
            ocr_status="error",
            row_warnings=row_warnings,
        )
        result = ParseDraft(
            student_ref=target.student_ref,
            question_id=target.question_id,
            status="error",
            blank=False,
            confidence=confidence,
            confidence_source=confidence_source,
            warnings=[
                *row_warnings,
                warning(
                    code="parse_ocr_failed",
                    message=str(attempt.error) or "OCR execution failed.",
                    scope=scope,
                ),
            ],
        )
        status: Literal["ok", "error", "blank"] = "error"
        failed = True
    else:
        response_raw = attempt.execution.provider_response.raw_text
        normalized_text = response_raw.strip()
        failed = False
        if normalized_text == "[blank]":
            confidence, confidence_source = _parse_confidence(
                None,
                prescreen_source=prescreen_source,
                ocr_status="blank",
                row_warnings=row_warnings,
            )
            result = ParseDraft(
                student_ref=target.student_ref,
                question_id=target.question_id,
                status="blank",
                parsed_text="",
                blank=True,
                confidence=confidence,
                confidence_source=confidence_source,
                warnings=row_warnings,
            )
            status = "blank"
            response_parsed = result.model_dump(mode="json", exclude_none=True)
        else:
            confidence, confidence_source = _parse_confidence(
                None,
                prescreen_source=prescreen_source,
                ocr_status="ok",
                row_warnings=row_warnings,
            )
            result = ParseDraft(
                student_ref=target.student_ref,
                question_id=target.question_id,
                status="ok",
                parsed_text=normalized_text,
                blank=False,
                confidence=confidence,
                confidence_source=confidence_source,
                warnings=row_warnings,
            )
            status = "ok"
            response_parsed = result.model_dump(mode="json", exclude_none=True)

    return OcrOutcome(
        result=result,
        status=status,
        failed=failed,
        trace_artifact=prompt_trace_artifact(
            output_artifacts_dir=request.output_artifacts_dir,
            ctx=ctx,
            attempt=attempt,
            step="parse_ocr",
            scope=scope,
            provider_name=provider_name,
            prompt_id="parse_ocr",
            prompt_variables={},
            input_artifacts=[
                str(target.question_crop_path),
                str(target.template_question_png_path),
            ],
            response_parsed=response_parsed,
            response_raw=response_raw,
        ),
    )


def handle_scans_parse(ctx: CommandContext, request: ScansParseRequest) -> CommandOutcome:
    """Build parse drafts from question crops using handwriting prescreen and OCR."""

    provider_name = request.providers.llm_provider
    assert provider_name is not None
    ctx.provider_registry.resolve_llm(provider_name)
    input_paths = [target.question_crop_path for target in request.parse_targets] + [
        target.template_question_png_path for target in request.parse_targets
    ]
    ensure_paths_exist(input_paths, command=COMMAND_NAME)

    total = len(request.parse_targets)
    ctx.emit(
        event="started",
        progress=progress(completed=0, total=total),
        data={"target_count": total, "total_stages": 2},
    )

    results: list[ParseDraft] = []
    artifacts: list[ArtifactReference] = []
    failed_count = 0
    low_confidence_count = 0
    error_status_count = 0
    prescreen_rows: list[tuple[ParseTarget, dict[str, object], PrescreenOutcome]] = []
    ctx.emit(
        event="stage_started",
        data={"stage_number": 1, "stage": "prescreen", "target_count": total},
    )
    for index, target in enumerate(request.parse_targets, start=1):
        ctx.check_cancelled()
        scope: dict[str, object] = {
            "student_ref": target.student_ref,
            "question_id": target.question_id,
        }
        reused = _pii_prescreen_reuse(ctx, request=request, target=target)
        prescreen = reused or _run_handwriting_prescreen(
            ctx,
            request=request,
            provider_name=provider_name,
            question_crop_path=target.question_crop_path,
            scope=scope,
        )
        artifacts.append(prescreen.trace_artifact)
        prescreen_rows.append((target, scope, prescreen))
        ctx.emit(
            event="stage_progress",
            progress=progress(completed=index, total=total),
            scope=scope,
            data={"stage": prescreen.source},
        )

    ctx.emit(
        event="stage_started", data={"stage_number": 2, "stage": "parse_ocr", "target_count": total}
    )
    for index, (target, scope, prescreen) in enumerate(prescreen_rows, start=1):
        ctx.check_cancelled()
        ctx.emit(
            event="item_started",
            progress=progress(completed=index - 1, total=total),
            scope=scope,
            data={"stage": "parse_ocr"},
        )
        row_warnings = [
            *prescreen.warnings,
            *_prescreen_followup_warnings(prescreen.payload, scope=scope),
        ]
        if any(item.code == "handwriting_verify_low_confidence" for item in row_warnings):
            low_confidence_count += 1
        if any(item.code == "handwriting_verify_error_status" for item in row_warnings):
            error_status_count += 1

        if _prescreen_implies_blank(prescreen.payload):
            confidence, confidence_source = _parse_confidence(
                prescreen.payload,
                prescreen_source=prescreen.source,
                ocr_status="blank",
                row_warnings=row_warnings,
            )
            result = ParseDraft(
                student_ref=target.student_ref,
                question_id=target.question_id,
                status="blank",
                parsed_text="",
                blank=True,
                confidence=confidence,
                confidence_source=confidence_source,
                warnings=row_warnings,
            )
            status: Literal["ok", "error", "blank"] = "blank"
            results.append(result)
            ctx.emit(
                event="item_completed",
                progress=progress(completed=index, total=total),
                scope=scope,
                data={"status": status, "stage": f"{prescreen.source}_blank"},
            )
            continue

        ocr = _run_parse_ocr(
            ctx,
            request=request,
            provider_name=provider_name,
            target=target,
            scope=scope,
            prescreen_source=prescreen.source,
            row_warnings=row_warnings,
        )
        confidence, confidence_source = _parse_confidence(
            prescreen.payload,
            prescreen_source=prescreen.source,
            ocr_status=ocr.status,
            row_warnings=row_warnings,
        )
        ocr.result.confidence = confidence
        ocr.result.confidence_source = confidence_source
        if ocr.failed:
            failed_count += 1
        artifacts.append(ocr.trace_artifact)
        results.append(ocr.result)
        ctx.emit(
            event="item_completed",
            progress=progress(completed=index, total=total),
            scope=scope,
            data={"status": ocr.status, "stage": "parse_ocr"},
        )

    ctx.emit(
        event="completed",
        progress=progress(completed=total, total=total),
        data={"target_count": total, "failed_count": failed_count},
    )
    return batch_outcome(
        data={
            "parse_results": [
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
        command_label="Parse",
        providers={"llm_provider": provider_name},
        extra_warnings=_prescreen_soft_degrade_warnings(
            low_confidence_count=low_confidence_count,
            error_status_count=error_status_count,
        ),
    )


def scans_parse_spec() -> CommandSpec:
    return CommandSpec(
        name=COMMAND_NAME, request_model=ScansParseRequest, handler=handle_scans_parse
    )
