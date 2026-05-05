// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import ImageRegionEditor from './ImageRegionEditor.svelte';
import type { ImageRegion, ImageRegionPresentation } from './imageRegionEditor';

const baseRegion: ImageRegion = {
  regionId: 'region-1',
  pageNumber: 1,
  x: 10,
  y: 10,
  width: 50,
  height: 30,
  kind: 'name'
};

function presentation(region: ImageRegion, index: number): ImageRegionPresentation {
  const isName = region.kind === 'name' || index === 0;
  return {
    label: isName ? 'Name' : 'Privacy',
    borderColor: isName
      ? 'var(--workspace-selection-name-border)'
      : 'var(--workspace-selection-border)',
    fillColor: isName
      ? 'var(--workspace-selection-name-fill)'
      : 'var(--workspace-selection-fill)',
    labelBackground: isName
      ? 'var(--workspace-selection-name-border)'
      : 'var(--workspace-selection-handle-fill)',
    labelForeground: isName
      ? 'var(--workspace-selection-name-foreground)'
      : 'var(--workspace-selection-handle-border)',
    labelBorder: isName ? undefined : 'var(--workspace-selection-handle-border)'
  };
}

function renderEditor(props: Partial<Parameters<typeof render>[1]> = {}) {
  return render(ImageRegionEditor, {
    imageSrc: 'asset:///tmp/page.png',
    imageAlt: 'Page preview',
    pageNumber: 1,
    imageWidth: 200,
    imageHeight: 100,
    regions: [],
    ariaLabel: 'Region editor',
    regionPresentation: presentation,
    ...props
  });
}

function installPointerCapture(frame: HTMLElement) {
  Object.defineProperty(frame, 'setPointerCapture', { value: vi.fn(), configurable: true });
  Object.defineProperty(frame, 'releasePointerCapture', { value: vi.fn(), configurable: true });
}

describe('ImageRegionEditor', () => {
  it('draws a new region in image-pixel coordinates and reports selection', async () => {
    const onRegionsChange = vi.fn().mockResolvedValue(undefined);
    const onSelectionChange = vi.fn();
    renderEditor({ onRegionsChange, onSelectionChange });

    const frame = screen.getByRole('button', { name: 'Region editor' });
    installPointerCapture(frame);

    await fireEvent.pointerDown(frame, { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
    await fireEvent.pointerMove(window, { pointerId: 1, clientX: 110, clientY: 60 });
    await fireEvent.pointerUp(window, { pointerId: 1, clientX: 110, clientY: 60 });

    await waitFor(() => {
      expect(onRegionsChange).toHaveBeenCalledWith([
        expect.objectContaining({
          pageNumber: 1,
          x: 10,
          y: 10,
          width: 100,
          height: 50
        })
      ]);
    });
    expect(onSelectionChange).toHaveBeenCalledWith(null);
    expect(onSelectionChange).toHaveBeenCalledWith(0);
  });

  it('moves an existing region and clamps it inside the image bounds', async () => {
    const onRegionsChange = vi.fn().mockResolvedValue(undefined);
    renderEditor({ regions: [baseRegion], onRegionsChange });

    const frame = screen.getByRole('button', { name: 'Region editor' });
    installPointerCapture(frame);

    await fireEvent.pointerDown(frame, { button: 0, pointerId: 2, clientX: 40, clientY: 25 });
    await fireEvent.pointerMove(window, { pointerId: 2, clientX: 260, clientY: 145 });
    await fireEvent.pointerUp(window, { pointerId: 2, clientX: 260, clientY: 145 });

    await waitFor(() => {
      expect(onRegionsChange).toHaveBeenCalledWith([
        expect.objectContaining({
          regionId: 'region-1',
          x: 150,
          y: 70,
          width: 50,
          height: 30
        })
      ]);
    });
  });

  it('resizes from a corner handle with the existing minimum size contract', async () => {
    const onRegionsChange = vi.fn().mockResolvedValue(undefined);
    renderEditor({ regions: [baseRegion], onRegionsChange });

    const frame = screen.getByRole('button', { name: 'Region editor' });
    installPointerCapture(frame);

    await fireEvent.pointerDown(frame, { button: 0, pointerId: 3, clientX: 60, clientY: 40 });
    await fireEvent.pointerMove(window, { pointerId: 3, clientX: 15, clientY: 15 });
    await fireEvent.pointerUp(window, { pointerId: 3, clientX: 15, clientY: 15 });

    await waitFor(() => {
      expect(onRegionsChange).toHaveBeenCalledWith([
        expect.objectContaining({
          x: 10,
          y: 10,
          width: 16,
          height: 16
        })
      ]);
    });
  });

  it('deletes a hit region from the context menu confirmation', async () => {
    const onRegionsChange = vi.fn().mockResolvedValue(undefined);
    renderEditor({
      regions: [baseRegion],
      deleteTitle: 'Delete redaction region?',
      deleteDescription: 'Remove this redaction region.',
      onRegionsChange
    });

    const frame = screen.getByRole('button', { name: 'Region editor' });
    await fireEvent.contextMenu(frame, { clientX: 20, clientY: 20 });
    expect(screen.getByRole('dialog', { name: 'Delete redaction region?' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Delete' }));

    await waitFor(() => {
      expect(onRegionsChange).toHaveBeenCalledWith([]);
    });
  });

  it('uses caller-provided labels and handle text', () => {
    renderEditor({
      regions: [
        {
          ...baseRegion,
          kind: 'question'
        }
      ],
      regionPresentation: () => ({
        label: 'Q3',
        handleLabel: 'Question 3',
        borderColor: 'var(--workspace-selection-border)',
        fillColor: 'var(--workspace-selection-fill)',
        labelBackground: 'var(--workspace-selection-handle-fill)',
        labelForeground: 'var(--workspace-selection-handle-border)',
        labelBorder: 'var(--workspace-selection-handle-border)'
      })
    });

    expect(screen.getByText('Q3')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Question 3 region top-left resize handle' })).toBeTruthy();
  });

  it('ignores pointer editing while disabled', async () => {
    const onRegionsChange = vi.fn().mockResolvedValue(undefined);
    renderEditor({ disabled: true, onRegionsChange });

    const frame = screen.getByRole('button', { name: 'Region editor' });
    expect(frame.className).toContain('cursor-wait');
    await fireEvent.pointerDown(frame, { button: 0, pointerId: 4, clientX: 10, clientY: 10 });
    await fireEvent.pointerMove(window, { pointerId: 4, clientX: 110, clientY: 60 });
    await fireEvent.pointerUp(window, { pointerId: 4, clientX: 110, clientY: 60 });

    expect(onRegionsChange).not.toHaveBeenCalled();
  });

  it('keeps square-corner previews fully visible inside a borderless sizing frame', () => {
    renderEditor({ squareCorners: true });

    const surface = screen.getByRole('button', { name: 'Region editor' }).parentElement;
    expect(surface?.className).toContain('overflow-visible');
    expect(surface?.className).not.toContain(' border ');
    expect(surface?.className).toContain('shadow-[inset_0_0_0_1px_var(--workspace-image-border)');
  });
});
