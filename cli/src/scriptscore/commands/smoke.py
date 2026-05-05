# SPDX-License-Identifier: AGPL-3.0-only
"""Test-only smoke command used to validate the shared execution path."""

from __future__ import annotations

from time import sleep

from pydantic import BaseModel, ConfigDict, Field

from scriptscore.contracts import ProgressSummary
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec


class SmokePingRequest(BaseModel):
    """Request schema for the internal smoke command."""

    model_config = ConfigDict(extra="forbid")

    message: str = "pong"
    sleep_ms: int = Field(default=0, ge=0, le=10000)
    steps: int = Field(default=1, ge=1, le=50)


def _percent(completed: int, total: int) -> int:
    return int((completed / total) * 100)


def handle_smoke_ping(ctx: CommandContext, request: SmokePingRequest) -> CommandOutcome:
    """Emit predictable progress events and return a small success payload."""

    total = request.steps
    ctx.emit(
        event="started",
        progress=ProgressSummary(completed=0, total=total, percent=0),
        data={"message": request.message, "total_stages": 1},
    )
    for index in range(total):
        ctx.check_cancelled()
        current_item = index + 1
        ctx.emit(
            event="item_started",
            progress=ProgressSummary(completed=index, total=total, percent=_percent(index, total)),
            scope={"item": current_item},
            data={"item": current_item},
        )
        if request.sleep_ms:
            sleep(request.sleep_ms / total / 1000)
        ctx.check_cancelled()
        ctx.emit(
            event="item_completed",
            progress=ProgressSummary(
                completed=current_item,
                total=total,
                percent=_percent(current_item, total),
            ),
            scope={"item": current_item},
            data={"item": current_item},
        )
    ctx.emit(
        event="completed",
        progress=ProgressSummary(completed=total, total=total, percent=100),
        data={"message": request.message},
    )
    return CommandOutcome(data={"message": request.message, "steps": request.steps})


def smoke_ping_spec() -> CommandSpec:
    """Return the registered smoke command spec."""

    return CommandSpec(name="smoke.ping", request_model=SmokePingRequest, handler=handle_smoke_ping)
