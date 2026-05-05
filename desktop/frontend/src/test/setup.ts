// SPDX-License-Identifier: AGPL-3.0-only
import { cleanup } from '@testing-library/svelte';
import { afterEach } from 'vitest';

class ResizeObserverStub {
  observe(): void {
    return;
  }
  unobserve(): void {
    return;
  }
  disconnect(): void {
    return;
  }
}

if (!('ResizeObserver' in globalThis)) {
  Object.defineProperty(globalThis, 'ResizeObserver', {
    value: ResizeObserverStub,
    writable: true
  });
}

if (!('animate' in HTMLElement.prototype)) {
  Object.defineProperty(HTMLElement.prototype, 'animate', {
    value: () => ({
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
      cancel: () => undefined,
      finish: () => undefined,
      finished: Promise.resolve()
    }),
    writable: true
  });
}

afterEach(() => {
  cleanup();
});
