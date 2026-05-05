# SPDX-License-Identifier: AGPL-3.0-only
"""Shared ArUco marker helpers for template stamping and alignment."""

from __future__ import annotations

from io import BytesIO
from typing import Any

from PIL import Image

from scriptscore.contracts import ErrorCategory, ScriptscoreError

ARUCO_DICTIONARY_NAME = "DICT_4X4_100"
ARUCO_DICTIONARY_CAPACITY = 100
_ARUCO_DICTIONARY_ID = "DICT_4X4_100"


def _provider_error(
    *, code: str, message: str, details: dict[str, Any], retryable: bool = False
) -> ScriptscoreError:
    return ScriptscoreError(
        code=code,
        message=message,
        category=ErrorCategory.PROVIDER,
        retryable=retryable,
        details=details,
    )


def _load_cv_dependencies() -> tuple[Any, Any]:
    try:
        import cv2
        import numpy as np
    except ModuleNotFoundError as exc:
        raise _provider_error(
            code="alignment_dependency_unavailable",
            message="OpenCV alignment dependencies are not installed.",
            details={"missing_module": exc.name},
        ) from exc
    return cv2, np


def aruco_dictionary(cv2: Any) -> Any:
    """Return the shared marker dictionary used by stamping and alignment."""

    dictionary_id = getattr(cv2.aruco, _ARUCO_DICTIONARY_ID)
    return cv2.aruco.getPredefinedDictionary(dictionary_id)


def _detect_aruco_markers(image: Any, *, cv2: Any) -> dict[int, Any]:
    if not hasattr(cv2, "aruco"):
        return {}
    aruco = cv2.aruco
    dictionary = aruco_dictionary(cv2)
    if hasattr(aruco, "ArucoDetector"):
        detector = aruco.ArucoDetector(dictionary, aruco.DetectorParameters())
        corners, ids, _rejected = detector.detectMarkers(image)
    else:
        corners, ids, _rejected = aruco.detectMarkers(image, dictionary)
    if ids is None:
        return {}
    return {
        int(marker_id): marker_corners.reshape(4, 2)
        for marker_id, marker_corners in zip(ids.flatten().tolist(), corners, strict=False)
    }


def _marker_centers(markers: dict[int, Any], *, np: Any) -> dict[int, Any]:
    return {marker_id: np.mean(corners, axis=0) for marker_id, corners in markers.items()}


def detect_aruco_ids_from_png_bytes(png_bytes: bytes) -> list[int]:
    """Decode PNG bytes and return sorted marker ids using the shared dictionary."""

    cv2, np = _load_cv_dependencies()
    image = Image.open(BytesIO(png_bytes)).convert("L")
    pixels = np.array(image)
    return sorted(_detect_aruco_markers(pixels, cv2=cv2))


def generate_marker_png_bytes(marker_id: int, *, size_px: int = 256, border_bits: int = 1) -> bytes:
    """Generate one shared-dictionary ArUco marker as PNG bytes."""

    cv2, _np = _load_cv_dependencies()
    if marker_id < 0 or marker_id >= ARUCO_DICTIONARY_CAPACITY:
        raise ValueError(f"marker_id must be within {ARUCO_DICTIONARY_NAME} capacity.")
    aruco = cv2.aruco
    dictionary = aruco_dictionary(cv2)
    if hasattr(aruco, "generateImageMarker"):
        marker = aruco.generateImageMarker(dictionary, marker_id, size_px, borderBits=border_bits)
    else:
        marker = aruco.drawMarker(dictionary, marker_id, size_px, borderBits=border_bits)
    image = Image.fromarray(marker).convert("L")
    buffer = BytesIO()
    image.save(buffer, format="PNG")
    return buffer.getvalue()
