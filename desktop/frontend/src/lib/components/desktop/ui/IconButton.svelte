<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  export let ariaLabel: string;
  export let variant: 'toolbar' | 'rail' | 'ghost' | 'danger' = 'toolbar';
  export let size: 'compact' | 'default' | 'rail' = 'default';
  export let type: 'button' | 'submit' | 'reset' = 'button';
  export let disabled = false;
  export let selected = false;
  export let attention = false;

  let className = '';
  export { className as class };

  $: sizeClass =
    size === 'compact' ? 'size-9' : size === 'rail' ? 'size-11 rounded-2xl' : 'size-10';

  $: variantClass =
    variant === 'rail'
      ? selected
        ? 'border-border-strong bg-interaction-active text-workspace-sidebar-foreground hover:bg-interaction-active focus-visible:bg-interaction-active'
        : 'border-border-subtle bg-transparent text-workspace-sidebar-muted hover:border-border-default hover:bg-interaction-hover hover:text-workspace-sidebar-foreground focus-visible:border-border-default focus-visible:bg-interaction-hover focus-visible:text-workspace-sidebar-foreground'
      : variant === 'ghost'
        ? selected
          ? 'border-border-strong bg-interaction-active text-text-primary'
          : 'border-transparent bg-transparent text-text-secondary hover:bg-interaction-hover hover:text-text-primary'
        : variant === 'danger'
          ? 'border-transparent bg-transparent text-text-muted hover:bg-destructive/10 hover:text-destructive'
          : selected
            ? 'border-border-strong bg-interaction-active text-text-primary'
            : 'border-border-default bg-surface-card-control text-text-primary hover:bg-interaction-hover';

  $: attentionClass = attention
    ? 'shadow-[0_0_0_2px_var(--border-strong)] text-workspace-sidebar-foreground'
    : '';
</script>

<button
  {...$$restProps}
  class={[
    'inline-flex shrink-0 items-center justify-center rounded-xl border transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring disabled:cursor-not-allowed disabled:opacity-50',
    sizeClass,
    variantClass,
    attentionClass,
    className
  ]}
  {type}
  disabled={disabled}
  aria-label={ariaLabel}
  title={$$restProps.title ?? ariaLabel}
>
  <slot />
</button>
