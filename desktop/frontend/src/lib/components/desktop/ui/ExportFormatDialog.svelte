<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import type { ResultsExportFormat } from '$lib/types';
  import DesktopButton from './DesktopButton.svelte';

  export let open = false;
  export let busy = false;
  export let onCancel: (() => void) | null = null;
  export let onChoose: ((format: ResultsExportFormat) => void | Promise<void>) | null = null;
</script>

{#if open}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-overlay-scrim px-4">
    <div
      class="w-full max-w-sm rounded-2xl border border-border-default bg-surface-canvas p-5 shadow-[var(--surface-shadow-strong)]"
      role="dialog"
      aria-modal="true"
      aria-labelledby="export-format-dialog-title"
      aria-describedby="export-format-dialog-description"
    >
      <div id="export-format-dialog-title" class="text-sm font-semibold text-text-primary">
        Export results
      </div>
      <p id="export-format-dialog-description" class="mt-2 text-sm leading-6 text-text-secondary">
        Choose an export format for the selected ready rows.
      </p>
      <div class="mt-5 flex justify-end gap-2">
        <DesktopButton size="compact" disabled={busy} onclick={() => onCancel?.()}>
          Cancel
        </DesktopButton>
        <DesktopButton
          size="compact"
          disabled={busy}
          onclick={() => void onChoose?.('csv')}
        >
          CSV
        </DesktopButton>
        <DesktopButton
          size="compact"
          variant="primary"
          disabled={busy}
          onclick={() => void onChoose?.('html_zip')}
        >
          HTML
        </DesktopButton>
      </div>
    </div>
  </div>
{/if}
