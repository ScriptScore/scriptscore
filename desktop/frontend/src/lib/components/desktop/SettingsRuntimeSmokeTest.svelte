<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import type { SmokePingResult } from '$lib/types';
  import { DesktopButton } from './ui';

  export let busy = false;
  export let smokeResult: SmokePingResult | null = null;
  export let onRuntimeCheck: (() => void | Promise<void>) | null = null;
  export let onRunSetupWizard: (() => void | Promise<void>) | null = null;
  export let onOpenTraceHistory: (() => void) | null = null;
  export let traceHistoryDisabled = false;

  function smokeSummary(result: SmokePingResult): string {
    return `${result.steps} step${result.steps === 1 ? '' : 's'} - ${result.eventCount} event${result.eventCount === 1 ? '' : 's'}`;
  }
</script>

<section class="space-y-4">
  {#if onRunSetupWizard}
    <div class="flex flex-wrap items-center justify-between gap-3 border-b border-workspace-border pb-4">
      <div class="min-w-0">
        <div class="shell-title text-workspace-text-primary">Setup wizard</div>
        <div class="mt-1 text-sm text-workspace-text-secondary">
          Reopen first-run setup to reselect project mode and AI assistance defaults.
        </div>
      </div>
      <DesktopButton onclick={() => void onRunSetupWizard?.()}>
        Run setup wizard
      </DesktopButton>
    </div>
  {/if}

  <div class="flex flex-wrap items-start justify-between gap-3">
    <div class="min-w-0">
      <div class="shell-title text-workspace-text-primary">Runtime smoke test</div>
      {#if smokeResult}
        <div class="mt-1 text-sm text-workspace-text-secondary">
          {smokeSummary(smokeResult)}
        </div>
      {:else}
        <div class="mt-1 text-sm text-workspace-text-secondary">
          Run a quick local runtime check. Project trace history is available below.
        </div>
      {/if}
    </div>
    <DesktopButton
      disabled={busy}
      onclick={() => void onRuntimeCheck?.()}
    >
      {busy ? 'Running smoke test...' : 'Runtime smoke test'}
    </DesktopButton>
  </div>

  <div class="flex flex-wrap items-start justify-between gap-3 border-t border-workspace-border pt-4">
    <div class="min-w-0">
      <div class="shell-title text-workspace-text-primary">Trace history</div>
      <div class="mt-1 text-sm text-workspace-text-secondary">
        Browse current-project worker runs, persisted requests, responses, and event streams.
      </div>
    </div>
    <DesktopButton
      disabled={traceHistoryDisabled}
      onclick={() => onOpenTraceHistory?.()}
    >
      Trace history
    </DesktopButton>
  </div>
</section>
