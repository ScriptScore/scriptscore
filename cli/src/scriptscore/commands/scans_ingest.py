# SPDX-License-Identifier: AGPL-3.0-only
"""Implementation of `scans ingest`."""

from __future__ import annotations

from collections.abc import Sequence
from typing import Any, Literal

from scriptscore.artifacts import (
    normalize_page_width,
    render_pdf_document,
    save_png,
    validate_pdf_path,
)
from scriptscore.commands.common import (
    batch_outcome,
    batch_result_data,
    image_artifact,
    progress,
    warning,
)
from scriptscore.contracts import ArtifactReference, Page, PdfIngestResult, ScansIngestRequest
from scriptscore.paths import join_under_root
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec


def _ordered_rendered_pages(
    page_order: Sequence[int] | None, rendered_pages: Sequence[Any]
) -> list[Any]:
    if not page_order:
        return list(rendered_pages)

    page_numbers = [int(rendered_page.page_number) for rendered_page in rendered_pages]
    expected = list(range(1, len(page_numbers) + 1))
    if sorted(page_numbers) != expected:
        raise ValueError(
            "scans.ingest expected rendered PDF pages to cover a contiguous 1-based page range."
        )
    if sorted(page_order) != expected:
        raise ValueError("page_order must contain every source PDF page number exactly once.")

    rendered_by_page_number = {
        int(rendered_page.page_number): rendered_page for rendered_page in rendered_pages
    }
    return [rendered_by_page_number[page_number] for page_number in page_order]


def handle_scans_ingest(ctx: CommandContext, request: ScansIngestRequest) -> CommandOutcome:
    """Render uploaded student PDFs into explicit page PNG artifacts."""

    for target in request.pdf_targets:
        validate_pdf_path(target.pdf_path, field_name="pdf_targets[].pdf_path")

    total = len(request.pdf_targets)
    ctx.emit(
        event="started",
        progress=progress(completed=0, total=total),
        data={"target_count": total, "total_stages": 1},
    )

    results: list[PdfIngestResult] = []
    artifacts: list[ArtifactReference] = []
    failed_count = 0
    for index, target in enumerate(request.pdf_targets, start=1):
        ctx.check_cancelled()
        scope: dict[str, object] = {"student_ref": target.student_ref}
        ctx.emit(
            event="item_started",
            progress=progress(completed=index - 1, total=total),
            scope=scope,
        )
        pages: list[Page] = []
        try:
            rendered_pages = _ordered_rendered_pages(
                target.page_order,
                render_pdf_document(target.pdf_path),
            )
            page_total = max(1, len(rendered_pages))
            for output_index, rendered_page in enumerate(rendered_pages, start=1):
                output_path = join_under_root(
                    request.output_artifacts_dir,
                    target.student_ref,
                    f"page_{output_index:03d}.png",
                )
                save_png(normalize_page_width(rendered_page.image), output_path)
                pages.append(
                    Page(
                        page_type="student_scan",
                        page_number=rendered_page.page_number,
                        image_path=output_path,
                    )
                )
                artifacts.append(
                    image_artifact(
                        role="rendered_page",
                        label=output_path.name,
                        path=output_path,
                        scope={
                            "student_ref": target.student_ref,
                            "page_number": rendered_page.page_number,
                        },
                    )
                )
                ctx.emit(
                    event="page_rendered",
                    progress=progress(completed=output_index, total=page_total),
                    scope={**scope, "page_number": rendered_page.page_number},
                    data={"page_number": rendered_page.page_number},
                )
            result = PdfIngestResult(
                student_ref=target.student_ref,
                source_pdf_path=target.pdf_path,
                status="ok",
                pages=pages,
                warnings=[],
            )
            status: Literal["ok", "error"] = "ok"
            data_payload: dict[str, object] = {"status": status, "pages_rendered": len(pages)}
        except Exception as exc:
            failed_count += 1
            row_warnings = [
                warning(
                    code="pdf_render_failed",
                    message=str(exc) or "PDF render failed.",
                    scope=scope,
                )
            ]
            result = PdfIngestResult(
                student_ref=target.student_ref,
                source_pdf_path=target.pdf_path,
                status="error",
                pages=pages,
                warnings=row_warnings,
            )
            status = "error"
            data_payload = {"status": status, "pages_rendered": len(pages)}
        results.append(result)
        ctx.emit(
            event="item_completed",
            progress=progress(completed=index, total=total),
            scope=scope,
            data=data_payload,
        )

    ctx.emit(
        event="completed",
        progress=progress(completed=total, total=total),
        data={"target_count": total, "failed_count": failed_count},
    )
    return batch_outcome(
        data=batch_result_data(
            rows_key="pdf_results",
            rows=[result.model_dump(mode="json", exclude_none=True) for result in results],
            output_artifacts_dir=request.output_artifacts_dir,
        ),
        output_artifacts_dir=request.output_artifacts_dir,
        artifacts=artifacts,
        result_row_count=len(results),
        failed_count=failed_count,
        command_label="PDF ingest",
    )


def scans_ingest_spec() -> CommandSpec:
    return CommandSpec(
        name="scans.ingest", request_model=ScansIngestRequest, handler=handle_scans_ingest
    )
