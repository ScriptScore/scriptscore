<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  type ConnectionTickerTone = 'warning' | 'info' | 'success' | 'error';

  export let text: string | null = null;
  export let tone: ConnectionTickerTone = 'info';
  export let sticky = false;

  function messageClass(messageTone: ConnectionTickerTone) {
    const toneClass = {
      error:
        'border-message-error-border bg-message-error-bg text-message-error-text',
      warning:
        'border-message-warning-border bg-message-warning-bg text-message-warning-text',
      info:
        'border-message-info-border bg-message-info-bg text-message-info-text',
      success:
        'border-message-success-border bg-message-success-bg text-message-success-text'
    }[messageTone];

    return `w-fit max-w-full rounded-xl border px-3 py-2 text-right text-sm font-medium ${toneClass}`;
  }
</script>

{#if text}
  <p
    class={`${messageClass(tone)} setup-message-tick`}
    role={sticky ? 'alert' : 'status'}
  >
    {text}
  </p>
{/if}

<style>
  @keyframes setup-message-tick {
    0% {
      opacity: 0;
      transform: translateY(0.45rem);
    }
    100% {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .setup-message-tick {
    animation: setup-message-tick 0.5s ease;
  }
</style>
