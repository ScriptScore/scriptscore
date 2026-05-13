<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { CheckListIcon } from '@hugeicons/core-free-icons';
  import { HugeiconsIcon } from '@hugeicons/svelte';

  import type { AppSettings } from '$lib/types';
  import { SelectField, TextareaField, ToggleRow } from './ui';

  export let settings: AppSettings;
  export let onChange: (() => void | Promise<void>) | null = null;

  const strictnessOptions = [
    { value: 'strict', label: 'strict' },
    { value: 'balanced', label: 'balanced' },
    { value: 'generous', label: 'generous' }
  ];
  const toleranceOptions = [
    { value: 'low', label: 'low' },
    { value: 'medium', label: 'medium' },
    { value: 'high', label: 'high' }
  ];
  const feedbackStyleOptions = [
    { value: 'brief', label: 'brief' },
    { value: 'balanced', label: 'balanced' },
    { value: 'detailed', label: 'detailed' }
  ];

  function setProfileTagEnabled(
    key: keyof AppSettings['instructorProfile']['enabledTags'],
    enabled: boolean
  ) {
    settings.instructorProfile.enabledTags = {
      ...settings.instructorProfile.enabledTags,
      [key]: enabled
    };
    settings = settings;
    void onChange?.();
  }

  function setMinimumPointsEnabled(enabled: boolean) {
    settings.instructorProfile.includeMinimumCreditCriterion = enabled;
    settings = settings;
    void onChange?.();
  }

  function setMinimumPointsPercent(input: HTMLInputElement) {
    const value = input.valueAsNumber;
    if (!Number.isFinite(value) || value < 0 || value > 100) {
      input.value = String(settings.instructorProfile.minimumCreditPercent);
      return;
    }
    settings.instructorProfile.minimumCreditPercent = value;
    input.value = String(value);
    settings = settings;
    void onChange?.();
  }
</script>

<section class="py-1">
  <div class="shell-eyebrow text-workspace-text-muted">Instructor grading profile</div>
  <div class="mt-4 grid gap-5">
    <div class="divide-y divide-workspace-border border-y border-workspace-border">
      <div class="grid gap-3 py-4 md:grid-cols-[minmax(0,1fr)_16rem] md:items-center">
        <ToggleRow
          class="min-h-14 hover:bg-surface-card-hover"
          align="center"
          title="Grading strictness"
          description="Include the overall strict, balanced, or generous grading posture."
          checked={settings.instructorProfile.enabledTags.gradingStrictness}
          onToggle={(checked) => setProfileTagEnabled('gradingStrictness', checked)}
        />
        {#if settings.instructorProfile.enabledTags.gradingStrictness}
          <SelectField
            label={null}
            ariaLabel={`Grading strictness: ${settings.instructorProfile.gradingStrictness}`}
            value={settings.instructorProfile.gradingStrictness}
            options={strictnessOptions}
            onChange={(value) => {
              settings.instructorProfile.gradingStrictness =
                value as AppSettings['instructorProfile']['gradingStrictness'];
              void onChange?.();
            }}
          />
        {/if}
      </div>
      <div class="grid gap-3 py-4 md:grid-cols-[minmax(0,1fr)_16rem] md:items-center">
        <ToggleRow
          class="min-h-14 hover:bg-surface-card-hover"
          align="center"
          title="Syntax leniency"
          description="Tell the grader how much syntax variation to tolerate."
          checked={settings.instructorProfile.enabledTags.syntaxLeniency}
          onToggle={(checked) => setProfileTagEnabled('syntaxLeniency', checked)}
        />
        {#if settings.instructorProfile.enabledTags.syntaxLeniency}
          <SelectField
            label={null}
            ariaLabel={`Syntax leniency: ${settings.instructorProfile.syntaxLeniency}`}
            value={settings.instructorProfile.syntaxLeniency}
            options={toleranceOptions}
            onChange={(value) => {
              settings.instructorProfile.syntaxLeniency =
                value as AppSettings['instructorProfile']['syntaxLeniency'];
              void onChange?.();
            }}
          />
        {/if}
      </div>
      <div class="grid gap-3 py-4 md:grid-cols-[minmax(0,1fr)_16rem] md:items-center">
        <ToggleRow
          class="min-h-14 hover:bg-surface-card-hover"
          align="center"
          title="OCR tolerance"
          description="Tell the grader how much transcription uncertainty to tolerate."
          checked={settings.instructorProfile.enabledTags.ocrTolerance}
          onToggle={(checked) => setProfileTagEnabled('ocrTolerance', checked)}
        />
        {#if settings.instructorProfile.enabledTags.ocrTolerance}
          <SelectField
            label={null}
            ariaLabel={`OCR tolerance: ${settings.instructorProfile.ocrTolerance}`}
            value={settings.instructorProfile.ocrTolerance}
            options={toleranceOptions}
            onChange={(value) => {
              settings.instructorProfile.ocrTolerance =
                value as AppSettings['instructorProfile']['ocrTolerance'];
              void onChange?.();
            }}
          />
        {/if}
      </div>
      <div class="grid gap-3 py-4 md:grid-cols-[minmax(0,1fr)_16rem] md:items-center">
        <ToggleRow
          class="min-h-14 hover:bg-surface-card-hover"
          align="center"
          title="Partial credit style"
          description="Include guidance for strict, balanced, or generous partial credit."
          checked={settings.instructorProfile.enabledTags.partialCreditStyle}
          onToggle={(checked) => setProfileTagEnabled('partialCreditStyle', checked)}
        />
        {#if settings.instructorProfile.enabledTags.partialCreditStyle}
          <SelectField
            label={null}
            ariaLabel={`Partial credit style: ${settings.instructorProfile.partialCreditStyle}`}
            value={settings.instructorProfile.partialCreditStyle}
            options={strictnessOptions}
            onChange={(value) => {
              settings.instructorProfile.partialCreditStyle =
                value as AppSettings['instructorProfile']['partialCreditStyle'];
              void onChange?.();
            }}
          />
        {/if}
      </div>
      <div class="grid gap-3 py-4 md:grid-cols-[minmax(0,1fr)_16rem] md:items-center">
        <ToggleRow
          class="min-h-14 hover:bg-surface-card-hover"
          align="center"
          title="Feedback style"
          description="Include the preferred level of feedback detail."
          checked={settings.instructorProfile.enabledTags.feedbackStyle}
          onToggle={(checked) => setProfileTagEnabled('feedbackStyle', checked)}
        />
        {#if settings.instructorProfile.enabledTags.feedbackStyle}
          <SelectField
            label={null}
            ariaLabel={`Feedback style: ${settings.instructorProfile.feedbackStyle}`}
            value={settings.instructorProfile.feedbackStyle}
            options={feedbackStyleOptions}
            onChange={(value) => {
              settings.instructorProfile.feedbackStyle =
                value as AppSettings['instructorProfile']['feedbackStyle'];
              void onChange?.();
            }}
          />
        {/if}
      </div>
    </div>
    <TextareaField
      class="col-start-1"
      label="Additional guidance"
      value={settings.instructorProfile.additionalGuidance}
      oninput={(event: Event) => {
        settings.instructorProfile.additionalGuidance = (event.currentTarget as HTMLTextAreaElement).value;
        void onChange?.();
      }}
    />
    <div class="mt-4 md:col-span-2">
      <div class="shell-eyebrow mb-4 text-workspace-text-muted">Non-blank answer minimum</div>
      <div class="space-y-3">
        <div class="flex flex-wrap items-center gap-x-4 gap-y-3">
          <button
            type="button"
            role="switch"
            aria-checked={settings.instructorProfile.includeMinimumCreditCriterion}
            aria-label="Award minimum points for non-blank answers"
            class={[
              'relative inline-flex h-7 w-12 shrink-0 items-center rounded-full border transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring',
              settings.instructorProfile.includeMinimumCreditCriterion
                ? 'border-toggle-active-border bg-toggle-active-bg'
                : 'border-border-default bg-workspace-empty'
            ]}
            onclick={() => {
              setMinimumPointsEnabled(!settings.instructorProfile.includeMinimumCreditCriterion);
            }}
          >
            <span
              class={[
                'pointer-events-none inline-flex size-5 rounded-full shadow-sm transition-transform',
                settings.instructorProfile.includeMinimumCreditCriterion
                  ? 'translate-x-6 bg-toggle-active-text'
                  : 'translate-x-1 bg-text-primary'
              ]}
              aria-hidden="true"
            ></span>
          </button>

          {#if settings.instructorProfile.includeMinimumCreditCriterion}
            <div class="flex min-w-0 flex-1 flex-wrap items-center gap-x-3 gap-y-2 text-base font-semibold text-text-primary">
              <span>Award at least</span>
              <div class="inline-flex h-10 shrink-0 items-stretch overflow-hidden rounded-xl border border-border-default bg-workspace-empty shadow-[var(--surface-shadow-inset)] transition-colors hover:border-border-strong focus-within:border-border-focus focus-within:ring-2 focus-within:ring-focus-ring/60">
                <input
                  class="w-16 bg-transparent px-3 text-center text-base font-semibold text-text-primary outline-none [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
                  type="number"
                  min="0"
                  max="100"
                  step="1"
                  value={settings.instructorProfile.minimumCreditPercent}
                  aria-label="Minimum percentage of question points for non-blank answers"
                  oninput={(event: Event) => {
                    setMinimumPointsPercent(event.currentTarget as HTMLInputElement);
                  }}
                />
                <span class="inline-flex min-w-10 items-center justify-center border-l border-border-default px-3 text-base font-semibold text-text-primary">
                  %
                </span>
              </div>
              <span>of question points for any non-blank answer</span>
            </div>
          {:else}
            <div class="min-w-0 flex-1">
              <div class="text-base font-semibold text-text-primary">
                Award minimum points for non-blank answers
              </div>
              <div class="mt-1 text-sm leading-6 text-text-secondary">
                Off — no minimum is guaranteed and no rubric criterion will be added.
              </div>
            </div>
          {/if}
        </div>

        {#if settings.instructorProfile.includeMinimumCreditCriterion}
          <div class="mt-3 flex items-center gap-3 text-sm leading-6 text-text-secondary">
            <span class="inline-flex size-8 shrink-0 items-center justify-center rounded-full bg-message-success-bg text-message-success-text" aria-hidden="true">
              <HugeiconsIcon icon={CheckListIcon} size={18} strokeWidth={1.8} />
            </span>
            <span>Automatically adds a rubric criterion for non-blank attempts.</span>
          </div>
        {/if}

      </div>
    </div>
  </div>
</section>
