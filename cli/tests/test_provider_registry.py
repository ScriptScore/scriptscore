# SPDX-License-Identifier: AGPL-3.0-only
"""Provider registry tests."""

from __future__ import annotations

import pytest

from scriptscore.contracts import ProviderCompatibilityError, ProviderUnavailableError
from scriptscore.providers import (
    PROVIDER_INTERFACE_VERSION,
    FakeAlignmentProvider,
    FakeLlmProvider,
    FakeProvider,
    ProviderRegistry,
)
from scriptscore.providers import registry as registry_module


class _CallableProviderFactory:
    def __call__(self) -> FakeProvider:
        return FakeProvider(capability="llm_provider", provider_name="callable_fake")


def test_builtin_core_provider_is_available() -> None:
    registry = ProviderRegistry.with_builtin_core()
    assert (
        registry.resolve("alignment_engine", "core_template_match").provider_name
        == "core_template_match"
    )
    assert registry.resolve("llm_provider", "ollama_native").provider_name == "ollama_native"
    assert registry.resolve("llm_provider", "ollama_cloud").provider_name == "ollama_cloud"


def test_builtin_fakes_are_available() -> None:
    registry = ProviderRegistry.with_builtin_fakes()
    assert isinstance(
        registry.resolve("alignment_engine", "core_template_match"), FakeAlignmentProvider
    )
    provider = registry.resolve("llm_provider", "ollama_native")
    assert isinstance(provider, FakeLlmProvider)
    assert provider.provider_name == "ollama_native"


def test_runtime_registry_uses_builtin_providers_only() -> None:
    registry = ProviderRegistry.for_runtime()
    assert (
        registry.resolve("alignment_engine", "core_template_match").provider_name
        == "core_template_match"
    )
    assert registry.resolve("llm_provider", "ollama_native").provider_name == "ollama_native"
    assert registry.resolve("llm_provider", "ollama_cloud").provider_name == "ollama_cloud"


def test_runtime_registry_can_include_test_fakes() -> None:
    registry = ProviderRegistry.for_runtime(include_builtin_fakes=True)
    assert isinstance(
        registry.resolve("alignment_engine", "core_template_match"), FakeAlignmentProvider
    )
    assert isinstance(registry.resolve("llm_provider", "ollama_native"), FakeLlmProvider)
    assert registry.resolve("llm_provider", "ollama_cloud").provider_name == "ollama_cloud"


def test_missing_provider_raises_typed_error() -> None:
    registry = ProviderRegistry.with_builtin_fakes()
    with pytest.raises(ProviderUnavailableError):
        registry.resolve("llm_provider", "missing")


def test_incompatible_provider_version_is_rejected() -> None:
    registry = ProviderRegistry()
    provider = FakeProvider(
        capability="llm_provider",
        provider_name="bad_fake",
        interface_version=f"{PROVIDER_INTERFACE_VERSION}.mismatch",
    )
    with pytest.raises(ProviderCompatibilityError):
        registry.register(provider)


def test_provider_coercion_accepts_callable_factories() -> None:
    provider = registry_module._coerce_provider("llm_provider", _CallableProviderFactory())

    assert provider.provider_name == "callable_fake"


def test_provider_coercion_rejects_wrong_capability() -> None:
    with pytest.raises(ProviderCompatibilityError):
        registry_module._coerce_provider(
            "alignment_engine",
            FakeProvider(capability="llm_provider", provider_name="wrong_capability"),
        )


def test_resolve_llm_rejects_provider_without_llm_protocol() -> None:
    registry = ProviderRegistry(
        {
            ("llm_provider", "basic_fake"): FakeProvider(
                capability="llm_provider", provider_name="basic_fake"
            )
        }
    )

    with pytest.raises(ProviderCompatibilityError):
        registry.resolve_llm("basic_fake")


def test_resolve_alignment_rejects_provider_without_alignment_protocol() -> None:
    registry = ProviderRegistry(
        {
            ("alignment_engine", "basic_fake"): FakeProvider(
                capability="alignment_engine", provider_name="basic_fake"
            )
        }
    )

    with pytest.raises(ProviderCompatibilityError):
        registry.resolve_alignment("basic_fake")


def test_available_lists_sorted_provider_names_by_capability() -> None:
    registry = ProviderRegistry(
        {
            ("llm_provider", "zeta"): FakeProvider(capability="llm_provider", provider_name="zeta"),
            ("llm_provider", "alpha"): FakeProvider(
                capability="llm_provider", provider_name="alpha"
            ),
            ("alignment_engine", "align"): FakeProvider(
                capability="alignment_engine", provider_name="align"
            ),
        }
    )

    assert registry.available("llm_provider") == ["alpha", "zeta"]
