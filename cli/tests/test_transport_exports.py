# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for lazy transport exports."""

from __future__ import annotations

import pytest

import scriptscore.transport as transport


def test_transport_lazy_exports_resolve_servers() -> None:
    assert transport.__getattr__("DesktopWorkerServer").__name__ == "DesktopWorkerServer"
    assert transport.__getattr__("SidecarServer").__name__ == "SidecarServer"


def test_transport_lazy_exports_reject_unknown_names() -> None:
    with pytest.raises(AttributeError, match="MissingServer"):
        transport.__getattr__("MissingServer")
