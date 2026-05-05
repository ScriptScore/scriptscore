<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  export let as: 'div' | 'section' | 'aside' | 'main' | 'article' = 'div';
  export let variant:
    | 'app'
    | 'shell'
    | 'rail'
    | 'sidebar'
    | 'panel'
    | 'canvas'
    | 'bottomBar'
    | 'card'
    | 'overlay'
    | 'page'
    | 'message'
    | 'cardSubtle'
    | 'cardControl'
    | 'cardRaised'
    | 'cardHover' = 'panel';
  export let bordered = false;
  export let shadow: 'none' | 'soft' | 'strong' = 'none';
  export let radius: 'none' | 'md' | 'lg' | 'xl' | '2xl' | '3xl' = 'none';

  let className = '';
  export { className as class };

  $: surfaceClass =
    variant === 'app'
      ? 'bg-surface-app'
      : variant === 'shell'
        ? 'bg-surface-shell'
        : variant === 'rail'
          ? 'bg-surface-rail'
          : variant === 'sidebar'
            ? 'bg-surface-sidebar'
            : variant === 'canvas'
              ? 'bg-surface-canvas'
              : variant === 'bottomBar'
                ? 'bg-surface-bottom-bar'
                : variant === 'card'
                  ? 'bg-surface-card'
                  : variant === 'overlay'
                    ? 'bg-surface-overlay'
                    : variant === 'page'
                      ? 'bg-surface-page'
                      : variant === 'message'
                        ? 'bg-surface-message'
                        : variant === 'cardSubtle'
                          ? 'bg-surface-card-subtle'
                          : variant === 'cardControl'
                            ? 'bg-surface-card-control'
                            : variant === 'cardRaised'
                              ? 'bg-surface-card-raised'
                              : variant === 'cardHover'
                                ? 'bg-surface-card-hover'
                                : 'bg-surface-panel';
  $: borderClass = bordered ? 'border border-border-default' : '';
  $: shadowClass =
    shadow === 'soft'
      ? 'shadow-[var(--surface-shadow-soft)]'
      : shadow === 'strong'
        ? 'shadow-[var(--surface-shadow-strong)]'
        : '';
  $: radiusClass =
    radius === 'none'
      ? ''
      : radius === 'md'
        ? 'rounded-md'
        : radius === 'lg'
          ? 'rounded-lg'
          : radius === 'xl'
            ? 'rounded-xl'
            : radius === '2xl'
              ? 'rounded-2xl'
              : 'rounded-3xl';
</script>

<svelte:element
  this={as}
  {...$$restProps}
  class={[surfaceClass, borderClass, shadowClass, radiusClass, className]}
>
  <slot />
</svelte:element>
