<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/svelte';

  export let legend: string;
  export let options: Array<{
    value: string;
    title: string;
    description: string;
    icon?: IconSvgElement;
    disabled?: boolean;
  }> = [];
  export let value = '';
  export let expandedValues: string[] = [];
  export let onChange: ((value: string) => void | Promise<void>) | null = null;

  let className = '';
  export { className as class };
</script>

<fieldset class={['grid gap-3', className]}>
  <legend class="sr-only">{legend}</legend>
  {#each options as option (option.value)}
    {@const selected = value === option.value}
    {@const optionId = `${legend.replace(/\s+/g, '-').toLowerCase()}-${option.value}`}
    <div
      class={[
        'rounded-2xl border px-4 py-4 transition-colors',
        selected
          ? 'border-border-strong bg-interaction-active text-text-primary'
          : 'border-border-default bg-workspace-empty text-text-secondary hover:bg-interaction-hover',
        option.disabled ? 'cursor-not-allowed opacity-50' : ''
      ]}
    >
      <input
        id={optionId}
        class="sr-only"
        type="radio"
        name={legend}
        checked={selected}
        disabled={option.disabled}
        onchange={() => {
          if (!option.disabled) {
            value = option.value;
            void onChange?.(option.value);
          }
        }}
      />
      <label class="grid cursor-pointer grid-cols-[auto_minmax(0,1fr)] items-start gap-3" for={optionId}>
        <span
          class={[
            'mt-1 inline-flex size-5 items-center justify-center rounded-full border',
            selected ? 'border-text-primary' : 'border-border-strong'
          ]}
          aria-hidden="true"
        >
          {#if selected}
            <span class="size-2.5 rounded-full bg-text-primary"></span>
          {/if}
        </span>
        <span class="min-w-0">
          <span class="flex min-w-0 items-center gap-3">
            {#if option.icon}
              <span
                class={[
                  'flex size-6 shrink-0 items-center justify-center transition-colors',
                  selected ? 'text-text-primary' : 'text-text-muted'
                ]}
                aria-hidden="true"
              >
                <HugeiconsIcon icon={option.icon} size={21} strokeWidth={1.7} />
              </span>
            {/if}
            <span class="min-w-0 text-sm font-semibold text-text-primary">{option.title}</span>
          </span>
          <span class={['mt-1 block text-sm leading-6 text-text-secondary', option.icon ? 'ml-9' : '']}>
            {option.description}
          </span>
        </span>
      </label>
      {#if selected && expandedValues.includes(option.value)}
        <div class="mt-4 sm:ml-[4.25rem]">
          <slot name="selected" optionValue={option.value} />
        </div>
      {/if}
    </div>
  {/each}
</fieldset>
