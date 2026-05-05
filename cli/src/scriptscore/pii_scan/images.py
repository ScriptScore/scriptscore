# SPDX-License-Identifier: AGPL-3.0-only
"""Image preparation helpers for local handwriting and PII scanning."""

from __future__ import annotations

from pathlib import Path
from typing import cast

import cv2
import numpy as np
from PIL import Image, ImageOps

from scriptscore.pii_scan.types import RasterBundle


def prepare_crop(path: Path, *, max_dimension: int = 1800) -> RasterBundle:
    """Load, orient, flatten, and binarize one crop image."""

    with Image.open(path) as source:
        oriented = ImageOps.exif_transpose(source).convert("RGBA")
        opaque_background = Image.new("RGBA", oriented.size, (255, 255, 255, 255))
        flattened = Image.alpha_composite(opaque_background, oriented).convert("RGB")
        color_bgr = cv2.cvtColor(np.array(flattened), cv2.COLOR_RGB2BGR)

    original_height, original_width = color_bgr.shape[:2]
    resize_ratio = 1.0
    longest_edge = max(original_height, original_width)
    if longest_edge > max_dimension:
        resize_ratio = max_dimension / float(longest_edge)
        color_bgr = cv2.resize(
            color_bgr,
            None,
            fx=resize_ratio,
            fy=resize_ratio,
            interpolation=cv2.INTER_AREA,
        )

    grayscale = cv2.cvtColor(color_bgr, cv2.COLOR_BGR2GRAY)
    grayscale = cv2.fastNlMeansDenoising(
        grayscale,
        None,
        h=7,
        templateWindowSize=7,
        searchWindowSize=21,
    )
    grayscale = cast(
        np.ndarray,
        cv2.normalize(grayscale, grayscale.copy(), alpha=0, beta=255, norm_type=cv2.NORM_MINMAX),
    )
    binary_mask = cv2.adaptiveThreshold(
        grayscale,
        255,
        cv2.ADAPTIVE_THRESH_GAUSSIAN_C,
        cv2.THRESH_BINARY,
        31,
        11,
    )

    return RasterBundle(
        path=path,
        color_bgr=color_bgr,
        grayscale=grayscale,
        binary_mask=binary_mask,
        original_size=(original_width, original_height),
        working_size=(color_bgr.shape[1], color_bgr.shape[0]),
        resize_ratio=resize_ratio,
    )


def text_structure_score(raster: RasterBundle) -> float:
    """Estimate how much text-like structure appears in the crop."""

    grayscale = cast(np.ndarray, raster.grayscale)
    binary_mask = cast(np.ndarray, raster.binary_mask)
    edges = cv2.Canny(grayscale, 80, 180)
    edge_density = float(np.count_nonzero(edges)) / float(edges.size)
    dark_fraction = float(np.count_nonzero(binary_mask < 160)) / float(binary_mask.size)
    return min(1.0, edge_density * 5.0 + dark_fraction * 4.0)


def region_ink_fraction(
    binary_mask: object,
    *,
    left: int,
    top: int,
    right: int,
    bottom: int,
) -> float:
    """Measure the share of dark pixels in one rectangular region."""

    mask = cast(np.ndarray, binary_mask)
    height, width = mask.shape[:2]
    clamped_left = max(0, min(width, left))
    clamped_right = max(0, min(width, right))
    clamped_top = max(0, min(height, top))
    clamped_bottom = max(0, min(height, bottom))
    if clamped_right <= clamped_left or clamped_bottom <= clamped_top:
        return 0.0
    region = mask[clamped_top:clamped_bottom, clamped_left:clamped_right]
    if region.size == 0:
        return 0.0
    return float(np.count_nonzero(region < 160)) / float(region.size)
