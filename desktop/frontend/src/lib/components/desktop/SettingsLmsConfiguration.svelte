<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { onDestroy } from 'svelte';

  import { createCanvasOnboardingValidator } from './onboardingValidation';
  import type { AppSettings } from '$lib/types';
  import ConnectionTicker from './ConnectionTicker.svelte';
  import { DesktopButton, InlineMessage, SelectField, TextField, ToggleRow } from './ui';

  export let settings: AppSettings;
  export let framed = false;
  export let hasDesktopHost = true;
  export let onChange: (() => void | Promise<void>) | null = null;

  let canvasTokenVisible = false;
  const canvasValidator = createCanvasOnboardingValidator(() => hasDesktopHost);
  const lmsProviderOptions = [
    { value: 'none', label: 'None' },
    { value: 'canvas', label: 'Canvas' }
  ];

  $: {
    canvasValidator.handle(
      settings.lmsProvider,
      settings.lmsCanvasBaseUrl.trim(),
      (settings.lmsCanvasApiKey ?? '').trim()
    );
  }

  onDestroy(() => {
    canvasValidator.destroy();
  });
</script>

<section class={framed ? 'rounded-2xl bg-surface-card-subtle p-4' : 'py-1'}>
  <div>
    <div class="shell-eyebrow text-workspace-text-muted">LMS configuration</div>
    <div class="mt-1 shell-body text-workspace-text-secondary">
      Connect Canvas to pick courses when creating a project. Use a personal access token with appropriate course scopes.
    </div>
  </div>
  <div class="mt-4 grid gap-3 md:grid-cols-2">
    <SelectField
      label="Provider"
      value={settings.lmsProvider}
      options={lmsProviderOptions}
      onChange={(value) => {
        settings.lmsProvider = value as AppSettings['lmsProvider'];
        void onChange?.();
      }}
    />
    {#if settings.lmsProvider === 'canvas'}
      <TextField
        label="Base URL"
        value={settings.lmsCanvasBaseUrl}
        oninput={(event: Event) => {
          settings.lmsCanvasBaseUrl = (event.currentTarget as HTMLInputElement).value;
          void onChange?.();
        }}
        placeholder="https://school.instructure.com"
      />
      <div class="grid gap-2 md:col-span-2">
        <div class="flex flex-col gap-2 sm:flex-row sm:items-end sm:gap-3">
          <TextField
            class="min-w-0 flex-1"
            label="API access token"
            type={canvasTokenVisible ? 'text' : 'password'}
            autocomplete="off"
            value={settings.lmsCanvasApiKey ?? ''}
            oninput={(event: Event) => {
              const value = (event.currentTarget as HTMLInputElement).value;
              settings.lmsCanvasApiKey = value.length > 0 ? value : null;
              void onChange?.();
            }}
            placeholder="Personal access token"
          />
          <DesktopButton
            class="shrink-0 sm:mb-0"
            variant="secondary"
            aria-pressed={canvasTokenVisible}
            aria-label={canvasTokenVisible ? 'Hide API access token' : 'Show API access token'}
            onclick={() => {
              canvasTokenVisible = !canvasTokenVisible;
            }}
          >
            {canvasTokenVisible ? 'Hide' : 'Show'}
          </DesktopButton>
        </div>
        <div class="flex justify-end">
          <ConnectionTicker
            text={$canvasValidator.displayedMessage?.text ?? null}
            tone={$canvasValidator.displayedMessage?.tone ?? 'info'}
            sticky={$canvasValidator.displayedMessage?.sticky ?? false}
          />
        </div>
      </div>
      <div class="grid gap-3 md:col-span-2">
        <ToggleRow
          title="Allow plaintext fallback for LMS binding secret"
          description="Recommended off. The default path keeps the LMS binding HMAC secret in the OS keyring and only imports an existing fallback file when no keyring value exists."
          checked={settings.lmsBindingSecretPlaintextFallback}
          onToggle={(checked: boolean) => {
            settings.lmsBindingSecretPlaintextFallback = checked;
            void onChange?.();
          }}
        />
        {#if settings.lmsBindingSecretPlaintextFallback}
          <InlineMessage tone="warning">
            <div class="font-medium">Plaintext fallback enabled</div>
            <div class="mt-1 leading-6">
              ScriptScore will mirror the LMS binding HMAC secret into a plaintext hex file under
              the local app config directory so roster matching can recover from broken keyring
              environments. Anyone with filesystem access to this desktop profile can reuse that
              secret to recompute pseudonymous LMS binding tokens. Leave this off unless the OS
              keyring is unavailable or unreliable on this machine.
            </div>
          </InlineMessage>
        {:else}
          <div class="text-xs leading-6 text-workspace-text-muted">
            Keyring-only is active. Existing plaintext fallback files are imported only when no
            keyring secret exists yet.
          </div>
        {/if}
      </div>
    {/if}
  </div>
</section>
