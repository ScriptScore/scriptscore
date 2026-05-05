<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { tick } from 'svelte';
  import { HugeiconsIcon } from '@hugeicons/svelte';
  import {
    AccountSetting01Icon,
    FilterIcon,
    FilterMailIcon,
    SortByDown02Icon,
    SortByUp02Icon,
  } from '@hugeicons/core-free-icons';
  import type { ResultStudentRow, ResultsExportFormat } from '$lib/types';
  import type {
    ResultsScoreDisplayMode,
    ResultsUploadProgressStatus,
    ResultsSortDirection,
    ResultsSortKey,
    ResultsStatusFilter,
  } from '$lib/stores/resultsWorkspaceView';
  import {
    aggregateScorePercent,
    displayNameForStudent,
    formatPercentValue,
    statusFilterLabel,
    uploadStatusSortRank,
  } from './results-workspace-helpers';
  import {
    DesktopButton,
    DesktopPopover,
    ExportFormatDialog,
    IconSelectField,
    SegmentedControl,
    StatusBadge,
    TextField,
    ToggleRow
  } from './ui';

  export let rows: ResultStudentRow[] = [];
  export let studentDisplayNamesByRef: Record<string, string> = {};
  export let selectedStudentRef: string | null = null;
  export let selectedStudentRefs: string[] = [];
  export let sortKey: ResultsSortKey = 'name';
  export let sortDirection: ResultsSortDirection = 'asc';
  export let statusFilter: ResultsStatusFilter = 'all';
  export let searchTerm = '';
  export let scoreDisplayMode: ResultsScoreDisplayMode = 'percent';
  export let uploadProgressByStudentRef: Record<string, ResultsUploadProgressStatus> = {};
  export let currentAssignmentId: string | null = null;
  export let selectedUploadableRowsCount = 0;
  export let selectedExportableRowsCount = 0;
  export let lmsLinked = true;
  export let showExportActionForLms = false;
  export let readyFinalizeCount = 0;
  export let busy = false;
  export let onSelect: ((studentRef: string) => void) | null = null;
  export let onToggleSelection: ((studentRef: string) => void) | null = null;
  export let onToggleFilteredSelection: ((selected: boolean) => void) | null = null;
  export let onChangeSortKey: ((sortKey: ResultsSortKey) => void) | null = null;
  export let onToggleSortDirection: (() => void) | null = null;
  export let onChangeStatusFilter: ((statusFilter: ResultsStatusFilter) => void) | null = null;
  export let onChangeSearchTerm: ((searchTerm: string) => void) | null = null;
  export let onChangeScoreDisplayMode: ((mode: ResultsScoreDisplayMode) => void) | null = null;
  export let onChangeShowExportActionForLms: ((enabled: boolean) => void) | null = null;
  export let onFinalizeReady: (() => Promise<boolean | void>) | null = null;
  export let onRunUpload: (() => Promise<boolean | void>) | null = null;
  export let onRunExport: ((format: ResultsExportFormat) => Promise<boolean | void>) | null = null;

  let displayControlsOpen = false;
  let exportFormatDialogOpen = false;
  let filteredSelectionRef: HTMLInputElement | null = null;

  const controlIconSize = 20;
  const controlIconStrokeWidth = 2;
  const scoreDisplayOptions = [
    { value: 'percent', label: 'Percentage%' },
    { value: 'points', label: 'Points' }
  ];
  const linkedStatusFilterOptions: ResultsStatusFilter[] = [
    'all',
    'ready',
    'finalized',
    'uploaded',
  ];
  const localStatusFilterOptions: ResultsStatusFilter[] = ['all', 'ready'];
  const sortOptions: ResultsSortKey[] = ['name', 'score', 'upload_status'];

  $: statusFilterOptions = lmsLinked ? linkedStatusFilterOptions : localStatusFilterOptions;
  $: statusFilterSelectOptions = statusFilterOptions.map((option) => ({
    value: option,
    label: statusFilterLabel(option)
  }));
  $: sortSelectOptions = sortOptions.map((option) => ({
    value: option,
    label: sortLabel(option)
  }));

  function statusLabel(row: ResultStudentRow): string {
    const transientStatus = uploadProgressByStudentRef[row.studentRef];
    if (transientStatus === 'ready') return 'Validated';
    if (transientStatus === 'uploading') return 'Uploading';
    if (transientStatus === 'uploaded') return 'Uploaded';
    if (transientStatus === 'failed') return 'Upload failed';
    if (row.uploadFailed) return 'Upload failed';
    if (row.uploaded) return 'Uploaded';
    if (row.staleFinalization) return 'Stale';
    if (row.finalized) return 'Finalized';
    if (row.readyToFinalize) return 'Ready';
    return 'Blocked';
  }

  function statusPillTone(row: ResultStudentRow): 'success' | 'info' | 'warning' | 'error' | 'muted' {
    const transientStatus = uploadProgressByStudentRef[row.studentRef];
    if (transientStatus === 'ready') {
      return 'info';
    }
    if (transientStatus === 'uploading') {
      return 'info';
    }
    if (transientStatus === 'uploaded') {
      return 'success';
    }
    if (transientStatus === 'failed') {
      return 'error';
    }
    switch (uploadStatusSortRank(row)) {
      case 0:
        return 'error';
      case 1:
      case 2:
        return 'warning';
      case 3:
      case 4:
        return 'success';
      default:
        return 'muted';
    }
  }

  function sortLabel(sortValue: ResultsSortKey): string {
    switch (sortValue) {
      case 'score':
        return 'Score';
      case 'upload_status':
        return 'Status';
      case 'name':
      default:
        return 'Name';
    }
  }

  function rowScoreLabel(row: ResultStudentRow): string {
    if (!row.aggregateComplete) {
      return 'Pending';
    }
    if (scoreDisplayMode === 'percent') {
      return formatPercentValue(aggregateScorePercent(row), 1);
    }
    return row.aggregateTotal.toString();
  }

  $: filteredSelectedCount = rows.filter((row) => selectedStudentRefs.includes(row.studentRef)).length;
  $: allFilteredSelected = rows.length > 0 && filteredSelectedCount === rows.length;
  $: someFilteredSelected = filteredSelectedCount > 0 && filteredSelectedCount < rows.length;
  $: if (filteredSelectionRef) {
    filteredSelectionRef.indeterminate = someFilteredSelected;
  }

  async function chooseExportFormat(format: ResultsExportFormat) {
    exportFormatDialogOpen = false;
    await tick();
    void onRunExport?.(format);
  }
</script>

<aside
  class="flex w-[19.5rem] shrink-0 flex-col bg-surface-sidebar"
  role="region"
  aria-label="Student results list"
>
  <div class="px-4 pt-4 pb-3">
    <div class="flex items-center gap-2">
      {#if statusFilter === 'ready'}
        <DesktopButton
          class="min-w-0 flex-1"
          variant="primary"
          disabled={busy || readyFinalizeCount === 0}
          onclick={() => void onFinalizeReady?.()}
        >
          Finalize
        </DesktopButton>
      {:else if lmsLinked}
        {#if showExportActionForLms}
          <DesktopButton
            class="min-w-0 flex-1 px-3"
            variant="primary"
            disabled={busy || !currentAssignmentId || selectedUploadableRowsCount === 0}
            onclick={() => void onRunUpload?.()}
          >
            Upload
          </DesktopButton>
          <DesktopButton
            class="min-w-0 flex-1 px-3"
            variant="primary"
            disabled={busy || selectedExportableRowsCount === 0}
            onclick={() => (exportFormatDialogOpen = true)}
          >
            Export
          </DesktopButton>
        {:else}
          <DesktopButton
            class="min-w-0 flex-1"
            variant="primary"
            disabled={busy || !currentAssignmentId || selectedUploadableRowsCount === 0}
            onclick={() => void onRunUpload?.()}
          >
            Upload
          </DesktopButton>
        {/if}
      {:else}
        <DesktopButton
          class="min-w-0 flex-1"
          variant="primary"
          disabled={busy || selectedExportableRowsCount === 0}
          onclick={() => (exportFormatDialogOpen = true)}
        >
          Export
        </DesktopButton>
      {/if}
      <div class="shrink-0">
        <DesktopPopover
          bind:open={displayControlsOpen}
          triggerLabel="Student display settings"
          aria-label="Student display settings"
          align="end"
          triggerClass="inline-flex h-10 w-10 items-center justify-center rounded-xl border border-border-default bg-surface-card-control text-text-primary shadow-[var(--surface-shadow-button)] transition-colors hover:bg-surface-card-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring"
          panelClass="w-56 p-3"
        >
          <svelte:fragment slot="trigger">
            <HugeiconsIcon
              icon={AccountSetting01Icon}
              size={controlIconSize}
              strokeWidth={controlIconStrokeWidth}
              aria-hidden="true"
            />
          </svelte:fragment>
          <div class="text-[11px] font-semibold uppercase tracking-wide text-workspace-text-muted">
            Display
          </div>
          <SegmentedControl
            class="mt-2 w-full"
            options={scoreDisplayOptions}
            value={scoreDisplayMode}
            ariaLabel="Score display mode"
            onChange={(value) => onChangeScoreDisplayMode?.(value as ResultsScoreDisplayMode)}
          />
          {#if lmsLinked}
            <ToggleRow
              class="mt-3 rounded-lg px-1 py-2"
              title="Show Export action"
              description={null}
              checked={showExportActionForLms}
              onToggle={(enabled) => onChangeShowExportActionForLms?.(enabled)}
            />
          {/if}
        </DesktopPopover>
      </div>
    </div>

  </div>

  <div class="border-y border-workspace-border px-2 py-3">
    <div class="flex min-w-0 items-center gap-2">
      <IconSelectField
        ariaLabel="Filter students by result status"
        menuLabel="Filter status"
        title={`Filter status: ${statusFilterLabel(statusFilter)}`}
        value={statusFilter}
        options={statusFilterSelectOptions}
        icon={FilterIcon}
        iconSize={controlIconSize}
        iconStrokeWidth={controlIconStrokeWidth}
        menuClass="w-44"
        onChange={(value) => onChangeStatusFilter?.(value as ResultsStatusFilter)}
      />

      <TextField
        class="min-w-0 flex-1"
        density="compact"
        controlClass="bg-surface-card-control shadow-none focus:bg-surface-card-hover"
        type="search"
        placeholder="Search"
        aria-label="Search results students"
        value={searchTerm}
        oninput={(event: Event) => onChangeSearchTerm?.((event.currentTarget as HTMLInputElement).value)}
      />

      <div class="flex h-9 shrink-0 items-center overflow-visible rounded-xl border border-workspace-border bg-surface-card-control text-workspace-text-primary">
        <IconSelectField
          class="self-stretch"
          ariaLabel="Sort students"
          menuLabel="Sort students"
          title={`Sort students: ${sortLabel(sortKey)}`}
          value={sortKey}
          options={sortSelectOptions}
          icon={FilterMailIcon}
          iconSize={controlIconSize}
          iconStrokeWidth={controlIconStrokeWidth}
          align="right"
          menuClass="w-56"
          triggerClass="inline-flex h-full w-10 items-center justify-center rounded-l-xl transition-colors hover:bg-surface-card-hover"
          onChange={(value) => onChangeSortKey?.(value as ResultsSortKey)}
        />

        <div class="h-5 w-px shrink-0 bg-workspace-border" aria-hidden="true"></div>

        <button
          type="button"
          class="inline-flex h-full w-10 items-center justify-center rounded-r-xl transition-colors hover:bg-surface-card-hover"
          aria-label={`Sort ${sortDirection === 'asc' ? 'descending' : 'ascending'}`}
          title={`Sort order: ${sortDirection === 'asc' ? 'Ascending' : 'Descending'}`}
          onclick={() => onToggleSortDirection?.()}
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
    </div>

    <div class="mt-2 grid w-full grid-cols-[1rem_minmax(0,1fr)] items-center gap-x-2 px-3">
      <label
        class={`flex h-9 items-center justify-center ${
          rows.length === 0 ? 'cursor-not-allowed opacity-50' : ''
        }`}
        aria-label={
          allFilteredSelected ? 'Deselect all filtered students' : 'Select all filtered students'
        }
      >
        <input
          bind:this={filteredSelectionRef}
          class="size-4 accent-[var(--workspace-border-strong)]"
          type="checkbox"
          disabled={rows.length === 0}
          checked={allFilteredSelected}
          onchange={(event) =>
            onToggleFilteredSelection?.((event.currentTarget as HTMLInputElement).checked)}
        />
      </label>

      <div class="flex min-w-0 items-center">
        <div class="min-w-0 truncate text-xs font-semibold text-workspace-text-muted">
          {statusFilterLabel(statusFilter)}
        </div>
      </div>
    </div>
  </div>

  <nav class="min-h-0 flex-1 overflow-y-auto pb-3">
    {#if rows.length === 0}
      <div class="px-4 py-5 text-sm text-workspace-text-secondary">
        No graded submissions are ready for Results yet.
      </div>
    {:else}
      {#each rows as row (row.studentRef)}
        {@const isActive = row.studentRef === selectedStudentRef}
        <button
          type="button"
          class={[
            'mx-2 mb-2 flex w-[calc(100%-1rem)] flex-col rounded-2xl px-3 py-3 text-left transition-colors',
            isActive
              ? 'bg-workspace-sidebar-active text-workspace-text-primary'
              : 'bg-transparent text-workspace-text-secondary hover:bg-workspace-sidebar-hover'
          ]}
          aria-pressed={isActive}
          onclick={() => onSelect?.(row.studentRef)}
        >
          <div class="grid grid-cols-[1rem_minmax(0,1fr)] items-start gap-x-3">
            <input
              class="mt-0.5 size-4 shrink-0 accent-[var(--workspace-border-strong)]"
              type="checkbox"
              checked={selectedStudentRefs.includes(row.studentRef)}
              aria-label={`Select ${displayNameForStudent(row.studentRef, studentDisplayNamesByRef)}`}
              onclick={(event) => event.stopPropagation()}
              onchange={() => onToggleSelection?.(row.studentRef)}
            />

            <div class="grid min-w-0 flex-1 grid-cols-[minmax(0,1fr)_auto] items-center gap-x-2 gap-y-2">
              <div
                class={`min-w-0 truncate text-sm font-medium ${
                  isActive ? 'text-workspace-text-primary' : 'text-workspace-text-secondary'
                }`}
              >
                {displayNameForStudent(row.studentRef, studentDisplayNamesByRef)}
              </div>

              <StatusBadge
                class="min-h-0 shrink-0 whitespace-nowrap px-2 py-0 text-[11px] leading-5"
                tone={statusPillTone(row)}
              >
                {statusLabel(row)}
              </StatusBadge>

              <div class="h-2 min-w-0 overflow-hidden rounded-full bg-workspace-empty">
                  <div
                    class="h-full rounded-full bg-primary transition-[width]"
                    style:width={`${aggregateScorePercent(row)}%`}
                  ></div>
              </div>
              <span class="shrink-0 whitespace-nowrap text-xs font-semibold text-workspace-text-primary">
                {rowScoreLabel(row)}
              </span>

              <div class="col-span-2 flex items-center justify-end gap-2">
                {#if row.latestUploadError}
                  <span class="truncate text-[11px] text-message-error-text">
                    {row.latestUploadError}
                  </span>
                {/if}
              </div>
            </div>
          </div>
        </button>
      {/each}
    {/if}
  </nav>
</aside>

<ExportFormatDialog
  open={exportFormatDialogOpen}
  {busy}
  onCancel={() => (exportFormatDialogOpen = false)}
  onChoose={chooseExportFormat}
/>
