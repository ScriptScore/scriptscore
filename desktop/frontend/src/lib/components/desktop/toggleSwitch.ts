// SPDX-License-Identifier: AGPL-3.0-only
export const switchTrackClass =
  'relative inline-flex h-6 w-11 items-center rounded-full border border-workspace-border transition-colors';

export const switchThumbClass =
  'pointer-events-none inline-flex size-4 rounded-full shadow-sm transition-transform';

export const toggleButtonClass = 'flex w-full items-start justify-between gap-4 text-left';

export function switchTrackStateClass(enabled: boolean): string {
  return enabled
    ? 'border-[var(--toggle-active-border)] bg-[var(--toggle-active-bg)]'
    : 'bg-workspace-empty';
}

export function switchThumbStateClass(enabled: boolean): string {
  return enabled
    ? 'translate-x-6 bg-[var(--toggle-active-text)]'
    : 'translate-x-1 bg-workspace-text-primary';
}
