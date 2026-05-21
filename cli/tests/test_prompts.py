# SPDX-License-Identifier: AGPL-3.0-only
"""Prompt loader scaffold tests."""

from __future__ import annotations

import pytest

from scriptscore.prompts import (
    BuiltinPromptLoadError,
    PromptDefinition,
    PromptInputDefinition,
    PromptLoader,
    get_builtin_prompt,
)
from scriptscore.prompts import builtin as builtin_module


class _UnreadablePromptResource:
    def __init__(self, error: OSError) -> None:
        self._error = error

    def read_text(self, *, encoding: str) -> str:
        assert encoding == "utf-8"
        raise self._error

    def __str__(self) -> str:
        return "fake-builtin-prompts.yaml"


def test_prompt_loader_register_and_get() -> None:
    loader = PromptLoader()
    definition = PromptDefinition(
        id="smoke_prompt",
        response_mode="plain_text",
        template_text="Hello {{name}}",
        command_scoped_inputs=[PromptInputDefinition(name="name", input_kind="text")],
    )
    loader.register(definition)
    assert loader.get("smoke_prompt").template_text == "Hello {{name}}"


def test_builtin_prompt_loader_reads_yaml_backed_definitions() -> None:
    prompt = get_builtin_prompt("question_text")

    assert prompt.id == "question_text"
    assert "Baseline text from `pdftotext`." in prompt.template_text
    assert prompt.command_scoped_inputs[0].name == "template_question_png"


def test_grading_prompts_include_incidental_multiple_choice_guardrails() -> None:
    for prompt_id in ("preliminary_score", "preliminary_score_multi_criterion"):
        prompt = get_builtin_prompt(prompt_id)

        assert "printed multiple-choice options" in prompt.template_text
        assert "do not treat the printed option text itself as the student's answer" in (
            prompt.template_text
        )
        assert "Do not treat a lone O, 0, or noisy OCR artifact as a selected option" in (
            prompt.template_text
        )
        assert "do not invent a selected option" in prompt.template_text


def test_builtin_prompt_loader_rejects_invalid_yaml(monkeypatch: pytest.MonkeyPatch) -> None:
    builtin_module.builtin_prompt_loader.cache_clear()
    monkeypatch.setattr(
        builtin_module,
        "_read_builtin_prompt_resource_text",
        lambda: "question_text: [",
    )

    with pytest.raises(BuiltinPromptLoadError, match="YAML is invalid"):
        builtin_module.builtin_prompt_loader()

    builtin_module.builtin_prompt_loader.cache_clear()


def test_builtin_prompt_loader_rejects_invalid_prompt_definition(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    builtin_module.builtin_prompt_loader.cache_clear()
    monkeypatch.setattr(
        builtin_module,
        "_load_builtin_prompt_mapping",
        lambda: {"question_text": {"template_text": "missing fields"}},
    )

    with pytest.raises(BuiltinPromptLoadError, match="schema validation"):
        builtin_module.builtin_prompt_loader()

    builtin_module.builtin_prompt_loader.cache_clear()


def test_builtin_prompt_resource_missing_raises_typed_error(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        builtin_module,
        "_builtin_prompt_resource",
        lambda: _UnreadablePromptResource(FileNotFoundError("missing")),
    )

    with pytest.raises(BuiltinPromptLoadError, match="resource is missing") as exc_info:
        builtin_module._read_builtin_prompt_resource_text()

    assert exc_info.value.code == "builtin_prompt_resource_missing"
    assert exc_info.value.details["resource"] == "fake-builtin-prompts.yaml"


def test_builtin_prompt_resource_unreadable_raises_typed_error(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        builtin_module,
        "_builtin_prompt_resource",
        lambda: _UnreadablePromptResource(OSError("permission denied")),
    )

    with pytest.raises(BuiltinPromptLoadError, match="could not be read") as exc_info:
        builtin_module._read_builtin_prompt_resource_text()

    assert exc_info.value.code == "builtin_prompt_resource_unreadable"
    assert exc_info.value.details["error_type"] == "OSError"


def test_builtin_prompt_loader_rejects_non_mapping_yaml(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    builtin_module.builtin_prompt_loader.cache_clear()
    monkeypatch.setattr(builtin_module, "_read_builtin_prompt_resource_text", lambda: "- one")

    with pytest.raises(BuiltinPromptLoadError, match="top-level mapping") as exc_info:
        builtin_module.builtin_prompt_loader()

    assert exc_info.value.details["payload_type"] == "list"
    builtin_module.builtin_prompt_loader.cache_clear()


def test_builtin_prompt_loader_rejects_non_string_prompt_id(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    builtin_module.builtin_prompt_loader.cache_clear()
    monkeypatch.setattr(
        builtin_module,
        "_load_builtin_prompt_mapping",
        lambda: {1: {"response_mode": "plain_text", "template_text": "hello"}},
    )

    with pytest.raises(BuiltinPromptLoadError, match="ids must be strings") as exc_info:
        builtin_module.builtin_prompt_loader()

    assert exc_info.value.details["prompt_id_type"] == "int"
    builtin_module.builtin_prompt_loader.cache_clear()


def test_builtin_prompt_loader_rejects_non_mapping_definition(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    builtin_module.builtin_prompt_loader.cache_clear()
    monkeypatch.setattr(
        builtin_module,
        "_load_builtin_prompt_mapping",
        lambda: {"question_text": ["not", "a", "mapping"]},
    )

    with pytest.raises(BuiltinPromptLoadError, match="definition must be a mapping") as exc_info:
        builtin_module.builtin_prompt_loader()

    assert exc_info.value.details["definition_type"] == "list"
    builtin_module.builtin_prompt_loader.cache_clear()
