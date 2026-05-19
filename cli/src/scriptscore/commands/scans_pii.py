# SPDX-License-Identifier: AGPL-3.0-only
"""Implementation of `scans pii`."""

from __future__ import annotations

from collections.abc import Sequence
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from queue import Empty, Queue
from typing import Literal, cast

from scriptscore.artifacts import write_trace_artifact
from scriptscore.commands.common import (
    batch_outcome,
    batch_result_data,
    progress,
    timing_info,
    warning,
)
from scriptscore.commands.scans_shared import ensure_paths_exist
from scriptscore.contracts import (
    ArtifactReference,
    ErrorCategory,
    PiiResult,
    ScansPiiRequest,
    ScriptscoreError,
    WarningObject,
    WriteState,
)
from scriptscore.pii_scan import (
    ScanFinding,
    ScanRuntimeOptions,
    create_reader,
    inspect_student_crop,
    verify_model_root,
)
from scriptscore.pii_scan.engine import TokenReader
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec


@dataclass(frozen=True)
class _PiiWorkItem:
    index: int
    student_ref: str
    trigger_words: list[str]
    question_id: str
    question_crop_path: Path


@dataclass(frozen=True)
class _PiiCompletedItem:
    work_item: _PiiWorkItem
    finding: ScanFinding
    warnings: list[WarningObject]
    status: Literal["ok", "warning", "error"]


@dataclass(frozen=True)
class _PiiProgressUpdate:
    event: Literal["item_started", "item_completed"]
    work_item: _PiiWorkItem
    completed_item: _PiiCompletedItem | None = None


def _scrub_message(message: str, *, trigger_words: list[str]) -> str:
    cleaned = message
    for trigger in sorted({item for item in trigger_words if item}, key=len, reverse=True):
        cleaned = cleaned.replace(trigger, "[redacted-trigger]")
    return cleaned


def _row_warnings(
    *,
    student_ref: str,
    question_id: str,
    trigger_words: list[str],
    backend_warnings: list[str],
    handwriting_state: str,
    fatal_error: str | None,
) -> list[WarningObject]:
    scope: dict[str, object] = {"student_ref": student_ref, "question_id": question_id}
    row_warnings: list[WarningObject] = []
    if fatal_error is not None:
        row_warnings.append(
            warning(
                code="pii_analysis_failed",
                message=_scrub_message(fatal_error, trigger_words=trigger_words)
                or "PII analysis failed.",
                scope=scope,
            )
        )
        return row_warnings
    if handwriting_state == "unknown":
        row_warnings.append(
            warning(
                code="pii_handwriting_unknown",
                message="Handwriting detection was inconclusive for this crop.",
                scope=scope,
            )
        )
    for item in backend_warnings:
        row_warnings.append(
            warning(
                code="pii_analysis_degraded",
                message=_scrub_message(item, trigger_words=trigger_words),
                scope=scope,
            )
        )
    return row_warnings


def _row_status(
    *,
    warnings: list[WarningObject],
    fatal_error: str | None,
) -> Literal["ok", "warning", "error"]:
    if fatal_error is not None:
        return "error"
    if warnings:
        return "warning"
    return "ok"


def _top_level_warning_rows(count: int) -> list[WarningObject]:
    if count == 0:
        return []
    noun = "row" if count == 1 else "rows"
    return [
        warning(
            code="pii_analysis_warning_rows",
            message=f"PII analysis returned warnings for {count} result {noun}.",
            scope={"row_count": count},
        )
    ]


def _timing_window(duration_seconds: float) -> tuple[datetime, datetime]:
    finished = datetime.now(UTC)
    started = finished
    if duration_seconds > 0:
        started = finished.fromtimestamp(finished.timestamp() - duration_seconds, tz=UTC)
    return started, finished


def _inspect_work_item(
    work_item: _PiiWorkItem,
    *,
    options: ScanRuntimeOptions,
    reader: TokenReader,
) -> _PiiCompletedItem:
    finding = inspect_student_crop(
        work_item.question_crop_path,
        trigger_words=work_item.trigger_words,
        options=options,
        reader=reader,
    )
    row_warnings = _row_warnings(
        student_ref=work_item.student_ref,
        question_id=work_item.question_id,
        trigger_words=work_item.trigger_words,
        backend_warnings=finding.backend_warnings,
        handwriting_state=finding.handwriting_state,
        fatal_error=finding.fatal_error,
    )
    return _PiiCompletedItem(
        work_item=work_item,
        finding=finding,
        warnings=row_warnings,
        status=_row_status(warnings=row_warnings, fatal_error=finding.fatal_error),
    )


def _inspect_work_items(
    work_items: list[_PiiWorkItem],
    *,
    options: ScanRuntimeOptions,
    reader: TokenReader,
    progress_queue: Queue[_PiiProgressUpdate] | None = None,
) -> list[_PiiCompletedItem]:
    completed_items = []
    for work_item in work_items:
        if progress_queue is not None:
            progress_queue.put(_PiiProgressUpdate(event="item_started", work_item=work_item))
        completed_item = _inspect_work_item(work_item, options=options, reader=reader)
        completed_items.append(completed_item)
        if progress_queue is not None:
            progress_queue.put(
                _PiiProgressUpdate(
                    event="item_completed",
                    work_item=work_item,
                    completed_item=completed_item,
                )
            )
    return completed_items


def _emit_pii_item_started(
    ctx: CommandContext,
    work_item: _PiiWorkItem,
    *,
    completed_count: int,
    total: int,
) -> None:
    ctx.emit(
        event="item_started",
        progress=progress(completed=completed_count, total=total),
        scope={"student_ref": work_item.student_ref, "question_id": work_item.question_id},
    )


def _emit_pii_item_completed(
    ctx: CommandContext,
    completed_item: _PiiCompletedItem,
    *,
    completed_count: int,
    total: int,
) -> None:
    item = completed_item.work_item
    ctx.emit(
        event="item_completed",
        progress=progress(completed=completed_count, total=total),
        scope={"student_ref": item.student_ref, "question_id": item.question_id},
        data={"status": completed_item.status},
    )


def _completed_items(
    *,
    ctx: CommandContext,
    work_items: list[_PiiWorkItem],
    options: ScanRuntimeOptions,
    readers: Sequence[TokenReader],
) -> list[_PiiCompletedItem]:
    total = len(work_items)
    if len(readers) == 1:
        completed_items = []
        for completed_count, item in enumerate(work_items, start=1):
            ctx.check_cancelled()
            _emit_pii_item_started(ctx, item, completed_count=completed_count - 1, total=total)
            completed_item = _inspect_work_item(item, options=options, reader=readers[0])
            completed_items.append(completed_item)
            _emit_pii_item_completed(
                ctx,
                completed_item,
                completed_count=completed_count,
                total=total,
            )
        return completed_items

    completed_by_index: dict[int, _PiiCompletedItem] = {}
    completed_count = 0
    progress_queue: Queue[_PiiProgressUpdate] = Queue()
    chunks = [work_items[index :: len(readers)] for index in range(len(readers))]
    with ThreadPoolExecutor(max_workers=len(readers), thread_name_prefix="scriptscore-pii") as pool:
        futures = {
            pool.submit(
                _inspect_work_items,
                chunk,
                options=options,
                reader=reader,
                progress_queue=progress_queue,
            )
            for chunk, reader in zip(chunks, readers, strict=True)
            if chunk
        }
        while futures:
            ctx.check_cancelled()
            try:
                update = progress_queue.get(timeout=0.05)
            except Empty:
                update = None
            if update is not None:
                if update.event == "item_started":
                    _emit_pii_item_started(
                        ctx,
                        update.work_item,
                        completed_count=completed_count,
                        total=total,
                    )
                elif update.completed_item is not None:
                    completed_count += 1
                    completed_by_index[update.work_item.index] = update.completed_item
                    _emit_pii_item_completed(
                        ctx,
                        update.completed_item,
                        completed_count=completed_count,
                        total=total,
                    )
            done = {future for future in futures if future.done()}
            for future in done:
                futures.remove(future)
                future.result()
        while not progress_queue.empty():
            update = progress_queue.get_nowait()
            if update.event == "item_started":
                _emit_pii_item_started(
                    ctx,
                    update.work_item,
                    completed_count=completed_count,
                    total=total,
                )
            elif update.completed_item is not None:
                completed_count += 1
                completed_by_index[update.work_item.index] = update.completed_item
                _emit_pii_item_completed(
                    ctx,
                    update.completed_item,
                    completed_count=completed_count,
                    total=total,
                )
    return [completed_by_index[item.index] for item in work_items]


def handle_scans_pii(ctx: CommandContext, request: ScansPiiRequest) -> CommandOutcome:
    """Analyze explicit question crops for handwriting and student-specific PII."""

    all_targets = [
        _PiiWorkItem(
            index=index,
            student_ref=student.student_ref,
            trigger_words=student.pii_trigger_words,
            question_id=target.question_id,
            question_crop_path=target.question_crop_path,
        )
        for index, (student, target) in enumerate(
            ((student, target) for student in request.students for target in student.pii_targets),
            start=1,
        )
    ]
    ensure_paths_exist(
        [target.question_crop_path for target in all_targets],
        command="scans.pii",
    )

    try:
        verify_model_root(request.pii_runtime_config.paddle_model_dir)
    except Exception as exc:
        raise ScriptscoreError(
            code="pii_runtime_invalid",
            message=str(exc) or "The local Paddle model directory is invalid.",
            category=ErrorCategory.PREREQUISITE,
            retryable=True,
            write_state=WriteState.NO_WRITE,
        ) from exc

    try:
        readers = [
            create_reader(request.pii_runtime_config.paddle_model_dir)
            for _ in range(request.pii_runtime_config.max_workers)
        ]
    except Exception as exc:
        raise ScriptscoreError(
            code="pii_runtime_unavailable",
            message=str(exc) or "The local Paddle runtime is unavailable.",
            category=ErrorCategory.EXTERNAL_DEPENDENCY,
            retryable=True,
            write_state=WriteState.NO_WRITE,
        ) from exc

    options = ScanRuntimeOptions(model_root=request.pii_runtime_config.paddle_model_dir)
    total = len(all_targets)
    results: list[PiiResult] = []
    artifacts: list[ArtifactReference] = []
    failed_count = 0
    warning_count = 0

    if total > 0:
        ctx.emit(
            event="started",
            progress=progress(completed=0, total=total),
            data={
                "result_row_count": total,
                "total_stages": 1,
                "max_workers": request.pii_runtime_config.max_workers,
            },
        )
    else:
        ctx.emit(event="started", data={"result_row_count": 0, "total_stages": 1})

    completed_items = _completed_items(
        ctx=ctx,
        work_items=all_targets,
        options=options,
        readers=readers,
    )

    for completed_item in completed_items:
        ctx.check_cancelled()
        item = completed_item.work_item
        finding = completed_item.finding
        scope: dict[str, object] = {
            "student_ref": item.student_ref,
            "question_id": item.question_id,
        }
        row_warnings = completed_item.warnings
        status = completed_item.status
        result = PiiResult(
            student_ref=item.student_ref,
            question_id=item.question_id,
            status=status,
            contains_handwriting=finding.handwriting_state,
            contains_pii=finding.pii_present,
            pii_types_detected=finding.pii_kinds,
            warnings=row_warnings,
        )
        if status == "error":
            failed_count += 1
        elif status == "warning":
            warning_count += 1
        results.append(result)
        trace_started, trace_finished = _timing_window(finding.duration_seconds)
        backend_name = "paddleocr_local"
        if isinstance(finding.metrics, dict):
            backend_name = cast(str, finding.metrics.get("backend_name", backend_name))
        artifacts.append(
            write_trace_artifact(
                output_artifacts_dir=request.output_artifacts_dir,
                command="scans.pii",
                operation_id=ctx.operation_id,
                request_id=ctx.request_id,
                step="pii_analysis",
                scope=scope,
                provider_capability="local_runtime",
                provider_name=backend_name,
                request_options={
                    "student_ref": item.student_ref,
                    "question_id": item.question_id,
                    "trigger_count": len(item.trigger_words),
                    "max_workers": request.pii_runtime_config.max_workers,
                },
                input_artifacts=[str(item.question_crop_path)],
                response_parsed={
                    "status": status,
                    "contains_handwriting": finding.handwriting_state,
                    "contains_pii": finding.pii_present,
                    "pii_types_detected": finding.pii_kinds,
                    "warning_count": len(row_warnings),
                    "backend_warning_count": len(finding.backend_warnings),
                },
                timing=timing_info(started=trace_started, finished=trace_finished),
            )
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
            rows_key="pii_results",
            rows=[result.model_dump(mode="json", exclude_none=True) for result in results],
            output_artifacts_dir=request.output_artifacts_dir,
        ),
        output_artifacts_dir=request.output_artifacts_dir,
        artifacts=artifacts,
        result_row_count=len(results),
        failed_count=failed_count,
        command_label="PII analysis",
        extra_warnings=_top_level_warning_rows(warning_count),
    )


def scans_pii_spec() -> CommandSpec:
    return CommandSpec(name="scans.pii", request_model=ScansPiiRequest, handler=handle_scans_pii)
