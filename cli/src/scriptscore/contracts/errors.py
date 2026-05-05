# SPDX-License-Identifier: AGPL-3.0-only
"""Typed error taxonomy for ScriptScore commands."""

from __future__ import annotations

from enum import StrEnum
from typing import Any

from pydantic import BaseModel, ConfigDict, ValidationError


class ErrorCategory(StrEnum):
    """Stable command error categories."""

    VALIDATION = "validation"
    PREREQUISITE = "prerequisite"
    NOT_FOUND = "not_found"
    CONFLICT = "conflict"
    PROVIDER = "provider"
    EXTERNAL_DEPENDENCY = "external_dependency"
    RESOURCE = "resource"
    EXECUTION = "execution"
    CANCELLED = "cancelled"


class WriteState(StrEnum):
    """Stable write-state semantics for command errors."""

    NO_WRITE = "no_write"
    PARTIAL_WRITE_ALLOWED = "partial_write_allowed"
    WRITTEN_BEFORE_FAILURE = "written_before_failure"


class ValidationIssue(BaseModel):
    """Structured validation issue emitted in validation failures."""

    model_config = ConfigDict(extra="forbid")

    path: list[str | int]
    code: str
    message: str


class ScriptscoreError(Exception):
    """Base typed exception mapped into the shared error envelope."""

    def __init__(
        self,
        *,
        code: str,
        message: str,
        category: ErrorCategory,
        retryable: bool,
        details: dict[str, Any] | None = None,
        write_state: WriteState = WriteState.NO_WRITE,
    ) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.category = category
        self.retryable = retryable
        self.details = details or {}
        self.write_state = write_state


class ValidationFailedError(ScriptscoreError):
    """Validation failure with structured issues."""

    def __init__(
        self, issues: list[ValidationIssue], *, message: str = "Request payload is invalid."
    ) -> None:
        super().__init__(
            code="validation_failed",
            message=message,
            category=ErrorCategory.VALIDATION,
            retryable=True,
            details={"issues": [issue.model_dump(mode="json") for issue in issues]},
            write_state=WriteState.NO_WRITE,
        )


class ProviderUnavailableError(ScriptscoreError):
    """Raised when a requested provider cannot be resolved."""

    def __init__(self, *, capability: str, provider_name: str) -> None:
        super().__init__(
            code="provider_unavailable",
            message=f"Provider '{provider_name}' for capability '{capability}' is unavailable.",
            category=ErrorCategory.PROVIDER,
            retryable=False,
            details={"capability": capability, "provider": provider_name},
            write_state=WriteState.NO_WRITE,
        )


class ProviderCompatibilityError(ScriptscoreError):
    """Raised when a provider violates the frozen interface contract."""

    def __init__(
        self,
        *,
        capability: str,
        provider_name: str,
        expected_version: str,
        actual_version: str | None,
    ) -> None:
        super().__init__(
            code="provider_incompatible",
            message=(
                f"Provider '{provider_name}' for capability '{capability}' is incompatible with this CLI build."
            ),
            category=ErrorCategory.PROVIDER,
            retryable=False,
            details={
                "capability": capability,
                "provider": provider_name,
                "expected_interface_version": expected_version,
                "actual_interface_version": actual_version,
            },
            write_state=WriteState.NO_WRITE,
        )


class ConflictError(ScriptscoreError):
    """Raised for transport or command-level conflicts."""

    def __init__(
        self,
        *,
        code: str = "conflict",
        message: str = "The requested operation conflicts with an active one.",
    ) -> None:
        super().__init__(
            code=code,
            message=message,
            category=ErrorCategory.CONFLICT,
            retryable=True,
            write_state=WriteState.NO_WRITE,
        )


class CancelledCommandError(ScriptscoreError):
    """Raised when the caller cancels the active command."""

    def __init__(self, *, message: str = "Command was cancelled.") -> None:
        super().__init__(
            code="cancelled",
            message=message,
            category=ErrorCategory.CANCELLED,
            retryable=True,
            write_state=WriteState.NO_WRITE,
        )


def validation_issues_from_exception(exc: ValidationError) -> list[ValidationIssue]:
    """Convert a pydantic validation error into stable issue objects."""

    issues: list[ValidationIssue] = []
    for entry in exc.errors():
        issues.append(
            ValidationIssue(
                path=list(entry.get("loc", ())),
                code=str(entry.get("type", "invalid")),
                message=str(entry.get("msg", "Invalid value.")),
            )
        )
    return issues


def exit_code_for_category(category: ErrorCategory) -> int:
    """Map stable error categories to direct CLI exit codes."""

    return {
        ErrorCategory.VALIDATION: 2,
        ErrorCategory.PREREQUISITE: 2,
        ErrorCategory.NOT_FOUND: 3,
        ErrorCategory.CONFLICT: 4,
        ErrorCategory.PROVIDER: 5,
        ErrorCategory.EXTERNAL_DEPENDENCY: 6,
        ErrorCategory.RESOURCE: 7,
        ErrorCategory.EXECUTION: 8,
        ErrorCategory.CANCELLED: 130,
    }[category]
