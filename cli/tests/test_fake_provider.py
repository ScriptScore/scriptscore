# SPDX-License-Identifier: AGPL-3.0-only
"""Fake provider tests."""

from __future__ import annotations

from scriptscore.providers import FakeLlmProvider, LlmProviderConfig, LlmRequest


def _llm_request(prompt_id: str, rendered_text: str) -> LlmRequest:
    return LlmRequest(
        prompt_id=prompt_id,
        response_mode="text",
        rendered_text=rendered_text,
        provider_config=LlmProviderConfig(model="fake-model"),
    )


def test_fake_feedback_draft_returns_strong_feedback_for_full_credit() -> None:
    provider = FakeLlmProvider()
    request = _llm_request(
        "feedback_draft",
        '<assessment question_max_points="10" total_points_awarded="10"></assessment>',
    )

    assert provider.generate(request).raw_text == "Strong work overall."


def test_fake_feedback_draft_returns_partial_feedback_for_non_matching_points() -> None:
    provider = FakeLlmProvider()
    request = _llm_request(
        "feedback_draft",
        '<assessment question_max_points="10" total_points_awarded="8"></assessment>',
    )

    assert "missed a key detail" in provider.generate(request).raw_text


def test_fake_feedback_draft_ignores_non_numeric_full_credit_attrs() -> None:
    provider = FakeLlmProvider()
    request = _llm_request(
        "feedback_draft",
        '<assessment question_max_points="full" total_points_awarded="full"></assessment>',
    )

    assert "missed a key detail" in provider.generate(request).raw_text


def test_fake_markup_returns_span_tagged_student_answer() -> None:
    provider = FakeLlmProvider()
    request = _llm_request("markup", "<student_answer>return xs[1:]</student_answer>")

    assert provider.generate(request).raw_text == '<span data-kind="correct">return xs[1:]</span>'
