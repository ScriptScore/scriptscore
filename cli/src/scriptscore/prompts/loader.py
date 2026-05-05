# SPDX-License-Identifier: AGPL-3.0-only
"""Prompt definition scaffolding."""

from __future__ import annotations

from typing import Any

from pydantic import BaseModel, ConfigDict, Field


class PromptInputDefinition(BaseModel):
    """Typed command-scoped prompt input declaration."""

    model_config = ConfigDict(extra="forbid")

    name: str
    input_kind: str


class PromptDefinition(BaseModel):
    """CLI-owned prompt definition scaffold."""

    model_config = ConfigDict(extra="forbid")

    id: str
    response_mode: str
    template_text: str
    command_scoped_inputs: list[PromptInputDefinition]
    allowed_user_policy_keys: list[str] = Field(default_factory=list)
    execution_options: dict[str, Any] = Field(default_factory=dict)
    response_contract: dict[str, Any] | None = None


class PromptLoader:
    """In-memory prompt registry used by the scaffold."""

    def __init__(self, definitions: dict[str, PromptDefinition] | None = None) -> None:
        self._definitions = definitions or {}

    def register(self, definition: PromptDefinition) -> None:
        """Register a prompt definition."""

        self._definitions[definition.id] = definition

    def get(self, prompt_id: str) -> PromptDefinition:
        """Return a registered prompt definition by id."""

        return self._definitions[prompt_id]
