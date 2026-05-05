# SPDX-License-Identifier: AGPL-3.0-only
"""Implementation of `scans crop`."""

from __future__ import annotations

from typing import Literal

from scriptscore.artifacts import crop_image, load_page_image, save_png
from scriptscore.commands.common import (
    batch_outcome,
    batch_result_data,
    image_artifact,
    progress,
    warning,
)
from scriptscore.commands.scans_shared import (
    crop_output_paths,
    ensure_paths_exist,
    matched_crop_jobs,
)
from scriptscore.contracts import (
    ArtifactReference,
    QuestionCropResult,
    ScansCropRequest,
    WarningObject,
)
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec


def handle_scans_crop(ctx: CommandContext, request: ScansCropRequest) -> CommandOutcome:
    """Crop question images from explicit student pages."""

    ensure_paths_exist([page.image_path for page in request.pages], command="scans.crop")
    jobs = matched_crop_jobs(request.pages, request.question_crop_targets)
    output_paths = crop_output_paths(request.output_artifacts_dir, jobs)
    results: list[QuestionCropResult] = []
    artifacts: list[ArtifactReference] = []
    failed_count = 0
    total = len(jobs)

    if total > 0:
        ctx.emit(
            event="started",
            progress=progress(completed=0, total=total),
            data={"result_row_count": total, "total_stages": 1},
        )
    else:
        ctx.emit(event="started", data={"result_row_count": 0, "total_stages": 1})

    for index, ((page, target), output_path) in enumerate(
        zip(jobs, output_paths, strict=True), start=1
    ):
        ctx.check_cancelled()
        assert page.student_ref is not None
        scope: dict[str, object] = {
            "student_ref": page.student_ref,
            "question_id": target.question_id,
        }
        ctx.emit(
            event="item_started",
            progress=progress(completed=index - 1, total=total),
            scope=scope,
        )
        try:
            image = load_page_image(page.image_path)
            cropped = crop_image(image, target.region)
            save_png(cropped, output_path)
            row_warnings: list[WarningObject] = []
            artifacts.append(
                image_artifact(
                    role="question_crop",
                    label=output_path.name,
                    path=output_path,
                    scope=scope,
                )
            )
            result = QuestionCropResult(
                student_ref=page.student_ref,
                question_id=target.question_id,
                status="ok",
                question_crop_path=output_path,
                warnings=row_warnings,
            )
            status: Literal["ok", "error"] = "ok"
        except Exception as exc:
            failed_count += 1
            row_warnings = [
                warning(
                    code="crop_failed",
                    message=str(exc) or "Crop execution failed.",
                    scope=scope,
                )
            ]
            result = QuestionCropResult(
                student_ref=page.student_ref,
                question_id=target.question_id,
                status="error",
                warnings=row_warnings,
            )
            status = "error"
        results.append(result)
        ctx.emit(
            event="item_completed",
            progress=progress(completed=index, total=total),
            scope=scope,
            data={"status": status},
        )

    if total > 0:
        ctx.emit(
            event="completed",
            progress=progress(completed=total, total=total),
            data={"result_row_count": total, "failed_count": failed_count},
        )
    else:
        ctx.emit(event="completed", data={"result_row_count": 0, "failed_count": 0})

    return batch_outcome(
        data=batch_result_data(
            rows_key="crop_results",
            rows=[result.model_dump(mode="json", exclude_none=True) for result in results],
            output_artifacts_dir=request.output_artifacts_dir,
        ),
        output_artifacts_dir=request.output_artifacts_dir,
        artifacts=artifacts,
        result_row_count=len(results),
        failed_count=failed_count,
        command_label="Crop",
    )


def scans_crop_spec() -> CommandSpec:
    return CommandSpec(name="scans.crop", request_model=ScansCropRequest, handler=handle_scans_crop)
