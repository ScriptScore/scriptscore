<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script context="module" lang="ts">
  let nextSelectFieldId = 0;
</script>

<script lang="ts">
  import DesktopPopover from './DesktopPopover.svelte';
  import FieldShell from './FieldShell.svelte';

  type SelectFieldOption = {
    value: string;
    label: string;
    subitem?: string | null;
    disabled?: boolean;
  };

  export let id: string | undefined = undefined;
  export let label: string | null = null;
  export let hint: string | null = null;
  export let error: string | null = null;
  export let value = '';
  export let values: string[] = [];
  export let multiple = false;
  export let density: 'compact' | 'default' | 'large' = 'default';
  export let disabled = false;
  export let options: SelectFieldOption[] = [];
  export let placeholder = 'Select...';
  export let searchable = false;
  export let searchPlaceholder = 'Search';
  export let showSubitems = false;
  export let allowEmpty = false;
  export let emptyLabel = 'None';
  export let emptyValue = '';
  export let loading = false;
  export let loadingLabel = 'Loading...';
  export let noOptionsLabel = 'No matching options.';
  export let ariaLabel: string | null = null;
  export let popoverPlacement: 'absolute' | 'fixed' = 'absolute';
  export let optionListClass: string | null = null;
  export let optionClass = '';
  export let onChange: ((value: string) => void | Promise<void>) | null = null;
  export let onChangeValues: ((values: string[]) => void | Promise<void>) | null = null;

  let className = '';
  export { className as class };
  export let controlClass = '';
  const generatedId = `select-field-${++nextSelectFieldId}`;
  let open = false;
  let searchTerm = '';

  $: customMode =
    options.length > 0 ||
    searchable ||
    allowEmpty ||
    loading ||
    noOptionsLabel !== 'No matching options.' ||
    placeholder !== 'Select...';

  $: densityClass =
    density === 'compact'
      ? 'h-9 px-3 text-sm'
      : density === 'large'
        ? 'min-h-12 px-4 py-3 text-base'
        : 'h-10 px-3 text-sm';
  $: optionRows = allowEmpty ? [{ value: emptyValue, label: emptyLabel, subitem: null }, ...options] : options;
  $: selectedValues = new Set(values);
  $: selectedOption = optionRows.find((option) => option.value === value) ?? null;
  $: selectedMultiOptions = options.filter((option) => selectedValues.has(option.value));
  $: controlId = id ?? generatedId;
  $: triggerLabel = multiple
    ? selectedMultiOptions.length === 0
      ? allowEmpty
        ? emptyLabel
        : placeholder
      : selectedMultiOptions.length === 1
        ? selectedMultiOptions[0].label
        : `${selectedMultiOptions.length} selected`
    : selectedOption?.label ?? placeholder;
  $: normalizedSearch = searchTerm.trim().toLowerCase();
  $: filteredOptions =
    normalizedSearch.length === 0
      ? optionRows
      : optionRows.filter((option) =>
          `${option.label} ${option.subitem ?? ''}`.toLowerCase().includes(normalizedSearch)
        );

  function closePopup() {
    open = false;
    searchTerm = '';
  }

  function selectOption(option: SelectFieldOption) {
    if (option.disabled) {
      return;
    }
    if (multiple) {
      const next = new Set(values);
      if (allowEmpty && option.value === emptyValue) {
        next.clear();
      } else if (next.has(option.value)) {
        next.delete(option.value);
      } else {
        next.add(option.value);
      }
      values = Array.from(next);
      void onChangeValues?.(values);
      return;
    }
    if (option.value === value) {
      closePopup();
      return;
    }
    value = option.value;
    void onChange?.(option.value);
    closePopup();
  }
</script>

<FieldShell class={className} {label} forId={controlId} {hint} {error}>
  {#if customMode}
    <DesktopPopover
      {...$$restProps}
      bind:open
      id={controlId}
      role="combobox"
      rootClass="relative block"
      placement={popoverPlacement}
      triggerClass={[
        'flex w-full items-center justify-between rounded-xl border border-border-default bg-surface-message text-left text-text-primary shadow-[var(--surface-shadow-inset)] outline-none transition-colors focus:border-border-focus disabled:cursor-not-allowed disabled:opacity-60',
        densityClass,
        controlClass
      ].join(' ')}
      triggerLabel={triggerLabel}
      triggerAriaHaspopup="listbox"
      panelRole={null}
      panelClass="left-0 right-0 w-full p-2"
      aria-label={ariaLabel ?? (label ? `${label}: ${triggerLabel}` : triggerLabel)}
      disabled={disabled || loading}
      onOpenChange={(next) => {
        open = next;
        if (!next) {
          searchTerm = '';
        }
      }}
    >
      <svelte:fragment slot="trigger">
        <span class={multiple ? (selectedMultiOptions.length > 0 ? 'truncate' : 'truncate text-text-muted') : (selectedOption ? 'truncate' : 'truncate text-text-muted')}>{loading ? loadingLabel : triggerLabel}</span>
        <span class="text-text-muted" aria-hidden="true">⌄</span>
      </svelte:fragment>

      {#if searchable}
        <input
          class="h-9 w-full rounded-xl border border-border-default bg-surface-card-control px-3 text-sm text-text-primary outline-none transition-colors placeholder:text-text-muted focus:border-border-focus"
          bind:value={searchTerm}
          placeholder={searchPlaceholder}
          aria-label={searchPlaceholder}
        />
      {/if}
      <div class={optionListClass ?? (searchable ? 'mt-2 max-h-56 overflow-y-auto' : 'max-h-56 overflow-y-auto')} role="listbox" aria-label={label ?? ariaLabel ?? 'Options'}>
        {#each filteredOptions as option (option.value)}
          <button
            class={[
              showSubitems && option.subitem ? 'grid gap-0.5' : 'flex items-center',
              'w-full rounded-xl text-left text-sm text-text-primary transition-colors hover:bg-interaction-hover disabled:cursor-not-allowed disabled:opacity-50',
              optionClass || 'my-1 px-3 py-1 font-medium',
              (multiple ? selectedValues.has(option.value) : option.value === value) ? 'bg-interaction-active' : ''
            ]}
            type="button"
            role="option"
            aria-selected={multiple ? selectedValues.has(option.value) : option.value === value}
            disabled={option.disabled}
            data-popover-close={multiple ? undefined : true}
            onclick={() => selectOption(option)}
          >
            <span class="truncate">{option.label}</span>
            {#if showSubitems && option.subitem}
              <span class="truncate text-xs text-text-muted">{option.subitem}</span>
            {/if}
          </button>
        {:else}
          <div class="px-3 py-2 text-sm text-text-muted">{loading ? loadingLabel : noOptionsLabel}</div>
        {/each}
      </div>
    </DesktopPopover>
  {:else}
    <select
      {...$$restProps}
      id={controlId}
      bind:value
      {disabled}
      class={[
        'w-full rounded-xl border border-border-default bg-surface-message text-text-primary shadow-[var(--surface-shadow-inset)] outline-none transition-colors focus:border-border-focus disabled:cursor-not-allowed disabled:opacity-60',
        densityClass,
        controlClass
      ]}
    >
      <slot />
    </select>
  {/if}
</FieldShell>
