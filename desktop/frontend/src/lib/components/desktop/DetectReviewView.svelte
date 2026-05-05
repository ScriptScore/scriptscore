<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { toDesktopAssetUrl } from '$lib/desktop';
  import type {
    StudentIntakeSummary,
    StudentWorkflowDetectReviewRow,
    StudentWorkflowDetectReviewResolutionInput,
    StudentWorkflowDetectRegion,
    StudentWorkflowSubmission
  } from '$lib/types';
  import { DesktopButton, StatusBadge } from './ui';
  import ImageRegionEditor from './ui/ImageRegionEditor.svelte';
  import type { ImageRegion } from './ui/imageRegionEditor';
  import { detectReviewRegionPresentation } from './detectReviewRegionPresentation';

  export let intakeItem: StudentIntakeSummary | null;
  export let submission: StudentWorkflowSubmission;
  export let displayName = '';
  export let busyAction: string | null = null;
  export let saving = false;
  export let onconfirm:
    | ((resolutions: StudentWorkflowDetectReviewResolutionInput[]) => Promise<void>)
    | null = null;
  export let ondelete: (() => void) | null = null;
  export let deleteDisabled = false;
  export let onback: (() => void) | null = null;

  let selectedIndex = 0;
  let localRows: StudentWorkflowDetectReviewRow[] = [];
  let syncKey = '';

  $: pendingRows = submission.detectReview?.pendingRows ?? [];
  $: {
    const nextKey = `${submission.studentRef}:${JSON.stringify(pendingRows)}`;
    if (nextKey !== syncKey) {
      syncKey = nextKey;
      localRows = pendingRows.map((row) => ({
        ...row,
        templateRegion: { ...row.templateRegion },
        resolvedRegion: row.resolvedRegion ? { ...row.resolvedRegion } : null,
        warnings: [...row.warnings]
      }));
      selectedIndex = Math.min(selectedIndex, Math.max(0, localRows.length - 1));
    }
  }
  $: selectedRow = localRows[selectedIndex] ?? null;
  $: selectedRegion = selectedRow
    ? selectedRow.resolvedRegion ?? selectedRow.templateRegion
    : null;
  $: selectedRegions = selectedRow && selectedRegion
    ? [regionToImageRegion(selectedRow, selectedRegion)]
    : [];
  $: allResolved =
    localRows.length > 0 && localRows.every((row) => isValidRegion(reviewRegion(row)));
  $: controlsDisabled = saving || (busyAction !== null && busyAction !== 'studentWorkflow');
  $: confirmDisabled = controlsDisabled || !allResolved || onconfirm === null;
  $: displayTitle = displayName || intakeItem?.localDisplayName || submission.studentRef;

  function regionToImageRegion(
    row: StudentWorkflowDetectReviewRow,
    region: StudentWorkflowDetectRegion
  ): ImageRegion {
    return {
      regionId: `${row.questionId}:${row.pageNumber}`,
      pageNumber: row.pageNumber,
      x: region.x,
      y: region.y,
      width: region.width,
      height: region.height,
      kind: row.resolvedRegion ? 'resolved' : 'template'
    };
  }

  function imageRegionToDetectRegion(region: ImageRegion): StudentWorkflowDetectRegion {
    return {
      x: Math.round(region.x),
      y: Math.round(region.y),
      width: Math.round(region.width),
      height: Math.round(region.height),
      units: 'rendered_page_pixels'
    };
  }

  function isValidRegion(region: StudentWorkflowDetectRegion | null | undefined): boolean {
    return (
      !!region &&
      region.units === 'rendered_page_pixels' &&
      Number.isFinite(region.x) &&
      Number.isFinite(region.y) &&
      Number.isFinite(region.width) &&
      Number.isFinite(region.height) &&
      region.x >= 0 &&
      region.y >= 0 &&
      region.width > 0 &&
      region.height > 0
    );
  }

  function reviewRegion(row: StudentWorkflowDetectReviewRow): StudentWorkflowDetectRegion | null {
    return row.resolvedRegion ?? row.templateRegion;
  }

  function handleRegionsChange(regions: ImageRegion[]): void {
    const next = regions[0] ?? null;
    localRows = localRows.map((row, index) =>
      index === selectedIndex
        ? {
            ...row,
            resolvedRegion: next ? imageRegionToDetectRegion(next) : null
          }
        : row
    );
  }

  function buildResolutions(): StudentWorkflowDetectReviewResolutionInput[] {
    return localRows.map((row) => ({
      questionId: row.questionId,
      pageNumber: row.pageNumber,
      region: reviewRegion(row)!
    }));
  }

  async function continueWithResolvedRegions(): Promise<void> {
    if (confirmDisabled || !onconfirm) {
      return;
    }
    await onconfirm(buildResolutions());
  }
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <div class="flex items-center justify-between gap-4 border-b border-workspace-border pb-3">
    <div class="min-w-0">
      <h2 class="truncate text-xl font-semibold text-workspace-text-primary">Region review</h2>
    </div>
    <div class="flex shrink-0 items-center gap-2">
      {#if ondelete}
        <DesktopButton variant="ghost" disabled={deleteDisabled} onclick={ondelete}>
          Delete
        </DesktopButton>
      {/if}
      <DesktopButton variant="ghost" onclick={() => onback?.()}>Back</DesktopButton>
      <DesktopButton
        variant="primary"
        disabled={confirmDisabled}
        onclick={() => void continueWithResolvedRegions()}
      >
        {saving ? 'Saving…' : 'Confirm Regions'}
      </DesktopButton>
    </div>
  </div>

  {#if localRows.length === 0}
    <div class="mt-6 rounded-2xl border border-workspace-border bg-surface-card-subtle p-5 text-sm text-workspace-text-secondary">
      No regions are pending review.
    </div>
  {:else}
    <div class="mt-4 grid min-h-0 flex-1 grid-cols-[18rem_minmax(0,1fr)] gap-4">
      <aside class="min-h-0 overflow-y-auto rounded-2xl border border-workspace-border bg-surface-card-subtle p-2">
        {#each localRows as row, index (`${row.questionId}:${row.pageNumber}`)}
          <button
            type="button"
            class={`mb-2 w-full rounded-xl border px-3 py-2 text-left text-sm transition-colors ${
              index === selectedIndex
                ? 'border-message-warning-border bg-message-warning-bg'
                : 'border-workspace-border bg-surface-card-control hover:bg-interaction-hover'
            }`}
            onclick={() => {
              selectedIndex = index;
            }}
          >
            <div class="flex items-center justify-between gap-2">
              <div class="min-w-0">
                <span class="font-medium text-workspace-text-primary">{row.questionId}</span>
                <span class="ml-2 text-xs text-workspace-text-muted">Page {row.pageNumber}</span>
              </div>
              <StatusBadge tone={isValidRegion(reviewRegion(row)) ? 'success' : 'warning'}>
                {isValidRegion(row.resolvedRegion) ? 'resolved' : 'seeded'}
              </StatusBadge>
            </div>
            {#if row.warnings.length > 0}
              <div
                class="mt-2 line-clamp-3 text-xs leading-5 text-message-warning-text"
                title={row.warnings[0]?.message}
              >
                {row.warnings[0]?.message}
              </div>
            {/if}
          </button>
        {/each}
      </aside>

      <section class="min-h-0">
        {#if selectedRow}
          <ImageRegionEditor
            imageSrc={toDesktopAssetUrl(selectedRow.sourcePageImagePath)}
            imageAlt={`Page ${selectedRow.pageNumber} for ${displayTitle}`}
            pageNumber={selectedRow.pageNumber}
            regions={selectedRegions}
            maxRegions={1}
            disabled={controlsDisabled}
            ariaLabel="Question region editor"
            regionPresentation={detectReviewRegionPresentation}
            onRegionsChange={handleRegionsChange}
            containerMinHeightClass="min-h-[34rem]"
          />
        {/if}
      </section>
    </div>
  {/if}
</div>
