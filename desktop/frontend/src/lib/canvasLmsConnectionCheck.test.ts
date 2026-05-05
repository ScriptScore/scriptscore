// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { defaultAppSettings } from '$lib/stores/appSettings';
import {
  fetchCanvasLmsNetworkOutcome,
  getCanvasLmsIdleOutcome
} from './canvasLmsConnectionCheck';

const desktopMocks = vi.hoisted(() => ({
  isDesktopHost: vi.fn(),
  listCanvasCourses: vi.fn()
}));

vi.mock('$lib/desktop', () => desktopMocks);

function canvasSettings(overrides: Record<string, unknown> = {}) {
  return {
    ...defaultAppSettings,
    lmsProvider: 'canvas' as const,
    lmsCanvasBaseUrl: 'https://canvas.example.test',
    lmsCanvasApiKey: 'token',
    ...overrides
  };
}

describe('canvasLmsConnectionCheck', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    desktopMocks.isDesktopHost.mockReturnValue(true);
  });

  it('returns an idle outcome for non-Canvas settings and incomplete Canvas credentials', () => {
    expect(getCanvasLmsIdleOutcome({ ...defaultAppSettings, lmsProvider: 'none' })).toEqual({
      kind: 'idle',
      message: ''
    });
    expect(
      getCanvasLmsIdleOutcome(canvasSettings({ lmsCanvasBaseUrl: '', lmsCanvasApiKey: null }))
    ).toEqual({
      kind: 'idle',
      message: 'Enter a Canvas base URL and API token to test the connection.'
    });
  });

  it('returns an idle browser-preview message when the desktop host is unavailable', () => {
    desktopMocks.isDesktopHost.mockReturnValue(false);

    expect(getCanvasLmsIdleOutcome(canvasSettings())).toEqual({
      kind: 'idle',
      message: 'Connection check runs in the ScriptScore desktop app.'
    });
  });

  it('returns null when a real network check should run', () => {
    expect(getCanvasLmsIdleOutcome(canvasSettings())).toBeNull();
  });

  it('reports course-count outcomes for the network check', async () => {
    desktopMocks.listCanvasCourses.mockResolvedValueOnce([]);
    await expect(fetchCanvasLmsNetworkOutcome(canvasSettings())).resolves.toEqual({
      kind: 'ok',
      message: 'Connected. No teacher courses returned for this token.'
    });

    desktopMocks.listCanvasCourses.mockResolvedValueOnce([{ id: '1', name: 'Course' }]);
    await expect(fetchCanvasLmsNetworkOutcome(canvasSettings())).resolves.toEqual({
      kind: 'ok',
      message: 'Connected. 1 course available.'
    });

    desktopMocks.listCanvasCourses.mockResolvedValueOnce([{ id: '1' }, { id: '2' }]);
    await expect(fetchCanvasLmsNetworkOutcome(canvasSettings())).resolves.toEqual({
      kind: 'ok',
      message: 'Connected. 2 courses available.'
    });
  });

  it('reports network failures as an error outcome', async () => {
    desktopMocks.listCanvasCourses.mockRejectedValue(new Error('401 unauthorized'));

    await expect(fetchCanvasLmsNetworkOutcome(canvasSettings())).resolves.toEqual({
      kind: 'error',
      message: 'Error: 401 unauthorized'
    });
  });
});
