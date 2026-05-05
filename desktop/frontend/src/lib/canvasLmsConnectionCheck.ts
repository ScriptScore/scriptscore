// SPDX-License-Identifier: AGPL-3.0-only
import { isDesktopHost, listCanvasCourses } from '$lib/desktop';
import { isCanvasLmsReady } from '$lib/stores/appSettings';
import type { AppSettings } from '$lib/types';

export type CanvasLmsIdleOutcome = { kind: 'idle'; message: string };

export type CanvasLmsNetworkOutcome =
  | { kind: 'ok'; message: string }
  | { kind: 'error'; message: string };

/**
 * If Canvas credentials need a network check, returns null. Otherwise returns the idle outcome to show immediately.
 */
export function getCanvasLmsIdleOutcome(settings: AppSettings): CanvasLmsIdleOutcome | null {
  if (settings.lmsProvider !== 'canvas') {
    return { kind: 'idle', message: '' };
  }
  if (!isCanvasLmsReady(settings)) {
    return {
      kind: 'idle',
      message: 'Enter a Canvas base URL and API token to test the connection.'
    };
  }
  if (!isDesktopHost()) {
    return {
      kind: 'idle',
      message: 'Connection check runs in the ScriptScore desktop app.'
    };
  }
  return null;
}

/**
 * Lists teacher courses via Canvas API (call only when getCanvasLmsIdleOutcome returned null).
 */
export async function fetchCanvasLmsNetworkOutcome(settings: AppSettings): Promise<CanvasLmsNetworkOutcome> {
  try {
    const courses = await listCanvasCourses(
      settings.lmsCanvasBaseUrl.trim(),
      (settings.lmsCanvasApiKey ?? '').trim()
    );
    let message: string;
    if (courses.length === 0) {
      message = 'Connected. No teacher courses returned for this token.';
    } else {
      const plural = courses.length === 1 ? '' : 's';
      message = `Connected. ${courses.length} course${plural} available.`;
    }
    return { kind: 'ok', message };
  } catch (error) {
    return { kind: 'error', message: String(error) };
  }
}
