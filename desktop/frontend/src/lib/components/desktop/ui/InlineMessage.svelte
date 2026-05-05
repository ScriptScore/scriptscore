<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { feedbackRole, feedbackToneClass, type FeedbackTone } from './feedback';

  export let tone: FeedbackTone = 'info';
  export let message: string | null = null;

  let className = '';
  export { className as class };

  $: toneClass = feedbackToneClass(tone);
  $: role = feedbackRole(tone);
</script>

{#if message || $$slots.default}
  <div
    {...$$restProps}
    class={['rounded-md border px-3 py-2 text-sm leading-5', toneClass, className]}
    {role}
  >
    <slot>{message}</slot>
  </div>
{/if}
