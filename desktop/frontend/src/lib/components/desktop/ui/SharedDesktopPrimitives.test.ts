// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import InlineMessage from './InlineMessage.svelte';
import RadioCardGroup from './RadioCardGroup.svelte';
import SidebarRow from './SidebarRow.svelte';
import StatusBadge from './StatusBadge.svelte';
import TextareaField from './TextareaField.svelte';
import TextField from './TextField.svelte';
import ToggleRow from './ToggleRow.svelte';

describe('shared desktop primitives', () => {
  it('renders inline feedback with tone-appropriate live-region roles', () => {
    render(InlineMessage, { tone: 'info', message: 'Ready to continue' });

    const status = screen.getByRole('status');
    expect(status.textContent).toContain('Ready to continue');
    expect(status.className).toContain('message-info');

    render(InlineMessage, { tone: 'warning', message: 'Review required' });

    const alert = screen.getByRole('alert');
    expect(alert.textContent).toContain('Review required');
    expect(alert.className).toContain('message-warning');
  });

  it('renders status badges with semantic tone styling', () => {
    render(StatusBadge, { tone: 'success', label: 'Approved' });

    const badge = screen.getByText('Approved');
    expect(badge.tagName).toBe('SPAN');
    expect(badge.className).toContain('message-success');
  });

  it('toggles explicit switch rows and ignores disabled interaction', async () => {
    const onToggle = vi.fn();
    render(ToggleRow, {
      title: 'Enable automation',
      description: 'Run supported workflow steps automatically.',
      onToggle
    });

    const switchRow = screen.getByRole('button', { name: /Enable automation/ });
    expect(switchRow.getAttribute('aria-pressed')).toBe('false');

    await fireEvent.click(switchRow);

    expect(switchRow.getAttribute('aria-pressed')).toBe('true');
    expect(onToggle).toHaveBeenCalledWith(true);

    const disabledToggle = vi.fn();
    render(ToggleRow, {
      title: 'Locked automation',
      checked: false,
      disabled: true,
      onToggle: disabledToggle
    });

    const disabledRow = screen.getByRole('button', { name: 'Locked automation' });
    await fireEvent.click(disabledRow);

    expect(disabledRow.getAttribute('aria-pressed')).toBe('false');
    expect(disabledToggle).not.toHaveBeenCalled();
  });

  it('selects enabled radio-card choices and leaves disabled choices unchanged', async () => {
    const onChange = vi.fn();
    render(RadioCardGroup, {
      legend: 'Project mode',
      value: 'local',
      expandedValues: ['lms'],
      options: [
        { value: 'local', title: 'Local', description: 'Keep projects on this computer.' },
        { value: 'lms', title: 'LMS-linked', description: 'Connect projects to an LMS.' },
        { value: 'plus', title: 'ScriptScore Plus', description: 'Premium workflow.', disabled: true }
      ],
      onChange
    });

    expect((screen.getByRole('radio', { name: /Local/ }) as HTMLInputElement).checked).toBe(true);

    await fireEvent.click(screen.getByRole('radio', { name: /LMS-linked/ }));

    expect((screen.getByRole('radio', { name: /LMS-linked/ }) as HTMLInputElement).checked).toBe(
      true
    );
    expect(onChange).toHaveBeenCalledWith('lms');

    const disabledOption = screen.getByRole('radio', { name: /ScriptScore Plus/ }) as HTMLInputElement;
    expect(disabledOption.disabled).toBe(true);

    await fireEvent.click(disabledOption);

    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it('keeps sidebar rows as semantic buttons across active and disabled states', () => {
    render(SidebarRow, { active: true, disabled: true });

    const row = screen.getByRole('button');
    expect((row as HTMLButtonElement).disabled).toBe(true);
    expect(row.className).toContain('bg-interaction-active');
    expect(row.className).toContain('text-workspace-sidebar-foreground');
  });

  it('renders labeled text controls with field shell feedback and semantic tokens', async () => {
    render(TextField, {
      id: 'course-code',
      label: 'Course code',
      hint: 'Shown in the project header.',
      value: '',
      placeholder: 'MATH 101'
    });

    const input = screen.getByLabelText('Course code');
    expect(screen.getByText('Shown in the project header.')).toBeTruthy();
    expect(input.className).toContain('bg-workspace-empty');

    await fireEvent.input(input, { target: { value: 'MATH 101' } });

    expect((input as HTMLInputElement).value).toBe('MATH 101');

    render(TextareaField, {
      id: 'grading-guidance',
      label: 'Grading guidance',
      error: 'Guidance is required.',
      value: ''
    });

    const textarea = screen.getByLabelText('Grading guidance');
    expect(screen.getByText('Guidance is required.')).toBeTruthy();
    expect(textarea.className).toContain('bg-workspace-empty');

    await fireEvent.input(textarea, { target: { value: 'Award partial credit.' } });

    expect((textarea as HTMLTextAreaElement).value).toBe('Award partial credit.');
  });
});
