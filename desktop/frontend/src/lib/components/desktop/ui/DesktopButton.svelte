<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  export let variant: 'primary' | 'secondary' | 'ghost' | 'subtle' | 'destructive' = 'secondary';
  export let size: 'compact' | 'default' | 'large' = 'default';
  export let type: 'button' | 'submit' | 'reset' = 'button';
  export let busy = false;
  export let disabled = false;
  export let selected = false;
  export let fullWidth = false;

  let className = '';
  export { className as class };

  $: sizeClass =
    size === 'compact'
      ? 'h-9 px-3 text-sm'
      : size === 'large'
        ? 'min-h-12 px-5 py-3 text-base'
        : 'h-10 px-3 text-sm';

  $: variantClass =
    variant === 'primary'
      ? 'border-border-default bg-interaction-active text-text-primary hover:bg-interaction-hover'
      : variant === 'ghost'
        ? 'border-transparent bg-transparent text-text-secondary hover:bg-interaction-hover hover:text-text-primary'
        : variant === 'subtle'
          ? 'border-border-default bg-surface-message text-text-primary hover:bg-surface-card-hover'
          : variant === 'destructive'
            ? 'border-destructive bg-destructive text-primary-foreground hover:opacity-90'
            : 'border-border-default bg-surface-card-control text-text-primary hover:bg-interaction-hover';

  $: selectedClass = selected
    ? 'shadow-[0_0_0_1px_var(--border-strong)]'
    : 'shadow-[var(--surface-shadow-button)]';
</script>

<button
  {...$$restProps}
  class={[
    'inline-flex items-center justify-center gap-2 rounded-xl border font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring disabled:cursor-not-allowed disabled:opacity-50',
    sizeClass,
    variantClass,
    selectedClass,
    fullWidth ? 'w-full' : '',
    className
  ]}
  {type}
  disabled={disabled || busy}
>
  <slot />
</button>
