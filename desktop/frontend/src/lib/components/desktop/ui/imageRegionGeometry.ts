// SPDX-License-Identifier: AGPL-3.0-only
import type { ImageRegion, ImageRegionPresentationResolver } from './imageRegionEditor';

export type ImageRegionPoint = { x: number; y: number };
export type ImageRegionRect = { x: number; y: number; width: number; height: number };
export type ImageRegionResizeHandle = 'nw' | 'ne' | 'sw' | 'se';

export const imageRegionMinSize = 16;
export const imageRegionHandleHitRadius = 18;

export function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

export function hitTestImageRegions(
  regions: ImageRegion[],
  point: ImageRegionPoint
):
  | { kind: 'move'; regionId: string }
  | { kind: 'resize'; regionId: string; handle: ImageRegionResizeHandle }
  | null {
  for (const region of [...regions].reverse()) {
    if (!region.regionId) {
      continue;
    }
    const corners: Array<{ handle: ImageRegionResizeHandle; x: number; y: number }> = [
      { handle: 'nw', x: region.x, y: region.y },
      { handle: 'ne', x: region.x + region.width, y: region.y },
      { handle: 'sw', x: region.x, y: region.y + region.height },
      { handle: 'se', x: region.x + region.width, y: region.y + region.height }
    ];
    for (const corner of corners) {
      if (
        Math.abs(point.x - corner.x) <= imageRegionHandleHitRadius &&
        Math.abs(point.y - corner.y) <= imageRegionHandleHitRadius
      ) {
        return { kind: 'resize', regionId: region.regionId, handle: corner.handle };
      }
    }
    if (
      point.x >= region.x &&
      point.x <= region.x + region.width &&
      point.y >= region.y &&
      point.y <= region.y + region.height
    ) {
      return { kind: 'move', regionId: region.regionId };
    }
  }
  return null;
}

export function resizeImageRegion(
  region: ImageRegion,
  origin: ImageRegionRect,
  point: ImageRegionPoint,
  handle: ImageRegionResizeHandle,
  bounds: { width: number; height: number }
): ImageRegion {
  let left = origin.x;
  let top = origin.y;
  let right = origin.x + origin.width;
  let bottom = origin.y + origin.height;
  if (handle.includes('n')) {
    top = clamp(point.y, 0, bottom - imageRegionMinSize);
  }
  if (handle.includes('s')) {
    bottom = clamp(point.y, top + imageRegionMinSize, bounds.height);
  }
  if (handle.includes('w')) {
    left = clamp(point.x, 0, right - imageRegionMinSize);
  }
  if (handle.includes('e')) {
    right = clamp(point.x, left + imageRegionMinSize, bounds.width);
  }
  return {
    ...region,
    x: left,
    y: top,
    width: right - left,
    height: bottom - top
  };
}

export function draftImageRegion(
  pageNumber: number,
  start: ImageRegionPoint,
  current: ImageRegionPoint
): ImageRegion | null {
  const left = Math.min(start.x, current.x);
  const top = Math.min(start.y, current.y);
  const width = Math.abs(current.x - start.x);
  const height = Math.abs(current.y - start.y);
  if (width < imageRegionMinSize || height < imageRegionMinSize) {
    return null;
  }
  return {
    regionId: null,
    pageNumber,
    x: left,
    y: top,
    width,
    height
  };
}

export function regionStyleKey(region: ImageRegion, index: number): string {
  const baseKey =
    region.regionId ?? `${region.pageNumber}-${region.x}-${region.y}-${region.width}-${region.height}`;
  return `${baseKey}:${index}`;
}

export function computeRegionStyleMap(
  regions: ImageRegion[],
  metrics: { left: number; top: number; width: number; height: number } | null,
  natural: { width: number; height: number },
  regionPresentation: ImageRegionPresentationResolver
): Map<string, string> {
  return new Map(
    regions.map((region, idx) => {
      if (natural.width <= 0 || natural.height <= 0 || !metrics) {
        return [regionStyleKey(region, idx), ''];
      }
      const presentation = regionPresentation(region, idx);
      const left = metrics.left + (region.x / natural.width) * metrics.width;
      const top = metrics.top + (region.y / natural.height) * metrics.height;
      const width = (region.width / natural.width) * metrics.width;
      const height = (region.height / natural.height) * metrics.height;
      return [
        regionStyleKey(region, idx),
        `left:${left}px;top:${top}px;width:${width}px;height:${height}px;position:absolute;z-index:10;border:2px solid ${presentation.borderColor};background:${presentation.fillColor};box-shadow:inset 0 0 0 1px var(--workspace-selection-inset);`
      ];
    })
  );
}
