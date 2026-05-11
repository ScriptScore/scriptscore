// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const desktopMocks = vi.hoisted(() => ({
  getLegalDisclosure: vi.fn(),
  isDesktopHost: vi.fn(),
  listCanvasCourses: vi.fn(),
  validateVisionModel: vi.fn()
}));

vi.mock('$lib/desktop', () => desktopMocks);

import SettingsWorkspace from './SettingsWorkspace.svelte';
import { defaultAppSettings } from '$lib/stores/appSettings';
import type { AppSettings } from '$lib/types';

describe('SettingsWorkspace', () => {
  beforeEach(() => {
    desktopMocks.listCanvasCourses.mockReset();
    desktopMocks.listCanvasCourses.mockResolvedValue([{ id: 'course-1', name: 'Course 1' }]);
    desktopMocks.getLegalDisclosure.mockReset();
    desktopMocks.isDesktopHost.mockReset();
    desktopMocks.isDesktopHost.mockReturnValue(false);
    desktopMocks.getLegalDisclosure.mockResolvedValue({
      licenseExpression: 'AGPL-3.0-only',
      sourceUrl: 'https://example.test/source',
      localNoticesPath: 'legal/THIRD_PARTY_NOTICES.md',
      thirdPartyNotices: 'Generated notices',
      policyReportJson: '{}',
      artifactStatus: 'bundled'
    });
    desktopMocks.validateVisionModel.mockReset();
    desktopMocks.validateVisionModel.mockResolvedValue({
      model: 'qwen3.5:cloud',
      displayName: 'qwen3.5:cloud',
      capabilities: ['vision', 'completion'],
      valid: true,
      reason: null,
      missingCapabilities: []
    });
  });

  it('defaults to Connections and switches settings categories from the sidebar', async () => {
    render(SettingsWorkspace, {
      settings: structuredClone(defaultAppSettings)
    });

    const connectionsTab = screen.getByRole('button', { name: 'Connections' });
    const gradingTab = screen.getByRole('button', { name: 'Grading' });
    expect(screen.getByRole('heading', { name: 'Connections' })).toBeTruthy();
    expect(connectionsTab.className).toContain('bg-workspace-sidebar-active');
    expect(gradingTab.className).not.toContain('bg-workspace-sidebar-active');
    expect(screen.queryByRole('button', { name: /save settings/i })).toBeNull();
    await fireEvent.click(gradingTab);
    expect(screen.getByRole('heading', { name: 'Grading' })).toBeTruthy();
    expect(connectionsTab.className).not.toContain('bg-workspace-sidebar-active');
    expect(gradingTab.className).toContain('bg-workspace-sidebar-active');
    await fireEvent.click(screen.getByRole('button', { name: 'AI Assistance' }));
    expect(screen.getByRole('heading', { name: 'AI Assistance' })).toBeTruthy();
  });

  it('shows a clear warning when plaintext LMS binding secret fallback is enabled', async () => {
    const settings: AppSettings = {
      ...defaultAppSettings,
      lmsProvider: 'canvas'
    };
    const onSettingsChange = vi.fn();

    render(SettingsWorkspace, {
      settings,
      onSettingsChange
    });

    expect(
      screen.getByText(
        'Keyring-only is active. Existing plaintext fallback files are imported only when no keyring secret exists yet.'
      )
    ).toBeTruthy();
    expect(screen.queryByText('Plaintext fallback enabled')).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: /allow plaintext fallback for lms binding secret/i }));

    expect(settings.lmsBindingSecretPlaintextFallback).toBe(true);
    expect(onSettingsChange).toHaveBeenCalledWith(
      expect.objectContaining({ lmsBindingSecretPlaintextFallback: true })
    );
    expect(screen.getByText('Plaintext fallback enabled')).toBeTruthy();
    expect(
      screen.getByText(/Anyone with filesystem access to this desktop profile can reuse that secret to recompute pseudonymous LMS binding tokens\./i)
    ).toBeTruthy();
  });

  it('shows Canvas connection ticker messages in LMS configuration', async () => {
    const settings: AppSettings = {
      ...defaultAppSettings,
      lmsProvider: 'canvas',
      lmsCanvasBaseUrl: 'https://canvas.example.test',
      lmsCanvasApiKey: 'token-123'
    };

    render(SettingsWorkspace, {
      settings,
      hasDesktopHost: true
    });

    await waitFor(
      () => {
        expect(desktopMocks.listCanvasCourses).toHaveBeenCalledWith(
          'https://canvas.example.test',
          'token-123'
        );
      },
      { timeout: 2500 }
    );
    expect(await screen.findByText('Connected. 1 course available.', {}, { timeout: 2500 })).toBeTruthy();
  });

  it('edits the optional Paddle model directory override', async () => {
    const settings: AppSettings = {
      ...defaultAppSettings,
      lmsProvider: 'canvas'
    };
    const onSettingsChange = vi.fn();

    render(SettingsWorkspace, {
      settings,
      onSettingsChange
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Storage' }));
    const input = screen.getByLabelText(/PII Paddle model directory/i);
    await fireEvent.input(input, { target: { value: '/tmp/paddle-models' } });
    expect(settings.piiPaddleModelDir).toBe('/tmp/paddle-models');
    expect(onSettingsChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ piiPaddleModelDir: '/tmp/paddle-models' })
    );

    await fireEvent.input(input, { target: { value: '   ' } });
    expect(settings.piiPaddleModelDir).toBeNull();
    expect(onSettingsChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ piiPaddleModelDir: null })
    );
  });

  it('offers folder picker controls for the Paddle model directory override', async () => {
    const settings: AppSettings = {
      ...defaultAppSettings,
      piiPaddleModelDir: '/tmp/paddle-models'
    };
    const onChoosePiiPaddleModelDirectory = vi.fn();
    const onClearPiiPaddleModelDirectory = vi.fn();

    render(SettingsWorkspace, {
      settings,
      onChoosePiiPaddleModelDirectory,
      onClearPiiPaddleModelDirectory
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Storage' }));
    await fireEvent.click(screen.getAllByRole('button', { name: 'Choose folder' })[1]);
    await fireEvent.click(screen.getByRole('button', { name: 'Clear model folder' }));

    expect(onChoosePiiPaddleModelDirectory).toHaveBeenCalledTimes(1);
    expect(onClearPiiPaddleModelDirectory).toHaveBeenCalledTimes(1);
  });

  it('shows configurable project storage controls', async () => {
    const settings: AppSettings = {
      ...defaultAppSettings,
      projectsDirectory: '/tmp/scriptscore-projects'
    };
    const onChooseProjectsDirectory = vi.fn();
    const onClearProjectsDirectory = vi.fn();

    render(SettingsWorkspace, {
      settings,
      resolvedProjectsDirectory: '/tmp/scriptscore-projects',
      onChooseProjectsDirectory,
      onClearProjectsDirectory
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Storage' }));
    expect(screen.getByText('Custom folder')).toBeTruthy();
    expect(screen.getByText('/tmp/scriptscore-projects')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Run setup wizard' })).toBeNull();
    await fireEvent.click(screen.getAllByRole('button', { name: 'Choose folder' })[0]);
    await fireEvent.click(screen.getByRole('button', { name: 'Use system default' }));

    expect(onChooseProjectsDirectory).toHaveBeenCalledTimes(1);
    expect(onClearProjectsDirectory).toHaveBeenCalledTimes(1);
  });

  it('shows the resolved system default project folder', async () => {
    render(SettingsWorkspace, {
      settings: structuredClone(defaultAppSettings),
      resolvedProjectsDirectory: '/home/user/Documents/ScriptScore'
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Storage' }));

    expect(screen.getByText('System default')).toBeTruthy();
    expect(screen.getByText('/home/user/Documents/ScriptScore')).toBeTruthy();
  });

  it('edits automation categories independently and updates the compatibility flag', async () => {
    const settings: AppSettings = {
      ...defaultAppSettings,
      aiAssistEnabled: false
    };
    const onSettingsChange = vi.fn();

    render(SettingsWorkspace, {
      settings,
      onSettingsChange
    });

    await fireEvent.click(screen.getByRole('button', { name: 'AI Assistance' }));
    await fireEvent.click(screen.getByText('Exam Analysis').closest('button') as HTMLButtonElement);
    expect(settings.aiAssistEnabled).toBe(true);
    expect(settings.aiAssistCategories.questionAnalysis).toBe(true);
    expect(onSettingsChange).toHaveBeenLastCalledWith(
      expect.objectContaining({
        aiAssistEnabled: true,
        aiAssistCategories: expect.objectContaining({ questionAnalysis: true })
      })
    );

    await fireEvent.click(screen.getAllByText('Grading')[1].closest('button') as HTMLButtonElement);
    expect(settings.aiAssistCategories.parsingReview).toBe(true);
    expect(settings.aiAssistCategories.rubrics).toBe(false);
  });

  it('renders the non-blank answer minimum disabled state clearly', async () => {
    const settings: AppSettings = {
      ...defaultAppSettings,
      instructorProfile: {
        ...defaultAppSettings.instructorProfile,
        includeMinimumCreditCriterion: false,
        minimumCreditPercent: 20
      }
    };

    render(SettingsWorkspace, {
      settings
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Grading' }));

    const switchControl = screen.getByRole('switch', {
      name: 'Award minimum points for non-blank answers'
    });
    expect(switchControl.getAttribute('aria-checked')).toBe('false');
    expect(screen.getByText('Non-blank answer minimum')).toBeTruthy();
    expect(screen.getByText('Award minimum points for non-blank answers')).toBeTruthy();
    expect(
      screen.getByText('Off — no minimum is guaranteed and no rubric criterion will be added.')
    ).toBeTruthy();
    expect(screen.queryByText('Non-blank answers may receive zero points.')).toBeNull();
    expect(
      screen.queryByLabelText('Minimum percentage of question points for non-blank answers')
    ).toBeNull();
  });

  it('edits and validates the non-blank answer minimum percentage inline', async () => {
    const settings: AppSettings = {
      ...defaultAppSettings,
      instructorProfile: {
        ...defaultAppSettings.instructorProfile,
        includeMinimumCreditCriterion: false,
        minimumCreditPercent: 20
      }
    };
    const onSettingsChange = vi.fn();

    render(SettingsWorkspace, {
      settings,
      onSettingsChange
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Grading' }));
    await fireEvent.click(screen.getByRole('switch', { name: 'Award minimum points for non-blank answers' }));

    expect(settings.instructorProfile.includeMinimumCreditCriterion).toBe(true);
    expect(onSettingsChange).toHaveBeenLastCalledWith(
      expect.objectContaining({
        instructorProfile: expect.objectContaining({ includeMinimumCreditCriterion: true })
      })
    );
    expect(screen.getByText('Award at least')).toBeTruthy();
    expect(screen.getByText('of question points for any non-blank answer')).toBeTruthy();
    expect(
      screen.getByText('Automatically adds a rubric criterion for non-blank attempts.')
    ).toBeTruthy();
    expect(
      screen.queryByText("The minimum is calculated as a percentage of this question's total point value.")
    ).toBeNull();

    const input = screen.getByLabelText(
      'Minimum percentage of question points for non-blank answers'
    ) as HTMLInputElement;
    expect(input.value).toBe('20');

    await fireEvent.input(input, { target: { value: '35' } });
    expect(settings.instructorProfile.minimumCreditPercent).toBe(35);
    expect(onSettingsChange).toHaveBeenLastCalledWith(
      expect.objectContaining({
        instructorProfile: expect.objectContaining({ minimumCreditPercent: 35 })
      })
    );

    await fireEvent.input(input, { target: { value: '101' } });
    expect(settings.instructorProfile.minimumCreditPercent).toBe(35);
    expect(input.value).toBe('35');

    await fireEvent.input(input, { target: { value: '0' } });
    expect(settings.instructorProfile.minimumCreditPercent).toBe(0);
  });

  it('applies Ollama provider defaults and uses text entry for cloud models', async () => {
    const settings: AppSettings = {
      ...defaultAppSettings,
      llmProvider: 'ollama_native',
      llmBaseUrl: 'http://127.0.0.1:11434',
      llmModel: 'qwen3.5:9b',
      llmApiKey: null
    };
    const onSettingsChange = vi.fn();

    render(SettingsWorkspace, {
      settings,
      visionModels: [{ name: 'llava:7b', displayName: 'llava:7b' }],
      onSettingsChange
    });

    await fireEvent.click(screen.getAllByRole('combobox', { name: /Provider:/ })[0]);
    await fireEvent.click(screen.getByRole('option', { name: 'ollama_cloud' }));

    expect(settings.llmProvider).toBe('ollama_cloud');
    expect(settings.llmBaseUrl).toBe('https://ollama.com/api');
    expect(settings.llmModel).toBe('qwen3.5:cloud');
    expect(onSettingsChange).toHaveBeenCalledWith(
      expect.objectContaining({
        llmProvider: 'ollama_cloud',
        llmBaseUrl: 'https://ollama.com/api',
        llmModel: 'qwen3.5:cloud'
      })
    );
    expect(screen.getByLabelText('Vision model')).toBeInstanceOf(HTMLInputElement);
  });

  it('commits local Ollama Base URL edits only after Accept in settings', async () => {
    const settings: AppSettings = {
      ...defaultAppSettings,
      llmProvider: 'ollama_native',
      llmBaseUrl: 'http://127.0.0.1:11434',
      llmModel: 'qwen3.5:9b',
      llmApiKey: null
    };
    const onSettingsChange = vi.fn();

    render(SettingsWorkspace, {
      settings,
      visionModels: [{ name: 'qwen3.5:9b', displayName: 'qwen3.5:9b' }],
      onSettingsChange
    });

    expect(screen.queryByRole('button', { name: 'Accept' })).toBeNull();

    const baseUrlInput = screen.getAllByLabelText('Base URL')[0] as HTMLInputElement;
    await fireEvent.input(baseUrlInput, { target: { value: 'http://127.0.0.1:11435' } });

    expect(settings.llmBaseUrl).toBe('http://127.0.0.1:11434');
    expect(onSettingsChange).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole('button', { name: 'Accept' }));

    expect(settings.llmBaseUrl).toBe('http://127.0.0.1:11435');
    expect(onSettingsChange).toHaveBeenCalledWith(
      expect.objectContaining({ llmBaseUrl: 'http://127.0.0.1:11435' })
    );
  });

  it('disables local Ollama endpoint and model controls while settings discovery is busy', () => {
    const settings: AppSettings = {
      ...defaultAppSettings,
      llmProvider: 'ollama_native',
      llmBaseUrl: 'http://127.0.0.1:11434',
      llmModel: 'qwen3.5:9b',
      llmApiKey: null
    };

    render(SettingsWorkspace, {
      settings,
      visionModelsBusy: true,
      visionModels: [{ name: 'qwen3.5:9b', displayName: 'qwen3.5:9b' }]
    });

    expect((screen.getAllByLabelText('Base URL')[0] as HTMLInputElement).disabled).toBe(true);
    expect(screen.queryByRole('button', { name: 'Accept' })).toBeNull();
    expect((screen.getByRole('combobox', { name: 'Vision model: qwen3.5:9b' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('does not fire provider changes when reselecting the current provider option', async () => {
    const settings: AppSettings = {
      ...defaultAppSettings,
      llmProvider: 'ollama_cloud',
      llmBaseUrl: 'https://custom.example.test/api',
      llmModel: 'custom-cloud-model',
      llmApiKey: 'ollama-key'
    };
    const onSettingsChange = vi.fn();

    render(SettingsWorkspace, {
      settings,
      onSettingsChange
    });

    await fireEvent.click(screen.getAllByRole('combobox', { name: /Provider:/ })[0]);
    await fireEvent.click(screen.getByRole('option', { name: 'ollama_cloud' }));

    expect(settings.llmProvider).toBe('ollama_cloud');
    expect(settings.llmBaseUrl).toBe('https://custom.example.test/api');
    expect(settings.llmModel).toBe('custom-cloud-model');
    expect(onSettingsChange).not.toHaveBeenCalled();
  });

  it('shows Ollama Cloud connection ticker messages in LLM provider settings', async () => {
    const settings: AppSettings = {
      ...defaultAppSettings,
      llmProvider: 'ollama_cloud',
      llmBaseUrl: 'https://ollama.com/api',
      llmModel: 'qwen3.5:cloud',
      llmApiKey: 'ollama-key'
    };

    render(SettingsWorkspace, {
      settings,
      hasDesktopHost: true
    });

    await waitFor(
      () => {
        expect(desktopMocks.validateVisionModel).toHaveBeenCalledWith(
          'ollama_cloud',
          'https://ollama.com/api',
          'qwen3.5:cloud',
          'ollama-key'
        );
      },
      { timeout: 2500 }
    );
    expect(
      await screen.findByText('Connected. qwen3.5:cloud supports vision.', {}, { timeout: 2500 })
    ).toBeTruthy();
  });

  it('keeps Preferences and Diagnostics isolated behind their own categories', async () => {
    const settings: AppSettings = {
      ...defaultAppSettings,
      theme: 'dark'
    };
    const onRuntimeCheck = vi.fn();
    const onRunSetupWizard = vi.fn();
    const onSettingsChange = vi.fn();

    render(SettingsWorkspace, {
      settings,
      onRuntimeCheck,
      onRunSetupWizard,
      onSettingsChange
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Preferences' }));
    await fireEvent.click(screen.getByRole('button', { name: /Dark theme/i }));
    expect(settings.theme).toBe('light');
    expect(onSettingsChange).toHaveBeenLastCalledWith(expect.objectContaining({ theme: 'light' }));

    await fireEvent.click(screen.getByRole('button', { name: 'Diagnostics' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Run setup wizard' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Runtime smoke test' }));
    expect(onRunSetupWizard).toHaveBeenCalledTimes(1);
    expect(onRuntimeCheck).toHaveBeenCalledTimes(1);
  });

  it('shows source code and open source license disclosure from bundled artifacts', async () => {
    desktopMocks.getLegalDisclosure.mockResolvedValue({
      licenseExpression: 'AGPL-3.0-only',
      sourceUrl: 'https://example.test/source',
      localNoticesPath: 'legal/THIRD_PARTY_NOTICES.md',
      thirdPartyNotices:
        '# Third-Party Notices\n\nGenerated notices\n\n## Inventory Summary\n\nName,Version,License,Source,Scope\nexample,1.0,MIT,npm,npm-runtime\npandas,3.0.2,BSD-3-Clause [1],python,python-runtime\ndesktop/frontend/build/scriptscore-app-icon.png,,,assets,frontend-asset\n\n## License Notes\n\n- [1] pandas: Full license text continues for a long time and should not stretch the table.\n\n## Review Findings\n\n- review: example package needs review',
      policyReportJson: '{}',
      artifactStatus: 'bundled'
    });

    render(SettingsWorkspace, {
      settings: structuredClone(defaultAppSettings)
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Legal' }));

    const legalHeading = screen.getByRole('heading', {
      name: 'Source Code and Open Source Licenses'
    });
    expect(legalHeading).toBeTruthy();
    expect(legalHeading.closest('.bg-surface-card-subtle')).toBeNull();
    const sourceLink = await screen.findByRole('link', { name: 'https://example.test/source' });
    expect(sourceLink).toHaveProperty('href', 'https://example.test/source');
    expect(sourceLink).toHaveProperty('target', '_blank');
    expect(sourceLink.className).toContain('underline');
    expect(sourceLink.className).not.toContain('border');
    const browserClick = new MouseEvent('click', { bubbles: true, cancelable: true });
    sourceLink.dispatchEvent(browserClick);
    expect(browserClick.defaultPrevented).toBe(false);
    expect(screen.getByText('AGPL-3.0-only')).toBeTruthy();
    expect(screen.getByText('Bundled Third-Party Notices')).toBeTruthy();
    expect(screen.queryByText('legal/THIRD_PARTY_NOTICES.md')).toBeNull();
    expect(screen.queryByText('Generated notices')).toBeNull();

    const showNotices = screen.getByRole('button', { name: 'Show notices' });
    expect(showNotices.className).toContain('min-w-32');
    expect(showNotices.className).toContain('whitespace-nowrap');
    await fireEvent.click(showNotices);

    expect(screen.getByRole('heading', { name: 'Third-Party Notices' })).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'Inventory Summary' })).toBeTruthy();
    expect(screen.queryByText(/Name,Version,License,Source,Scope/)).toBeNull();
    expect(screen.getByText('example')).toBeTruthy();
    expect(screen.getByText('pandas')).toBeTruthy();
    expect(screen.getByText('1.0')).toBeTruthy();
    expect(screen.getByText('npm-runtime')).toBeTruthy();
    expect(screen.getByText('BSD-3-Clause [1]')).toBeTruthy();
    expect(screen.getByText('License Notes')).toBeTruthy();
    expect(screen.getByText('[1] pandas: Full license text continues for a long time and should not stretch the table.')).toBeTruthy();
    expect(screen.getByText(/Full license text continues/)).toBeTruthy();
    expect(screen.getByText('desktop/frontend/build/scriptscore-app-icon.png')).toBeTruthy();
    expect(screen.getByText('Not applicable')).toBeTruthy();
    expect(screen.getByText('Release review required')).toBeTruthy();
    expect(screen.queryByRole('heading', { name: 'Review Findings' })).toBeNull();
    expect(screen.queryByText('review: example package needs review')).toBeNull();
    expect(screen.getByText('Generated notices')).toBeTruthy();
  });

  it('renders the legal fallback when generated artifacts are unavailable', async () => {
    desktopMocks.getLegalDisclosure.mockResolvedValue({
      licenseExpression: 'AGPL-3.0-only',
      sourceUrl: 'https://github.com/ScriptScore/scriptscore',
      localNoticesPath: 'desktop/dist/legal/THIRD_PARTY_NOTICES.md',
      thirdPartyNotices: 'Generated third-party notices are bundled in packaged desktop builds.',
      policyReportJson: '{}',
      artifactStatus: 'fallback'
    });

    render(SettingsWorkspace, {
      settings: structuredClone(defaultAppSettings)
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Legal' }));

    expect(await screen.findByText('Dev/Test Fallback')).toBeTruthy();
    expect(screen.getByText('https://github.com/ScriptScore/scriptscore')).toBeTruthy();
    expect(
      screen.getByText('Generated third-party notices are bundled in packaged desktop builds.')
    ).toBeTruthy();
  });

  it('edits preliminary grading worker preference', async () => {
    const settings: AppSettings = {
      ...defaultAppSettings,
      preliminaryGradingMaxWorkers: 1
    };
    const onSettingsChange = vi.fn();

    render(SettingsWorkspace, {
      settings,
      onSettingsChange
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Preferences' }));
    const workerControl = screen.getByRole('combobox', { name: /Preliminary grading workers:/ });
    expect(workerControl.closest('.max-w-xs')).toBeTruthy();
    await fireEvent.click(workerControl);
    await fireEvent.click(screen.getByRole('option', { name: /3/ }));

    expect(settings.preliminaryGradingMaxWorkers).toBe(3);
    expect(onSettingsChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ preliminaryGradingMaxWorkers: 3 })
    );
  });
});
