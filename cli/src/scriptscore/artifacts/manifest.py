# SPDX-License-Identifier: AGPL-3.0-only
"""Filesystem helpers for durable artifact manifests."""

from __future__ import annotations

import json
from pathlib import Path

from scriptscore.contracts import ArtifactReference, OutputMetadataManifest
from scriptscore.paths import ensure_within_root


def write_output_metadata(
    *,
    command: str,
    operation_id: str,
    request_id: str | None,
    output_artifacts_dir: Path,
    artifacts: list[ArtifactReference],
    data: dict[str, object],
) -> Path:
    """Write the standard output_metadata.json manifest."""

    resolved_dir = output_artifacts_dir.resolve()
    resolved_dir.mkdir(parents=True, exist_ok=True)
    for artifact in artifacts:
        ensure_within_root(Path(artifact.path), root=resolved_dir, field_name="artifact.path")
    manifest = OutputMetadataManifest(
        command=command,
        operation_id=operation_id,
        request_id=request_id,
        output_artifacts_dir=str(resolved_dir),
        artifacts=artifacts,
        data=data,
    )
    manifest_path = resolved_dir / "output_metadata.json"
    manifest_path.write_text(
        json.dumps(manifest.model_dump(mode="json", exclude_none=True), indent=2, sort_keys=True),
        encoding="utf-8",
    )
    return manifest_path
