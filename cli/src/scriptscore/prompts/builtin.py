# SPDX-License-Identifier: AGPL-3.0-only
"""Built-in CLI-owned prompt definitions loaded from packaged YAML."""

from __future__ import annotations

from functools import lru_cache
from importlib import resources
from typing import Any

import yaml
from pydantic import ValidationError

from scriptscore.contracts import (
    ErrorCategory,
    ScriptscoreError,
    WriteState,
    validation_issues_from_exception,
)
from scriptscore.prompts.loader import PromptDefinition, PromptLoader

_BUILTIN_PROMPT_RESOURCE = "builtin_prompts.yaml"


class BuiltinPromptLoadError(ScriptscoreError):
    """Raised when packaged built-in prompt definitions cannot be loaded."""

    def __init__(
        self,
        *,
        code: str,
        message: str,
        details: dict[str, Any] | None = None,
    ) -> None:
        super().__init__(
            code=code,
            message=message,
            category=ErrorCategory.RESOURCE,
            retryable=False,
            details=details or {},
            write_state=WriteState.NO_WRITE,
        )


def _builtin_prompt_resource() -> resources.abc.Traversable:
    return resources.files("scriptscore.prompts").joinpath(_BUILTIN_PROMPT_RESOURCE)


def _read_builtin_prompt_resource_text() -> str:
    resource = _builtin_prompt_resource()
    try:
        return resource.read_text(encoding="utf-8")
    except FileNotFoundError as exc:
        raise BuiltinPromptLoadError(
            code="builtin_prompt_resource_missing",
            message="The packaged built-in prompt resource is missing.",
            details={"resource": str(resource)},
        ) from exc
    except OSError as exc:
        raise BuiltinPromptLoadError(
            code="builtin_prompt_resource_unreadable",
            message="The packaged built-in prompt resource could not be read.",
            details={
                "resource": str(resource),
                "error_type": type(exc).__name__,
                "error": str(exc),
            },
        ) from exc


def _load_builtin_prompt_mapping() -> dict[str, Any]:
    try:
        payload = yaml.safe_load(_read_builtin_prompt_resource_text())
    except yaml.YAMLError as exc:
        raise BuiltinPromptLoadError(
            code="builtin_prompt_yaml_invalid",
            message="The packaged built-in prompt YAML is invalid.",
            details={
                "resource": _BUILTIN_PROMPT_RESOURCE,
                "error_type": type(exc).__name__,
                "error": str(exc),
            },
        ) from exc
    if not isinstance(payload, dict):
        raise BuiltinPromptLoadError(
            code="builtin_prompt_yaml_invalid",
            message="The packaged built-in prompt YAML must contain a top-level mapping.",
            details={
                "resource": _BUILTIN_PROMPT_RESOURCE,
                "payload_type": type(payload).__name__,
            },
        )
    return payload


def _load_builtin_prompt_definitions() -> list[PromptDefinition]:
    definitions: list[PromptDefinition] = []
    for prompt_id, raw_definition in _load_builtin_prompt_mapping().items():
        if not isinstance(prompt_id, str):
            raise BuiltinPromptLoadError(
                code="builtin_prompt_yaml_invalid",
                message="Built-in prompt ids must be strings.",
                details={
                    "resource": _BUILTIN_PROMPT_RESOURCE,
                    "prompt_id_type": type(prompt_id).__name__,
                },
            )
        if not isinstance(raw_definition, dict):
            raise BuiltinPromptLoadError(
                code="builtin_prompt_yaml_invalid",
                message="Each built-in prompt definition must be a mapping.",
                details={
                    "resource": _BUILTIN_PROMPT_RESOURCE,
                    "prompt_id": prompt_id,
                    "definition_type": type(raw_definition).__name__,
                },
            )
        try:
            definitions.append(PromptDefinition.model_validate({"id": prompt_id, **raw_definition}))
        except ValidationError as exc:
            raise BuiltinPromptLoadError(
                code="builtin_prompt_definition_invalid",
                message=f"Built-in prompt '{prompt_id}' failed schema validation.",
                details={
                    "resource": _BUILTIN_PROMPT_RESOURCE,
                    "prompt_id": prompt_id,
                    "issues": [
                        issue.model_dump(mode="json")
                        for issue in validation_issues_from_exception(exc)
                    ],
                },
            ) from exc
    return definitions


@lru_cache(maxsize=1)
def builtin_prompt_loader() -> PromptLoader:
    """Return the cached built-in prompt registry."""

    loader = PromptLoader()
    for definition in _load_builtin_prompt_definitions():
        loader.register(definition)
    return loader


def get_builtin_prompt(prompt_id: str) -> PromptDefinition:
    """Resolve one built-in prompt definition."""

    return builtin_prompt_loader().get(prompt_id)
