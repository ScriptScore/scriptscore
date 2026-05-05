<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  export let selected = false;
  export let draft = false;
  export let label = 'Selection handle';

  let className = '';
  export { className as class };
</script>

<span
  {...$$restProps}
  class={[
    'absolute size-4 rounded-full border-2 bg-[var(--workspace-selection-handle-fill)] shadow-sm outline-none focus-visible:ring-2 focus-visible:ring-focus-ring',
    draft ? 'border-message-warning-border' : 'border-[var(--workspace-selection-handle-border)]',
    selected ? 'ring-2 ring-focus-ring' : '',
    className
  ]}
  role="button"
  tabindex="0"
  aria-label={label}
></span>
