<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { LaptopIcon, SchoolIcon } from '@hugeicons/core-free-icons';

  import { appSettings } from '$lib/stores/appSettings';
  import ConnectionTicker from './ConnectionTicker.svelte';
  import TokenGuidePopover from './TokenGuidePopover.svelte';
  import { DesktopButton, RadioCardGroup, SelectField, TextField } from './ui';

  type ConnectionTickerTone = 'warning' | 'info' | 'success' | 'error';

  export let ready = false;
  export let message: { text: string; tone: ConnectionTickerTone; sticky: boolean } | null = null;
  export let onChooseLocal: (() => void) | null = null;
  export let onChooseCanvas: (() => void) | null = null;
  export let onCanvasBaseUrlInput: ((value: string) => void) | null = null;
  export let onCanvasTokenInput: ((value: string | null) => void) | null = null;
  export let onContinue: (() => void) | null = null;

  const projectModeOptions = [
    {
      value: 'none',
      title: 'Local',
      description: 'Projects stay on this computer with no LMS connection.',
      icon: LaptopIcon
    },
    {
      value: 'canvas',
      title: 'LMS-linked',
      description: 'Connect to Canvas to import courses and upload results.',
      icon: SchoolIcon
    }
  ];
  const lmsProviderOptions = [{ value: 'canvas', label: 'Canvas' }];
  const projectModeExpandedValues = ['canvas'];

  let canvasTokenVisible = false;
  let canvasTokenHelpOpen = false;

  $: projectModeValue = $appSettings.lmsProvider === 'none' ? 'none' : 'canvas';
</script>

<div class="grid gap-4 px-2 pb-2 pt-3">
  <div class="grid gap-3">
    <RadioCardGroup
      legend="Project mode"
      options={projectModeOptions}
      expandedValues={projectModeExpandedValues}
      value={projectModeValue}
      onChange={(value) => {
        if (value === 'none') {
          onChooseLocal?.();
        } else {
          onChooseCanvas?.();
        }
      }}
    >
      <svelte:fragment slot="selected" let:optionValue>
        {#if optionValue === 'canvas'}
          <div class="grid gap-3 md:grid-cols-2">
            <SelectField
              label="Provider"
              value="canvas"
              options={lmsProviderOptions}
              onChange={() => onChooseCanvas?.()}
            />
            <TextField
              label="Base URL"
              density="large"
              value={$appSettings.lmsCanvasBaseUrl}
              placeholder="https://school.instructure.com"
              oninput={(event: Event) => {
                onCanvasBaseUrlInput?.((event.currentTarget as HTMLInputElement).value);
              }}
            />
            <div class="grid gap-1.5 text-sm text-muted-foreground md:col-span-2">
              <div class="flex flex-wrap items-center gap-2">
                <label for="canvasApiKey">Canvas API key</label>
                <TokenGuidePopover
                  bind:open={canvasTokenHelpOpen}
                  imageSrc="/canvas-api-token-guide.png"
                  imageAlt="How to get your Canvas API token"
                />
              </div>
              <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:gap-3">
                <TextField
                  class="min-w-0 flex-1"
                  id="canvasApiKey"
                  density="large"
                  type={canvasTokenVisible ? 'text' : 'password'}
                  autocomplete="off"
                  value={$appSettings.lmsCanvasApiKey ?? ''}
                  placeholder="Paste access token"
                  oninput={(event: Event) => {
                    const value = (event.currentTarget as HTMLInputElement).value.trim();
                    onCanvasTokenInput?.(value.length > 0 ? value : null);
                  }}
                />
                <DesktopButton
                  class="min-w-[5rem] shrink-0 px-4 text-sm"
                  variant="secondary"
                  aria-pressed={canvasTokenVisible}
                  aria-label={canvasTokenVisible ? 'Hide Canvas API key' : 'Show Canvas API key'}
                  onclick={() => {
                    canvasTokenVisible = !canvasTokenVisible;
                  }}
                >
                  {canvasTokenVisible ? 'Hide' : 'Show'}
                </DesktopButton>
              </div>
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
