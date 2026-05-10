<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { onDestroy } from 'svelte';

  import type { AppSettings, VisionCapableModel } from '$lib/types';
  import ConnectionTicker from './ConnectionTicker.svelte';
  import { createOllamaOnboardingValidator } from './onboardingValidation';
  import { InlineMessage, SelectField, TextField, compactTabActionButtonClass } from './ui';

  export let settings: AppSettings;
  export let visionModels: VisionCapableModel[] = [];
  export let visionModelsBusy = false;
  export let visionModelsError: string | null = null;
  export let framed = false;
  export let hasDesktopHost = true;
  export let onChange: (() => void | Promise<void>) | null = null;

  const OLLAMA_NATIVE_DEFAULT_BASE_URL = 'http://127.0.0.1:11434';
  const OLLAMA_NATIVE_DEFAULT_MODEL = 'qwen3.5:9b';
  const OLLAMA_CLOUD_DEFAULT_BASE_URL = 'https://ollama.com/api';
  const OLLAMA_CLOUD_DEFAULT_MODEL = 'qwen3.5:cloud';
  const ollamaCloudValidator = createOllamaOnboardingValidator(() => hasDesktopHost);
  const llmProviderOptions = [
    { value: 'ollama_native', label: 'ollama_native' },
    { value: 'ollama_cloud', label: 'ollama_cloud' }
  ];

  let localBaseUrlDraft = '';
  let lastCommittedLocalBaseUrl = '';

  $: modelOptions =
    settings.llmModel && !visionModels.some((model) => model.name === settings.llmModel)
      ? [{ name: settings.llmModel, displayName: `${settings.llmModel} (current)` }, ...visionModels]
      : visionModels;
  $: visionModelOptions =
    modelOptions.length > 0
      ? modelOptions.map((model) => ({ value: model.name, label: model.displayName }))
      : [
          {
            value: settings.llmModel,
            label: visionModelsBusy ? 'Loading local models...' : 'No vision models found'
          }
        ];
  $: if (settings.llmProvider === 'ollama_native' && settings.llmBaseUrl !== lastCommittedLocalBaseUrl) {
    localBaseUrlDraft = settings.llmBaseUrl;
    lastCommittedLocalBaseUrl = settings.llmBaseUrl;
  }
  $: if (settings.llmProvider !== 'ollama_native') {
    localBaseUrlDraft = settings.llmBaseUrl;
    lastCommittedLocalBaseUrl = settings.llmBaseUrl;
  }
  $: localBaseUrlChanged = localBaseUrlDraft.trim() !== settings.llmBaseUrl.trim();

  $: {
    ollamaCloudValidator.handle(
      settings.llmProvider === 'ollama_cloud',
      settings.llmProvider,
      settings.llmBaseUrl.trim(),
      settings.llmModel.trim(),
      (settings.llmApiKey ?? '').trim()
    );
  }

  onDestroy(() => {
    ollamaCloudValidator.destroy();
  });

  function handleLlmProviderChange(provider: string) {
    settings.llmProvider = provider as AppSettings['llmProvider'];
    if (provider === 'ollama_cloud') {
      settings.llmBaseUrl = OLLAMA_CLOUD_DEFAULT_BASE_URL;
      settings.llmModel = OLLAMA_CLOUD_DEFAULT_MODEL;
    } else if (provider === 'ollama_native') {
      settings.llmBaseUrl = OLLAMA_NATIVE_DEFAULT_BASE_URL;
      settings.llmModel = OLLAMA_NATIVE_DEFAULT_MODEL;
      settings.llmApiKey = null;
    }
    void onChange?.();
  }

  function acceptLocalBaseUrl() {
    if (visionModelsBusy || !localBaseUrlChanged) {
      return;
    }
    settings.llmBaseUrl = localBaseUrlDraft;
    lastCommittedLocalBaseUrl = localBaseUrlDraft;
    void onChange?.();
  }
</script>

<section class={framed ? 'rounded-2xl bg-surface-card-subtle p-4' : 'py-1'}>
  <div>
    <div class="shell-eyebrow text-workspace-text-muted">LLM provider</div>
    <div class="mt-1 shell-body text-workspace-text-secondary">
      Configure the Ollama endpoint and vision-capable model used by automated workflow options.
    </div>
  </div>

  <div class="mt-4 grid gap-3 md:grid-cols-2">
    <SelectField
      class="md:col-span-2"
      label="Provider"
      value={settings.llmProvider}
      options={llmProviderOptions}
      onChange={handleLlmProviderChange}
    />

    {#if settings.llmProvider === 'ollama_native'}
      {#if localBaseUrlChanged}
        <TextField
          label="Base URL"
          value={localBaseUrlDraft}
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
          disabled={visionModelsBusy}
          oninput={(event: Event) => {
            localBaseUrlDraft = (event.currentTarget as HTMLInputElement).value;
          }}
        />
      {/if}
    {:else}
      <TextField
        label="Base URL"
        value={settings.llmBaseUrl}
        oninput={(event: Event) => {
          settings.llmBaseUrl = (event.currentTarget as HTMLInputElement).value;
          void onChange?.();
        }}
      />
    {/if}

    {#if settings.llmProvider === 'ollama_native'}
      <SelectField
        label="Vision model"
        value={settings.llmModel}
        options={visionModelOptions}
        searchable
        searchPlaceholder="Search models"
        disabled={visionModelsBusy || modelOptions.length === 0}
        onChange={(value) => {
          settings.llmModel = value;
          void onChange?.();
        }}
      />
    {:else}
      <TextField
        label="Vision model"
        value={settings.llmModel}
        oninput={(event: Event) => {
          settings.llmModel = (event.currentTarget as HTMLInputElement).value;
          void onChange?.();
        }}
      />
    {/if}

    <TextField
      class="md:col-span-2"
      label="API key"
      value={settings.llmApiKey ?? ''}
      oninput={(event: Event) => {
        const value = (event.currentTarget as HTMLInputElement).value;
        settings.llmApiKey = value.length > 0 ? value : null;
        void onChange?.();
      }}
      placeholder={settings.llmProvider === 'ollama_cloud' ? 'Required for Ollama Cloud' : 'Optional for local providers'}
    />
    {#if settings.llmProvider === 'ollama_cloud'}
      <div class="flex justify-end md:col-span-2">
        <ConnectionTicker
          text={$ollamaCloudValidator.displayedMessage?.text ?? null}
          tone={$ollamaCloudValidator.displayedMessage?.tone ?? 'info'}
          sticky={$ollamaCloudValidator.displayedMessage?.sticky ?? false}
        />
      </div>
    {/if}
  </div>

  {#if visionModelsError}
    <InlineMessage class="mt-3" tone="warning">
      {visionModelsError}
    </InlineMessage>
  {:else if settings.llmProvider === 'ollama_native'}
    <div class="mt-3 shell-meta text-workspace-text-muted">
      {#if visionModelsBusy}
        Querying the local Ollama server for vision-capable models...
      {:else if visionModels.length > 0}
        Loaded {visionModels.length} vision-capable local model{visionModels.length === 1 ? '' : 's'}.
      {:else}
        No vision-capable models are loaded yet for the current endpoint.
      {/if}
    </div>
  {/if}
</section>
