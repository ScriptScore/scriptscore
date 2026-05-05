<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  export let tabs: Array<{ value: string; label: string }> = [];
  export let value = '';
  export let ariaLabel: string;
  export let onChange: ((value: string) => void | Promise<void>) | null = null;

  let className = '';
  export { className as class };
</script>

<div class={['flex items-center gap-2 border-b border-border-default', className]} role="tablist" aria-label={ariaLabel}>
  {#each tabs as tab (tab.value)}
    <button
      type="button"
      role="tab"
      aria-selected={value === tab.value}
      class={[
        'border-b-2 px-3 py-2 text-sm transition-colors',
        value === tab.value
          ? 'border-text-primary text-text-primary'
          : 'border-transparent text-text-secondary hover:text-text-primary'
      ]}
      onclick={() => {
        value = tab.value;
        void onChange?.(tab.value);
      }}
    >
      {tab.label}
    </button>
  {/each}
</div>
