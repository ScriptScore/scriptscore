<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import DesktopButton from './DesktopButton.svelte';

  export let open = false;
  export let title = 'Confirm action';
  export let description = '';
  export let confirmLabel = 'Confirm';
  export let cancelLabel: string | null = 'Cancel';
  export let destructive = false;
  export let busy = false;
  export let onCancel: (() => void) | null = null;
  export let onConfirm: (() => void | Promise<void>) | null = null;
</script>

{#if open}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-overlay-scrim px-4">
    <div
      class="w-full max-w-sm rounded-2xl border border-border-default bg-surface-canvas p-5 shadow-[var(--surface-shadow-strong)]"
      role="dialog"
      aria-modal="true"
      aria-labelledby="confirm-dialog-title"
      aria-describedby="confirm-dialog-description"
    >
      <div id="confirm-dialog-title" class="text-sm font-semibold text-text-primary">{title}</div>
      <p id="confirm-dialog-description" class="mt-2 text-sm leading-6 text-text-secondary">
        {description}
      </p>
      <div class="mt-5 flex justify-end gap-2">
        {#if cancelLabel}
          <DesktopButton size="compact" onclick={() => onCancel?.()}>{cancelLabel}</DesktopButton>
        {/if}
        <DesktopButton
          size="compact"
          variant={destructive ? 'destructive' : 'primary'}
          disabled={busy}
          onclick={() => void onConfirm?.()}
        >
          {confirmLabel}
        </DesktopButton>
      </div>
    </div>
  </div>
{/if}
