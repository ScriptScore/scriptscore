<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  export let alt = '';
  export let src: string | null = null;
  export let squareCorners = false;
  export let imageClass = 'block h-auto w-full select-none';

  let className = '';
  export { className as class };
</script>

<div
  class={[
    'overflow-hidden border border-[var(--workspace-image-border)] bg-surface-page shadow-[var(--surface-shadow-canvas)]',
    squareCorners ? '' : 'rounded-2xl',
    className
  ]}
>
  {#if src}
    <img class={imageClass} {src} {alt} />
  {:else}
    <slot />
  {/if}
</div>
