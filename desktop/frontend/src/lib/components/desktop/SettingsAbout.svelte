<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import type { AppUpdateCheck } from '$lib/types';
  import { DesktopButton } from './ui';

  export let appVersion: string | null = null;
  export let updateCheck: AppUpdateCheck | null = null;
  export let updateBusy = false;
  export let hasDesktopHost = true;
  export let onCheckForUpdates: (() => void | Promise<void>) | null = null;
  export let onDownloadUpdate: ((url: string) => void | Promise<void>) | null = null;

  $: installedVersion = appVersion ?? updateCheck?.installedVersion ?? 'Browser preview';
  $: releaseUrl = updateCheck?.releaseUrl ?? null;
  $: canDownload = hasDesktopHost && Boolean(updateCheck?.updateAvailable && releaseUrl);
</script>

<section class="flex flex-wrap items-center justify-between gap-4">
  <div class="flex min-w-0 items-center gap-4">
    <img
      class="h-14 w-14 shrink-0 rounded-xl"
      src="/scriptscore-app-icon.png"
      alt=""
      aria-hidden="true"
    />
    <div class="min-w-0">
      <div class="shell-title text-workspace-text-primary">ScriptScore Desktop</div>
      <div class="mt-1 text-sm text-workspace-text-secondary">Version {installedVersion}</div>
    </div>
  </div>

  {#if canDownload && releaseUrl}
    <DesktopButton
      size="compact"
      variant="primary"
      onclick={() => void onDownloadUpdate?.(releaseUrl)}
    >
      Download update
    </DesktopButton>
  {:else}
    <DesktopButton
      size="compact"
      disabled={!hasDesktopHost || updateBusy}
      onclick={() => void onCheckForUpdates?.()}
    >
      {updateBusy ? 'Checking...' : 'Check for updates'}
    </DesktopButton>
  {/if}
</section>
