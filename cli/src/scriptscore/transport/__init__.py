# SPDX-License-Identifier: AGPL-3.0-only
"""Transport exports."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from scriptscore.transport.desktop_worker import DesktopWorkerServer
    from scriptscore.transport.sidecar import SidecarServer

__all__ = ["DesktopWorkerServer", "SidecarServer"]


def __getattr__(name: str) -> Any:
    if name == "DesktopWorkerServer":
        from scriptscore.transport.desktop_worker import DesktopWorkerServer

        return DesktopWorkerServer
    if name == "SidecarServer":
        from scriptscore.transport.sidecar import SidecarServer

        return SidecarServer
    raise AttributeError(name)
