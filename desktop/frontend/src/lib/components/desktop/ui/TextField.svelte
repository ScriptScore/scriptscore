<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import FieldShell from './FieldShell.svelte';

  export let id: string | undefined = undefined;
  export let label: string | null = null;
  export let hint: string | null = null;
  export let error: string | null = null;
  export let value: string | number = '';
  export let type = 'text';
  export let density: 'compact' | 'default' | 'large' = 'default';

  let className = '';
  export { className as class };
  export let controlClass = '';
  export let trailingClass = '';

  $: densityClass =
    density === 'compact'
      ? 'h-9 px-3 text-sm'
      : density === 'large'
        ? 'min-h-12 px-4 py-3 text-base'
        : 'h-10 px-3 text-sm';
  $: hasTrailing = Boolean($$slots.trailing);
</script>

<FieldShell class={className} {label} forId={id} {hint} {error}>
  <div class="relative">
    <input
      {...$$restProps}
      {id}
      bind:value
      {type}
      class={[
        'w-full rounded-xl border border-border-default bg-workspace-empty text-text-primary shadow-[var(--surface-shadow-inset)] outline-none transition-colors placeholder:text-text-muted focus:border-border-focus disabled:cursor-not-allowed disabled:opacity-60',
        densityClass,
        hasTrailing ? 'pr-20' : '',
        controlClass
      ]}
    />
    {#if hasTrailing}
      <div class={['absolute inset-y-0 right-2 flex items-center', trailingClass]}>
        <slot name="trailing" />
      </div>
    {/if}
  </div>
</FieldShell>
