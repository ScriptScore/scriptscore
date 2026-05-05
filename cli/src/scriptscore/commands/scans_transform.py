# SPDX-License-Identifier: AGPL-3.0-only
"""Implementation of `scans transform`."""

from __future__ import annotations

from typing import Literal

from scriptscore.artifacts import apply_manual_transform, load_page_image, save_png
from scriptscore.commands.common import (
    batch_outcome,
    batch_result_data,
    image_artifact,
    progress,
    warning,
)
from scriptscore.commands.scans_shared import (
    ensure_paths_exist,
    transform_output_page,
    transform_output_path,
)
from scriptscore.contracts import (
    ArtifactReference,
    ScansTransformRequest,
    TransformResult,
    WarningObject,
)
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec


def handle_scans_transform(ctx: CommandContext, request: ScansTransformRequest) -> CommandOutcome:
    """Apply manual transforms to explicit student pages."""

    ensure_paths_exist(
        [target.page.image_path for target in request.transform_targets], command="scans.transform"
    )
    total = len(request.transform_targets)
    ctx.emit(
        event="started",
        progress=progress(completed=0, total=total),
        data={"target_count": total, "total_stages": 1},
    )

    results: list[TransformResult] = []
    artifacts: list[ArtifactReference] = []
    failed_count = 0
    for index, target in enumerate(request.transform_targets, start=1):
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
        output_path = transform_output_path(request.output_artifacts_dir, target)
        output_page = transform_output_page(target, output_path)
        try:
            image = load_page_image(target.page.image_path)
            transformed = apply_manual_transform(image, target.transform)
            save_png(transformed, output_path)
            warnings: list[WarningObject] = []
            artifacts.append(
                image_artifact(
                    role="transformed_page",
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
                    code="transform_failed",
                    message=str(exc) or "Transform execution failed.",
                    scope=scope,
                )
            ]
            status = "error"

        results.append(
            TransformResult(
                student_ref=target.page.student_ref,
                page_number=target.page.page_number,
                status=status,
                transform=target.transform,
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
            rows_key="transform_results",
            rows=[result.model_dump(mode="json", exclude_none=True) for result in results],
            output_artifacts_dir=request.output_artifacts_dir,
        ),
        output_artifacts_dir=request.output_artifacts_dir,
        artifacts=artifacts,
        result_row_count=len(results),
        failed_count=failed_count,
        command_label="Manual transform",
    )


def scans_transform_spec() -> CommandSpec:
    return CommandSpec(
        name="scans.transform", request_model=ScansTransformRequest, handler=handle_scans_transform
    )
