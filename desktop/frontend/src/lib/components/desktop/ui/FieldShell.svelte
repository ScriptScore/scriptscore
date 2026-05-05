<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  export let label: string | null = null;
  export let forId: string | undefined = undefined;
  export let hint: string | null = null;
  export let error: string | null = null;

  let className = '';
  export { className as class };
</script>

{#if label && !forId}
  <label class={['grid gap-1.5 text-sm text-text-secondary', className]}>
    <span class="font-medium text-text-primary">{label}</span>
    <slot />
    {#if error}
      <span class="text-sm leading-6 text-message-error-text">{error}</span>
    {:else if hint}
      <span class="text-xs leading-6 text-text-muted">{hint}</span>
    {/if}
  </label>
{:else}
  <div class={['grid gap-1.5 text-sm text-text-secondary', className]}>
    {#if label}
      <label class="font-medium text-text-primary" for={forId}>{label}</label>
    {/if}
    <slot />
    {#if error}
      <div class="text-sm leading-6 text-message-error-text">{error}</div>
    {:else if hint}
      <div class="text-xs leading-6 text-text-muted">{hint}</div>
    {/if}
  </div>
{/if}
