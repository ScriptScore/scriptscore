<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { fly } from 'svelte/transition';

  import { notifications } from '$lib/stores/notifications';
  import { feedbackRole, feedbackToneClass } from './ui';

  export let placement: 'viewport' | 'topbar' = 'viewport';

  $: isTopbar = placement === 'topbar';
  $: viewportShellClass = isTopbar
    ? 'pointer-events-none flex min-w-0 flex-1 justify-end'
    : 'pointer-events-none absolute inset-x-0 bottom-5 z-30 px-6';
  $: viewportInnerClass = isTopbar
    ? 'flex min-w-0 max-w-full justify-end'
    : 'mx-auto flex max-w-5xl justify-end';
  $: stackClass = isTopbar
    ? 'pointer-events-auto grid min-w-0 max-w-full justify-items-end overflow-hidden'
    : 'pointer-events-auto grid justify-items-end overflow-hidden';
  $: toastBaseClass = isTopbar
    ? 'col-start-1 row-start-1 flex h-9 max-w-full min-w-0 items-center gap-2 rounded-full border px-3 text-xs shadow-sm'
    : 'col-start-1 row-start-1 flex min-w-64 items-center gap-3 rounded-2xl border px-4 py-3 text-sm shadow-lg backdrop-blur-sm';
  $: messageClass = isTopbar ? 'min-w-0 flex-1 truncate' : 'flex-1';
</script>

{#if $notifications.length > 0}
  <div class={viewportShellClass}>
    <div class={viewportInnerClass}>
      <div class={stackClass}>
        {#each $notifications as toast (toast.id)}
          <div
            class={[toastBaseClass, feedbackToneClass(toast.kind)]}
            in:fly={{ y: 12, duration: 320, delay: 160 }}
            out:fly={{ y: -12, duration: 320 }}
            role={feedbackRole(toast.kind)}
            aria-live="polite"
          >
            <span class={messageClass}>{toast.message}</span>
            <button
              class="shrink-0 rounded-full p-1 opacity-60 transition-opacity hover:opacity-100"
              type="button"
              aria-label="Dismiss notification"
              onclick={() => notifications.dismiss(toast.id)}
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>
            </button>
          </div>
        {/each}
      </div>
    </div>
  </div>
{/if}
