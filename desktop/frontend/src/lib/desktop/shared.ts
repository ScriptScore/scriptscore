// SPDX-License-Identifier: AGPL-3.0-only
import { convertFileSrc, invoke, isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { get } from 'svelte/store';

import { appSettings } from '$lib/stores/appSettings';
import type { AppSettings, RuntimeJobEvent, ShellState } from '$lib/types';

export const DESKTOP_HOST_ERROR =
  'Desktop commands require the Tauri host. The browser preview cannot run Rust or Python commands.';
export const RUNTIME_JOB_EVENT_NAME = 'scriptscore:runtime-job';

export const browserShellState: ShellState = {
  currentProject: null,
  workerStatus: 'error',
  workerActivity: { activeJobs: [], pendingJobCount: 0 },
  lastRuntimeError: DESKTOP_HOST_ERROR,
  debugFeatures: { redactionToggle: false }
};

export function isDesktopHost(): boolean {
  return isTauri();
}

export function ensureDesktopHost(): void {
  if (!isDesktopHost()) {
    throw new Error(DESKTOP_HOST_ERROR);
  }
}

export function currentDesktopSettings(): AppSettings {
  return get(appSettings);
}

export function invokeDesktopHost<T>(
  commandName: string,
  payload: Record<string, unknown> = {}
): Promise<T> {
  ensureDesktopHost();
  return invoke<T>(commandName, payload);
}

export function invokeDesktopHostOrDefault<T>(
  fallback: T,
  commandName: string,
  payload: Record<string, unknown> = {}
): Promise<T> {
  if (!isDesktopHost()) {
    return Promise.resolve(fallback);
  }
  return invoke<T>(commandName, payload);
}

export function invokeDesktopHostWithSettings<T>(
  commandName: string,
  payload: Record<string, unknown> = {},
  settings: AppSettings = currentDesktopSettings()
): Promise<T> {
  return invokeDesktopHost<T>(commandName, {
    ...payload,
    settings
  });
}

export function listenRuntimeJobEventsInternal(
  onEvent: (event: RuntimeJobEvent) => void
): Promise<UnlistenFn> {
  ensureDesktopHost();
  return listen<RuntimeJobEvent>(RUNTIME_JOB_EVENT_NAME, (event) => {
    onEvent(event.payload);
  });
}

export function toDesktopAssetUrl(filePath: string): string {
  return isDesktopHost() ? convertFileSrc(filePath) : filePath;
}
