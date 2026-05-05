<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { feedbackToneClass, type FeedbackTone } from './feedback';

  export let tone: FeedbackTone = 'muted';
  export let label: string | null = null;

  let className = '';
  export { className as class };

  $: toneClass = feedbackToneClass(tone);
</script>

<span
  {...$$restProps}
  class={[
    'inline-flex min-h-6 max-w-full items-center rounded-full border px-2.5 py-0.5 text-xs font-medium leading-5',
    toneClass,
    className
  ]}
>
  <slot>{label}</slot>
</span>
