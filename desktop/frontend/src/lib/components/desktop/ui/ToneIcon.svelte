<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/svelte';
  import { feedbackToneClass, type FeedbackTone } from './feedback';

  export let tone: FeedbackTone = 'muted';
  export let icon: IconSvgElement;
  export let label: string;
  export let title: string | null = null;
  export let size = 14;
  export let strokeWidth = 1.7;

  let className = '';
  export { className as class };

  $: toneClass = feedbackToneClass(tone);
</script>

<span
  {...$$restProps}
  class={[
    'inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-full border',
    toneClass,
    className
  ]}
  aria-label={label}
  title={title ?? label}
>
  <HugeiconsIcon {icon} {size} {strokeWidth} aria-hidden="true" />
</span>
