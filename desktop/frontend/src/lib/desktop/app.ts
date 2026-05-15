// SPDX-License-Identifier: AGPL-3.0-only
import { getVersion } from '@tauri-apps/api/app';
import { open as openExternal } from '@tauri-apps/plugin-shell';

import type { AppUpdateCheck } from '$lib/types';

import { invokeDesktopHostOrDefault, isDesktopHost } from './shared';

const BROWSER_UPDATE_CHECK: AppUpdateCheck = {
  installedVersion: 'Browser preview',
  latestStableVersion: null,
  latestStableTag: null,
  releaseUrl: null,
  updateAvailable: false,
  status: 'unavailable',
  message: 'Stable update checks are available in the packaged desktop app.'
};

export async function getAppVersion(): Promise<string | null> {
  if (!isDesktopHost()) {
    return null;
  }
  try {
    return await getVersion();
  } catch {
    return null;
  }
}

export function checkAppUpdate(): Promise<AppUpdateCheck> {
  return invokeDesktopHostOrDefault<AppUpdateCheck>(
    BROWSER_UPDATE_CHECK,
    'check_app_update'
  ).catch((error) => ({
    ...BROWSER_UPDATE_CHECK,
    installedVersion: 'Unknown',
    message: `Could not check for updates: ${String(error)}`
  }));
}

export async function openExternalUrl(url: string): Promise<void> {
  if (!isDesktopHost()) {
    globalThis.window?.open(url, '_blank', 'noopener,noreferrer');
    return;
  }
  await openExternal(url);
}
