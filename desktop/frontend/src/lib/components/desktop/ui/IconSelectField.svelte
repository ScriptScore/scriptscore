<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/svelte';
  import DesktopPopover from './DesktopPopover.svelte';

  type IconSelectOption = {
    value: string;
    label: string;
    disabled?: boolean;
  };

  export let ariaLabel: string;
  export let dialogLabel: string | null = null;
  export let menuLabel: string;
  export let value = '';
  export let options: IconSelectOption[] = [];
  export let icon: IconSvgElement;
  export let title: string | null = null;
  export let iconSize = 20;
  export let iconStrokeWidth = 2;
  export let disabled = false;
  export let align: 'left' | 'right' = 'left';
  export let menuClass = 'w-52';
  export let triggerClass =
    'inline-flex shrink-0 items-center justify-center rounded-xl border border-border-default bg-surface-card-control text-text-primary transition-colors hover:bg-interaction-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring disabled:cursor-not-allowed disabled:opacity-50 size-10';
  export let optionClass = '';
  export let selectedOptionClass = 'bg-interaction-active text-text-primary';
  export let unselectedOptionClass =
    'text-text-secondary hover:bg-interaction-hover hover:text-text-primary';
  export let onChange: ((value: string) => void | Promise<void>) | null = null;

  let className = '';
  export { className as class };
  let open = false;

  $: selectedOption = options.find((option) => option.value === value) ?? null;
  $: triggerTitle = title ?? `${menuLabel}: ${selectedOption?.label ?? 'None'}`;

  function selectOption(option: IconSelectOption) {
    if (option.disabled) {
      return;
    }
    value = option.value;
    void onChange?.(option.value);
    open = false;
  }
</script>

<DesktopPopover
  bind:open
  rootClass={['relative shrink-0', className].filter(Boolean).join(' ')}
  {triggerClass}
  triggerLabel={ariaLabel}
  triggerAriaHaspopup="dialog"
  panelRole="dialog"
  panelAriaLabel={dialogLabel ?? ariaLabel}
  panelClass={['top-10 p-3', align === 'right' ? 'right-0' : 'left-0', menuClass].join(' ')}
  align={align === 'right' ? 'end' : 'start'}
  aria-label={ariaLabel}
  title={triggerTitle}
  {disabled}
>
  <svelte:fragment slot="trigger">
    <HugeiconsIcon {icon} size={iconSize} strokeWidth={iconStrokeWidth} aria-hidden="true" />
  </svelte:fragment>

  <div class="text-[11px] font-semibold uppercase tracking-wide text-text-muted">
    {menuLabel}
  </div>
  <div class="mt-2 flex flex-col gap-1">
    {#each options as option (option.value)}
      <button
        type="button"
        class={[
          'rounded-xl px-3 py-2 text-left text-sm font-medium transition-colors',
          value === option.value ? selectedOptionClass : unselectedOptionClass,
          optionClass
        ]}
        disabled={option.disabled}
        aria-current={value === option.value ? 'true' : undefined}
        data-popover-close
        onclick={() => selectOption(option)}
      >
        {option.label}
      </button>
    {/each}
  </div>
</DesktopPopover>
