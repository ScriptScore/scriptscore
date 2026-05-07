<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import type { JobTraceState } from '$lib/types';
  import { DesktopButton, InlineMessage, Surface, TabStrip } from './ui';

  export let trace: JobTraceState | null = null;
  export let title = 'Trace Details';
  export let emptyMessage = 'Select a trace to inspect request, response, and event details.';

  type Tab = 'formatted' | 'events' | 'request' | 'response';
  let activeTab: Tab = 'formatted';
  const tabs = [
    { value: 'formatted', label: 'Formatted Events' },
    { value: 'events', label: 'Event Stream' },
    { value: 'request', label: 'Request' },
    { value: 'response', label: 'Response' }
  ];

  function formatJson(value: Record<string, unknown> | null): string {
    if (!value) {
      return '{}';
    }
    return JSON.stringify(value, null, 2);
  }

  function responsePayload(traceState: JobTraceState): Record<string, unknown> {
    const payload: Record<string, unknown> = {};
    if (traceState.result) {
      payload.result = traceState.result;
    }
    if (traceState.error) {
      payload.error = traceState.error;
    }
    return payload;
  }

  function summarizeEvent(event: JobTraceState['events'][number]): string {
    if (event.eventType === 'started' || event.eventType === 'completed') {
      const completed = event.progress?.completed;
      const total = event.progress?.total;
      if (typeof completed === 'number' && typeof total === 'number' && total > 0) {
        return `progress ${completed}/${total} - ${Math.round((completed / total) * 100)}%`;
      }
    }
    if (event.eventType === 'step' && typeof event.data?.step === 'string') {
      return event.data.step;
    }
    if (event.eventType === 'step_result') {
      const ok = event.data?.ok;
      const degraded = event.data?.degraded;
      if (typeof ok === 'boolean' || typeof degraded === 'boolean') {
        return `ok: ${String(ok)}, degraded: ${String(degraded)}`;
      }
    }
    const entries = Object.entries(event.data ?? {});
    if (entries.length === 0) {
      return 'no details';
    }
    return entries
      .slice(0, 3)
      .map(([key, value]) => `${key}: ${String(value)}`)
      .join(' - ');
  }

  async function copyTabContent() {
    if (!trace || activeTab === 'formatted' || typeof navigator === 'undefined') {
      return;
    }

    const payload =
      activeTab === 'request'
        ? formatJson(trace.request)
        : activeTab === 'response'
          ? formatJson(responsePayload(trace))
          : JSON.stringify(trace.events, null, 2);

    try {
      await navigator.clipboard.writeText(payload);
    } catch {
      // Ignore clipboard failures in restricted environments.
    }
  }
</script>

{#if trace}
  <div class="space-y-4">
    <div class="flex items-center justify-between gap-3">
      <div class="text-sm font-medium text-workspace-text-primary">{title}</div>
      <DesktopButton
        class={activeTab === 'formatted' ? 'invisible' : ''}
        size="compact"
        variant="secondary"
        disabled={activeTab === 'formatted'}
        aria-hidden={activeTab === 'formatted'}
        tabindex={activeTab === 'formatted' ? -1 : 0}
        onclick={() => void copyTabContent()}
      >
        Copy
      </DesktopButton>
    </div>

    <TabStrip
      {tabs}
      value={activeTab}
      ariaLabel="Trace detail views"
      onChange={(value) => {
        activeTab = value as Tab;
      }}
    />

    <div class="pt-3">
      {#if activeTab === 'formatted'}
        <div>
          <div class="overflow-hidden rounded-xl border border-workspace-border">
            {#each trace.events as event (event.sequence)}
              <div class="grid grid-cols-[2.25rem_minmax(0,10rem)_minmax(0,1fr)] items-center gap-4 border-b border-workspace-border px-3 py-2 last:border-b-0">
                <div class="text-sm text-workspace-text-muted">{event.sequence}</div>
                <div class={`text-sm font-medium ${
                  event.eventType === 'started'
                    ? 'text-message-info-text'
                    : event.eventType === 'step'
                      ? 'text-message-warning-text'
                      : event.eventType === 'step_result'
                        ? 'text-workspace-text-primary'
                        : event.eventType === 'completed'
                          ? 'text-message-success-text'
                          : 'text-workspace-text-primary'
                }`}>
                  {event.eventType}
                </div>
                <div class="truncate text-sm text-workspace-text-secondary">{summarizeEvent(event)}</div>
              </div>
            {/each}
          </div>
        </div>
      {:else if activeTab === 'request'}
        <div>
          <pre class="mt-2 overflow-x-auto whitespace-pre-wrap text-xs text-workspace-text-secondary">{formatJson(trace.request)}</pre>
        </div>
      {:else if activeTab === 'response'}
        <div>
          {#if trace.result}
            <pre class="mt-2 overflow-x-auto whitespace-pre-wrap text-xs text-workspace-text-secondary">{formatJson(trace.result)}</pre>
          {:else}
            <div class="mt-2 text-xs text-workspace-text-muted">No response data.</div>
          {/if}
          {#if trace.error}
            <InlineMessage class="mt-3" tone="warning">
              <div class="text-[11px] font-semibold uppercase tracking-[0.18em]">Error</div>
              <pre class="mt-2 overflow-x-auto whitespace-pre-wrap text-xs">{formatJson(trace.error)}</pre>
            </InlineMessage>
          {/if}
        </div>
      {:else}
        <div>
          <pre class="mt-2 overflow-x-auto whitespace-pre-wrap text-xs text-workspace-text-secondary">{JSON.stringify(trace.events, null, 2)}</pre>
        </div>
      {/if}
    </div>
  </div>
{:else}
  <Surface
    variant="cardControl"
    bordered
    radius="2xl"
    class="px-4 py-5 text-sm text-workspace-text-secondary"
  >
    {emptyMessage}
  </Surface>
{/if}
