// SPDX-License-Identifier: AGPL-3.0-only
import { writable } from 'svelte/store';

function createThemeStore() {
  const { subscribe, set, update } = writable<'dark' | 'light'>('dark');

  function init() {
    if (typeof globalThis.window === 'undefined') return;
    const saved = localStorage.getItem('scriptscore-theme');
    const theme: 'dark' | 'light' = saved === 'light' ? 'light' : 'dark';
    applyTheme(theme);
    set(theme);
  }

  function toggle() {
    update((current) => {
      const next = current === 'dark' ? 'light' : 'dark';
      applyTheme(next);
      return next;
    });
  }

  function setExplicit(theme: 'dark' | 'light') {
    applyTheme(theme);
    set(theme);
  }

  return { subscribe, init, toggle, setExplicit };
}

function applyTheme(theme: 'dark' | 'light') {
  if (typeof globalThis.window === 'undefined') return;
  if (theme === 'light') {
    document.documentElement.classList.add('light');
  } else {
    document.documentElement.classList.remove('light');
  }
  localStorage.setItem('scriptscore-theme', theme);
}

export const theme = createThemeStore();
