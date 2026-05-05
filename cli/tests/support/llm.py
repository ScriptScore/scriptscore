# SPDX-License-Identifier: AGPL-3.0-only
"""Shared LLM request helpers for tests."""

from __future__ import annotations

from typing import Any


def llm_request_fields(
    provider_name: str = "ollama_native",
    *,
    model: str = "test-model",
) -> dict[str, Any]:
    """Return the shared provider-selection and llm_config fields."""

    return {
        "providers": {"llm_provider": provider_name},
        "llm_config": {
            "model": model,
        },
    }
