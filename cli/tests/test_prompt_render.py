# SPDX-License-Identifier: AGPL-3.0-only
"""Prompt rendering and XML projection tests."""

from __future__ import annotations

import pytest

from scriptscore.prompts import (
    PromptDefinition,
    PromptInput,
    PromptInputDefinition,
    PromptRenderError,
    get_builtin_prompt,
    render_prompt,
    render_xml,
    xml_node,
    xml_text,
)


def test_render_prompt_combines_command_inputs_and_allowed_user_policy() -> None:
    definition = PromptDefinition(
        id="draft",
        response_mode="plain_text",
        template_text="Hello {{subject}} in {{language}}.",
        command_scoped_inputs=[PromptInputDefinition(name="subject", input_kind="text")],
        allowed_user_policy_keys=["language"],
    )
    rendered = render_prompt(
        definition,
        command_inputs={"subject": "calculus"},
        prompt=PromptInput(user_policy={"language": "en"}),
    )
    assert rendered.rendered_text == "Hello calculus in en."


def test_render_prompt_separates_file_inputs_from_text_substitution() -> None:
    definition = PromptDefinition(
        id="vision",
        response_mode="plain_text",
        template_text="Look at {{question_text}}.",
        command_scoped_inputs=[
            PromptInputDefinition(name="question_text", input_kind="text"),
            PromptInputDefinition(name="question_image", input_kind="file"),
        ],
    )
    rendered = render_prompt(
        definition,
        command_inputs={"question_text": "What is 2+2?", "question_image": "/tmp/question.png"},
    )
    assert rendered.rendered_text == "Look at What is 2+2?."
    assert rendered.file_inputs == {"question_image": "/tmp/question.png"}


def test_render_prompt_rejects_disallowed_user_policy_keys() -> None:
    definition = PromptDefinition(
        id="draft",
        response_mode="plain_text",
        template_text="Hello {{subject}}.",
        command_scoped_inputs=[PromptInputDefinition(name="subject", input_kind="text")],
    )
    with pytest.raises(PromptRenderError, match="not allowed"):
        render_prompt(
            definition,
            command_inputs={"subject": "calculus"},
            prompt=PromptInput(user_policy={"tone": "friendly"}),
        )


def test_render_prompt_rejects_missing_required_token() -> None:
    definition = PromptDefinition(
        id="draft",
        response_mode="plain_text",
        template_text="Hello {{subject}} in {{language}}.",
        command_scoped_inputs=[PromptInputDefinition(name="subject", input_kind="text")],
    )
    with pytest.raises(PromptRenderError, match="missing"):
        render_prompt(definition, command_inputs={"subject": "calculus"})


def test_render_prompt_command_inputs_override_caller_variables() -> None:
    definition = PromptDefinition(
        id="draft",
        response_mode="plain_text",
        template_text="Hello {{subject}}.",
        command_scoped_inputs=[PromptInputDefinition(name="subject", input_kind="text")],
    )
    rendered = render_prompt(
        definition,
        command_inputs={"subject": "calculus"},
        prompt=PromptInput(variables={"subject": "chemistry"}),
    )
    assert rendered.rendered_text == "Hello calculus."


def test_render_xml_escapes_text_and_attributes() -> None:
    node = xml_node(
        "assessment",
        xml_text("label", 'x < y & "z"'),
        attrs={"kind": 'a"b', "points": 5},
    )
    rendered = render_xml(node)
    assert '<assessment kind="a&quot;b" points="5">' in rendered
    assert '<label>x &lt; y &amp; "z"</label>' in rendered


def test_builtin_question_context_prompt_keeps_normative_rules() -> None:
    template = get_builtin_prompt("question_context").template_text
    assert "Describe only context visible in the question image" in template
    assert "Do not answer the exam question, solve it, or explain how to solve it." in template
    assert "Do not simply restate the main question prose" in template
    assert "Do not include page headers, page footers, page numbers" in template
    assert "course or exam boilerplate" in template
    assert "If the image adds no material context beyond the cleaned question text" in template
    assert "If the only extra visible content is a header, footer, page number" in template
    assert "respond with exactly: None" in template


def test_builtin_rubric_generate_prompt_keeps_normative_hybrid_rules() -> None:
    template = get_builtin_prompt("rubric_generate").template_text
    assert "RUBRIC DESIGN RULES (HYBRID STRUCTURE)" in template
    assert "60-80% of total points should come from binary criteria" in template
    assert "No hidden gates:" in template
