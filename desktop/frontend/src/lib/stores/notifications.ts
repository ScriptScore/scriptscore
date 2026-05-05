// SPDX-License-Identifier: AGPL-3.0-only
import { get, writable } from 'svelte/store';

export interface NotificationToast {
  id: number;
  kind: 'success' | 'info' | 'warning' | 'error';
  message: string;
  durationMs: number;
}

const DEFAULT_DURATION_MS = 2500;
type NotificationInput = Omit<NotificationToast, 'id' | 'durationMs'> & {
  durationMs?: number;
};

function createNotificationStore() {
  const { subscribe, set } = writable<NotificationToast[]>([]);
  let nextId = 1;
  let queue: NotificationToast[] = [];
  let dismissTimer: ReturnType<typeof setTimeout> | null = null;

  function clearTimer() {
    if (dismissTimer) {
      clearTimeout(dismissTimer);
      dismissTimer = null;
    }
  }

  function scheduleDismiss(toast: NotificationToast) {
    clearTimer();
    const { durationMs } = toast;
    if (durationMs <= 0) {
      return;
    }
    dismissTimer = setTimeout(() => {
      dismissTimer = null;
      showNext();
    }, durationMs);
  }

  function show(toast: NotificationToast) {
    set([toast]);
    scheduleDismiss(toast);
  }

  function showNext() {
    clearTimer();
    const next = queue.shift();
    if (next) {
      show(next);
      return;
    }
    set([]);
  }

  function dismiss(id: number) {
    const visible = get({ subscribe })[0] ?? null;
    if (visible?.id === id) {
      showNext();
      return;
    }
    queue = queue.filter((toast) => toast.id !== id);
  }

  function push(input: NotificationInput) {
    const toast: NotificationToast = {
      id: nextId++,
      kind: input.kind,
      message: input.message,
      durationMs: input.durationMs ?? DEFAULT_DURATION_MS
    };
    if (get({ subscribe }).length === 0) {
      show(toast);
    } else {
      queue = [...queue, toast];
    }
    return toast.id;
  }

  function pushSuccess(message: string, durationMs = DEFAULT_DURATION_MS) {
    return push({ kind: 'success', message, durationMs });
  }

  function pushInfo(message: string, durationMs = DEFAULT_DURATION_MS) {
    return push({ kind: 'info', message, durationMs });
  }

  function pushWarning(message: string, durationMs = DEFAULT_DURATION_MS) {
    return push({ kind: 'warning', message, durationMs });
  }

  function pushError(message: string, durationMs = DEFAULT_DURATION_MS) {
    return push({ kind: 'error', message, durationMs });
  }

  function clear() {
    clearTimer();
    queue = [];
    set([]);
  }

  return {
    subscribe,
    push,
    pushSuccess,
    pushInfo,
    pushWarning,
    pushError,
    dismiss,
    clear
  };
}

export const notifications = createNotificationStore();
