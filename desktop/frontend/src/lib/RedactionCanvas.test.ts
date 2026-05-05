// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import RedactionCanvas from './RedactionCanvas.svelte';

type ImageBehavior = 'success' | 'failure';

function installImageMock(behavior: ImageBehavior) {
  class MockImage {
    naturalWidth = 200;
    naturalHeight = 100;
    onload: (() => void) | null = null;
    onerror: (() => void) | null = null;

    set src(_value: string) {
      queueMicrotask(() => {
        if (behavior === 'success') {
          this.onload?.();
        } else {
          this.onerror?.();
        }
      });
    }
  }

  vi.stubGlobal('Image', MockImage);
}

describe('RedactionCanvas', () => {
  beforeEach(() => {
    installImageMock('success');
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('shows the empty prompt when no image is available', () => {
    render(RedactionCanvas, {
      imageUrl: '',
      pageNumber: 1,
      regions: []
    });

    expect(
      screen.getByText('Choose a template PDF to run setup, or open a project with existing template pages.')
    ).toBeTruthy();
  });

  it('shows an error when the generated preview PNG cannot be loaded', async () => {
    installImageMock('failure');

    render(RedactionCanvas, {
      imageUrl: 'asset:///tmp/page.png',
      pageNumber: 1,
      regions: []
    });

    expect(await screen.findByText('Page preview unavailable')).toBeTruthy();
  });

  it('creates a new redaction region from pointer drag coordinates', async () => {
    const onRegionsChange = vi.fn().mockResolvedValue(undefined);

    render(RedactionCanvas, {
      imageUrl: 'asset:///tmp/page.png',
      pageNumber: 2,
      regions: [],
      onRegionsChange
    });

    const frame = await screen.findByRole('button', { name: 'Draw redaction region' });
    const image = screen.getByAltText('Template page 2') as HTMLImageElement;

    Object.defineProperty(image, 'getBoundingClientRect', {
      value: () => ({
        left: 0,
        top: 0,
        width: 200,
        height: 100,
        right: 200,
        bottom: 100
      })
    });
    Object.defineProperty(frame, 'setPointerCapture', { value: vi.fn() });
    Object.defineProperty(frame, 'releasePointerCapture', { value: vi.fn() });

    await fireEvent.pointerDown(frame, {
      button: 0,
      pointerId: 1,
      clientX: 10,
      clientY: 10
    });
    await fireEvent.pointerMove(window, {
      pointerId: 1,
      clientX: 110,
      clientY: 60
    });
    await fireEvent.pointerUp(window, {
      pointerId: 1,
      clientX: 110,
      clientY: 60
    });

    await waitFor(() => {
      expect(onRegionsChange).toHaveBeenCalledTimes(1);
      expect(onRegionsChange).toHaveBeenCalledWith([
        expect.objectContaining({
          pageNumber: 2,
          x: 10,
          y: 10,
          width: 100,
          height: 50
        })
      ]);
    });
  });

  it('deletes an existing region from the context menu hit target', async () => {
    const onRegionsChange = vi.fn().mockResolvedValue(undefined);

    render(RedactionCanvas, {
      imageUrl: 'asset:///tmp/page.png',
      pageNumber: 1,
      regions: [
        {
          regionId: 'region_1',
          pageNumber: 1,
          x: 10,
          y: 10,
          width: 100,
          height: 50,
          label: 'student name',
          sortOrder: 0
        }
      ],
      onRegionsChange
    });

    const frame = await screen.findByRole('button', { name: 'Draw redaction region' });
    expect(frame.parentElement?.className).toContain('bg-surface-page');
    expect(
      await screen.findByRole('button', { name: 'Name region top-left resize handle' })
    ).toBeTruthy();
    const image = screen.getByAltText('Template page 1') as HTMLImageElement;

    Object.defineProperty(image, 'getBoundingClientRect', {
      value: () => ({
        left: 0,
        top: 0,
        width: 200,
        height: 100,
        right: 200,
        bottom: 100
      })
    });

    await fireEvent.contextMenu(frame, {
      clientX: 50,
      clientY: 35
    });
    expect(screen.getByRole('dialog', { name: 'Delete redaction region?' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Delete' }));

    await waitFor(() => {
      expect(onRegionsChange).toHaveBeenCalledWith([]);
    });
  });

  it('uses exam-wide redaction metadata instead of page-local order for labels', async () => {
    render(RedactionCanvas, {
      imageUrl: 'asset:///tmp/page-2.png',
      pageNumber: 2,
      examRegionCount: 2,
      regions: [
        {
          regionId: 'region_2',
          pageNumber: 2,
          x: 10,
          y: 10,
          width: 100,
          height: 50,
          label: 'privacy_protection',
          sortOrder: 1
        }
      ]
    });

    await screen.findByRole('button', { name: 'Draw redaction region' });

    expect(await screen.findByText('Privacy')).toBeTruthy();
    expect(screen.queryByText('Name')).toBeNull();
    expect(
      screen.getByRole('button', { name: 'Privacy region top-left resize handle' })
    ).toBeTruthy();
  });

  it('keeps the next left drag active after cancelling a context-menu delete', async () => {
    const onRegionsChange = vi.fn().mockResolvedValue(undefined);

    render(RedactionCanvas, {
      imageUrl: 'asset:///tmp/page.png',
      pageNumber: 1,
      regions: [
        {
          regionId: 'region_1',
          pageNumber: 1,
          x: 10,
          y: 10,
          width: 100,
          height: 50,
          label: 'student name',
          sortOrder: 0
        }
      ],
      onRegionsChange
    });

    const frame = await screen.findByRole('button', { name: 'Draw redaction region' });
    const image = screen.getByAltText('Template page 1') as HTMLImageElement;

    Object.defineProperty(image, 'getBoundingClientRect', {
      value: () => ({
        left: 0,
        top: 0,
        width: 200,
        height: 100,
        right: 200,
        bottom: 100
      })
    });
    Object.defineProperty(frame, 'setPointerCapture', { value: vi.fn() });
    Object.defineProperty(frame, 'releasePointerCapture', { value: vi.fn() });

    await fireEvent.pointerDown(frame, {
      button: 2,
      pointerId: 8,
      clientX: 50,
      clientY: 35
    });
    await fireEvent.contextMenu(frame, {
      clientX: 50,
      clientY: 35
    });
    expect(screen.getByRole('dialog', { name: 'Delete redaction region?' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    expect(onRegionsChange).not.toHaveBeenCalled();

    await fireEvent.pointerDown(frame, {
      button: 0,
      pointerId: 9,
      clientX: 140,
      clientY: 70
    });
    await fireEvent.pointerMove(window, {
      pointerId: 9,
      clientX: 180,
      clientY: 90
    });
    await fireEvent.pointerUp(window, {
      pointerId: 9,
      clientX: 180,
      clientY: 90
    });

    await waitFor(() => {
      expect(onRegionsChange).toHaveBeenCalledWith([
        expect.objectContaining({ regionId: 'region_1' }),
        expect.objectContaining({
          pageNumber: 1,
          x: 140,
          y: 70,
          width: 40,
          height: 20
        })
      ]);
    });
  });
});
