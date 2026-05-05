// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import SettingsRuntimeSmokeTest from './SettingsRuntimeSmokeTest.svelte';

describe('SettingsRuntimeSmokeTest', () => {
  it('runs the smoke test callback and opens trace history', async () => {
    const onRuntimeCheck = vi.fn();
    const onOpenTraceHistory = vi.fn();

    render(SettingsRuntimeSmokeTest, {
      busy: false,
      smokeResult: {
        command: 'smoke.ping',
        message: 'ok',
        steps: 1,
        eventCount: 1
      },
      onRuntimeCheck,
      onOpenTraceHistory
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Runtime smoke test' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Trace history' }));

    expect(onRuntimeCheck).toHaveBeenCalledTimes(1);
    expect(onOpenTraceHistory).toHaveBeenCalledTimes(1);
    expect(screen.getByText('1 step - 1 event')).toBeTruthy();
  });

  it('can reopen the setup wizard from Diagnostics', async () => {
    const onRunSetupWizard = vi.fn();

    render(SettingsRuntimeSmokeTest, {
      onRunSetupWizard
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Run setup wizard' }));

    expect(onRunSetupWizard).toHaveBeenCalledTimes(1);
  });
});
