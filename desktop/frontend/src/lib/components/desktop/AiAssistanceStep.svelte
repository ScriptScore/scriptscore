<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { AiCloudIcon, AiLaptopIcon, MagicWand01Icon, SparklesIcon } from '@hugeicons/core-free-icons';

  import type { VisionCapableModel } from '$lib/types';
  import { appSettings } from '$lib/stores/appSettings';
  import ConnectionTicker from './ConnectionTicker.svelte';
  import TokenGuidePopover from './TokenGuidePopover.svelte';
  import { DesktopButton, RadioCardGroup, SelectField, TextField, compactTabActionButtonClass } from './ui';

  type ConnectionTickerTone = 'warning' | 'info' | 'success' | 'error';

  export let ready = false;
  export let message: { text: string; tone: ConnectionTickerTone; sticky: boolean } | null = null;
  export let visionModels: VisionCapableModel[] = [];
  export let visionModelsBusy = false;
  export let onDisableAiAssist: (() => void) | null = null;
  export let onEnableAiAssist: ((provider: 'ollama_native' | 'ollama_cloud') => void) | null = null;
  export let onLlmBaseUrlInput: ((value: string) => void) | null = null;
  export let onLlmModelInput: ((value: string) => void) | null = null;
  export let onLlmApiKeyInput: ((value: string | null) => void) | null = null;
  export let onContinue: (() => void) | null = null;

  const OLLAMA_LOCAL_BASE_URL = 'http://127.0.0.1:11434';
  const OLLAMA_CLOUD_BASE_URL = 'https://ollama.com/api';
  const OLLAMA_CLOUD_DEFAULT_MODEL = 'qwen3.5:cloud';
  const aiAssistanceExpandedValues = ['ollama_native', 'ollama_cloud'];
  const aiAssistanceOptions = [
    {
      value: 'manual',
      title: 'No AI Assistance',
      description: 'No AI models will be used to automate any part of the workflow.',
      icon: MagicWand01Icon
    },
    {
      value: 'ollama_native',
      title: 'Local Ollama',
      description: 'Use a local Ollama server on this computer or your network.',
      icon: AiLaptopIcon
    },
    {
      value: 'ollama_cloud',
      title: 'Ollama Cloud',
      description: 'Connect to Ollama-hosted cloud models with an API key.',
      icon: AiCloudIcon
    },
    {
      value: 'scriptscore_plus',
      title: 'ScriptScorePlus',
      description: 'Hosted ScriptScore AI provider support will be added later.',
      icon: SparklesIcon,
      disabled: true
    }
  ];

  let ollamaApiKeyVisible = false;
  let ollamaTokenHelpOpen = false;
  let localBaseUrlDraft = '';
  let lastCommittedLocalBaseUrl = '';

  $: aiAssistanceValue = !$appSettings.aiAssistEnabled
    ? 'manual'
    : $appSettings.llmProvider === 'ollama_cloud'
      ? 'ollama_cloud'
      : 'ollama_native';
  $: localModelOptions =
    $appSettings.llmModel && !visionModels.some((model) => model.name === $appSettings.llmModel)
      ? [{ name: $appSettings.llmModel, displayName: `${$appSettings.llmModel} (current)` }, ...visionModels]
      : visionModels;
  $: localVisionModelOptions =
    localModelOptions.length > 0
      ? localModelOptions.map((model) => ({ value: model.name, label: model.displayName }))
      : [
          {
            value: $appSettings.llmModel,
            label: visionModelsBusy ? 'Loading local models...' : 'No vision models found'
          }
        ];
  $: if (aiAssistanceValue === 'ollama_native' && $appSettings.llmBaseUrl !== lastCommittedLocalBaseUrl) {
    localBaseUrlDraft = $appSettings.llmBaseUrl;
    lastCommittedLocalBaseUrl = $appSettings.llmBaseUrl;
  }
  $: if (aiAssistanceValue !== 'ollama_native') {
    localBaseUrlDraft = $appSettings.llmBaseUrl;
    lastCommittedLocalBaseUrl = $appSettings.llmBaseUrl;
  }
  $: localBaseUrlChanged = localBaseUrlDraft.trim() !== $appSettings.llmBaseUrl.trim();

  function acceptLocalBaseUrl() {
    if (visionModelsBusy || !localBaseUrlChanged) {
      return;
    }
    lastCommittedLocalBaseUrl = localBaseUrlDraft;
    onLlmBaseUrlInput?.(localBaseUrlDraft);
  }
</script>

<div class="grid gap-4 px-2 pb-2 pt-3">
  <div class="grid gap-3">
    <RadioCardGroup
      legend="AI assistance mode"
      options={aiAssistanceOptions}
      expandedValues={aiAssistanceExpandedValues}
      value={aiAssistanceValue}
      onChange={(value) => {
        if (value === 'manual') {
          onDisableAiAssist?.();
        } else if (value === 'ollama_native' || value === 'ollama_cloud') {
          onEnableAiAssist?.(value);
        }
      }}
    >
      <svelte:fragment slot="selected" let:optionValue>
        {#if optionValue === 'ollama_native'}
          <div class="grid gap-4 md:grid-cols-2">
            {#if localBaseUrlChanged}
              <TextField
                label="Base URL"
                value={localBaseUrlDraft}
                placeholder={OLLAMA_LOCAL_BASE_URL}
                disabled={visionModelsBusy}
                oninput={(event: Event) => {
                  localBaseUrlDraft = (event.currentTarget as HTMLInputElement).value;
                }}
                onkeydown={(event: KeyboardEvent) => {
                  if (event.key === 'Enter') {
                    acceptLocalBaseUrl();
                  }
                }}
              >
                <button
                  slot="trailing"
                  type="button"
                  class={compactTabActionButtonClass}
                  disabled={visionModelsBusy}
                  onclick={acceptLocalBaseUrl}
                >
                  Accept
                </button>
              </TextField>
            {:else}
              <TextField
                label="Base URL"
                value={localBaseUrlDraft}
                placeholder={OLLAMA_LOCAL_BASE_URL}
                disabled={visionModelsBusy}
                oninput={(event: Event) => {
                  localBaseUrlDraft = (event.currentTarget as HTMLInputElement).value;
                }}
              />
            {/if}
            <SelectField
              label="Vision model"
              value={$appSettings.llmModel}
              options={localVisionModelOptions}
              searchable
              searchPlaceholder="Search models"
              disabled={visionModelsBusy || localModelOptions.length === 0}
              onChange={(value) => onLlmModelInput?.(value)}
            />
          </div>
        {:else if optionValue === 'ollama_cloud'}
          <div class="grid gap-4 md:grid-cols-2">
            <TextField
              label="Base URL"
              value={$appSettings.llmBaseUrl}
              placeholder={OLLAMA_CLOUD_BASE_URL}
              oninput={(event: Event) => onLlmBaseUrlInput?.((event.currentTarget as HTMLInputElement).value)}
            />
            <TextField
              label="Vision model"
              value={$appSettings.llmModel}
              placeholder={OLLAMA_CLOUD_DEFAULT_MODEL}
              oninput={(event: Event) => onLlmModelInput?.((event.currentTarget as HTMLInputElement).value)}
            />
          </div>

          <div class="mt-4 grid gap-1.5 text-sm text-muted-foreground">
            <div class="flex flex-wrap items-center gap-2">
              <label for="ollamaCloudApiKey">Ollama Cloud API key</label>
              <TokenGuidePopover
                bind:open={ollamaTokenHelpOpen}
                imageSrc="/ollama-api-token-guide.png"
                imageAlt="How to get your Ollama API token"
              />
            </div>
            <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:gap-3">
              <TextField
                class="min-w-0 flex-1"
                id="ollamaCloudApiKey"
                density="large"
                type={ollamaApiKeyVisible ? 'text' : 'password'}
                autocomplete="off"
                value={$appSettings.llmApiKey ?? ''}
                placeholder="Paste API key"
                oninput={(event: Event) => {
                  const value = (event.currentTarget as HTMLInputElement).value.trim();
                  onLlmApiKeyInput?.(value.length > 0 ? value : null);
                }}
              />
              <DesktopButton
                class="min-w-[5rem] shrink-0 px-4 text-sm"
                variant="secondary"
                aria-pressed={ollamaApiKeyVisible}
                aria-label={ollamaApiKeyVisible ? 'Hide Ollama Cloud API key' : 'Show Ollama Cloud API key'}
                onclick={() => {
                  ollamaApiKeyVisible = !ollamaApiKeyVisible;
                }}
              >
                {ollamaApiKeyVisible ? 'Hide' : 'Show'}
              </DesktopButton>
            </div>
          </div>
        {/if}
      </svelte:fragment>
    </RadioCardGroup>
  </div>

  <div class="flex flex-wrap items-center justify-end gap-3">
    <ConnectionTicker text={message?.text ?? null} tone={message?.tone ?? 'info'} sticky={message?.sticky ?? false} />
    <DesktopButton size="large" disabled={!ready} onclick={() => onContinue?.()}>
      Continue
    </DesktopButton>
  </div>
</div>
