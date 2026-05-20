// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { get } from 'svelte/store';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { appSettings, defaultAppSettings } from '$lib/stores/appSettings';
import { notifications } from '$lib/stores/notifications';
import NoProjectScreen from './NoProjectScreen.svelte';

const desktopMocks = vi.hoisted(() => ({
  listCanvasCourses: vi.fn(),
  validateVisionModel: vi.fn()
}));

vi.mock('$lib/desktop', () => desktopMocks);

const scrollIntoView = vi.fn();
const ONBOARDING_VALIDATION_DEBOUNCE_MS = 300;
const CONNECTION_MESSAGE_VISIBLE_MS = 1500;

function completeOnboarding() {
  appSettings.save({
    ...defaultAppSettings,
    onboardingCompleted: true
  });
}

function baseProps() {
  return {
    hasDesktopHost: true,
    showCreateForm: false,
    busyAction: null,
    createInput: {
      displayName: '',
      subject: null,
      courseCode: null,
      lmsCourseId: null,
      projectRoot: null,
      templatePdfPath: ''
    },
    actionError: null,
    recentProjects: [],
    onShowCreateForm: vi.fn(),
    onHideCreateForm: vi.fn(),
    onOpenProject: vi.fn(),
    onOpenRecentProject: vi.fn(),
    onChooseTemplatePdfForCreate: vi.fn(),
    onSubmitCreate: vi.fn(),
    onOpenSettings: vi.fn()
  };
}

async function flushOnboardingValidationDebounce() {
  await tick();
  await Promise.resolve();
  await tick();
  await vi.advanceTimersByTimeAsync(ONBOARDING_VALIDATION_DEBOUNCE_MS);
  for (let i = 0; i < 3; i += 1) {
    await Promise.resolve();
    await tick();
  }
}

async function flushConnectionTicker() {
  await vi.advanceTimersByTimeAsync(CONNECTION_MESSAGE_VISIBLE_MS);
  await tick();
}

describe('NoProjectScreen', () => {
  beforeEach(() => {
    vi.useRealTimers();
    localStorage.clear();
    appSettings.save(defaultAppSettings);
    notifications.clear();
    desktopMocks.listCanvasCourses.mockReset();
    desktopMocks.listCanvasCourses.mockResolvedValue([]);
    desktopMocks.validateVisionModel.mockReset();
    desktopMocks.validateVisionModel.mockResolvedValue({
      model: 'qwen3.5:9b',
      displayName: 'qwen3.5:9b',
      capabilities: ['vision', 'completion'],
      valid: true,
      reason: null,
      missingCapabilities: []
    });
    scrollIntoView.mockReset();
    Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', {
      configurable: true,
      value: scrollIntoView
    });
  });

  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
  });

  it('shows the browser preview notice and disables backend actions outside the desktop host', () => {
    completeOnboarding();
    render(NoProjectScreen, {
      ...baseProps(),
      hasDesktopHost: false
    });

    const createButton = screen.getByRole('button', { name: 'Create Project' }) as HTMLButtonElement;
    const openButton = screen.getByRole('button', { name: 'Open Project' }) as HTMLButtonElement;

    expect(screen.getByText('Browser preview mode')).toBeTruthy();
    expect(createButton.disabled).toBe(true);
    expect(openButton.disabled).toBe(true);
  });

  it('invokes the create and open callbacks from the primary actions', async () => {
    completeOnboarding();
    const props = baseProps();
    render(NoProjectScreen, props);

    await fireEvent.click(screen.getByRole('button', { name: 'Create Project' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Open Project' }));

    expect(props.onShowCreateForm).toHaveBeenCalledTimes(1);
    expect(props.onOpenProject).toHaveBeenCalledTimes(1);
  });

  it('invokes onOpenSettings when the Settings control is used', async () => {
    const props = baseProps();
    render(NoProjectScreen, props);

    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    expect(props.onOpenSettings).toHaveBeenCalledTimes(1);
  });

  it('keeps only Settings as a persistent navbar control', () => {
    render(NoProjectScreen, baseProps());

    expect(screen.getByRole('button', { name: 'Settings' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Switch to .* theme/ })).toBeNull();
    expect(screen.queryByText('ready')).toBeNull();
  });

  it('renders navbar notifications before the Settings control', () => {
    notifications.pushError('Runtime smoke test failed', 0);
    render(NoProjectScreen, baseProps());

    const toast = screen.getByRole('alert');
    const settings = screen.getByRole('button', { name: 'Settings' });

    expect(toast.textContent).toContain('Runtime smoke test failed');
    expect(toast.compareDocumentPosition(settings) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('scrolls the create form into view after it is opened', async () => {
    completeOnboarding();
    const props = baseProps();
    const { rerender } = render(NoProjectScreen, props);

    await rerender({ ...props, showCreateForm: true });

    await waitFor(() => {
      expect(scrollIntoView).toHaveBeenCalledWith({ behavior: 'smooth', block: 'start' });
    });
  });

  it('does not scroll when the create form is visible on initial render', () => {
    completeOnboarding();
    render(NoProjectScreen, {
      ...baseProps(),
      showCreateForm: true
    });

    expect(scrollIntoView).not.toHaveBeenCalled();
  });

  it('shows first-run setup until the operator skips or finishes it', async () => {
    render(NoProjectScreen, baseProps());

    expect(screen.getByText('First-run setup')).toBeTruthy();
    expect(screen.queryByText("Let's Score an Exam!")).toBeNull();
    expect(screen.queryByRole('button', { name: 'Create Project' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Open Project' })).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Skip' }));

    expect(screen.queryByText('First-run setup')).toBeNull();
    expect(screen.getByText("Let's Score an Exam!")).toBeTruthy();
  });

  it('captures LMS-linked Canvas setup details during onboarding', async () => {
    vi.useFakeTimers();
    render(NoProjectScreen, baseProps());

    await fireEvent.click(screen.getByRole('radio', { name: /LMS-linked/ }));
    const lmsProviderButton = screen.getByRole('combobox', { name: 'Provider: Canvas' });
    expect(lmsProviderButton.textContent).toContain('Canvas');
    await fireEvent.click(lmsProviderButton);
    expect(screen.getByRole('option', { name: 'Canvas' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('option', { name: 'Canvas' }));
    expect(lmsProviderButton.getAttribute('aria-expanded')).toBe('false');
    await fireEvent.input(screen.getByLabelText('Base URL'), {
      target: { value: 'https://canvas.example.test' }
    });
    const apiKeyInput = screen.getByLabelText('Canvas API key') as HTMLInputElement;
    expect(apiKeyInput.type).toBe('password');
    await fireEvent.click(screen.getByRole('button', { name: 'Show Canvas API key' }));
    expect(apiKeyInput.type).toBe('text');
    await fireEvent.click(screen.getByRole('button', { name: 'Hide Canvas API key' }));
    expect(apiKeyInput.type).toBe('password');
    await fireEvent.input(apiKeyInput, {
      target: { value: 'token-123' }
    });

    expect(screen.queryByRole('img', { name: 'How to get your Canvas API token' })).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Not sure how?' }));

    const guideImage = screen.getByRole('img', {
      name: 'How to get your Canvas API token'
    }) as HTMLImageElement;
    expect(guideImage.getAttribute('src')).toBe('/canvas-api-token-guide.png');
    await flushOnboardingValidationDebounce();
    vi.useRealTimers();
    await waitFor(() => {
      expect(desktopMocks.listCanvasCourses).toHaveBeenCalledWith(
        'https://canvas.example.test',
        'token-123'
      );
    });
    await waitFor(() => {
      expect((screen.getByRole('button', { name: 'Continue' }) as HTMLButtonElement).disabled).toBe(
        false
      );
    });
    expect((screen.getByRole('radio', { name: /LMS-linked/ }) as HTMLInputElement).checked).toBe(
      true
    );
  });

  it('keeps project setup blocked when the Canvas connectivity check fails', async () => {
    vi.useFakeTimers();
    desktopMocks.listCanvasCourses.mockRejectedValueOnce(new Error('401 unauthorized'));
    render(NoProjectScreen, baseProps());

    await fireEvent.click(screen.getByRole('radio', { name: /LMS-linked/ }));
    await fireEvent.input(screen.getByLabelText('Base URL'), {
      target: { value: 'https://canvas.example.test' }
    });
    await fireEvent.input(screen.getByLabelText('Canvas API key'), {
      target: { value: 'bad-token' }
    });

    await flushOnboardingValidationDebounce();
    await flushConnectionTicker();
    vi.useRealTimers();
    await waitFor(() => {
      expect(screen.getByText('Error: 401 unauthorized')).toBeTruthy();
    });
    expect((screen.getByRole('button', { name: 'Continue' }) as HTMLButtonElement).disabled).toBe(
      true
    );
  });

  it('captures manual and cloud AI setup choices during onboarding', async () => {
    vi.useFakeTimers();
    render(NoProjectScreen, baseProps());

    await fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
    expect(
      (screen.getByRole('radio', { name: /No AI Assistance/ }) as HTMLInputElement).checked
    ).toBe(true);
    await fireEvent.click(screen.getByRole('radio', { name: /Ollama Cloud/ }));
    const ollamaApiKeyInput = screen.getByLabelText('Ollama Cloud API key') as HTMLInputElement;
    expect(ollamaApiKeyInput.type).toBe('password');
    await fireEvent.click(screen.getByRole('button', { name: 'Show Ollama Cloud API key' }));
    expect(ollamaApiKeyInput.type).toBe('text');
    await fireEvent.click(screen.getByRole('button', { name: 'Hide Ollama Cloud API key' }));
    expect(ollamaApiKeyInput.type).toBe('password');
    expect(screen.queryByRole('img', { name: 'How to get your Ollama API token' })).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Not sure how?' }));
    const ollamaGuideImage = screen.getByRole('img', {
      name: 'How to get your Ollama API token'
    }) as HTMLImageElement;
    expect(ollamaGuideImage.getAttribute('src')).toBe('/ollama-api-token-guide.png');
    await fireEvent.input(ollamaApiKeyInput, {
      target: { value: 'ollama-key' }
    });

    expect((screen.getByRole('radio', { name: /Ollama Cloud/ }) as HTMLInputElement).checked).toBe(
      true
    );
    expect((screen.getByLabelText('Base URL') as HTMLInputElement).value).toBe(
      'https://ollama.com/api'
    );
    expect((screen.getByLabelText('Vision model') as HTMLInputElement).value).toBe(
      'qwen3.5:cloud'
    );
    await flushOnboardingValidationDebounce();
    await flushConnectionTicker();
    vi.useRealTimers();
    await waitFor(() => {
      expect(desktopMocks.validateVisionModel).toHaveBeenCalledWith(
        'ollama_cloud',
        'https://ollama.com/api',
        'qwen3.5:cloud',
        'ollama-key'
      );
    });
    await waitFor(() => {
      expect((screen.getByRole('button', { name: 'Continue' }) as HTMLButtonElement).disabled).toBe(
        false
      );
    });
  });

  it('checks Local Ollama connectivity without an API key before continuing AI setup', async () => {
    vi.useFakeTimers();
    render(NoProjectScreen, {
      ...baseProps(),
      visionModels: [
        { name: 'qwen3.5:9b', displayName: 'qwen3.5:9b' },
        { name: 'llava:7b', displayName: 'llava:7b' }
      ]
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
    await fireEvent.click(screen.getByRole('radio', { name: /Local Ollama/ }));
    await fireEvent.click(screen.getByRole('combobox', { name: 'Vision model: qwen3.5:9b' }));
    await fireEvent.input(screen.getByRole('textbox', { name: 'Search models' }), {
      target: { value: 'llava' }
    });
    await fireEvent.click(screen.getByRole('option', { name: 'llava:7b' }));

    await flushOnboardingValidationDebounce();
    vi.useRealTimers();
    await waitFor(() => {
      expect(desktopMocks.validateVisionModel).toHaveBeenCalledWith(
        'ollama_native',
        'http://127.0.0.1:11434',
        'llava:7b',
        null
      );
    });
    await waitFor(() => {
      expect((screen.getByRole('button', { name: 'Continue' }) as HTMLButtonElement).disabled).toBe(
        false
      );
    });
  });

  it('shows grading profile setup after AI assistance is enabled', async () => {
    vi.useFakeTimers();
    render(NoProjectScreen, {
      ...baseProps(),
      visionModels: [{ name: 'qwen3.5:9b', displayName: 'qwen3.5:9b' }]
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
    await fireEvent.click(screen.getByRole('radio', { name: /Local Ollama/ }));
    await flushOnboardingValidationDebounce();
    vi.useRealTimers();
    await waitFor(() => {
      expect((screen.getByRole('button', { name: 'Continue' }) as HTMLButtonElement).disabled).toBe(
        false
      );
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Continue' }));

    expect(screen.getByRole('button', { name: /Grading profile/ }).getAttribute('disabled')).toBeNull();
    expect((screen.getByRole('radio', { name: /Simple/ }) as HTMLInputElement).checked).toBe(true);

    await fireEvent.click(screen.getByRole('radio', { name: /Detailed/ }));
    expect(Object.values(get(appSettings).instructorProfile.enabledTags).every(Boolean)).toBe(true);
    await fireEvent.click(screen.getByRole('button', { name: 'Continue' }));

    const finishSetupButton = screen.getByRole('button', { name: 'Finish Setup' }) as HTMLButtonElement;
    expect(finishSetupButton.disabled).toBe(false);
  });

  it('keeps AI setup blocked when the Cloud Ollama connectivity check fails', async () => {
    vi.useFakeTimers();
    desktopMocks.validateVisionModel.mockRejectedValueOnce(new Error('Ollama rejected the API key'));
    render(NoProjectScreen, baseProps());

    await fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
    await fireEvent.click(screen.getByRole('radio', { name: /Ollama Cloud/ }));
    await fireEvent.input(screen.getByLabelText('Ollama Cloud API key'), {
      target: { value: 'bad-key' }
    });

    await flushOnboardingValidationDebounce();
    await flushConnectionTicker();
    vi.useRealTimers();
    await waitFor(() => {
      expect(screen.getByText('Error: Ollama rejected the API key')).toBeTruthy();
    });
    expect((screen.getByRole('button', { name: 'Continue' }) as HTMLButtonElement).disabled).toBe(
      true
    );
  });

  it('validates a typed Ollama vision model instead of requiring a discovered list option', async () => {
    vi.useFakeTimers();
    render(NoProjectScreen, baseProps());

    await fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
    await fireEvent.click(screen.getByRole('radio', { name: /Ollama Cloud/ }));
    await fireEvent.input(screen.getByLabelText('Vision model'), {
      target: { value: 'gemma3:cloud' }
    });
    await fireEvent.input(screen.getByLabelText('Ollama Cloud API key'), {
      target: { value: 'ollama-key' }
    });

    await flushOnboardingValidationDebounce();
    vi.useRealTimers();
    await waitFor(() => {
      expect(desktopMocks.validateVisionModel).toHaveBeenCalledWith(
        'ollama_cloud',
        'https://ollama.com/api',
        'gemma3:cloud',
        'ollama-key'
      );
    });
  });

  it('collapses completed wizard tasks and opens the next task', async () => {
    render(NoProjectScreen, baseProps());

    expect(screen.getByRole('button', { name: /AI assistance/ }).getAttribute('disabled')).not.toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Continue' }));

    expect(screen.getByRole('button', { name: /Project mode/ }).textContent).toContain('✓');
    expect(screen.getByRole('button', { name: /Project mode/ }).textContent).toContain('Complete');
    expect(screen.getByRole('button', { name: /AI assistance/ }).getAttribute('disabled')).toBeNull();
    expect(screen.getByRole('radio', { name: /No AI Assistance/ })).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
    const aiAssistanceHeader = screen.getByRole('button', { name: /AI assistance/ });
    expect(aiAssistanceHeader.textContent).toContain('✓');
    expect(aiAssistanceHeader.className).toContain('bg-message-success-bg');
    expect(screen.queryByRole('radio', { name: /No AI Assistance/ })).toBeNull();
    const finishSetupButton = screen.getByRole('button', { name: 'Finish Setup' }) as HTMLButtonElement;
    expect(finishSetupButton.disabled).toBe(false);
    expect(finishSetupButton.className).toContain('!bg-message-success-bg');
    expect(finishSetupButton.className).toContain('!text-message-success-text');
  });

  it('hides recent projects until onboarding is complete', async () => {
    const props = {
      ...baseProps(),
      recentProjects: [
        {
          projectPath: '/tmp/project',
          displayName: 'Midterm 1',
          courseCode: 'PHYS 221',
          openedAt: '2026-04-27T00:00:00Z'
        }
      ]
    };
    const { rerender } = render(NoProjectScreen, props);

    expect(screen.queryByText('Recent Projects')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Skip' }));
    await rerender(props);

    expect(screen.getByText('Recent Projects')).toBeTruthy();
  });
});
