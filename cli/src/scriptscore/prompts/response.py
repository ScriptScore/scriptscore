# SPDX-License-Identifier: AGPL-3.0-only
"""Helpers for parsing and validating provider prompt responses."""

from __future__ import annotations

import json
import re
from typing import Any

from json_repair import repair_json
from pydantic import BaseModel, ValidationError

from scriptscore.contracts import ErrorCategory, ScriptscoreError


class PromptResponseError(ScriptscoreError):
    """Typed prompt-response parse/validation failure."""

    def __init__(self, *, code: str, message: str, details: dict[str, Any] | None = None) -> None:
        super().__init__(
            code=code,
            message=message,
            category=ErrorCategory.EXECUTION,
            retryable=True,
            details=details or {},
        )


_MARKDOWN_FENCE_RE = re.compile(r"^\s*```[^\n]*\n(?P<body>.*)\n```\s*$", re.DOTALL)


def _strip_markdown_fences(raw_text: str) -> str:
    match = _MARKDOWN_FENCE_RE.match(raw_text.strip())
    if match is None:
        return raw_text
    return match.group("body").strip()


def _extract_json_object(raw_text: str) -> str:
    start = raw_text.find("{")
    end = raw_text.rfind("}")
    if start == -1 or end == -1 or start >= end:
        raise PromptResponseError(
            code="prompt_json_repair_failed",
            message="Prompt response did not contain a recoverable JSON object.",
        )
    return raw_text[start : end + 1]


def parse_json_object(raw_text: str) -> dict[str, Any]:
    """Parse one JSON object with shared markdown scrubbing and JSON repair."""

    cleaned = _strip_markdown_fences(raw_text)
    try:
        parsed = json.loads(cleaned)
    except json.JSONDecodeError:
        try:
            repaired = repair_json(cleaned, return_objects=False)
        except Exception as exc:
            raise PromptResponseError(
                code="prompt_json_repair_failed",
                message="Prompt response JSON repair failed.",
                details={"error_type": type(exc).__name__, "error": str(exc)},
            ) from exc
        try:
            parsed = json.loads(repaired)
        except json.JSONDecodeError:
            parsed = json.loads(_extract_json_object(repaired))
    if not isinstance(parsed, dict):
        raise PromptResponseError(
            code="prompt_json_not_object",
            message="Prompt response must be a JSON object.",
        )
    return parsed


def parse_json_model[T: BaseModel](raw_text: str, model: type[T]) -> T:
    """Parse a JSON object and validate it with a strict pydantic model."""

    parsed = parse_json_object(raw_text)
    try:
        return model.model_validate(parsed)
    except ValidationError as exc:
        raise PromptResponseError(
            code="prompt_response_schema_invalid",
            message="Prompt response failed schema validation.",
            details={"errors": exc.errors()},
        ) from exc
