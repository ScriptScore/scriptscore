<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  export let active = false;
  export let type: 'button' | 'submit' | 'reset' = 'button';
  export let disabled = false;

  let className = '';
  export { className as class };
</script>

<button
  {...$$restProps}
  class={[
    'flex min-h-12 w-full items-center gap-3 rounded-xl border px-4 text-left text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring disabled:cursor-not-allowed disabled:opacity-50',
    active
      ? 'border-border-strong bg-workspace-sidebar-active bg-interaction-active text-workspace-sidebar-foreground shadow-[0_0_0_1px_var(--border-strong)]'
      : 'border-border-subtle bg-transparent text-workspace-sidebar-muted hover:bg-interaction-hover hover:text-workspace-sidebar-foreground',
    className
  ]}
  {type}
  {disabled}
>
  <slot />
</button>
