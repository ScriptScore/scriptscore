# SPDX-License-Identifier: AGPL-3.0-only
"""Provider interface exports."""

from scriptscore.providers.constants import PROVIDER_INTERFACE_VERSION
from scriptscore.providers.core_template_match import CoreTemplateMatchProvider
from scriptscore.providers.fake import (
    FakeAlignmentProvider,
    FakeLlmProvider,
    FakeProvider,
    builtin_fake_providers,
)
from scriptscore.providers.interfaces import (
    AlignmentProvider,
    AlignmentRequest,
    AlignmentResponse,
    LlmProvider,
    LlmProviderConfig,
    LlmRequest,
    LlmResponse,
    Provider,
    ProviderCapability,
    ProviderDescriptor,
)
from scriptscore.providers.ollama import OllamaCloudProvider, OllamaNativeProvider
from scriptscore.providers.registry import ProviderRegistry

__all__ = [
    "PROVIDER_INTERFACE_VERSION",
    "AlignmentProvider",
    "AlignmentRequest",
    "AlignmentResponse",
    "CoreTemplateMatchProvider",
    "FakeAlignmentProvider",
    "FakeLlmProvider",
    "FakeProvider",
    "LlmProvider",
    "LlmProviderConfig",
    "LlmRequest",
    "LlmResponse",
    "OllamaCloudProvider",
    "OllamaNativeProvider",
    "Provider",
    "ProviderCapability",
    "ProviderDescriptor",
    "ProviderRegistry",
    "builtin_fake_providers",
]
