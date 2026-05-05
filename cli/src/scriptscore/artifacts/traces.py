# SPDX-License-Identifier: AGPL-3.0-only
"""Shared trace-artifact writing helpers."""

from __future__ import annotations

import json
from hashlib import sha256
from pathlib import Path
from re import sub

from scriptscore.contracts import (
    ArtifactReference,
    LlmTokenUsage,
    TimingInfo,
    TraceArtifactRecord,
    TracePromptInfo,
    TraceProviderInfo,
    TraceRequestInfo,
    TraceResponseInfo,
)
from scriptscore.paths import join_under_root


def _slug(value: str) -> str:
    return sub(r"[^a-zA-Z0-9._-]+", "_", value).strip("_") or "trace"


def _scope_suffix(scope: dict[str, object] | None) -> str:
    """Return the filename scope suffix, adding a hash only when slugging would collapse raw values."""

    if not scope:
        return ""
    ordered_parts: list[str] = []
    raw_scope: dict[str, str] = {}
    ambiguous = False
    for key in sorted(scope):
        raw_key = str(key)
        raw_value = str(scope[key])
        raw_scope[raw_key] = raw_value
        slugged_value = _slug(raw_value)
        ordered_parts.append(f"{_slug(raw_key)}-{slugged_value}")
        if raw_value != slugged_value:
            ambiguous = True
    suffix = "__" + "__".join(ordered_parts)
    if ambiguous:
        digest = sha256(json.dumps(raw_scope, sort_keys=True).encode("utf-8")).hexdigest()[:8]
        suffix += f"__{digest}"
    return suffix


def write_trace_artifact(
    *,
    output_artifacts_dir: Path,
    command: str,
    operation_id: str,
    request_id: str | None,
    step: str,
    provider_capability: str,
    provider_name: str,
    timing: TimingInfo,
    scope: dict[str, object] | None = None,
    filename_suffix: str | None = None,
    prompt_id: str | None = None,
    prompt_variables: dict[str, str] | None = None,
    prompt_rendered: str | None = None,
    request_options: dict[str, object] | None = None,
    input_artifacts: list[str] | None = None,
    response_raw: str | None = None,
    response_parsed: dict[str, object] | None = None,
    response_usage: LlmTokenUsage | None = None,
) -> ArtifactReference:
    """Write one trace JSON artifact and return its artifact reference."""

    scope_suffix = _scope_suffix(scope)
    suffix = ""
    if filename_suffix:
        suffix = f"__{_slug(filename_suffix)}"
    filename = f"{_slug(step)}{scope_suffix}{suffix}.json"
    path = join_under_root(output_artifacts_dir, "traces", filename)

    record = TraceArtifactRecord(
        command=command,
        operation_id=operation_id,
        request_id=request_id,
        step=step,
        scope=scope,
        provider=TraceProviderInfo(capability=provider_capability, provider=provider_name),
        prompt=(
            None
            if prompt_id is None
            else TracePromptInfo(
                id=prompt_id,
                variables=prompt_variables or {},
                rendered=prompt_rendered,
            )
        ),
        request=TraceRequestInfo(
            options=request_options or {},
            input_artifacts=input_artifacts or [],
        ),
        response=TraceResponseInfo(raw=response_raw, parsed=response_parsed, usage=response_usage),
        timing=timing,
    )
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(record.model_dump(mode="json", exclude_none=True), indent=2, sort_keys=True),
        encoding="utf-8",
    )
    return ArtifactReference(
        kind="file",
        role="trace",
        label=path.name,
        path=str(path),
        format="json",
        schema_version="cli.trace.v1",
        entity_scope=scope,
    )
