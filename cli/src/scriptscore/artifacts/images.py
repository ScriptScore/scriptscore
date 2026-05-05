# SPDX-License-Identifier: AGPL-3.0-only
"""Deterministic local image helpers for phase-one scan commands."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from PIL import Image

from scriptscore.contracts.scans import Region, Transform

NORMALIZED_PAGE_WIDTH_PX = 900
VISIBLE_CONTENT_THRESHOLD = 250
VISIBLE_CONTENT_CLIP_TOLERANCE_PX = 16


@dataclass(frozen=True)
class TransformClipReport:
    """Visible-content clipping introduced by placing a transformed image on a canvas."""

    left_px: int
    top_px: int
    right_px: int
    bottom_px: int
    tolerance_px: int = VISIBLE_CONTENT_CLIP_TOLERANCE_PX

    @property
    def clips_visible_content(self) -> bool:
        return max(self.left_px, self.top_px, self.right_px, self.bottom_px) > self.tolerance_px

    def warning_scope(self) -> dict[str, int]:
        return {
            "left_px": self.left_px,
            "top_px": self.top_px,
            "right_px": self.right_px,
            "bottom_px": self.bottom_px,
            "tolerance_px": self.tolerance_px,
        }

    def adjusted_transform(self, transform: Transform) -> Transform:
        """Shift a transform so clipped visible content is moved toward the canvas."""

        return Transform(
            rotation=transform.rotation,
            scale=transform.scale,
            translate_x=transform.translate_x + self.left_px - self.right_px,
            translate_y=transform.translate_y + self.top_px - self.bottom_px,
        )


def _normalize_mode(image: Image.Image) -> Image.Image:
    if image.mode in {"RGB", "L"}:
        return image
    return image.convert("RGB")


def _fill_color(mode: str) -> int | tuple[int, int, int]:
    if mode == "L":
        return 255
    return (255, 255, 255)


def _visible_content_bbox(image: Image.Image) -> tuple[int, int, int, int] | None:
    grayscale = _normalize_mode(image).convert("L")
    mask = grayscale.point(lambda value: 255 if value < VISIBLE_CONTENT_THRESHOLD else 0)
    return mask.getbbox()


def load_page_image(path: Path) -> Image.Image:
    """Load an image and detach it from the filesystem handle."""

    with Image.open(path) as image:
        return _normalize_mode(image.copy())


def normalize_page_width(
    image: Image.Image,
    *,
    target_width: int = NORMALIZED_PAGE_WIDTH_PX,
) -> Image.Image:
    """Resize a page image to a fixed width while preserving aspect ratio."""

    if target_width <= 0:
        raise ValueError("target_width must be positive.")
    if image.width == target_width:
        return image
    height = max(1, round(image.height * target_width / image.width))
    return image.resize((target_width, height), resample=Image.Resampling.LANCZOS)


def _transformed_image(
    image: Image.Image, transform: Transform
) -> tuple[Image.Image, str, int | tuple[int, int, int]]:
    source = _normalize_mode(image)
    fill = _fill_color(source.mode)
    transformed = source.rotate(
        -transform.rotation,
        expand=True,
        resample=Image.Resampling.BICUBIC,
        fillcolor=fill,
    )
    if transform.scale != 1.0:
        width = max(1, round(transformed.width * transform.scale))
        height = max(1, round(transformed.height * transform.scale))
        transformed = transformed.resize((width, height), resample=Image.Resampling.BICUBIC)
    return transformed, source.mode, fill


def apply_manual_transform(image: Image.Image, transform: Transform) -> Image.Image:
    """Apply rotate -> scale -> translate with a fixed output canvas size."""

    transformed, mode, fill = _transformed_image(image, transform)
    source = _normalize_mode(image)
    canvas = Image.new(mode, source.size, fill)
    canvas.paste(
        transformed,
        (round(transform.translate_x), round(transform.translate_y)),
    )
    return canvas


def apply_canonical_transform(
    image: Image.Image,
    transform: Transform,
    *,
    output_width: int,
    output_height: int,
) -> Image.Image:
    """Apply rotate -> scale -> translate onto an explicit output canvas."""

    if output_width <= 0 or output_height <= 0:
        raise ValueError("canonical output dimensions must be positive.")

    transformed, mode, fill = _transformed_image(image, transform)

    canvas = Image.new(mode, (output_width, output_height), fill)
    canvas.paste(
        transformed,
        (round(transform.translate_x), round(transform.translate_y)),
    )
    return canvas


def transformed_visible_content_clip_report(
    image: Image.Image,
    transform: Transform,
    *,
    output_width: int,
    output_height: int,
    tolerance_px: int = VISIBLE_CONTENT_CLIP_TOLERANCE_PX,
) -> TransformClipReport:
    """Measure whether canonical placement would push non-white source content off-canvas."""

    if output_width <= 0 or output_height <= 0:
        raise ValueError("canonical output dimensions must be positive.")

    transformed, _mode, _fill = _transformed_image(image, transform)
    bbox = _visible_content_bbox(transformed)
    if bbox is None:
        return TransformClipReport(0, 0, 0, 0, tolerance_px=tolerance_px)

    left, top, right, bottom = bbox
    placed_left = round(float(transform.translate_x) + left)
    placed_top = round(float(transform.translate_y) + top)
    placed_right = round(float(transform.translate_x) + right)
    placed_bottom = round(float(transform.translate_y) + bottom)
    return TransformClipReport(
        left_px=max(0, -placed_left),
        top_px=max(0, -placed_top),
        right_px=max(0, placed_right - output_width),
        bottom_px=max(0, placed_bottom - output_height),
        tolerance_px=tolerance_px,
    )


def crop_image(image: Image.Image, region: Region) -> Image.Image:
    """Crop a region after verifying it lies fully within the source image."""

    if region.x + region.width > image.width or region.y + region.height > image.height:
        raise ValueError(
            "Crop region lies outside the image bounds: "
            f"region=({region.x},{region.y},{region.width},{region.height}), "
            f"image=({image.width},{image.height})."
        )
    return image.crop((region.x, region.y, region.x + region.width, region.y + region.height))


def save_png(image: Image.Image, path: Path) -> None:
    """Persist a PNG artifact, creating parent directories as needed."""

    path.parent.mkdir(parents=True, exist_ok=True)
    image.save(path, format="PNG", optimize=True, compress_level=6)
