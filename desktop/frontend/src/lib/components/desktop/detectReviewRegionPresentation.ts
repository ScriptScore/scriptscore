// SPDX-License-Identifier: AGPL-3.0-only
import type { ImageRegionPresentationResolver } from './ui/imageRegionEditor';

export const detectReviewRegionPresentation: ImageRegionPresentationResolver = (region) => ({
  label: region.kind === 'resolved' ? 'Resolved' : 'Seed',
  handleLabel: 'Question',
  borderColor: 'var(--message-warning-border)',
  fillColor: 'var(--message-warning-selection-fill)',
  labelBackground: 'var(--message-warning-bg)',
  labelForeground: 'var(--message-warning-text)',
  labelBorder: 'var(--message-warning-border)'
});
