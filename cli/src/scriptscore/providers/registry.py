# SPDX-License-Identifier: AGPL-3.0-only
"""Provider registry assembly and compatibility checks."""

from __future__ import annotations

from typing import Any, cast

from scriptscore.contracts import ProviderCompatibilityError, ProviderUnavailableError
from scriptscore.providers.constants import PROVIDER_INTERFACE_VERSION
from scriptscore.providers.core_template_match import CoreTemplateMatchProvider
from scriptscore.providers.fake import builtin_fake_providers
from scriptscore.providers.interfaces import (
    AlignmentProvider,
    LlmProvider,
    Provider,
    ProviderCapability,
)
from scriptscore.providers.ollama import OllamaCloudProvider, OllamaNativeProvider


def _coerce_provider(capability: ProviderCapability, loaded: Any) -> Provider:
    if isinstance(loaded, type):
        candidate = loaded()
    else:
        candidate = (
            loaded() if callable(loaded) and not hasattr(loaded, "provider_name") else loaded
        )
    provider_name = getattr(candidate, "provider_name", None)
    provider_capability = getattr(candidate, "capability", None)
    interface_version = getattr(candidate, "interface_version", None)
    if not provider_name or provider_capability != capability:
        raise ProviderCompatibilityError(
            capability=capability,
            provider_name=str(provider_name or "<unknown>"),
            expected_version=PROVIDER_INTERFACE_VERSION,
            actual_version=interface_version,
        )
    if interface_version != PROVIDER_INTERFACE_VERSION:
        raise ProviderCompatibilityError(
            capability=capability,
            provider_name=provider_name,
            expected_version=PROVIDER_INTERFACE_VERSION,
            actual_version=interface_version,
        )
    return cast(Provider, candidate)


class ProviderRegistry:
    """Registry for resolved providers."""

    def __init__(self, providers: dict[tuple[str, str], Provider] | None = None) -> None:
        self._providers: dict[tuple[str, str], Provider] = providers or {}

    @classmethod
    def with_builtin_core(cls) -> ProviderRegistry:
        """Create a registry preloaded with built-in runtime providers."""

        providers = [CoreTemplateMatchProvider(), OllamaNativeProvider(), OllamaCloudProvider()]
        return cls(
            providers={
                (provider.capability, provider.provider_name): provider for provider in providers
            }
        )

    @classmethod
    def with_builtin_fakes(cls) -> ProviderRegistry:
        """Create a registry preloaded with the test fake providers."""

        return cls(providers=cast(dict[tuple[str, str], Provider], builtin_fake_providers()))

    @classmethod
    def for_runtime(
        cls,
        *,
        include_builtin_fakes: bool = False,
    ) -> ProviderRegistry:
        """Create the runtime registry from built-in providers only."""

        providers: dict[tuple[str, str], Provider] = dict(cls.with_builtin_core()._providers)
        if include_builtin_fakes:
            providers.update(cast(dict[tuple[str, str], Provider], builtin_fake_providers()))
        return cls(providers=providers)

    def register(self, provider: Provider) -> None:
        """Register a provider explicitly."""

        validated = _coerce_provider(provider.capability, provider)
        self._providers[(provider.capability, validated.provider_name)] = validated

    def resolve(self, capability: ProviderCapability, provider_name: str) -> Provider:
        """Resolve a provider by capability and name."""

        provider = self._providers.get((capability, provider_name))
        if provider is None:
            raise ProviderUnavailableError(capability=capability, provider_name=provider_name)
        return provider

    def resolve_llm(self, provider_name: str) -> LlmProvider:
        """Resolve an LLM provider and validate its richer protocol."""

        provider = self.resolve("llm_provider", provider_name)
        if not isinstance(provider, LlmProvider):
            raise ProviderCompatibilityError(
                capability="llm_provider",
                provider_name=provider_name,
                expected_version=PROVIDER_INTERFACE_VERSION,
                actual_version=getattr(provider, "interface_version", None),
            )
        return provider

    def resolve_alignment(self, provider_name: str) -> AlignmentProvider:
        """Resolve an alignment provider and validate its richer protocol."""

        provider = self.resolve("alignment_engine", provider_name)
        if not isinstance(provider, AlignmentProvider):
            raise ProviderCompatibilityError(
                capability="alignment_engine",
                provider_name=provider_name,
                expected_version=PROVIDER_INTERFACE_VERSION,
                actual_version=getattr(provider, "interface_version", None),
            )
        return provider

    def available(self, capability: ProviderCapability) -> list[str]:
        """List available providers for a given capability."""

        names = [
            provider_name
            for registered_capability, provider_name in self._providers
            if registered_capability == capability
        ]
        return sorted(names)
