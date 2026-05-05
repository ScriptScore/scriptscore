# SPDX-License-Identifier: AGPL-3.0-only
"""Runtime execution context and cancellation support."""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
from datetime import UTC, datetime
from threading import Event as ThreadEvent
from threading import Lock

from scriptscore.contracts import (
    CancelledCommandError,
    LlmTokenUsage,
    LlmTokenUsageCall,
    LlmTokenUsageSummary,
    ProgressEvent,
    ProgressSummary,
)
from scriptscore.providers import ProviderRegistry

EventSink = Callable[[ProgressEvent], None]


def utc_now_iso() -> str:
    """Return an RFC 3339 UTC timestamp."""

    return datetime.now(UTC).isoformat().replace("+00:00", "Z")


class CancellationToken:
    """Thread-safe cancellation flag shared across transports and commands."""

    def __init__(self) -> None:
        self._event = ThreadEvent()

    def cancel(self) -> None:
        self._event.set()

    def is_cancelled(self) -> bool:
        return self._event.is_set()

    def check(self) -> None:
        if self.is_cancelled():
            raise CancelledCommandError()


@dataclass
class CommandContext:
    """Per-invocation command context."""

    command: str
    operation_id: str
    request_id: str | None
    provider_registry: ProviderRegistry
    event_sink: EventSink | None = None
    cancellation_token: CancellationToken = field(default_factory=CancellationToken)
    _sequence_lock: Lock = field(default_factory=Lock, init=False, repr=False)
    _sequence: int = field(default=0, init=False, repr=False)
    _llm_usage_calls: list[LlmTokenUsageCall] = field(default_factory=list, init=False, repr=False)

    def check_cancelled(self) -> None:
        """Raise a typed cancellation error when the invocation has been cancelled."""

        self.cancellation_token.check()

    def emit(
        self,
        *,
        event: str,
        level: str = "info",
        progress: ProgressSummary | None = None,
        scope: dict[str, object] | None = None,
        data: dict[str, object] | None = None,
    ) -> None:
        """Emit a structured progress event when an event sink is active."""

        if self.event_sink is None:
            return
        with self._sequence_lock:
            self._sequence += 1
            sequence = self._sequence
        event_model = ProgressEvent(
            command=self.command,
            operation_id=self.operation_id,
            request_id=self.request_id,
            sequence=sequence,
            event=event,
            level=level,
            timestamp=utc_now_iso(),
            progress=progress,
            scope=scope,
            data=data or {},
        )
        self.event_sink(event_model)

    def record_llm_usage(
        self,
        *,
        provider: str,
        model: str,
        prompt_id: str | None,
        step: str | None,
        scope: dict[str, object] | None,
        usage: LlmTokenUsage,
    ) -> None:
        """Record one LLM usage observation for the terminal command envelope."""

        self._llm_usage_calls.append(
            LlmTokenUsageCall(
                provider=provider,
                model=model,
                prompt_id=prompt_id,
                step=step,
                scope=None if scope is None else dict(scope),
                input_tokens=usage.input_tokens,
                output_tokens=usage.output_tokens,
                total_tokens=usage.total_tokens,
                count_source=usage.count_source,
                estimation_method=usage.estimation_method,
                image_input_count=usage.image_input_count,
            )
        )

    def llm_usage_summary(self) -> LlmTokenUsageSummary | None:
        """Return aggregate LLM usage recorded during this invocation."""

        if not self._llm_usage_calls:
            return None
        sources = {call.count_source for call in self._llm_usage_calls}
        count_source = sources.pop() if len(sources) == 1 else "mixed"
        return LlmTokenUsageSummary(
            input_tokens=sum(call.input_tokens for call in self._llm_usage_calls),
            output_tokens=sum(call.output_tokens for call in self._llm_usage_calls),
            total_tokens=sum(call.total_tokens for call in self._llm_usage_calls),
            count_source=count_source,
            calls=list(self._llm_usage_calls),
        )
