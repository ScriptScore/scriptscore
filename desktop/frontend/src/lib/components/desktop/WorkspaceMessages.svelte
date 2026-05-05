<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import type { SmokePingResult, WorkspaceWarning } from '$lib/types';
  import { InlineMessage, type FeedbackTone } from './ui';

  export let actionError: string | null = null;
  export let runtimeError: string | null = null;
  export let failureMessage: string | null = null;
  export let runtimeResult: SmokePingResult | null = null;
  export let warnings: WorkspaceWarning[] = [];

  let dismissedMessages = new Set<string>();

  $: if (actionError && dismissedMessages.has(actionError)) dismissedMessages.delete(actionError);
  $: if (runtimeError && dismissedMessages.has(runtimeError)) dismissedMessages.delete(runtimeError);
  $: if (runtimeResult && dismissedMessages.has(`${runtimeResult.command}: ${runtimeResult.message}`)) dismissedMessages.delete(`${runtimeResult.command}: ${runtimeResult.message}`);

  $: effectiveActionError = actionError && !dismissedMessages.has(actionError) ? actionError : null;
  $: effectiveRuntimeError = runtimeError && !dismissedMessages.has(runtimeError) ? runtimeError : null;
  $: effectiveFailureMessage = failureMessage && !dismissedMessages.has(failureMessage) ? failureMessage : null;
  $: effectiveWarnings = warnings.filter((w) => !dismissedMessages.has(w.message));
  $: effectiveRuntimeResult = runtimeResult && !dismissedMessages.has(`${runtimeResult.command}: ${runtimeResult.message}`) ? runtimeResult : null;

  $: hasErrors = effectiveActionError || effectiveRuntimeError || effectiveFailureMessage || effectiveWarnings.length > 0 || effectiveRuntimeResult;

  function deduplicatedMessages(): Array<{ type: FeedbackTone; message: string }> {
    const messages: Array<{ type: FeedbackTone; message: string }> = [];
    const seen = new Set<string>();

    function add(type: FeedbackTone, message: string) {
      if (seen.has(message)) return;
      seen.add(message);
      messages.push({ type, message });
    }

    if (effectiveActionError) add('error', effectiveActionError);
    if (effectiveRuntimeError) add('error', effectiveRuntimeError);
    if (effectiveFailureMessage) add('error', effectiveFailureMessage);
    for (const warning of effectiveWarnings) add('warning', warning.message);
    if (effectiveRuntimeResult) add('info', `${effectiveRuntimeResult.command}: ${effectiveRuntimeResult.message}`);

    return messages;
  }

  $: visibleMessages = deduplicatedMessages();

  function dismissMessage(message: string) {
    dismissedMessages.add(message);
    dismissedMessages = new Set(dismissedMessages);
  }
</script>

{#if hasErrors}
  <div class="absolute inset-x-0 top-16 z-20 px-6 py-3">
    <div class="mx-auto max-w-5xl space-y-2">
      {#each visibleMessages as item (item.message)}
        <InlineMessage class="flex items-start gap-3 rounded-2xl shadow-lg" tone={item.type}>
          <span class="flex-1">{item.message}</span>
          <button
            class="shrink-0 rounded-full p-1 opacity-60 transition-opacity hover:opacity-100"
            type="button"
            aria-label="Dismiss message"
            onclick={() => dismissMessage(item.message)}
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>
          </button>
        </InlineMessage>
      {/each}
    </div>
  </div>
{/if}
