# SPDX-License-Identifier: AGPL-3.0-only
"""Local handwriting and PII scan dataclasses."""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Literal

ScanPiiKind = Literal["name", "username", "email", "phone_number"]
ScanHandwritingState = Literal["true", "false", "unknown"]


@dataclass(slots=True)
class VisionToken:
    """One OCR token in image coordinates."""

    text: str
    confidence: float
    corners: tuple[tuple[int, int], tuple[int, int], tuple[int, int], tuple[int, int]]

    @property
    def left(self) -> int:
        return min(point[0] for point in self.corners)

    @property
    def right(self) -> int:
        return max(point[0] for point in self.corners)

    @property
    def top(self) -> int:
        return min(point[1] for point in self.corners)

    @property
    def bottom(self) -> int:
        return max(point[1] for point in self.corners)

    @property
    def width(self) -> int:
        return max(1, self.right - self.left)

    @property
    def height(self) -> int:
        return max(1, self.bottom - self.top)

    @property
    def center_x(self) -> float:
        return (self.left + self.right) / 2.0

    @property
    def center_y(self) -> float:
        return (self.top + self.bottom) / 2.0


@dataclass(slots=True)
class RasterBundle:
    """Prepared image data for OCR and heuristic analysis."""

    path: Path
    color_bgr: object
    grayscale: object
    binary_mask: object
    original_size: tuple[int, int]
    working_size: tuple[int, int]
    resize_ratio: float


@dataclass(slots=True)
class WritingSignal:
    """Local handwriting decision with supporting metrics."""

    state: ScanHandwritingState
    score: float
    reasons: list[str]
    metrics: dict[str, object] = field(default_factory=dict)


@dataclass(slots=True)
class SensitiveHit:
    """One candidate PII match before command-level summarization."""

    kind: ScanPiiKind
    snippet: str
    confidence: float
    reason: str


@dataclass(slots=True)
class ReadResult:
    """OCR read result for one crop."""

    tokens: list[VisionToken]
    backend_name: str


@dataclass(slots=True)
class ScanRuntimeOptions:
    """Execution settings for one local scan run."""

    model_root: Path
    max_dimension: int = 1800
    include_text: bool = False
    include_metrics: bool = True


@dataclass(slots=True)
class ScanFinding:
    """Combined handwriting and PII result for one crop."""

    image_path: str
    handwriting_state: ScanHandwritingState
    handwriting_score: float
    pii_present: bool
    pii_kinds: list[ScanPiiKind]
    reasons: list[str]
    duration_seconds: float
    stage_durations: dict[str, float] = field(default_factory=dict)
    metrics: dict[str, object] = field(default_factory=dict)
    extracted_text: str | None = None
    backend_warnings: list[str] = field(default_factory=list)
    fatal_error: str | None = None
