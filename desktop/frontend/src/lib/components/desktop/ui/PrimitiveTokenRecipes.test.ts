// SPDX-License-Identifier: AGPL-3.0-only
import { ApproximatelyEqualIcon } from '@hugeicons/core-free-icons';
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import DesktopButton from './DesktopButton.svelte';
import IconButton from './IconButton.svelte';
import IconSelectField from './IconSelectField.svelte';
import SegmentedControl from './SegmentedControl.svelte';
import SelectField from './SelectField.svelte';

describe('desktop primitive token recipes', () => {
  it('uses semantic surface and interaction tokens for default buttons', () => {
    render(DesktopButton, { 'aria-label': 'Save changes' });

    const button = screen.getByRole('button', { name: 'Save changes' });
    expect(button.className).toContain('bg-surface-card-control');
    expect(button.className).toContain('hover:bg-interaction-hover');
    expect(button.className).not.toContain('bg-card');
    expect(button.className).not.toContain('bg-muted');
    expect(button.className).not.toContain('bg-workspace-empty');
  });

  it('uses semantic surface and interaction tokens for default icon buttons', () => {
    render(IconButton, { ariaLabel: 'Refresh project' });

    const button = screen.getByRole('button', { name: 'Refresh project' });
    expect(button.className).toContain('bg-surface-card-control');
    expect(button.className).toContain('hover:bg-interaction-hover');
    expect(button.className).not.toContain('bg-card');
  });

  it('uses one semantic recipe for unselected rail icon buttons', () => {
    render(IconButton, { ariaLabel: 'Students', variant: 'rail', size: 'rail' });

    const button = screen.getByRole('button', { name: 'Students' });
    expect(button.className).toContain('border-border-subtle');
    expect(button.className).toContain('bg-transparent');
    expect(button.className).toContain('text-workspace-sidebar-muted');
    expect(button.className).toContain('hover:border-border-default');
    expect(button.className).toContain('hover:bg-interaction-hover');
    expect(button.className).toContain('hover:text-workspace-sidebar-foreground');
    expect(button.className).toContain('focus-visible:border-border-default');
    expect(button.className).toContain('focus-visible:bg-interaction-hover');
    expect(button.className).toContain('focus-visible:text-workspace-sidebar-foreground');
  });

  it('uses active semantic tokens for selected rail icon buttons', () => {
    render(IconButton, { ariaLabel: 'Results', variant: 'rail', size: 'rail', selected: true });

    const button = screen.getByRole('button', { name: 'Results' });
    expect(button.className).toContain('border-border-strong');
    expect(button.className).toContain('bg-interaction-active');
    expect(button.className).toContain('text-workspace-sidebar-foreground');
    expect(button.className).toContain('hover:bg-interaction-active');
    expect(button.className).toContain('focus-visible:bg-interaction-active');
  });

  it('uses semantic control and selected tokens for segmented controls', () => {
    render(SegmentedControl, {
      ariaLabel: 'Score display',
      value: 'points',
      options: [
        { value: 'points', label: 'Points' },
        { value: 'percent', label: 'Percent' }
      ]
    });

    const group = screen.getByRole('group', { name: 'Score display' });
    expect(group.className).toContain('bg-surface-card-control');
    expect(group.className).not.toContain('bg-card');
    expect(screen.getByRole('button', { name: 'Points' }).className).toContain('bg-interaction-selected');
  });

  it('uses semantic overlay and option tokens for icon select menus', async () => {
    render(IconSelectField, {
      ariaLabel: 'Filter students',
      menuLabel: 'Filter',
      value: 'all',
      icon: ApproximatelyEqualIcon,
      options: [
        { value: 'all', label: 'All' },
        { value: 'ready', label: 'Ready' }
      ]
    });

    const trigger = screen.getByRole('button', { name: 'Filter students' });
    expect(trigger.className).toContain('bg-surface-card-control');
    expect(trigger.className).toContain('hover:bg-interaction-hover');

    await fireEvent.click(trigger);

    const dialog = screen.getByRole('dialog', { name: 'Filter students' });
    expect(dialog.className).toContain('bg-surface-overlay');
    expect(dialog.className).not.toContain('bg-workspace-canvas');
    expect(within(dialog).getByRole('button', { name: 'All' }).className).toContain('bg-interaction-active');
    expect(within(dialog).getByRole('button', { name: 'Ready' }).className).toContain('hover:bg-interaction-hover');
  });

  it('uses semantic overlay, control, and option tokens for custom select menus', async () => {
    render(SelectField, {
      label: 'Course',
      value: 'math',
      searchable: true,
      showSubitems: true,
      options: [
        { value: 'math', label: 'Math', subitem: 'Period 1' },
        { value: 'science', label: 'Science', subitem: 'Period 2' }
      ]
    });

    const trigger = screen.getByRole('combobox', { name: 'Course: Math' });
    expect(trigger.className).toContain('bg-surface-message');
    expect(trigger.className).not.toContain('bg-workspace-empty');

    await fireEvent.click(trigger);

    const search = screen.getByRole('textbox', { name: 'Search' });
    expect(search.className).toContain('bg-surface-card-control');

    const popup = screen.getByRole('listbox', { name: 'Course' }).parentElement;
    expect(popup?.className).toContain('bg-surface-overlay');
    expect(popup?.className).not.toContain('bg-surface-sidebar');

    const selectedOption = screen.getByRole('option', { name: /Math/ });
    const availableOption = screen.getByRole('option', { name: /Science/ });
    expect(selectedOption.className).toContain('bg-interaction-active');
    expect(availableOption.className).toContain('hover:bg-interaction-hover');
  });
});
