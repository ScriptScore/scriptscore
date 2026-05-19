# SPDX-License-Identifier: AGPL-3.0-only
"""Test-only smoke command used to validate the shared execution path."""

from __future__ import annotations

from pathlib import Path
from time import monotonic, sleep

from pydantic import BaseModel, ConfigDict, Field

from scriptscore.contracts import ProgressSummary
from scriptscore.runtime import CommandContext, CommandOutcome, CommandSpec

WAIT_FOR_FILE_POLL_SECONDS = 0.05


class SmokePingRequest(BaseModel):
    """Request schema for the internal smoke command."""

    model_config = ConfigDict(extra="forbid")

    message: str = "pong"
    sleep_ms: int = Field(default=0, ge=0, le=10000)
    steps: int = Field(default=1, ge=1, le=50)
    wait_for_file: str | None = None
    wait_timeout_ms: int = Field(default=30000, ge=1, le=300000)


def _percent(completed: int, total: int) -> int:
    return int((completed / total) * 100)


def _wait_for_file(ctx: CommandContext, path: str, timeout_ms: int) -> None:
    marker_path = Path(path)
    deadline = monotonic() + (timeout_ms / 1000)
    while not marker_path.exists():
        ctx.check_cancelled()
        if monotonic() >= deadline:
            raise TimeoutError("Timed out waiting for smoke.ping release marker.")
        sleep(WAIT_FOR_FILE_POLL_SECONDS)
    ctx.check_cancelled()


def handle_smoke_ping(ctx: CommandContext, request: SmokePingRequest) -> CommandOutcome:
    """Emit predictable progress events and return a small success payload."""

    total = request.steps
    ctx.emit(
        event="started",
        progress=ProgressSummary(completed=0, total=total, percent=0),
        data={"message": request.message, "total_stages": 1},
    )
    if request.wait_for_file is not None:
        _wait_for_file(ctx, request.wait_for_file, request.wait_timeout_ms)
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
