// SPDX-License-Identifier: AGPL-3.0-only
import { tick } from 'svelte';
import { get } from 'svelte/store';

import { ensureAutomaticRubricJobs, isDesktopHost } from '$lib/desktop';
import { appSettings } from '$lib/stores/appSettings';
import { isExamReviewSurfaceVisible } from '$lib/stores/workspaceView';

function afterDoubleRequestAnimationFrame(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => resolve());
    });
  });
}

function afterNextMicrotask(): Promise<void> {
  return new Promise((resolve) => {
    queueMicrotask(() => resolve());
  });
}

/**
 * Invokes `ensure_automatic_rubric_jobs` only after Svelte has flushed the DOM and the browser has
 * had a chance to paint (double rAF + microtask). Avoids blocking the transition into Review.
 *
 * @param onlyWhenExamReviewVisible — When true, skips the host call unless the Exam Review surface
 *   is visible (`templateSetup` + `review`). Use from route-level handlers so completion events
 *   cannot enqueue while Setup is shown; omit or pass false from ReviewWorkspace (already mounted
 *   only on that surface).
 */
export function scheduleAutomaticRubricEnsureAfterUiPaint(
  onlyWhenExamReviewVisible = false
): void {
  if (globalThis.window === undefined || !isDesktopHost()) {
    return;
  }
  void tick()
    .then(() => afterDoubleRequestAnimationFrame())
    .then(() => afterNextMicrotask())
    .then(() => {
      if (onlyWhenExamReviewVisible && !isExamReviewSurfaceVisible()) {
        return;
      }
      const settings = structuredClone(get(appSettings));
      if (!settings.aiAssistCategories.rubrics) {
        return;
      }
      void ensureAutomaticRubricJobs(settings).catch(() => {});
    });
}
