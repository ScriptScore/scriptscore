<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import FieldShell from './FieldShell.svelte';

  export let id: string | undefined = undefined;
  export let label: string | null = null;
  export let hint: string | null = null;
  export let error: string | null = null;
  export let value = '';
  export let density: 'default' | 'large' = 'default';

  let className = '';
  export { className as class };
  export let controlClass = '';

  $: densityClass = density === 'large' ? 'min-h-44 px-5 py-4 text-lg' : 'min-h-24 px-4 py-3 text-sm';
</script>

<FieldShell class={className} {label} forId={id} {hint} {error}>
  <textarea
    {...$$restProps}
    {id}
    bind:value
    class={[
      'w-full resize-y rounded-xl border border-border-default bg-workspace-empty text-text-primary shadow-[var(--surface-shadow-inset)] outline-none transition-colors placeholder:text-text-muted focus:border-border-focus disabled:cursor-not-allowed disabled:opacity-60',
      densityClass,
      controlClass
    ]}
  ></textarea>
</FieldShell>
