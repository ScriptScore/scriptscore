# SPDX-License-Identifier: AGPL-3.0-only
"""Implementation of `scans align-auto`."""

from __future__ import annotations

from scriptscore.artifacts import load_page_image, transformed_visible_content_clip_report
from scriptscore.commands.common import batch_outcome, batch_result_data, progress, warning
from scriptscore.commands.scans_shared import ensure_paths_exist
from scriptscore.contracts import (
    AlignmentResult,
    ErrorCategory,
    ScansAlignAutoRequest,
    ScriptscoreError,
    Transform,
)
from scriptscore.providers import AlignmentRequest
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec


def handle_scans_align_auto(ctx: CommandContext, request: ScansAlignAutoRequest) -> CommandOutcome:
    """Compute auto-alignment proposals for explicit student pages."""

    provider_name = request.providers.alignment_engine
    assert provider_name is not None
    provider = ctx.provider_registry.resolve_alignment(provider_name)
    all_input_paths = [page.image_path for page in request.template_pages]
    all_input_paths.extend(page.image_path for page in request.student_pages)
    ensure_paths_exist(
        all_input_paths,
        command="scans.align-auto",
    )

    total = len(request.student_pages)
    template_pages_by_number = {page.page_number: page for page in request.template_pages}
    ctx.emit(
        event="started",
        progress=progress(completed=0, total=total),
        data={"target_count": total, "total_stages": 1},
    )

    results: list[AlignmentResult] = []
    failed_count = 0
    marker_fallback_count = 0
    for index, student_page in enumerate(request.student_pages, start=1):
        ctx.check_cancelled()
        assert student_page.student_ref is not None
        scope: dict[str, object] = {
            "student_ref": student_page.student_ref,
            "page_number": student_page.page_number,
        }
        ctx.emit(
            event="item_started",
            progress=progress(completed=index - 1, total=total),
            scope=scope,
        )

        template_page = template_pages_by_number.get(student_page.page_number)
        if template_page is None:
            failed_count += 1
            result = AlignmentResult(
                student_ref=student_page.student_ref,
                page_number=student_page.page_number,
                status="failed",
                warnings=[
                    warning(
                        code="template_page_missing",
                        message="No matching template page was supplied for this student page number.",
                        scope=scope,
                    )
                ],
            )
        else:
            try:
                provider_result = provider.align(
                    AlignmentRequest(
                        template_page_path=str(template_page.image_path),
                        student_page_path=str(student_page.image_path),
                        mode=request.mode,
                        marker_mode=request.marker_mode,
                    )
                )
            except ScriptscoreError as exc:
                if exc.category in {ErrorCategory.CANCELLED, ErrorCategory.PROVIDER}:
                    raise
                failed_count += 1
                result = AlignmentResult(
                    student_ref=student_page.student_ref,
                    page_number=student_page.page_number,
                    status="failed",
                    warnings=[
                        warning(
                            code=exc.code,
                            message=exc.message,
                            scope=scope,
                        )
                    ],
                )
            except Exception as exc:
                failed_count += 1
                result = AlignmentResult(
                    student_ref=student_page.student_ref,
                    page_number=student_page.page_number,
                    status="failed",
                    warnings=[
                        warning(
                            code="alignment_failed",
                            message=str(exc) or "Alignment execution failed.",
                            scope=scope,
                        )
                    ],
                )
            else:
                if provider_result.status in {"ok", "low_confidence"}:
                    rotation = provider_result.rotation
                    scale = provider_result.scale
                    translate_x = provider_result.translate_x
                    translate_y = provider_result.translate_y
                    if None in {rotation, scale, translate_x, translate_y}:
                        failed_count += 1
                        result = AlignmentResult(
                            student_ref=student_page.student_ref,
                            page_number=student_page.page_number,
                            status="failed",
                            warnings=[
                                warning(
                                    code="alignment_response_invalid",
                                    message="Alignment provider returned an incomplete transform proposal.",
                                    scope=scope,
                                )
                            ],
                        )
                    else:
                        assert rotation is not None
                        assert scale is not None
                        assert translate_x is not None
                        assert translate_y is not None
                        transform = Transform(
                            rotation=rotation,
                            scale=scale,
                            translate_x=translate_x,
                            translate_y=translate_y,
                        )
                        result = AlignmentResult(
                            student_ref=student_page.student_ref,
                            page_number=student_page.page_number,
                            status=provider_result.status,
                            confidence=provider_result.confidence,
                            transform=transform,
                            warnings=provider_result.warnings,
                        )
                        template_image = load_page_image(template_page.image_path)
                        clip_report = transformed_visible_content_clip_report(
                            load_page_image(student_page.image_path),
                            transform,
                            output_width=template_image.width,
                            output_height=template_image.height,
                        )
                        if clip_report.clips_visible_content:
                            result = AlignmentResult(
                                student_ref=student_page.student_ref,
                                page_number=student_page.page_number,
                                status=provider_result.status,
                                confidence=provider_result.confidence,
                                transform=transform,
                                warnings=[
                                    *provider_result.warnings,
                                    warning(
                                        code="alignment_transform_clips_content",
                                        message=(
                                            "Alignment transform would clip visible page content "
                                            "during canonicalization; canonicalize will normalize "
                                            "the placement before detect."
                                        ),
                                        scope={**scope, **clip_report.warning_scope()},
                                    ),
                                ],
                            )
                else:
                    failed_count += 1
                    result = AlignmentResult(
                        student_ref=student_page.student_ref,
                        page_number=student_page.page_number,
                        status="failed",
                        confidence=provider_result.confidence,
                        warnings=provider_result.warnings,
                    )

        results.append(result)
        if any(item.code == "marker_guided_alignment_not_used" for item in result.warnings):
            marker_fallback_count += 1
        ctx.emit(
            event="item_completed",
            progress=progress(completed=index, total=total),
            scope=scope,
            data={"status": result.status},
        )

    ctx.emit(
        event="completed",
        progress=progress(completed=total, total=total),
        data={"target_count": total, "failed_count": failed_count},
    )
    return batch_outcome(
        data=batch_result_data(
            rows_key="alignment_results",
            rows=[result.model_dump(mode="json", exclude_none=True) for result in results],
            output_artifacts_dir=request.output_artifacts_dir,
        ),
        output_artifacts_dir=request.output_artifacts_dir,
        artifacts=[],
        result_row_count=len(results),
        failed_count=failed_count,
        command_label="Auto-align",
        providers={"alignment_engine": provider_name},
        extra_warnings=[
            warning(
                code="marker_guided_alignment_not_used",
                message=(
                    "Marker-guided alignment was not used for one or more pages; "
                    "template matching fallback was applied."
                ),
                scope={"fallback_count": marker_fallback_count},
            )
        ]
        if marker_fallback_count
        else None,
    )


def scans_align_auto_spec() -> CommandSpec:
    return CommandSpec(
        name="scans.align-auto",
        request_model=ScansAlignAutoRequest,
        handler=handle_scans_align_auto,
    )
