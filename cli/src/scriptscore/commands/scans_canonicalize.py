# SPDX-License-Identifier: AGPL-3.0-only
"""Implementation of `scans.canonicalize`."""

from __future__ import annotations

from typing import Literal

from scriptscore.artifacts import (
    apply_canonical_transform,
    load_page_image,
    save_png,
    transformed_visible_content_clip_report,
)
from scriptscore.commands.common import (
    batch_outcome,
    batch_result_data,
    image_artifact,
    progress,
    warning,
)
from scriptscore.commands.scans_shared import (
    canonicalize_output_page,
    canonicalize_output_path,
    ensure_paths_exist,
)
from scriptscore.contracts import (
    ArtifactReference,
    CanonicalizeResult,
    ScansCanonicalizeRequest,
)
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec

COMMAND_NAME = "scans.canonicalize"


def handle_scans_canonicalize(
    ctx: CommandContext, request: ScansCanonicalizeRequest
) -> CommandOutcome:
    """Apply caller-supplied transforms onto template-sized output canvases."""

    ensure_paths_exist(
        [target.page.image_path for target in request.canonicalize_targets],
        command=COMMAND_NAME,
    )
    ensure_paths_exist(
        [target.template_page.image_path for target in request.canonicalize_targets],
        command=COMMAND_NAME,
    )

    total = len(request.canonicalize_targets)
    ctx.emit(
        event="started",
        progress=progress(completed=0, total=total),
        data={"target_count": total, "total_stages": 1},
    )

    results: list[CanonicalizeResult] = []
    artifacts: list[ArtifactReference] = []
    failed_count = 0
    for index, target in enumerate(request.canonicalize_targets, start=1):
        ctx.check_cancelled()
        assert target.page.student_ref is not None
        scope: dict[str, object] = {
            "student_ref": target.page.student_ref,
            "page_number": target.page.page_number,
        }
        ctx.emit(
            event="item_started",
            progress=progress(completed=index - 1, total=total),
            scope=scope,
        )
        output_path = canonicalize_output_path(request.output_artifacts_dir, target)
        output_page = canonicalize_output_page(target, output_path)
        applied_transform = target.transform
        try:
            image = load_page_image(target.page.image_path)
            template_image = load_page_image(target.template_page.image_path)
            clip_report = transformed_visible_content_clip_report(
                image,
                target.transform,
                output_width=template_image.width,
                output_height=template_image.height,
            )
            warnings = []
            if clip_report.clips_visible_content:
                applied_transform = clip_report.adjusted_transform(target.transform)
                warnings.append(
                    warning(
                        code="canonicalize_transform_clips_content",
                        message=(
                            "Canonicalization transform was adjusted to keep visible page "
                            "content on the template canvas."
                        ),
                        scope={
                            **scope,
                            **clip_report.warning_scope(),
                            "adjusted_translate_x": applied_transform.translate_x,
                            "adjusted_translate_y": applied_transform.translate_y,
                        },
                    )
                )

            canonicalized = apply_canonical_transform(
                image,
                applied_transform,
                output_width=template_image.width,
                output_height=template_image.height,
            )
            save_png(canonicalized, output_path)
            artifacts.append(
                image_artifact(
                    role="canonicalized_page",
                    label=output_path.name,
                    path=output_path,
                    scope=scope,
                )
            )
            status: Literal["ok", "error"] = "ok"
        except Exception as exc:
            failed_count += 1
            warnings = [
                warning(
                    code="canonicalize_failed",
                    message=str(exc) or "Canonicalization failed.",
                    scope=scope,
                )
            ]
            status = "error"

        results.append(
            CanonicalizeResult(
                student_ref=target.page.student_ref,
                page_number=target.page.page_number,
                status=status,
                transform=applied_transform if status == "ok" else target.transform,
                output_page=output_page,
                warnings=warnings,
            )
        )
        ctx.emit(
            event="item_completed",
            progress=progress(completed=index, total=total),
            scope=scope,
            data={"status": status},
        )

    ctx.emit(
        event="completed",
        progress=progress(completed=total, total=total),
        data={"target_count": total, "failed_count": failed_count},
    )
    return batch_outcome(
        data=batch_result_data(
            rows_key="canonicalize_results",
            rows=[result.model_dump(mode="json", exclude_none=True) for result in results],
            output_artifacts_dir=request.output_artifacts_dir,
        ),
        output_artifacts_dir=request.output_artifacts_dir,
        artifacts=artifacts,
        result_row_count=len(results),
        failed_count=failed_count,
        command_label="Canonicalize",
    )


def scans_canonicalize_spec() -> CommandSpec:
    return CommandSpec(
        name=COMMAND_NAME,
        request_model=ScansCanonicalizeRequest,
        handler=handle_scans_canonicalize,
    )
