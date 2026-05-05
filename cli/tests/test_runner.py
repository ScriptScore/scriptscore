# SPDX-License-Identifier: AGPL-3.0-only
"""Command runner error-path and success-path tests."""

from __future__ import annotations

from pydantic import BaseModel, ConfigDict

from scriptscore.contracts import CommandErrorEnvelope, ErrorCategory, ScriptscoreError
from scriptscore.providers import ProviderRegistry
from scriptscore.runtime import CommandOutcome, CommandRegistry, CommandRunner, CommandSpec


class EmptyRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")


def test_runner_returns_validation_envelope_for_unknown_command() -> None:
    runner = CommandRunner(
        registry=CommandRegistry(), provider_registry=ProviderRegistry.with_builtin_fakes()
    )
    result = runner.run("missing.command", {})
    assert result.exit_code == 2
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.category == ErrorCategory.VALIDATION


def test_runner_returns_validation_envelope_for_bad_request() -> None:
    registry = CommandRegistry()
    from scriptscore.commands.smoke import smoke_ping_spec

    registry.register(smoke_ping_spec())
    runner = CommandRunner(
        registry=registry, provider_registry=ProviderRegistry.with_builtin_fakes()
    )
    result = runner.run("smoke.ping", {"sleep_ms": -1})
    assert result.exit_code == 2
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.code == "validation_failed"


def test_runner_maps_typed_command_error() -> None:
    def handler(_ctx: object, _request: EmptyRequest) -> CommandOutcome:
        raise ScriptscoreError(
            code="boom",
            message="broken",
            category=ErrorCategory.EXECUTION,
            retryable=False,
        )

    registry = CommandRegistry()
    registry.register(CommandSpec(name="test.boom", request_model=EmptyRequest, handler=handler))
    runner = CommandRunner(
        registry=registry, provider_registry=ProviderRegistry.with_builtin_fakes()
    )
    result = runner.run("test.boom", {})
    assert result.exit_code == 8
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.code == "boom"


def test_runner_maps_unhandled_exception_to_execution_error() -> None:
    def handler(_ctx: object, _request: EmptyRequest) -> CommandOutcome:
        raise RuntimeError("unexpected")

    registry = CommandRegistry()
    registry.register(CommandSpec(name="test.crash", request_model=EmptyRequest, handler=handler))
    runner = CommandRunner(
        registry=registry, provider_registry=ProviderRegistry.with_builtin_fakes()
    )
    result = runner.run("test.crash", {})
    assert result.exit_code == 8
    assert isinstance(result.envelope, CommandErrorEnvelope)
    assert result.envelope.error.code == "execution_failed"
