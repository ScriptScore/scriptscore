<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  export let squareCorners = false;

  let className = '';
  export { className as class };
</script>

<div
  {...$$restProps}
  class={[
    'relative inline-block max-w-full touch-none select-none bg-surface-page shadow-[inset_0_0_0_1px_var(--workspace-image-border),var(--surface-shadow-canvas)] focus-within:ring-2 focus-within:ring-focus-ring',
    squareCorners ? '' : 'rounded-2xl',
    className
  ]}
>
  <slot />
</div>
