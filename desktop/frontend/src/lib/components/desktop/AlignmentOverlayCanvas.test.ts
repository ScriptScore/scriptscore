// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import AlignmentOverlayCanvas from './AlignmentOverlayCanvas.svelte';

function installImageMetrics(image: HTMLImageElement, width: number, height: number) {
  Object.defineProperty(image, 'naturalWidth', { value: width, configurable: true });
  Object.defineProperty(image, 'naturalHeight', { value: height, configurable: true });
  Object.defineProperty(image, 'getBoundingClientRect', {
    value: () => ({
      left: 0,
      top: 0,
      right: width,
      bottom: height,
      width,
      height
    }),
    configurable: true
  });
}

describe('AlignmentOverlayCanvas', () => {
  it('renders the editor inside the shared interactive canvas surface', () => {
    render(AlignmentOverlayCanvas, {
      templateImageUrl: 'asset:///template.png',
      submissionImageUrl: 'asset:///submission.png',
      pageNumber: 1,
      transform: { rotation: 0, scale: 1, translateX: 0, translateY: 0 }
    });

    const editor = screen.getByRole('application', { name: 'Alignment overlay editor' });
    expect(editor.parentElement?.className).toContain('bg-surface-page');
  });

  it('updates translation from pointer drag using template display scale', async () => {
    const ontransformchange = vi.fn();

    render(AlignmentOverlayCanvas, {
      templateImageUrl: 'asset:///template.png',
      submissionImageUrl: 'asset:///submission.png',
      pageNumber: 1,
      transform: { rotation: 0, scale: 1, translateX: 0, translateY: 0 },
      ontransformchange
    });

    const editor = screen.getByRole('application', { name: 'Alignment overlay editor' });
    const template = screen.getByAltText('Template page 1') as HTMLImageElement;
    const submission = screen.getByAltText('Submission page 1') as HTMLImageElement;
    installImageMetrics(template, 100, 50);
    installImageMetrics(submission, 100, 50);
    Object.defineProperty(editor, 'setPointerCapture', { value: vi.fn(), configurable: true });
    Object.defineProperty(editor, 'releasePointerCapture', { value: vi.fn(), configurable: true });

    await fireEvent.load(template);
    await fireEvent.load(submission);
    await fireEvent.pointerDown(editor, { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
    await fireEvent.pointerMove(editor, { pointerId: 1, clientX: 35, clientY: 45 });
    await fireEvent.pointerUp(editor, { pointerId: 1, clientX: 35, clientY: 45 });

    expect(ontransformchange).toHaveBeenLastCalledWith({
      rotation: 0,
      scale: 1,
      translateX: 25,
      translateY: 35
    });
  });

  it('updates scale from wheel input without snapping sub-one transforms', async () => {
    const ontransformchange = vi.fn();

    const { rerender } = render(AlignmentOverlayCanvas, {
      templateImageUrl: 'asset:///template.png',
      submissionImageUrl: 'asset:///submission.png',
      pageNumber: 1,
      transform: { rotation: 0, scale: 1, translateX: 0, translateY: 0 },
      ontransformchange
    });

    const editor = screen.getByRole('application', { name: 'Alignment overlay editor' });
    await fireEvent.wheel(editor, { deltaY: -100 });

    expect(ontransformchange).toHaveBeenLastCalledWith({
      rotation: 0,
      scale: 1.005,
      translateX: 0,
      translateY: 0
    });

    await rerender({
      templateImageUrl: 'asset:///template.png',
      submissionImageUrl: 'asset:///submission.png',
      pageNumber: 1,
      transform: { rotation: 0, scale: 0.455, translateX: 0, translateY: 0 },
      ontransformchange
    });
    await fireEvent.wheel(editor, { deltaY: -100 });

    expect(ontransformchange).toHaveBeenLastCalledWith({
      rotation: 0,
      scale: 0.46,
      translateX: 0,
      translateY: 0
    });
  });

  it('applies template tint as an image filter', () => {
    render(AlignmentOverlayCanvas, {
      templateImageUrl: 'asset:///template.png',
      submissionImageUrl: 'asset:///submission.png',
      pageNumber: 1,
      transform: { rotation: 0, scale: 1, translateX: 0, translateY: 0 },
      templateTintColor: '#16a34a'
    });

    expect(screen.getByAltText('Template page 1').getAttribute('style')).toContain(
      'filter: sepia'
    );
  });

  it('defaults the submission overlay opacity to sixty percent', () => {
    render(AlignmentOverlayCanvas, {
      templateImageUrl: 'asset:///template.png',
      submissionImageUrl: 'asset:///submission.png',
      pageNumber: 1,
      transform: { rotation: 0, scale: 1, translateX: 0, translateY: 0 }
    });

    expect(screen.getByAltText('Submission page 1').getAttribute('style')).toContain(
      'opacity: 0.6'
    );
  });
});
