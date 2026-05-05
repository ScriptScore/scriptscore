<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { onDestroy } from 'svelte';

  type AriaHasPopup = 'dialog' | 'grid' | 'listbox' | 'menu' | 'tree' | 'false' | 'true';

  export let open = false;
  export let onOpenChange: ((open: boolean) => void) | null = null;
  export let triggerLabel = 'Open menu';
  export let triggerClass = '';
  export let panelClass = '';
  export let panelRole: string | null = 'menu';
  export let panelAriaLabel: string | null = null;
  export let triggerAriaHaspopup: AriaHasPopup = 'menu';
  export let rootClass = 'relative inline-block';
  export let align: 'start' | 'end' = 'start';
  export let placement: 'absolute' | 'fixed' = 'absolute';

  let rootElement: HTMLDivElement;
  let triggerElement: HTMLButtonElement;
  let fixedPanelStyle = '';

  $: if (typeof document !== 'undefined') {
    if (open) {
      document.addEventListener('pointerdown', handleDocumentPointerDown, true);
      document.addEventListener('keydown', handleDocumentKeyDown, true);
    } else {
      document.removeEventListener('pointerdown', handleDocumentPointerDown, true);
      document.removeEventListener('keydown', handleDocumentKeyDown, true);
    }
  }

  onDestroy(() => {
    if (typeof document !== 'undefined') {
      document.removeEventListener('pointerdown', handleDocumentPointerDown, true);
      document.removeEventListener('keydown', handleDocumentKeyDown, true);
    }
  });

  function setOpen(next: boolean) {
    if (open !== next) {
      open = next;
      onOpenChange?.(next);
    }
    if (next) {
      updateFixedPanelStyle();
    }
    if (!next) {
      queueMicrotask(() => triggerElement?.focus());
    }
  }

  function updateFixedPanelStyle() {
    if (placement !== 'fixed' || !triggerElement) {
      fixedPanelStyle = '';
      return;
    }
    const rect = triggerElement.getBoundingClientRect();
    const left = align === 'end' ? rect.right - rect.width : rect.left;
    fixedPanelStyle = `left:${Math.max(0, left)}px;top:${rect.bottom + 8}px;width:${rect.width}px;`;
  }

  function handleDocumentPointerDown(event: PointerEvent) {
    if (!open || rootElement?.contains(event.target as Node)) {
      return;
    }
    setOpen(false);
  }

  function handleDocumentKeyDown(event: KeyboardEvent) {
    if (!open || event.key !== 'Escape') {
      return;
    }
    event.preventDefault();
    setOpen(false);
  }

  function handlePanelClick(event: MouseEvent) {
    const target = event.target;
    const closeTarget =
      target instanceof Element ? target.closest<HTMLElement>('[data-popover-close]') : null;
    if (!closeTarget) {
      return;
    }
    if (closeTarget.matches(':disabled') || closeTarget.getAttribute('aria-disabled') === 'true') {
      return;
    }
    if (closeTarget) {
      setOpen(false);
    }
  }
</script>

<svelte:window on:resize={updateFixedPanelStyle} on:scroll={updateFixedPanelStyle} />

<div class={rootClass} bind:this={rootElement}>
  <button
    {...$$restProps}
    bind:this={triggerElement}
    type="button"
    class={triggerClass}
    aria-haspopup={triggerAriaHaspopup}
    aria-expanded={open}
    onclick={() => setOpen(!open)}
  >
    <slot name="trigger">{triggerLabel}</slot>
  </button>
  {#if open}
    <div
      class={[
        placement === 'fixed'
          ? 'fixed z-40 min-w-48 rounded-2xl border border-border-default bg-surface-overlay p-2 text-text-primary shadow-[var(--surface-shadow-strong)]'
          : 'absolute z-40 mt-2 min-w-48 rounded-2xl border border-border-default bg-surface-overlay p-2 text-text-primary shadow-[var(--surface-shadow-strong)]',
        placement === 'absolute' ? (align === 'end' ? 'right-0' : 'left-0') : '',
        panelClass
      ]}
      style={fixedPanelStyle}
      role={panelRole}
      aria-label={panelAriaLabel}
      onclick={handlePanelClick}
    >
      <slot />
    </div>
  {/if}
</div>
