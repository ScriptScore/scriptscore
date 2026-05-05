<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { FilterMailIcon, SortByDown02Icon, SortByUp02Icon } from '@hugeicons/core-free-icons';
  import { HugeiconsIcon } from '@hugeicons/svelte';
  import type { JobTraceState, JobTraceSummary } from '$lib/types';
  import TabbedTraceViewer from './TabbedTraceViewer.svelte';
  import { DesktopButton, IconSelectField, InlineMessage, SelectField, Surface, TextField } from './ui';

  type TraceSortKey = 'timestamp' | 'duration' | 'command' | 'state';

  export let open = false;
  export let loadSummaries: (() => Promise<JobTraceSummary[]>) | null = null;
  export let loadTrace: ((jobId: string) => Promise<JobTraceState | null>) | null = null;
  export let onClose: (() => void) | null = null;
  export let initialJobIds: string[] = [];
  export let initialStateFilter = '';

  let summaries: JobTraceSummary[] = [];
  let selectedJobId: string | null = null;
  let selectedTrace: JobTraceState | null = null;
  let search = '';
  let commandFilters: string[] = [];
  let stateFilter = '';
  let activeJobFilters: string[] = [];
  let sortKey: TraceSortKey = 'timestamp';
  let sortDirection: 'desc' | 'asc' = 'desc';
  let loadingSummaries = false;
  let loadingTrace = false;
  let errorMessage: string | null = null;
  let traceLoadToken = 0;
  let appliedInitialFilterKey = '';
  const controlIconSize = 20;
  const controlIconStrokeWidth = 2;
  const sortOptions: Array<{ value: TraceSortKey; label: string }> = [
    { value: 'timestamp', label: 'Timestamp' },
    { value: 'duration', label: 'Duration' },
    { value: 'command', label: 'Command' },
    { value: 'state', label: 'State' }
  ];

  $: commandOptions = Array.from(new Set(summaries.map((trace) => trace.commandName))).sort((a, b) =>
    a.localeCompare(b)
  );
  $: stateOptions = Array.from(new Set(summaries.map((trace) => trace.state))).sort((a, b) =>
    a.localeCompare(b)
  );
  $: filteredSummaries = sortSummaries(
    filterSummaries(summaries, search, commandFilters, stateFilter, activeJobFilters),
    sortKey,
    sortDirection
  );
  $: visibleSelectedSummary =
    filteredSummaries.find((trace) => trace.jobId === selectedJobId) ?? filteredSummaries[0] ?? null;

  $: if (commandFilters.some((command) => !commandOptions.includes(command))) {
    commandFilters = commandFilters.filter((command) => commandOptions.includes(command));
  }

  $: if (summaries.length > 0 && !stateOptions.includes(stateFilter)) {
    stateFilter = '';
  }

  $: if (open) {
    const initialFilterKey = `${initialStateFilter}|${initialJobIds.join(',')}`;
    if (initialFilterKey !== appliedInitialFilterKey) {
      activeJobFilters = [...initialJobIds];
      stateFilter = initialStateFilter;
      appliedInitialFilterKey = initialFilterKey;
    }
  }

  if (typeof document !== 'undefined') {
    document.addEventListener('keydown', handleDocumentKeyDown, true);
  }

  onMount(() => {
    if (open) {
      void refreshSummaries();
    }
  });

  onDestroy(() => {
    if (typeof document !== 'undefined') {
      document.removeEventListener('keydown', handleDocumentKeyDown, true);
    }
  });

  function close() {
    onClose?.();
  }

  function handleDocumentKeyDown(event: KeyboardEvent) {
    if (open && event.key === 'Escape') {
      event.preventDefault();
      close();
    }
  }

  function normalized(value: string | null | undefined): string {
    return (value ?? '').toLocaleLowerCase();
  }

  function filterSummaries(
    rows: JobTraceSummary[],
    searchText: string,
    commandValues: string[],
    stateValue: string,
    jobIds: string[]
  ): JobTraceSummary[] {
    return rows.filter((trace) =>
      traceMatchesFilters(trace, searchText, commandValues, stateValue, jobIds)
    );
  }

  function sortLabel(value: TraceSortKey): string {
    return sortOptions.find((option) => option.value === value)?.label ?? 'Timestamp';
  }

  function timestampMs(value: string | null): number | null {
    if (!value) {
      return null;
    }
    const trimmed = value.trim();
    if (/^\d+$/.test(trimmed)) {
      const numeric = Number.parseInt(trimmed, 10);
      if (!Number.isFinite(numeric)) {
        return null;
      }
      return trimmed.length <= 10 ? numeric * 1000 : numeric;
    }
    const parsed = Date.parse(trimmed);
    return Number.isNaN(parsed) ? null : parsed;
  }

  function durationMs(trace: JobTraceSummary): number | null {
    const started = timestampMs(trace.startedAt);
    const finished = timestampMs(trace.finishedAt);
    if (started === null || finished === null) {
      return null;
    }
    return Math.max(0, finished - started);
  }

  function compareSummaries(left: JobTraceSummary, right: JobTraceSummary, key: TraceSortKey): number {
    if (key === 'timestamp') {
      return (timestampMs(left.submittedAt) ?? 0) - (timestampMs(right.submittedAt) ?? 0);
    }
    if (key === 'duration') {
      return (durationMs(left) ?? -1) - (durationMs(right) ?? -1);
    }
    if (key === 'command') {
      return left.commandName.localeCompare(right.commandName);
    }
    return left.state.localeCompare(right.state);
  }

  function sortSummaries(
    rows: JobTraceSummary[],
    key: TraceSortKey,
    directionValue: 'desc' | 'asc'
  ): JobTraceSummary[] {
    return [...rows].sort((left, right) => {
      const comparison = compareSummaries(left, right, key);
      if (comparison !== 0) {
        return directionValue === 'asc' ? comparison : -comparison;
      }
      return directionValue === 'asc'
        ? left.jobId.localeCompare(right.jobId)
        : right.jobId.localeCompare(left.jobId);
    });
  }

  function currentVisibleSummaries(): JobTraceSummary[] {
    return sortSummaries(
      filterSummaries(summaries, search, commandFilters, stateFilter, activeJobFilters),
      sortKey,
      sortDirection
    );
  }

  function selectFirstVisibleTrace() {
    const next = currentVisibleSummaries()[0] ?? null;
    selectedJobId = next?.jobId ?? null;
    if (next) {
      void loadSelectedTrace(next.jobId);
    } else {
      clearSelectedTrace();
    }
  }

  function traceMatchesFilters(
    trace: JobTraceSummary,
    searchText: string,
    commandValues: string[],
    stateValue: string,
    jobIds: string[]
  ): boolean {
    if (jobIds.length > 0 && !jobIds.includes(trace.jobId)) {
      return false;
    }
    if (commandValues.length > 0 && !commandValues.includes(trace.commandName)) {
      return false;
    }
    if (stateValue && trace.state !== stateValue) {
      return false;
    }
    const query = normalized(searchText.trim());
    if (!query) {
      return true;
    }
    return [
      trace.jobId,
      trace.commandName,
      trace.state,
      trace.submittedAt,
      trace.startedAt,
      trace.finishedAt,
      ...(trace.studentRefs ?? []),
      formatTimestamp(trace.submittedAt),
      formatTimestamp(trace.startedAt),
      formatTimestamp(trace.finishedAt)
    ]
      .map(normalized)
      .some((value) => value.includes(query));
  }

  function formatTimestamp(value: string | null): string {
    if (!value) {
      return 'Not recorded';
    }
    const parsed = timestampMs(value);
    if (parsed === null) {
      return value;
    }
    return new Intl.DateTimeFormat(undefined, {
      month: 'short',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit'
    }).format(new Date(parsed));
  }

  function durationLabel(trace: JobTraceSummary): string {
    if (!trace.startedAt || !trace.finishedAt) {
      return 'Duration pending';
    }
    const ms = durationMs(trace);
    if (ms === null) {
      return 'Duration unavailable';
    }
    if (ms < 1000) {
      return `${ms} ms`;
    }
    return `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)} s`;
  }

  async function refreshSummaries() {
    if (!loadSummaries) {
      summaries = [];
      clearSelectedTrace();
      selectedJobId = null;
      return;
    }
    loadingSummaries = true;
    errorMessage = null;
    try {
      const next = await loadSummaries();
      summaries = next;
      const previous = selectedJobId;
      const visibleRows = sortSummaries(
        filterSummaries(next, search, commandFilters, stateFilter, activeJobFilters),
        sortKey,
        sortDirection
      );
      const nextSelectedJobId =
        previous && visibleRows.some((trace) => trace.jobId === previous)
          ? previous
          : visibleRows[0]?.jobId ?? null;
      selectedJobId = nextSelectedJobId;
      if (nextSelectedJobId) {
        await loadSelectedTrace(nextSelectedJobId);
      } else {
        clearSelectedTrace();
      }
    } catch (error) {
      summaries = [];
      clearSelectedTrace();
      selectedJobId = null;
      errorMessage = String(error);
    } finally {
      loadingSummaries = false;
    }
  }

  async function loadSelectedTrace(jobId: string) {
    if (!loadTrace) {
      clearSelectedTrace();
      return;
    }
    const token = ++traceLoadToken;
    loadingTrace = true;
    errorMessage = null;
    try {
      const trace = await loadTrace(jobId);
      if (token === traceLoadToken) {
        selectedTrace = trace;
      }
    } catch (error) {
      if (token === traceLoadToken) {
        selectedTrace = null;
        errorMessage = String(error);
      }
    } finally {
      if (token === traceLoadToken) {
        loadingTrace = false;
      }
    }
  }

  function clearSelectedTrace() {
    traceLoadToken += 1;
    selectedTrace = null;
    loadingTrace = false;
  }
</script>

{#if open}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-overlay-scrim px-5 py-6">
    <Surface
      class="flex h-full max-h-[48rem] w-full max-w-7xl flex-col overflow-hidden border border-border-default shadow-[var(--surface-shadow-strong)]"
      variant="shell"
      radius="2xl"
      role="dialog"
      aria-modal="true"
      aria-labelledby="trace-history-title"
    >
      <header class="flex items-center justify-between gap-4 px-5 py-4">
        <div class="min-w-0">
          <div id="trace-history-title" class="text-sm font-semibold text-text-primary">
            Trace history
          </div>
          <div class="mt-1 text-sm text-text-secondary">
            Current-project worker runs and persisted event payloads.
          </div>
        </div>
        <DesktopButton size="compact" onclick={close}>Close</DesktopButton>
      </header>

      <div class="flex flex-wrap items-center gap-2 border-b border-border-default px-5 py-4">
        <TextField
          class="min-w-[16rem] flex-1"
          controlClass="bg-surface-card-control shadow-none focus:bg-surface-card-hover"
          density="compact"
          type="search"
          placeholder="Search job, command, state, student, or time"
          aria-label="Search traces"
          value={search}
          oninput={(event: Event) => {
            search = (event.currentTarget as HTMLInputElement).value;
            selectFirstVisibleTrace();
          }}
        />
        <SelectField
          class="w-72"
          ariaLabel="Command filter"
          density="compact"
          multiple
          values={commandFilters}
          allowEmpty
          emptyLabel="All commands"
          emptyValue=""
          options={commandOptions.map((command) => ({ value: command, label: command }))}
          optionListClass="max-h-72 overflow-y-auto"
          onChangeValues={(values) => {
            commandFilters = values;
            selectFirstVisibleTrace();
          }}
        />
        <SelectField
          class="w-40"
          ariaLabel="State filter"
          density="compact"
          value={stateFilter}
          allowEmpty
          emptyLabel="All states"
          emptyValue=""
          options={stateOptions.map((state) => ({ value: state, label: state }))}
          onChange={(value) => {
            stateFilter = value;
            selectFirstVisibleTrace();
          }}
        />
        <div class="flex h-9 w-20 shrink-0 items-center overflow-visible rounded-xl border border-workspace-border bg-surface-card-control text-workspace-text-primary">
          <IconSelectField
            class="self-stretch"
            ariaLabel="Sort traces"
            menuLabel="Sort traces"
            title={`Sort traces: ${sortLabel(sortKey)}`}
            value={sortKey}
            options={sortOptions}
            icon={FilterMailIcon}
            iconSize={controlIconSize}
            iconStrokeWidth={controlIconStrokeWidth}
            align="right"
            menuClass="w-44"
            triggerClass="inline-flex h-full w-10 shrink-0 items-center justify-center rounded-l-xl transition-colors hover:bg-surface-card-hover"
            onChange={(value) => {
              sortKey = value as TraceSortKey;
              selectFirstVisibleTrace();
            }}
          />
          <div class="h-5 w-px shrink-0 bg-workspace-border" aria-hidden="true"></div>
          <button
            type="button"
            class="inline-flex h-full w-10 shrink-0 items-center justify-center rounded-r-xl transition-colors hover:bg-surface-card-hover"
            aria-label={`Sort ${sortDirection === 'asc' ? 'descending' : 'ascending'}`}
            title={`Sort order: ${sortDirection === 'asc' ? 'Ascending' : 'Descending'}`}
            onclick={() => {
              sortDirection = sortDirection === 'asc' ? 'desc' : 'asc';
              selectFirstVisibleTrace();
            }}
          >
            {#key sortDirection}
              <HugeiconsIcon
                icon={sortDirection === 'asc' ? SortByUp02Icon : SortByDown02Icon}
                size={controlIconSize}
                strokeWidth={controlIconStrokeWidth}
                aria-hidden="true"
              />
            {/key}
          </button>
        </div>
        <DesktopButton
          size="compact"
          disabled={loadingSummaries}
          onclick={() => void refreshSummaries()}
        >
          Refresh
        </DesktopButton>
        {#if activeJobFilters.length > 0}
          <div class="flex h-9 items-center gap-2 rounded-xl border border-border-default bg-surface-card-control px-3 text-sm text-text-secondary">
            <span>Active jobs</span>
            <DesktopButton
              size="compact"
              variant="ghost"
              class="h-7 px-2 text-xs"
              onclick={() => {
                activeJobFilters = [];
                stateFilter = '';
                selectFirstVisibleTrace();
              }}
            >
              Clear
            </DesktopButton>
          </div>
        {/if}
      </div>

      {#if errorMessage}
        <div class="px-5 pt-4">
          <InlineMessage tone="error" message={errorMessage} />
        </div>
      {/if}

      <div class="grid min-h-0 flex-1 grid-cols-1 md:grid-cols-[18rem_minmax(0,1fr)]">
        <aside class="min-h-0 border-b border-border-default md:border-b-0 md:border-r">
          <div class="flex h-full min-h-0 flex-col">
            <div class="border-b border-border-default px-4 py-3 text-xs font-semibold uppercase tracking-[0.18em] text-text-muted">
              {filteredSummaries.length} trace{filteredSummaries.length === 1 ? '' : 's'}
            </div>
            <div class="min-h-0 flex-1 overflow-y-auto p-3">
              {#if loadingSummaries}
                <div class="px-2 py-4 text-sm text-text-secondary">Loading traces...</div>
              {:else if summaries.length === 0}
                <div class="px-2 py-4 text-sm text-text-secondary">
                  No project traces are available yet.
                </div>
              {:else if filteredSummaries.length === 0}
                <div class="px-2 py-4 text-sm text-text-secondary">
                  No traces match the current search and filters.
                </div>
              {:else}
                <div class="space-y-1.5">
                  {#each filteredSummaries as trace (trace.jobId)}
                    <button
                      type="button"
                      class={[
                        'w-full rounded-lg border px-2.5 py-2 text-left transition-colors',
                        visibleSelectedSummary?.jobId === trace.jobId
                          ? 'border-border-focus bg-surface-card-control text-text-primary'
                          : 'border-border-default bg-surface-panel text-text-secondary hover:border-border-strong hover:text-text-primary'
                      ]}
                      aria-pressed={visibleSelectedSummary?.jobId === trace.jobId}
                      onclick={() => {
                        selectedJobId = trace.jobId;
                        void loadSelectedTrace(trace.jobId);
                      }}
                    >
                      <div class="flex items-start justify-between gap-2">
                        <div class="min-w-0">
                          <div class="truncate text-sm font-semibold">{trace.commandName}</div>
                        </div>
                        <span class="shrink-0 rounded-md border border-border-default px-2 py-0.5 text-xs text-text-secondary">
                          {trace.state}
                        </span>
                      </div>
                      <div class="mt-1 truncate text-xs text-text-muted">
                        {formatTimestamp(trace.submittedAt)} - {durationLabel(trace)} - {trace.eventCount} event{trace.eventCount === 1 ? '' : 's'}
                      </div>
                      {#if trace.studentRefs && trace.studentRefs.length > 0}
                        <div class="mt-1 truncate text-xs text-text-muted">
                          Students: {trace.studentRefs.slice(0, 3).join(', ')}{trace.studentRefs.length > 3 ? ` +${trace.studentRefs.length - 3}` : ''}
                        </div>
                      {/if}
                    </button>
                  {/each}
                </div>
              {/if}
            </div>
          </div>
        </aside>

        <section class="min-h-0 overflow-y-auto px-5 py-4">
          {#if visibleSelectedSummary}
            <div class="mb-4 flex flex-wrap items-center justify-between gap-3">
              <div class="min-w-0">
                <div class="text-sm font-semibold text-text-primary">{visibleSelectedSummary.commandName}</div>
                <div class="mt-1 text-xs text-text-muted">
                  {visibleSelectedSummary.state} - {formatTimestamp(visibleSelectedSummary.submittedAt)} - {durationLabel(visibleSelectedSummary)} - {visibleSelectedSummary.eventCount} event{visibleSelectedSummary.eventCount === 1 ? '' : 's'}
                </div>
                <div class="mt-1 break-all font-mono text-xs text-text-muted">
                  Job ID: {visibleSelectedSummary.jobId}
                </div>
                {#if visibleSelectedSummary.studentRefs && visibleSelectedSummary.studentRefs.length > 0}
                  <div class="mt-1 break-all text-xs text-text-muted">
                    Students: {visibleSelectedSummary.studentRefs.join(', ')}
                  </div>
                {/if}
              </div>
              {#if loadingTrace}
                <div class="text-sm text-text-secondary">Loading details...</div>
              {/if}
            </div>
          {/if}
          <TabbedTraceViewer trace={selectedTrace} title="Trace Details" />
        </section>
      </div>
    </Surface>
  </div>
{/if}
