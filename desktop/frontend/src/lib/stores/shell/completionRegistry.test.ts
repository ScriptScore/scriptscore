// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it, vi } from 'vitest';

import { createCompletionRegistry } from './completionRegistry';
import type { RuntimeJobEvent } from '$lib/types';

function terminalEvent(eventType: RuntimeJobEvent['eventType'], jobId = 'job_1'): RuntimeJobEvent {
  return {
    eventType,
    commandName: 'scans.parse',
    workerStatus: eventType === 'job_finished' ? 'ready' : 'error',
    requestId: 'req_1',
    jobId,
    payload: {}
  };
}

describe('completionRegistry', () => {
  it('notifies filtered job-completion handlers for terminal events', () => {
    const registry = createCompletionRegistry();
    const onFinished = vi.fn();
    const onFailed = vi.fn();

    registry.onJobCompletion({ onFinished, onFailed }, 'job_1');
    registry.emitTerminalEvent(terminalEvent('job_finished'));
    registry.emitTerminalEvent(terminalEvent('job_failed', 'job_2'));

    expect(onFinished).toHaveBeenCalledTimes(1);
    expect(onFailed).not.toHaveBeenCalled();
  });

  it('replays completed events to late subscribers', async () => {
    const registry = createCompletionRegistry();
    const onFailed = vi.fn();

    registry.emitTerminalEvent(terminalEvent('job_failed'));
    registry.onJobCompletion({ onFinished: vi.fn(), onFailed }, 'job_1');

    await Promise.resolve();
    expect(onFailed).toHaveBeenCalledTimes(1);
  });

  it('treats skipped queued jobs as terminal failures', () => {
    const registry = createCompletionRegistry();
    const onFinished = vi.fn();
    const onFailed = vi.fn();

    registry.onJobCompletion({ onFinished, onFailed }, 'job_1');
    registry.emitTerminalEvent(terminalEvent('job_skipped'));

    expect(onFinished).not.toHaveBeenCalled();
    expect(onFailed).toHaveBeenCalledTimes(1);
  });

  it('notifies generic runtime-event subscribers and clears state', () => {
    const registry = createCompletionRegistry();
    const onEvent = vi.fn();

    registry.onRuntimeJobEvent(onEvent);
    registry.emitRuntimeEvent(terminalEvent('job_finished'));
    expect(onEvent).toHaveBeenCalledTimes(1);

    registry.clear();
    registry.emitRuntimeEvent(terminalEvent('job_finished'));
    expect(onEvent).toHaveBeenCalledTimes(1);
  });
});
