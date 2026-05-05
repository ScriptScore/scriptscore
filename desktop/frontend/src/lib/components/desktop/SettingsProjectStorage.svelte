<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import type { AppSettings } from '$lib/types';
  import { DesktopButton, TextField } from './ui';

  export let settings: AppSettings;
  export let busy = false;
  export let resolvedProjectsDirectory: string | null = null;
  export let onChooseProjectsDirectory: (() => void | Promise<void>) | null = null;
  export let onClearProjectsDirectory: (() => void) | null = null;
  export let onChoosePiiPaddleModelDirectory: (() => void | Promise<void>) | null = null;
  export let onClearPiiPaddleModelDirectory: (() => void) | null = null;
  export let onChange: (() => void | Promise<void>) | null = null;

</script>

<section class="py-1">
  <div>
    <div>
      <div class="shell-eyebrow text-workspace-text-muted">Project storage</div>
      <div class="mt-1 shell-body text-workspace-text-secondary">
        Choose where newly created ScriptScore projects are stored on this machine.
      </div>
    </div>
  </div>
  <div class="mt-4 grid gap-3">
    <div class="grid gap-2 rounded-xl bg-workspace-empty px-3 py-3 text-sm text-workspace-text-secondary">
      <div>
        <span class="font-medium text-workspace-text-primary">Folder setting:</span>
        <span>{settings.projectsDirectory ? 'Custom folder' : 'System default'}</span>
      </div>
      <div>
        <span class="font-medium text-workspace-text-primary">Current folder:</span>
        <span class="break-all">
          {resolvedProjectsDirectory ?? settings.projectsDirectory ?? 'System default folder not resolved yet'}
        </span>
      </div>
    </div>
    <div class="flex flex-wrap gap-3">
      <DesktopButton
        disabled={busy}
        onclick={() => void onChooseProjectsDirectory?.()}
      >
        Choose folder
      </DesktopButton>
      <DesktopButton
        disabled={busy || !settings.projectsDirectory}
        onclick={() => onClearProjectsDirectory?.()}
      >
        Use system default
      </DesktopButton>
    </div>
  </div>
  <div class="mt-8 border-t border-workspace-border pt-5">
    <div class="grid gap-3">
      <TextField
        label="PII Paddle model directory"
        type="text"
        value={settings.piiPaddleModelDir ?? ''}
        placeholder="Optional absolute path; otherwise use bundled or dev fallback"
        hint="Optional override for the local Paddle OCR models used by `scans.pii`. The desktop resolves this from env var, this setting, bundled app resources, then the repo checkout fallback during development."
        oninput={(event: Event) => {
          const value = (event.currentTarget as HTMLInputElement).value.trim();
          settings.piiPaddleModelDir = value.length > 0 ? value : null;
          void onChange?.();
        }}
      />
      <div class="flex flex-wrap gap-3">
        <DesktopButton
          disabled={busy}
          onclick={() => void onChoosePiiPaddleModelDirectory?.()}
        >
          Choose folder
        </DesktopButton>
        <DesktopButton
          disabled={busy || !settings.piiPaddleModelDir}
          onclick={() => onClearPiiPaddleModelDirectory?.()}
        >
          Clear model folder
        </DesktopButton>
      </div>
    </div>
  </div>
</section>
