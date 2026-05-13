<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { appSettings } from '$lib/stores/appSettings';
  import { DesktopButton, RadioCardGroup } from './ui';

  export let onChooseSimple: (() => void) | null = null;
  export let onChooseDetailed: (() => void) | null = null;
  export let onContinue: (() => void) | null = null;

  const gradingProfileOptions = [
    {
      value: 'simple',
      title: 'Simple',
      description: 'Use balanced grading strictness and brief feedback without extra tolerance controls.'
    },
    {
      value: 'detailed',
      title: 'Detailed',
      description: 'Show all grading profile controls during exam setup and include every profile tag.'
    }
  ];

  $: gradingProfileValue = Object.values($appSettings.instructorProfile.enabledTags).every(Boolean)
    ? 'detailed'
    : 'simple';
</script>

<div class="grid gap-4 px-2 pb-2 pt-3">
  <RadioCardGroup
    legend="Grading profile"
    options={gradingProfileOptions}
    value={gradingProfileValue}
    onChange={(value) => {
      if (value === 'detailed') {
        onChooseDetailed?.();
      } else {
        onChooseSimple?.();
      }
    }}
  />

  <div class="flex flex-wrap items-center justify-end gap-3">
    <DesktopButton size="large" onclick={() => onContinue?.()}>
      Continue
    </DesktopButton>
  </div>
</div>
