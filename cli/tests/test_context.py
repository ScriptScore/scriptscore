# SPDX-License-Identifier: AGPL-3.0-only
"""Runtime context and cancellation tests."""

from __future__ import annotations

import pytest

from scriptscore.contracts import CancelledCommandError
from scriptscore.contracts.envelopes import ProgressEvent
from scriptscore.providers import ProviderRegistry
from scriptscore.runtime import CancellationToken, CommandContext


def test_cancellation_token_raises_cancelled_error() -> None:
    token = CancellationToken()
    token.cancel()
    with pytest.raises(CancelledCommandError):
        token.check()


def test_command_context_emit_increments_sequence() -> None:
    events: list[ProgressEvent] = []
    ctx = CommandContext(
        command="smoke.ping",
        operation_id="op_test",
        request_id="req_1",
        provider_registry=ProviderRegistry.with_builtin_fakes(),
        event_sink=events.append,
    )
    ctx.emit(event="started", data={"a": 1})
    ctx.emit(event="completed", data={"a": 2})
    assert [event.sequence for event in events] == [1, 2]
