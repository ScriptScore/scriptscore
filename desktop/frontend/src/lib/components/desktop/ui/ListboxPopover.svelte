<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import DesktopPopover from './DesktopPopover.svelte';

  interface ListboxOption {
    value: string;
    label: string;
    disabled?: boolean;
  }

  export let open = false;
  export let options: ListboxOption[] = [];
  export let value: string | null = null;
  export let triggerLabel = 'Select option';
  export let ariaLabel = 'Options';
  export let onOpenChange: ((open: boolean) => void) | null = null;
  export let onSelect: ((value: string) => void) | null = null;
  export let triggerClass =
    'inline-flex h-9 items-center justify-center rounded-xl border border-border-default bg-surface-card-control px-3 text-sm text-text-primary hover:bg-surface-card-hover';

  let currentOpen = open;
  let lastOpenProp = open;

  $: syncOpenProp(open);

  function syncOpenProp(next: boolean) {
    if (next === lastOpenProp) {
      return;
    }
    lastOpenProp = next;
    currentOpen = next;
  }

  function setOpen(next: boolean) {
    currentOpen = next;
    onOpenChange?.(next);
  }

  function select(option: ListboxOption) {
    if (option.disabled) {
      return;
    }
    onSelect?.(option.value);
    setOpen(false);
  }
</script>

<DesktopPopover
  bind:open={currentOpen}
  {triggerLabel}
  {triggerClass}
  onOpenChange={(next) => {
    setOpen(next);
  }}
>
  <div role="listbox" aria-label={ariaLabel} class="grid gap-1">
    {#each options as option (option.value)}
      <button
        type="button"
        role="option"
        aria-selected={option.value === value}
        disabled={option.disabled}
        data-popover-close
        class={[
          'rounded-xl px-3 py-2 text-left text-sm transition-colors disabled:cursor-not-allowed disabled:opacity-50',
          option.value === value
            ? 'bg-interaction-selected text-interaction-selected-text'
            : 'text-text-primary hover:bg-interaction-hover'
        ]}
        onclick={() => select(option)}
      >
        {option.label}
      </button>
    {/each}
  </div>
</DesktopPopover>
