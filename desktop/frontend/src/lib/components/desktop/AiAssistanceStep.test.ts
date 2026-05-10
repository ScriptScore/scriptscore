// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { appSettings, defaultAppSettings } from '$lib/stores/appSettings';
import AiAssistanceStep from './AiAssistanceStep.svelte';

describe('AiAssistanceStep', () => {
  beforeEach(() => {
    localStorage.clear();
    appSettings.save({
      ...defaultAppSettings,
      aiAssistEnabled: true,
      llmProvider: 'ollama_native',
      llmBaseUrl: 'http://127.0.0.1:11434',
      llmModel: 'qwen3.5:9b',
      llmApiKey: null
    });
  });

  it('commits local Ollama Base URL edits only after Accept', async () => {
    const onLlmBaseUrlInput = vi.fn();
    render(AiAssistanceStep, {
      visionModels: [{ name: 'qwen3.5:9b', displayName: 'qwen3.5:9b' }],
      onLlmBaseUrlInput
    });

    expect(screen.queryByRole('button', { name: 'Accept' })).toBeNull();

    const baseUrlInput = screen.getByLabelText('Base URL') as HTMLInputElement;
    await fireEvent.input(baseUrlInput, { target: { value: 'http://127.0.0.1:11435' } });

    expect(onLlmBaseUrlInput).not.toHaveBeenCalled();
    expect(baseUrlInput.value).toBe('http://127.0.0.1:11435');

    await fireEvent.click(screen.getByRole('button', { name: 'Accept' }));

    expect(onLlmBaseUrlInput).toHaveBeenCalledWith('http://127.0.0.1:11435');
  });

  it('disables local Ollama endpoint and model controls while model discovery is busy', () => {
    render(AiAssistanceStep, {
      visionModelsBusy: true,
      visionModels: [{ name: 'qwen3.5:9b', displayName: 'qwen3.5:9b' }]
    });

    expect((screen.getByLabelText('Base URL') as HTMLInputElement).disabled).toBe(true);
    expect(screen.queryByRole('button', { name: 'Accept' })).toBeNull();
    expect((screen.getByRole('combobox', { name: 'Vision model: qwen3.5:9b' }) as HTMLButtonElement).disabled).toBe(true);
  });
});
