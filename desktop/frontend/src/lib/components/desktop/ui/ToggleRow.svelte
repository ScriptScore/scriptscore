<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  export let checked = false;
  export let disabled = false;
  export let title: string;
  export let description: string | null = null;
  export let onToggle: ((checked: boolean) => void | Promise<void>) | null = null;

  let className = '';
  export { className as class };

  function toggle() {
    if (disabled) {
      return;
    }
    checked = !checked;
    void onToggle?.(checked);
  }
</script>

<button
  {...$$restProps}
  class={[
    'flex w-full items-start justify-between gap-4 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring disabled:cursor-not-allowed disabled:opacity-60',
    className
  ]}
  type="button"
  aria-pressed={checked}
  {disabled}
  onclick={toggle}
>
  <span class="min-w-0">
    <span class="block text-sm font-medium text-text-primary">{title}</span>
    {#if description}
      <span class="mt-1 block text-sm leading-7 text-text-secondary">{description}</span>
    {/if}
    <slot name="detail" />
  </span>
  <span
    class={[
      'relative inline-flex h-6 w-11 shrink-0 items-center rounded-full border border-border-default transition-colors',
      checked ? 'border-toggle-active-border bg-toggle-active-bg' : 'bg-workspace-empty'
    ]}
    aria-hidden="true"
  >
    <span
      class={[
        'pointer-events-none inline-flex size-4 rounded-full shadow-sm transition-transform',
        checked ? 'translate-x-6 bg-toggle-active-text' : 'translate-x-1 bg-text-primary'
      ]}
    ></span>
  </span>
</button>
