<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  export let options: Array<{ value: string; label: string }> = [];
  export let value = '';
  export let ariaLabel: string;
  export let onChange: ((value: string) => void | Promise<void>) | null = null;

  let className = '';
  export { className as class };
</script>

<div
  class={['inline-flex items-center rounded-full bg-surface-card-control p-1', className]}
  role="group"
  aria-label={ariaLabel}
>
  {#each options as option (option.value)}
    <button
      type="button"
      class={[
        'min-w-0 flex-1 rounded-full px-2 py-1.5 text-[12px] font-semibold transition-colors',
        value === option.value
          ? 'bg-interaction-selected text-interaction-selected-text'
          : 'text-text-secondary hover:text-text-primary'
      ]}
      aria-pressed={value === option.value}
      onclick={() => {
        value = option.value;
        void onChange?.(option.value);
      }}
    >
      {option.label}
    </button>
  {/each}
</div>
