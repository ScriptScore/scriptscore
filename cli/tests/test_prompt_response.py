# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for shared prompt JSON response parsing."""

from __future__ import annotations

from typing import Any

import pytest
from pydantic import BaseModel, ConfigDict

from scriptscore.prompts import PromptResponseError, parse_json_model, parse_json_object
from scriptscore.prompts import response as response_module


class _Payload(BaseModel):
    model_config = ConfigDict(extra="forbid")

    value: int


def test_parse_json_object_strips_markdown_fences_and_repairs_json() -> None:
    parsed = parse_json_object('```json\n{"value": 1,}\n```')

    assert parsed == {"value": 1}


def test_parse_json_model_recovers_from_hallucinated_text_fence() -> None:
    payload = parse_json_model('```text\n{"value": 7}\n```', _Payload)

    assert payload.value == 7


def test_parse_json_object_extracts_object_after_repair(monkeypatch: pytest.MonkeyPatch) -> None:
    def _repair_json(_raw_text: str, *, return_objects: bool) -> str:
        assert return_objects is False
        return 'prefix {"value": 3} suffix'

    monkeypatch.setattr(response_module, "repair_json", _repair_json)

    assert parse_json_object("value: 3") == {"value": 3}


def test_parse_json_object_rejects_unrecoverable_text(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(
        response_module,
        "repair_json",
        lambda _raw_text, *, return_objects: "still not json",
    )

    with pytest.raises(PromptResponseError, match="recoverable JSON object"):
        parse_json_object("not json")


def test_parse_json_object_reports_repair_failures(monkeypatch: pytest.MonkeyPatch) -> None:
    def _repair_json(_raw_text: str, *, return_objects: bool) -> str:
        raise RuntimeError("repair unavailable")

    monkeypatch.setattr(response_module, "repair_json", _repair_json)

    with pytest.raises(PromptResponseError, match="JSON repair failed") as exc_info:
        parse_json_object("not json")

    assert exc_info.value.details["error_type"] == "RuntimeError"


def test_parse_json_object_rejects_non_object_json() -> None:
    with pytest.raises(PromptResponseError, match="must be a JSON object"):
        parse_json_object("[1, 2, 3]")


def test_parse_json_model_reports_schema_errors() -> None:
    with pytest.raises(PromptResponseError, match="schema validation") as exc_info:
        parse_json_model('{"value": "not an int"}', _Payload)

    assert isinstance(exc_info.value.details["errors"], list)


def test_parse_json_object_accepts_plain_object_without_repair(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    def _repair_json(_raw_text: str, *, return_objects: bool) -> Any:
        raise AssertionError("repair should not run for valid JSON")

    monkeypatch.setattr(response_module, "repair_json", _repair_json)

    assert parse_json_object('{"value": 9}') == {"value": 9}
