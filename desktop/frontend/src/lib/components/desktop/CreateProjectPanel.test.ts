// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { appSettings, defaultAppSettings } from '$lib/stores/appSettings';
import CreateProjectPanel from './CreateProjectPanel.svelte';

const createInput = () => ({
  displayName: '',
  subject: null,
  courseCode: null,
  lmsCourseId: null,
  projectRoot: null,
  templatePdfPath: ''
});

describe('CreateProjectPanel', () => {
  beforeEach(() => {
    localStorage.clear();
    appSettings.save({
      ...defaultAppSettings,
      onboardingCompleted: true
    });
  });

  it('wires template selection, cancel, and submit callbacks', async () => {
    const input = createInput();
    const onChooseTemplatePdfForCreate = vi.fn();
    const onHideCreateForm = vi.fn();
    const onSubmitCreate = vi.fn();

    const props = {
      hasDesktopHost: true,
      busyAction: null,
      createInput: input,
      onChooseTemplatePdfForCreate,
      onHideCreateForm,
      onSubmitCreate
    };
    const { rerender } = render(CreateProjectPanel, props);

    await fireEvent.input(screen.getByLabelText('Exam Name'), {
      target: { value: 'Final Exam' }
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Choose PDF' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    expect(input.displayName).toBe('Final Exam');
    expect(onChooseTemplatePdfForCreate).toHaveBeenCalledTimes(1);
    expect(onHideCreateForm).toHaveBeenCalledTimes(1);

    await fireEvent.click(screen.getByRole('button', { name: 'Create Project' }));
    expect(onSubmitCreate).not.toHaveBeenCalled();

    input.templatePdfPath = '/tmp/template.pdf';
    await rerender(props);
    await fireEvent.click(screen.getByRole('button', { name: 'Create Project' }));
    expect(onSubmitCreate).toHaveBeenCalledTimes(1);
  });

  it('opens the template PDF guide from the template field', async () => {
    render(CreateProjectPanel, {
      hasDesktopHost: true,
      busyAction: null,
      createInput: createInput()
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Template guide' }));

    expect(screen.getByRole('dialog', { name: 'Template PDF guide' })).toBeTruthy();
    const guideImage = screen.getByRole('img', {
      name: 'Synthetic guide showing supported template PDF question numbering and page margin examples'
    }) as HTMLImageElement;
    expect(guideImage.getAttribute('src')).toBe('/template-pdf-question-id-guide.png');
    expect(screen.getByText(/Use one full-width column/)).toBeTruthy();
    expect(screen.getByText(/avoid embedded parts/)).toBeTruthy();
  });
});
