// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import TabbedTraceViewer from './TabbedTraceViewer.svelte';
import { buildJobTrace } from '../../../test/traceTestUtils';

describe('TabbedTraceViewer', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    Object.defineProperty(globalThis.navigator, 'clipboard', {
      configurable: true,
      value: {
        writeText: vi.fn().mockResolvedValue(undefined)
      }
    });
  });

  it('shows formatted event summaries by default', () => {
    render(TabbedTraceViewer, {
      trace: buildJobTrace()
    });

    expect(screen.getByText('Trace Details')).toBeTruthy();
    expect(screen.getByText('started')).toBeTruthy();
    expect(screen.getByText('progress 0/2 - 0%')).toBeTruthy();
    expect(screen.getByText('ok: true, degraded: false')).toBeTruthy();
  });

  it('switches tabs and copies the active payload', async () => {
    render(TabbedTraceViewer, {
      trace: buildJobTrace()
    });

    await fireEvent.click(screen.getByRole('tab', { name: 'Response' }));
    expect(screen.getByText(/"output": "done"/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Copy' }));

    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
        JSON.stringify({ result: { ok: true, output: 'done' } }, null, 2)
      );
    });
  });

  it('shows the empty state when no trace exists', () => {
    render(TabbedTraceViewer, {
      trace: null
    });

    expect(screen.getByText('Select a trace to inspect request, response, and event details.')).toBeTruthy();
  });

  it('renders request and error payloads when present', async () => {
    render(TabbedTraceViewer, {
      trace: buildJobTrace({
        result: null,
        error: { message: 'worker failed' }
      })
    });

    await fireEvent.click(screen.getByRole('tab', { name: 'Request' }));
    expect(screen.getByText(/"input": "value"/)).toBeTruthy();

    await fireEvent.click(screen.getByRole('tab', { name: 'Response' }));
    expect(screen.getByText('No response data.')).toBeTruthy();
    expect(screen.getByText('Error')).toBeTruthy();
    expect(screen.getByText(/worker failed/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Copy' }));

    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
        JSON.stringify({ error: { message: 'worker failed' } }, null, 2)
      );
    });
  });
});
