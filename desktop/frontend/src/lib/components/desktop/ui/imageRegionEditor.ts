// SPDX-License-Identifier: AGPL-3.0-only
export interface ImageRegion {
  regionId: string | null;
  pageNumber: number;
  x: number;
  y: number;
  width: number;
  height: number;
  kind?: string;
}

export interface ImageRegionMetrics {
  naturalWidth: number;
  naturalHeight: number;
}

export interface ImageRegionPresentation {
  label: string;
  handleLabel?: string;
  borderColor: string;
  fillColor: string;
  labelBackground: string;
  labelForeground: string;
  labelBorder?: string;
}

export type ImageRegionPresentationResolver = (
  region: ImageRegion,
  index: number
) => ImageRegionPresentation;

export const defaultImageRegionPresentation: ImageRegionPresentationResolver = (_region, index) => ({
  label: `Region ${index + 1}`,
  borderColor: 'var(--workspace-selection-border)',
  fillColor: 'var(--workspace-selection-fill)',
  labelBackground: 'var(--workspace-selection-handle-fill)',
  labelForeground: 'var(--workspace-selection-handle-border)',
  labelBorder: 'var(--workspace-selection-handle-border)'
});
