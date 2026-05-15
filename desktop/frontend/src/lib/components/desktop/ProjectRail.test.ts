// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen } from '@testing-library/svelte';
import type { ComponentProps } from 'svelte';
import { describe, expect, it, vi } from 'vitest';

import ProjectRail from './ProjectRail.svelte';

function renderRail(props: Partial<ComponentProps<typeof ProjectRail>> = {}) {
  const defaults = {
    activeWorkflowStep: 'templateSetup' as const,
    hasDesktopHost: true,
    busy: false,
    onOpenProject: vi.fn(),
    onCloseProject: vi.fn(),
    onSelectWorkflowStep: vi.fn(),
    onSelectTemplateSetupSubstep: vi.fn()
  };

  render(ProjectRail, { ...defaults, ...props });
  return { props: { ...defaults, ...props } };
}

describe('ProjectRail', () => {
  it('opens the template setup submenu on focus and selects a substep', async () => {
    const { props } = renderRail();

    const trigger = screen.getByRole('button', { name: 'Template' });
    await fireEvent.focus(trigger);
    await fireEvent.click(await screen.findByRole('menuitem', { name: 'Review' }));

    expect(props.onSelectTemplateSetupSubstep).toHaveBeenCalledWith('review');
  });

  it('opens the template setup submenu on hover and closes on escape', async () => {
    renderRail();

    const trigger = screen.getByRole('button', { name: 'Template' });
    const wrapper = trigger.parentElement;
    if (!(wrapper instanceof HTMLElement)) {
      throw new TypeError('expected rail item wrapper');
    }

    await fireEvent.mouseEnter(wrapper);
    expect(await screen.findByRole('menuitem', { name: 'Setup' })).toBeTruthy();

    await fireEvent.keyDown(wrapper, { key: 'Escape' });
    expect(screen.queryByRole('menuitem', { name: 'Setup' })).toBeNull();
  });

  it('keeps the results rail action accessible and selectable', async () => {
    const { props } = renderRail({ activeWorkflowStep: 'exportResults' });

    const results = screen.getByRole('button', { name: 'Results' });
    expect(results.className).toContain('border-border-strong');
    expect(results.className).toContain('bg-interaction-active');
    expect(results.className).toContain('text-workspace-sidebar-foreground');

    await fireEvent.click(results);

    expect(props.onSelectWorkflowStep).toHaveBeenCalledWith('exportResults');
  });

  it('keeps the close project rail action accessible and wired to the same callback', async () => {
    const { props } = renderRail();

    const closeProject = screen.getByRole('button', { name: 'Close Project' });
    expect(closeProject.className).toContain('border-border-subtle');
    expect(closeProject.className).toContain('bg-transparent');
    expect(closeProject.className).toContain('text-workspace-sidebar-muted');

    await fireEvent.click(closeProject);

    expect(props.onCloseProject).toHaveBeenCalledOnce();
  });

  it('highlights settings when an update is available', () => {
    renderRail({ updateAvailable: true });

    const settings = screen.getByRole('button', { name: 'Settings' });
    expect(settings.getAttribute('title')).toBe('Update available');
    expect(settings.className).toContain('border-message-info-border');
  });
});
