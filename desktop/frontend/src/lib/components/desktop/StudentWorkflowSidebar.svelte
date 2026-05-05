<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import {
    FilterIcon,
    FilterMailIcon,
    SortByDown02Icon,
    SortByUp02Icon
  } from '@hugeicons/core-free-icons';
  import { HugeiconsIcon } from '@hugeicons/svelte';
  import type { BusyAction } from '$lib/stores/workspaceView';
  import { notifications } from '$lib/stores/notifications';
  import type { SidebarStudentEntry } from './student-workflow-helpers';
  import {
    DesktopButton,
    IconSelectField,
    InlineMessage,
    StatusBadge,
    TextField,
    type FeedbackTone
  } from './ui';

  export let entries: SidebarStudentEntry[] = [];
  export let selectedStudentRef: string | null = null;
  export let busyAction: BusyAction = null;
  export let rosterBusy = false;
  export let rosterActionLabel = 'Retry';
  export let rosterActionDisabled = false;
  export let rosterError: string | null = null;
  export let onselect: ((studentRef: string) => void | Promise<void>) | null = null;
  export let onbrowsepdf: (() => void | Promise<void>) | null = null;
  export let onrefreshroster: (() => void | Promise<void>) | null = null;
  export let onintakedrop: ((paths: string[]) => void | Promise<void>) | null = null;

  let dragActive = false;
  let sidebarEl: HTMLElement | null = null;
  type StatusFilter = 'all' | SidebarStudentEntry['statusGroup'];
  type SortMode = 'name' | 'status' | 'progress';
  type SortDirection = 'asc' | 'desc';

  let statusFilter: StatusFilter = 'all';
  let sortMode: SortMode = 'name';
  let sortDirection: SortDirection = 'asc';
  let searchTerm = '';

  const filterOptions: Array<{
    value: StatusFilter;
    label: string;
  }> = [
    { value: 'all', label: 'All' },
    { value: 'needsReview', label: 'Needs review' },
    { value: 'processing', label: 'Processing' },
    { value: 'ready', label: 'Ready' },
    { value: 'graded', label: 'Graded' },
    { value: 'failedStopped', label: 'Failed/stopped' },
    { value: 'noSubmission', label: 'No submission' }
  ];
  const sortOptions: Array<{ value: SortMode; label: string }> = [
    { value: 'name', label: 'Name' },
    { value: 'status', label: 'Status' },
    { value: 'progress', label: 'Progress' }
  ];
  const statusRank: Record<SidebarStudentEntry['statusGroup'], number> = {
    needsReview: 0,
    processing: 1,
    ready: 2,
    graded: 3,
    failedStopped: 4,
    noSubmission: 5
  };
  const statusPresentation: Record<
    SidebarStudentEntry['statusGroup'],
    { label: string; tone: FeedbackTone }
  > = {
    graded: { label: 'Graded', tone: 'success' },
    ready: { label: 'Ready', tone: 'warning' },
    processing: { label: 'Processing', tone: 'info' },
    needsReview: { label: 'Review', tone: 'warning' },
    failedStopped: { label: 'Blocked', tone: 'error' },
    noSubmission: { label: 'Missing', tone: 'muted' }
  };
  const controlIconSize = 20;
  const controlIconStrokeWidth = 2;
  const actionButtonClass =
    'inline-flex h-9 items-center justify-center whitespace-nowrap rounded-xl bg-surface-card-control px-3 text-sm font-medium text-workspace-text-primary transition-colors hover:bg-surface-card-hover disabled:cursor-not-allowed disabled:opacity-50';
  const controlIconButtonClass = `${actionButtonClass} w-10 border border-workspace-border px-0`;

  $: visibleEntries = entries
    .filter((entry) => {
      const normalizedSearch = searchTerm.trim().toLocaleLowerCase();
      return (
        (statusFilter === 'all' || entry.statusGroup === statusFilter) &&
        (normalizedSearch.length === 0 ||
          entry.displayName.toLocaleLowerCase().includes(normalizedSearch))
      );
    })
    .slice()
    .sort((left, right) => {
      const direction = sortDirection === 'asc' ? 1 : -1;
      let comparison = 0;
      if (sortMode === 'status') {
        comparison = statusRank[left.statusGroup] - statusRank[right.statusGroup];
      } else if (sortMode === 'progress') {
        comparison = left.progress - right.progress;
      } else {
        comparison = left.displayName.localeCompare(right.displayName);
      }
      if (comparison !== 0) {
        return comparison * direction;
      }
      return left.displayName.localeCompare(right.displayName);
    });

  function statusFilterLabel(value: StatusFilter): string {
    return filterOptions.find((option) => option.value === value)?.label ?? 'All';
  }

  function sortModeLabel(value: SortMode): string {
    return sortOptions.find((option) => option.value === value)?.label ?? 'Name';
  }

  onMount(() => {
    let stopNative: (() => void) | null = null;
    void (async () => {
      try {
        stopNative = await getCurrentWebview().onDragDropEvent((event) => {
          if (event.payload.type === 'over') {
            dragActive = true;
            return;
          }
          if (event.payload.type === 'drop') {
            dragActive = false;
            const { position, paths } = event.payload;
            const el = document.elementFromPoint(position.x, position.y);
            if (!el || !sidebarEl?.contains(el)) {
              return;
            }
            const filtered = (paths ?? []).filter(
              (p) => typeof p === 'string' && p.trim().length > 0
            );
            if (filtered.length === 0) {
              notifications.pushWarning(
                'Drop received, but no file paths were available.'
              );
              return;
            }
            void onintakedrop?.(filtered);
            return;
          }
          dragActive = false;
        });
      } catch {
        // If native drag-drop isn't enabled, fall back to the file picker.
      }
    })();

    return () => {
      stopNative?.();
      stopNative = null;
    };
  });
</script>

<aside
  bind:this={sidebarEl}
  class={`flex w-[19.5rem] shrink-0 flex-col border-r border-workspace-border ${dragActive ? 'bg-workspace-sidebar-active' : 'bg-surface-sidebar'}`}
  role="region"
  aria-label="Student roster and submission upload"
>
  <div class="flex items-center justify-between px-4 pt-4 pb-2">
    <div class="shell-eyebrow text-workspace-text-muted">
      Course Roster
    </div>
    {#if !rosterBusy && !rosterActionDisabled}
      <button
        type="button"
        class="text-xs font-medium text-workspace-text-muted transition-colors hover:text-workspace-text-primary"
        onclick={() => void onrefreshroster?.()}
      >
        {rosterActionLabel}
      </button>
    {/if}
  </div>

  <div class="mb-2 px-4">
    <DesktopButton
      variant="secondary"
      fullWidth
      disabled={busyAction !== null}
      onclick={() => void onbrowsepdf?.()}
    >
      Upload Submission
    </DesktopButton>
  </div>

  {#if rosterError}
    <InlineMessage class="mx-4 mb-2" tone="warning">
      {rosterError}
    </InlineMessage>
  {/if}

  <div class="border-y border-workspace-border px-2 py-3">
    <div class="flex min-w-0 items-center gap-2">
      <IconSelectField
        ariaLabel="Filter students by workflow status"
        menuLabel="Filter status"
        title={`Filter status: ${statusFilterLabel(statusFilter)}`}
        bind:value={statusFilter}
        options={filterOptions}
        icon={FilterIcon}
        iconSize={controlIconSize}
        iconStrokeWidth={controlIconStrokeWidth}
        triggerClass={controlIconButtonClass}
      />

      <TextField
        class="min-w-0 flex-1"
        controlClass="bg-surface-card-control shadow-none focus:bg-surface-card-hover"
        density="compact"
        type="search"
        placeholder="Search"
        aria-label="Search students"
        bind:value={searchTerm}
      />

      <div class="flex h-9 shrink-0 items-center overflow-visible rounded-xl border border-workspace-border bg-surface-card-control text-workspace-text-primary">
        <IconSelectField
          class="self-stretch"
          ariaLabel="Sort students"
          menuLabel="Sort students"
          title={`Sort students: ${sortModeLabel(sortMode)}`}
          bind:value={sortMode}
          options={sortOptions}
          icon={FilterMailIcon}
          iconSize={controlIconSize}
          iconStrokeWidth={controlIconStrokeWidth}
          align="right"
          menuClass="w-48"
          triggerClass="inline-flex h-full w-10 items-center justify-center rounded-l-xl transition-colors hover:bg-surface-card-hover"
        />

        <div class="h-5 w-px shrink-0 bg-workspace-border" aria-hidden="true"></div>

        <button
          type="button"
          class="inline-flex h-full w-10 items-center justify-center rounded-r-xl transition-colors hover:bg-surface-card-hover"
          aria-label={`Sort ${sortDirection === 'asc' ? 'descending' : 'ascending'}`}
          title={`Sort order: ${sortDirection === 'asc' ? 'Ascending' : 'Descending'}`}
          onclick={() => {
            sortDirection = sortDirection === 'asc' ? 'desc' : 'asc';
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
    </div>
  </div>

  <nav class="min-h-0 flex-1 overflow-y-auto">
    {#each visibleEntries as entry (entry.studentRef)}
      {@const isActive = entry.studentRef === selectedStudentRef}
      {@const status = statusPresentation[entry.statusGroup]}
      <div
        class={[
          'group relative flex w-full items-start px-4 py-2.5 transition-colors',
          isActive
            ? 'bg-workspace-sidebar-active text-workspace-text-primary'
            : 'text-workspace-text-secondary hover:bg-workspace-sidebar-hover'
        ]}
      >
        <div
          class={[
            'absolute inset-y-0 left-0 w-[3px] rounded-r',
            isActive ? 'bg-primary' : 'bg-transparent'
          ]}
        ></div>

        <button
          type="button"
          class="min-w-0 flex-1 text-left"
          onclick={() => void onselect?.(entry.studentRef)}
        >
          <div class="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] grid-rows-2 items-center gap-x-2">
            <div class="row-span-2 min-w-0">
              <span
                class={`block truncate text-sm font-medium ${isActive ? 'text-workspace-text-primary' : 'text-workspace-text-secondary'}`}
              >
                {entry.displayName}
              </span>
            </div>
            <StatusBadge
              class="row-span-2 min-h-0 shrink-0 justify-self-end whitespace-nowrap px-2 py-0 text-[11px] leading-5"
              title={entry.label}
              tone={status.tone}
            >
              {status.label}
            </StatusBadge>
          </div>
        </button>
      </div>
      <div class="h-px bg-workspace-border"></div>
    {/each}
  </nav>

</aside>
