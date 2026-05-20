# SPDX-License-Identifier: AGPL-3.0-only
"""Shared prompt rendering and XML projection helpers."""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from html import escape
from typing import Any

from pydantic import BaseModel, ConfigDict, Field

from scriptscore.contracts import ErrorCategory, ScriptscoreError
from scriptscore.prompts.loader import PromptDefinition

TOKEN_PATTERN = re.compile(r"\{\{([A-Za-z0-9_]+)\}\}")


class PromptRenderError(ScriptscoreError):
    """Typed prompt-render failure."""

    def __init__(self, *, code: str, message: str, details: dict[str, Any] | None = None) -> None:
        super().__init__(
            code=code,
            message=message,
            category=ErrorCategory.VALIDATION,
            retryable=True,
            details=details or {},
        )


class PromptInput(BaseModel):
    """Caller-provided prompt customization input."""

    model_config = ConfigDict(extra="forbid")

    variables: dict[str, str] = Field(default_factory=dict)
    user_policy: dict[str, str] = Field(default_factory=dict)


class RenderedPrompt(BaseModel):
    """Rendered prompt payload ready for provider dispatch."""

    model_config = ConfigDict(extra="forbid")

    prompt_id: str
    rendered_text: str
    file_inputs: dict[str, str] = Field(default_factory=dict)
    substitutions: dict[str, str] = Field(default_factory=dict)


@dataclass(frozen=True)
class XmlNode:
    """Simple XML tree used for strict CLI-owned prompt projections."""

    tag: str
    text: str | None = None
    attrs: dict[str, str] = field(default_factory=dict)
    children: tuple[XmlNode, ...] = ()


def xml_text(tag: str, value: object, *, attrs: dict[str, object] | None = None) -> XmlNode:
    """Build a text XML node."""

    rendered_attrs = {name: str(attr_value) for name, attr_value in (attrs or {}).items()}
    return XmlNode(tag=tag, text="" if value is None else str(value), attrs=rendered_attrs)


def xml_node(tag: str, *children: XmlNode, attrs: dict[str, object] | None = None) -> XmlNode:
    """Build an XML node with children."""

    rendered_attrs = {name: str(attr_value) for name, attr_value in (attrs or {}).items()}
    return XmlNode(tag=tag, attrs=rendered_attrs, children=tuple(children))


def render_xml(node: XmlNode, *, indent: str = "  ", level: int = 0) -> str:
    """Render an XML node tree with escaped text content."""

    prefix = indent * level
    attrs = "".join(f' {name}="{escape(value, quote=True)}"' for name, value in node.attrs.items())
    if node.children:
        child_lines = [render_xml(child, indent=indent, level=level + 1) for child in node.children]
        return (
            f"{prefix}<{node.tag}{attrs}>\n" + "\n".join(child_lines) + f"\n{prefix}</{node.tag}>"
        )
    text = "" if node.text is None else escape(node.text, quote=False)
    return f"{prefix}<{node.tag}{attrs}>{text}</{node.tag}>"


def render_prompt(
    definition: PromptDefinition,
    *,
    command_inputs: dict[str, str],
    prompt: PromptInput | None = None,
) -> RenderedPrompt:
    """Render a CLI-owned prompt definition using shared substitution rules."""

    prompt_input = prompt or PromptInput()

    disallowed = sorted(set(prompt_input.user_policy) - set(definition.allowed_user_policy_keys))
    if disallowed:
        raise PromptRenderError(
            code="prompt_user_policy_key_not_allowed",
            message="Prompt user policy contains keys not allowed by this prompt definition.",
            details={"disallowed_keys": disallowed},
        )

    file_inputs: dict[str, str] = {}
    text_inputs: dict[str, str] = {}

    for input_definition in definition.command_scoped_inputs:
        if input_definition.name not in command_inputs:
            raise PromptRenderError(
                code="prompt_input_missing",
                message=f"Prompt input '{input_definition.name}' is missing.",
                details={"input_name": input_definition.name},
            )
        value = command_inputs[input_definition.name]
        if input_definition.input_kind == "file":
            file_inputs[input_definition.name] = value
        else:
            text_inputs[input_definition.name] = value

    substitutions = {
        **prompt_input.variables,
        **{
            key: value
            for key, value in prompt_input.user_policy.items()
            if key in definition.allowed_user_policy_keys
        },
        **text_inputs,
    }

    required_tokens = sorted(set(TOKEN_PATTERN.findall(definition.template_text)))
    missing_tokens = [token for token in required_tokens if token not in substitutions]
    if missing_tokens:
        raise PromptRenderError(
            code="prompt_render_missing_token",
            message="Prompt rendering is missing one or more required substitution tokens.",
            details={"missing_tokens": missing_tokens},
        )

    rendered_text = definition.template_text
    for token in required_tokens:
        rendered_text = rendered_text.replace(f"{{{{{token}}}}}", substitutions[token])

    return RenderedPrompt(
        prompt_id=definition.id,
        rendered_text=rendered_text,
        file_inputs=file_inputs,
        substitutions={token: substitutions[token] for token in required_tokens},
    )
