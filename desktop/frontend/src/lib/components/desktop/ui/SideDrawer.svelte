<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { onDestroy } from 'svelte';
  import DesktopButton from './DesktopButton.svelte';

  export let open = false;
  export let title = 'Panel';
  export let description: string | null = null;
  export let onClose: (() => void) | null = null;
  export let overlay = false;
  export let labelledBy = 'side-drawer-title';
  export let describedBy = 'side-drawer-description';

  let className = '';
  export { className as class };

  $: if (typeof document !== 'undefined') {
    if (open) {
      document.addEventListener('keydown', handleDocumentKeyDown, true);
    } else {
      document.removeEventListener('keydown', handleDocumentKeyDown, true);
    }
  }

  onDestroy(() => {
    if (typeof document !== 'undefined') {
      document.removeEventListener('keydown', handleDocumentKeyDown, true);
    }
  });

  function close() {
    onClose?.();
  }

  function handleDocumentKeyDown(event: KeyboardEvent) {
    if (open && event.key === 'Escape') {
      event.preventDefault();
      close();
    }
  }
</script>

{#if open}
  {#if overlay}
    <button
      type="button"
      class="fixed inset-0 z-30 cursor-default bg-overlay-scrim"
      aria-label="Close panel"
      onclick={close}
    ></button>
  {/if}
  <div
    class={[
      overlay ? 'fixed' : 'absolute',
      'inset-y-0 right-0 z-40 flex w-[32rem] max-w-full flex-col border-l border-border-default bg-surface-sidebar shadow-[var(--surface-shadow-strong)]',
      className
    ]}
    role="dialog"
    aria-modal={overlay}
    aria-labelledby={labelledBy}
    aria-describedby={description ? describedBy : undefined}
  >
    <div class="flex items-center justify-between gap-4 border-b border-border-default px-5 py-4">
      <div>
        <div id={labelledBy} class="text-xs font-semibold uppercase tracking-[0.2em] text-text-muted">
          {title}
        </div>
        {#if description}
          <div id={describedBy} class="mt-1 text-sm text-text-secondary">{description}</div>
        {/if}
      </div>
      <DesktopButton size="compact" onclick={close}>Close</DesktopButton>
    </div>
    <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
      <slot />
    </div>
  </div>
{/if}
