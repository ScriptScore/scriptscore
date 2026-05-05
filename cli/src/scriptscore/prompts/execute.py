# SPDX-License-Identifier: AGPL-3.0-only
"""Shared prompt execution helpers backed by the LLM provider interface."""

from __future__ import annotations

from dataclasses import dataclass

from scriptscore.contracts.common import LlmConfig
from scriptscore.prompts.builtin import get_builtin_prompt
from scriptscore.prompts.render import RenderedPrompt, render_prompt
from scriptscore.providers import LlmProviderConfig, LlmRequest, LlmResponse
from scriptscore.runtime import CommandContext


@dataclass(frozen=True)
class PromptExecution:
    """Resolved prompt execution details for tracing and parsing."""

    provider_name: str
    rendered_prompt: RenderedPrompt
    request_options: dict[str, object]
    provider_response: LlmResponse


def execute_builtin_prompt(
    ctx: CommandContext,
    *,
    provider_name: str,
    prompt_id: str,
    command_inputs: dict[str, str],
    llm_config: LlmConfig,
) -> PromptExecution:
    """Render and execute one built-in CLI-owned prompt."""

    definition = get_builtin_prompt(prompt_id)
    rendered = render_prompt(definition, command_inputs=command_inputs)
    provider = ctx.provider_registry.resolve_llm(provider_name)
    merged_options = dict(llm_config.options)
    merged_options.update(definition.execution_options)
    response = provider.generate(
        LlmRequest(
            prompt_id=definition.id,
            response_mode=definition.response_mode,
            rendered_text=rendered.rendered_text,
            provider_config=LlmProviderConfig(
                base_url=llm_config.base_url,
                model=llm_config.model,
                api_key=llm_config.api_key,
                timeout_seconds=llm_config.timeout_seconds,
                keep_alive=llm_config.keep_alive,
                options=llm_config.options,
            ),
            file_inputs=rendered.file_inputs,
            execution_options=merged_options,
            response_contract=definition.response_contract,
        )
    )
    return PromptExecution(
        provider_name=provider_name,
        rendered_prompt=rendered,
        request_options=dict(merged_options),
        provider_response=response,
    )
