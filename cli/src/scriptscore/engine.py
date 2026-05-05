# SPDX-License-Identifier: AGPL-3.0-only
"""Public in-process engine surface for ScriptScore."""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

from scriptscore.commands import build_command_registry
from scriptscore.contracts import ProgressEvent
from scriptscore.providers import ProviderRegistry
from scriptscore.runtime import CancellationToken, CommandRunner, RunResult

ProgressSink = Callable[[ProgressEvent], None]


class ScriptScoreEngine:
    """Transport-free in-process command execution engine."""

    def __init__(self, *, runner: CommandRunner) -> None:
        self._runner = runner

    def run(
        self,
        command_name: str,
        request_payload: Any,
        *,
        request_id: str | None = None,
        event_sink: ProgressSink | None = None,
        cancellation_token: CancellationToken | None = None,
    ) -> RunResult:
        """Validate and execute one bounded command invocation."""

        return self._runner.run(
            command_name,
            request_payload,
            request_id=request_id,
            event_sink=event_sink,
            cancellation_token=cancellation_token,
        )


def create_engine(*, include_builtin_fakes: bool = False) -> ScriptScoreEngine:
    """Create a ScriptScore engine with built-in runtime providers only."""

    runner = CommandRunner(
        registry=build_command_registry(),
        provider_registry=ProviderRegistry.for_runtime(
            include_builtin_fakes=include_builtin_fakes,
        ),
    )
    return ScriptScoreEngine(runner=runner)
