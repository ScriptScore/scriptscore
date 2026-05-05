# SPDX-License-Identifier: AGPL-3.0-only
"""Small shared helpers for command handlers."""

from __future__ import annotations

from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from scriptscore.contracts import ArtifactReference, ProgressSummary, TimingInfo, WarningObject
from scriptscore.runtime import CommandOutcome


def progress(*, completed: int, total: int) -> ProgressSummary:
    """Create normalized progress payloads."""

    if total <= 0:
        raise ValueError("progress total must be positive.")
    return ProgressSummary(
        completed=completed,
        total=total,
        percent=int((completed / total) * 100),
    )


def warning(*, code: str, message: str, scope: dict[str, object] | None = None) -> WarningObject:
    """Create a shared warning object."""

    return WarningObject(code=code, message=message, scope=scope)


def image_artifact(
    *, role: str, label: str, path: Path, scope: dict[str, object]
) -> ArtifactReference:
    """Create a PNG image artifact reference."""

    return ArtifactReference(
        kind="image",
        role=role,
        label=label,
        path=str(path),
        format="png",
        entity_scope=scope,
    )


def file_artifact(
    *,
    role: str,
    label: str,
    path: Path,
    fmt: str,
    scope: dict[str, object] | None = None,
    schema_version: str | None = None,
) -> ArtifactReference:
    """Create a generic file artifact reference."""

    return ArtifactReference(
        kind="file",
        role=role,
        label=label,
        path=str(path),
        format=fmt,
        schema_version=schema_version,
        entity_scope=scope,
    )


def partial_failure_warning(*, command_label: str, failed_count: int) -> WarningObject:
    """Create the shared batch partial-failure warning."""

    noun = "row" if failed_count == 1 else "rows"
    return WarningObject(
        code="partial_failure",
        message=f"{command_label} completed with {failed_count} failed result {noun}.",
        scope={"failed_count": failed_count},
    )


def inventory_manifest_data(
    *, result_row_count: int, written_artifact_count: int, failed_count: int
) -> dict[str, int]:
    """Return inventory-only manifest summary data."""

    return {
        "result_row_count": result_row_count,
        "written_artifact_count": written_artifact_count,
        "failed_count": failed_count,
    }


def output_metadata_path(output_artifacts_dir: Path) -> str:
    """Return the normalized output-metadata manifest path."""

    return str((output_artifacts_dir / "output_metadata.json").resolve())


def batch_result_data(
    *,
    rows_key: str,
    rows: list[dict[str, Any]],
    output_artifacts_dir: Path,
    extra: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Build the common batch response payload with manifest path."""

    data = {
        rows_key: rows,
        "output_metadata_path": output_metadata_path(output_artifacts_dir),
    }
    if extra:
        data.update(extra)
    return data


def timing_info(*, started: datetime, finished: datetime) -> TimingInfo:
    """Create shared timing metadata."""

    normalized_started = started.astimezone(UTC)
    normalized_finished = finished.astimezone(UTC)
    return TimingInfo(
        started_at=normalized_started.isoformat().replace("+00:00", "Z"),
        finished_at=normalized_finished.isoformat().replace("+00:00", "Z"),
        duration_ms=int((normalized_finished - normalized_started).total_seconds() * 1000),
    )


def batch_outcome(
    *,
    data: dict[str, Any],
    output_artifacts_dir: Path,
    artifacts: list[ArtifactReference],
    result_row_count: int,
    failed_count: int,
    command_label: str,
    providers: dict[str, str] | None = None,
    extra_warnings: list[WarningObject] | None = None,
) -> CommandOutcome:
    """Build a standard degraded-capable batch command outcome."""

    warnings: list[WarningObject] = []
    if failed_count:
        warnings.append(
            partial_failure_warning(command_label=command_label, failed_count=failed_count)
        )
    if extra_warnings:
        warnings.extend(extra_warnings)
    return CommandOutcome(
        data=data,
        degraded=bool(failed_count or extra_warnings),
        warnings=warnings,
        artifacts=artifacts,
        providers=providers,
        output_artifacts_dir=output_artifacts_dir,
        manifest_data=inventory_manifest_data(
            result_row_count=result_row_count,
            written_artifact_count=len(artifacts),
            failed_count=failed_count,
        ),
    )
