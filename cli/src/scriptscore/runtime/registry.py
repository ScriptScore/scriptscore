# SPDX-License-Identifier: AGPL-3.0-only
"""Command registration primitives."""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from pydantic import BaseModel

from scriptscore.contracts import ArtifactReference, WarningObject
from scriptscore.runtime.context import CommandContext


@dataclass
class CommandOutcome:
    """Normalized command handler return payload."""

    data: dict[str, Any] = field(default_factory=dict)
    degraded: bool = False
    warnings: list[WarningObject] = field(default_factory=list)
    artifacts: list[ArtifactReference] = field(default_factory=list)
    providers: dict[str, str] | None = None
    output_artifacts_dir: Path | None = None
    manifest_data: dict[str, Any] = field(default_factory=dict)


CommandHandler = Callable[[CommandContext, Any], CommandOutcome]


@dataclass(frozen=True)
class CommandSpec:
    """Registered command definition."""

    name: str
    request_model: type[BaseModel]
    handler: CommandHandler


class CommandRegistry:
    """Lookup table for registered commands."""

    def __init__(self) -> None:
        self._commands: dict[str, CommandSpec] = {}

    def register(self, spec: CommandSpec) -> None:
        """Register a command specification."""

        self._commands[spec.name] = spec

    def get(self, name: str) -> CommandSpec:
        """Look up a command specification by dotted name."""

        return self._commands[name]

    def has(self, name: str) -> bool:
        """Return whether the registry contains the named command."""

        return name in self._commands
