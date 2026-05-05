<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { toDesktopAssetUrl } from '$lib/desktop';
  import { Delete02Icon } from '@hugeicons/core-free-icons';
  import { HugeiconsIcon } from '@hugeicons/svelte';
  import type { BusyAction } from '$lib/stores/workspaceView';
  import type {
    StudentIntakeSummary,
    StudentWorkflowSubmission,
    StudentWorkflowAlignmentPage,
    StudentWorkflowTransform,
    TemplatePageArtifactSummary
  } from '$lib/types';
  import AlignmentOverlayCanvas from '$lib/components/desktop/AlignmentOverlayCanvas.svelte';
  import AlignmentTransformUndoIcon from '$lib/components/desktop/AlignmentTransformUndoIcon.svelte';
  import { ConfirmDialog, DesktopButton, IconButton, StatusBadge } from './ui';

  export let intakeItem: StudentIntakeSummary | null;
  export let submission: StudentWorkflowSubmission;
  export let templatePreviewArtifacts: TemplatePageArtifactSummary[];
  export let displayName: string;
  export let busyAction: BusyAction = null;
  export let saving = false;
  export let onconfirm: (pages: StudentWorkflowAlignmentPage[]) => Promise<void>;
  export let ondelete: (() => void) | null = null;
  export let deleteDisabled = false;
  export let onback: () => void;

  const minScale = 0.05;
  const maxScale = 5;
  const mediumConfidenceMin = 0.55;
  const highConfidenceMin = 0.9;
  const confidenceBandOrder: AlignmentConfidenceBand[] = ['low', 'medium', 'high'];

  type AlignmentConfidenceBand = 'low' | 'medium' | 'high';
  type AlignmentTabGroup = {
    band: AlignmentConfidenceBand;
    pages: StudentWorkflowAlignmentPage[];
  };

  let alignmentPageNumber = 1;
  let alignmentDraftPages: StudentWorkflowAlignmentPage[] = [];
  let originalAlignmentPages: StudentWorkflowAlignmentPage[] = [];
  let alignmentDraftKey: string | null = null;
  let reviewedPageNumbers = new Set<number>();
  let hoveredPageNumber: number | null = null;
  let confidenceTabGroups: AlignmentTabGroup[] = [];
  let pendingReuseTransform:
    | {
        pageNumber: number;
        band: AlignmentConfidenceBand;
      }
    | null = null;
  let templateTintColor = '#2563eb';
  let submissionOpacity = 0.6;

  $: selectedAlignmentPage =
    alignmentDraftPages.find((page) => page.pageNumber === alignmentPageNumber) ??
    alignmentDraftPages[0] ??
    null;
  $: selectedTemplatePath =
    templatePreviewArtifacts.find((page) => page.pageNumber === alignmentPageNumber)?.imagePath ??
    '';
  $: selectedSubmissionPath = intakeItem?.examPagePaths?.[alignmentPageNumber - 1] ?? '';
  $: selectedTemplateUrl = selectedTemplatePath ? toDesktopAssetUrl(selectedTemplatePath) : '';
  $: selectedSubmissionUrl = selectedSubmissionPath ? toDesktopAssetUrl(selectedSubmissionPath) : '';
  $: selectedOriginalAlignmentPage = !selectedAlignmentPage
    ? null
    : (originalAlignmentPages.find((p) => p.pageNumber === selectedAlignmentPage.pageNumber) ??
      null);
  $: alignmentConfidenceLabel = selectedAlignmentPage
    ? selectedAlignmentPage.reviewExempt
      ? 'Review exempt'
      : confidenceLabel(selectedAlignmentPage.confidence)
    : 'Alignment review';
  $: lowAlignmentPages = pagesForConfidenceBand('low');
  $: mediumAlignmentPages = pagesForConfidenceBand('medium');
  $: highAlignmentPages = pagesForConfidenceBand('high');
  $: confidenceTabGroups = [
    { band: 'low', pages: lowAlignmentPages },
    { band: 'medium', pages: mediumAlignmentPages },
    { band: 'high', pages: highAlignmentPages }
  ].filter((group) => group.pages.length > 0) as AlignmentTabGroup[];
  $: allAlignmentPagesReady =
    alignmentDraftPages.length > 0 &&
    alignmentDraftPages.every((page) => pageHasCheck(page, reviewedPageNumbers));
  $: controlsDisabled = saving || (busyAction !== null && busyAction !== 'studentWorkflow');

  $: {
    const alignmentPagesKey = (submission.alignmentPages ?? [])
      .map((page) =>
        [
          page.pageNumber,
          page.confidence ?? 'na',
          page.lowConfidence,
          page.reviewExempt === true,
          page.reviewExemptReason ?? '',
          page.questionCount ?? 0
        ].join(':')
      )
      .join('|');
    const key = `${submission.studentRef}:${submission.latestJobId ?? ''}:${alignmentPagesKey}`;
    if (key !== alignmentDraftKey) {
      originalAlignmentPages = structuredClone(submission.alignmentPages ?? []);
      alignmentDraftPages = structuredClone(originalAlignmentPages);
      reviewedPageNumbers = new Set();
      pendingReuseTransform = null;
      alignmentPageNumber =
        firstActionablePage(reviewedPageNumbers)?.pageNumber ??
        alignmentDraftPages[0]?.pageNumber ??
        1;
      alignmentDraftKey = key;
    }
  }

  function updatePageTransform(pageNumber: number, transform: StudentWorkflowTransform) {
    alignmentDraftPages = alignmentDraftPages.map((page) =>
      page.pageNumber === pageNumber ? { ...page, transform } : page
    );
  }

  function updateAlignmentTransform(
    pageNumber: number,
    field: keyof StudentWorkflowAlignmentPage['transform'],
    value: number
  ) {
    const page = alignmentDraftPages.find((candidate) => candidate.pageNumber === pageNumber);
    if (!page) return;
    const next = sanitizeTransformField(field, value, page.transform[field]);
    updatePageTransform(pageNumber, {
      ...page.transform,
      [field]: next
    });
  }

  function restoreAutoAlignTransform() {
    if (!selectedAlignmentPage) return;
    const originalPage = originalAlignmentPages.find(
      (page) => page.pageNumber === selectedAlignmentPage?.pageNumber
    );
    if (!originalPage) return;
    updatePageTransform(selectedAlignmentPage.pageNumber, structuredClone(originalPage.transform));
  }

  function restoreAutoAlignField(field: keyof StudentWorkflowTransform) {
    if (!selectedAlignmentPage) return;
    const originalPage = originalAlignmentPages.find(
      (page) => page.pageNumber === selectedAlignmentPage.pageNumber
    );
    if (!originalPage) return;
    const page = alignmentDraftPages.find(
      (candidate) => candidate.pageNumber === selectedAlignmentPage.pageNumber
    );
    if (!page) return;
    updatePageTransform(selectedAlignmentPage.pageNumber, {
      ...page.transform,
      [field]: originalPage.transform[field] as number
    });
  }

  function transformFieldMatchesAuto(
    field: keyof StudentWorkflowTransform,
    current: number,
    autoValue: number
  ): boolean {
    const eps = field === 'scale' ? 1e-5 : 1e-4;
    return Math.abs(current - autoValue) <= eps;
  }

  function resetSelectedTransformToZero() {
    if (!selectedAlignmentPage) return;
    updatePageTransform(selectedAlignmentPage.pageNumber, {
      rotation: 0,
      scale: 1,
      translateX: 0,
      translateY: 0
    });
  }

  function sanitizeTransformField(
    field: keyof StudentWorkflowTransform,
    value: number,
    fallback: number
  ): number {
    if (!Number.isFinite(value)) {
      return fallback;
    }
    if (field === 'scale') {
      return Math.min(Math.max(Math.round(value * 1000) / 1000, minScale), maxScale);
    }
    return value;
  }

  function opacityLabel(value: number): string {
    return `${Math.round(value * 100)}%`;
  }

  function confidenceLabel(confidence: number | null): string {
    return confidence === null ? 'Confidence n/a' : `${Math.round(confidence * 100)}% confidence`;
  }

  function confidencePercentLabel(confidence: number | null): string {
    return confidence === null ? 'n/a' : `${Math.round(confidence * 100)}%`;
  }

  function confidenceBand(page: StudentWorkflowAlignmentPage): AlignmentConfidenceBand {
    if (page.confidence === null || page.confidence < mediumConfidenceMin) {
      return 'low';
    }
    if (page.confidence < highConfidenceMin) {
      return 'medium';
    }
    return 'high';
  }

  function pagesForConfidenceBand(band: AlignmentConfidenceBand): StudentWorkflowAlignmentPage[] {
    return alignmentDraftPages.filter((page) => confidenceBand(page) === band);
  }

  function tabBorderClass(band: AlignmentConfidenceBand): string {
    if (band === 'high') {
      return 'border-[var(--message-success-border)]';
    }
    if (band === 'medium') {
      return 'border-[var(--message-warning-border)]';
    }
    return 'border-[var(--message-error-border)]';
  }

  function pageHasCheck(page: StudentWorkflowAlignmentPage, reviewedPages: Set<number>): boolean {
    return !page.lowConfidence || page.reviewExempt === true || reviewedPages.has(page.pageNumber);
  }

  function pageNeedsManualAcceptance(
    page: StudentWorkflowAlignmentPage,
    reviewedPages: Set<number>
  ): boolean {
    return page.lowConfidence && page.reviewExempt !== true && !reviewedPages.has(page.pageNumber);
  }

  function firstActionablePage(
    reviewedPages: Set<number>
  ): StudentWorkflowAlignmentPage | null {
    return alignmentDraftPages.find((page) => pageNeedsManualAcceptance(page, reviewedPages)) ?? null;
  }

  function acceptPage(pageNumber: number) {
    const page = alignmentDraftPages.find((candidate) => candidate.pageNumber === pageNumber);
    if (!page) return;
    const nextReviewedPages = new Set(reviewedPageNumbers).add(pageNumber);
    reviewedPageNumbers = nextReviewedPages;
    if (alignmentDraftPages.some((candidate) => pageNeedsManualAcceptance(candidate, nextReviewedPages))) {
      pendingReuseTransform = {
        pageNumber,
        band: confidenceBand(page)
      };
      return;
    }
    advanceAfterAcceptance(pageNumber, confidenceBand(page), nextReviewedPages);
  }

  function advanceAfterAcceptance(
    pageNumber: number,
    band: AlignmentConfidenceBand,
    reviewedPages: Set<number>
  ) {
    const nextPage = nextUnacceptedPageInBand(
      pageNumber,
      band,
      reviewedPages
    ) ?? firstActionablePageInLaterConfidenceBand(band, reviewedPages);
    if (nextPage) {
      alignmentPageNumber = nextPage.pageNumber;
    }
  }

  function clearPageAcceptableMark(pageNumber: number) {
    const next = new Set(reviewedPageNumbers);
    next.delete(pageNumber);
    reviewedPageNumbers = next;
    if (pendingReuseTransform?.pageNumber === pageNumber) {
      pendingReuseTransform = null;
    }
  }

  function nextUnacceptedPageInBand(
    currentPageNumber: number,
    band: AlignmentConfidenceBand,
    reviewedPages: Set<number>
  ): StudentWorkflowAlignmentPage | null {
    const candidates = alignmentDraftPages.filter(
      (page) => confidenceBand(page) === band && pageNeedsManualAcceptance(page, reviewedPages)
    );
    return (
      candidates.find((page) => page.pageNumber > currentPageNumber) ??
      candidates[0] ??
      null
    );
  }

  function firstActionablePageInLaterConfidenceBand(
    band: AlignmentConfidenceBand,
    reviewedPages: Set<number>
  ): StudentWorkflowAlignmentPage | null {
    const bandIndex = confidenceBandOrder.indexOf(band);
    if (bandIndex < 0) return null;
    for (const nextBand of confidenceBandOrder.slice(bandIndex + 1)) {
      const nextPage = alignmentDraftPages.find(
        (page) => confidenceBand(page) === nextBand && pageNeedsManualAcceptance(page, reviewedPages)
      );
      if (nextPage) return nextPage;
    }
    return null;
  }

  function cancelReuseTransform() {
    if (!pendingReuseTransform) return;
    const { pageNumber, band } = pendingReuseTransform;
    pendingReuseTransform = null;
    advanceAfterAcceptance(pageNumber, band, reviewedPageNumbers);
  }

  function confirmReuseTransform() {
    if (!pendingReuseTransform) return;
    const acceptedPage = alignmentDraftPages.find(
      (page) => page.pageNumber === pendingReuseTransform?.pageNumber
    );
    if (!acceptedPage) {
      pendingReuseTransform = null;
      return;
    }
    const acceptedTransform = structuredClone(acceptedPage.transform);
    const nextReviewedPages = new Set(reviewedPageNumbers);
    alignmentDraftPages = alignmentDraftPages.map((page) => {
      if (!pageNeedsManualAcceptance(page, nextReviewedPages)) {
        return page;
      }
      nextReviewedPages.add(page.pageNumber);
      return {
        ...page,
        transform: structuredClone(acceptedTransform)
      };
    });
    reviewedPageNumbers = nextReviewedPages;
    alignmentPageNumber = acceptedPage.pageNumber;
    pendingReuseTransform = null;
  }

  function updateTemplateTintColor(value: string) {
    if (/^#[0-9a-f]{6}$/i.test(value)) {
      templateTintColor = value;
    }
  }

  async function handleConfirm() {
    if (alignmentDraftPages.length === 0) return;
    await onconfirm(alignmentDraftPages);
  }

  const utilityButtonClass =
    'inline-flex h-11 items-center justify-center gap-2 rounded-lg border border-workspace-border bg-transparent px-4 text-sm font-medium text-workspace-text-primary transition-colors hover:bg-workspace-sidebar-active disabled:cursor-not-allowed disabled:opacity-50';
  const fieldClass =
    'w-full rounded-lg border border-workspace-border bg-card/20 px-3 py-2 text-sm text-workspace-text-primary';
  const rangeClass = 'w-full accent-primary';
  const pageTabBaseClass =
    'group relative flex shrink-0 items-center gap-3 border-t px-3 pb-2.5 pt-2 text-sm transition-colors';
  const tabActionButtonClass =
    'relative z-10 rounded-md bg-workspace-sidebar-active px-2 py-0.5 text-[10px] font-semibold text-workspace-text-primary transition-colors hover:bg-workspace-sidebar-hover disabled:cursor-not-allowed disabled:opacity-50';
  const readyConfirmButtonClass =
    'inline-flex h-11 items-center justify-center gap-2 rounded-lg border border-primary bg-primary px-4 text-sm font-semibold text-primary-foreground shadow-[var(--surface-shadow-button)] transition-colors hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50';
  const transformUndoButtonClass =
    'inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-workspace-border bg-transparent text-workspace-text-muted transition-colors hover:bg-workspace-sidebar-active hover:text-workspace-text-primary disabled:cursor-not-allowed disabled:opacity-50';
</script>

<div class="flex h-full min-h-0 flex-col">
  <div class="flex shrink-0 items-center px-6 pt-0 pb-2">
    <div class="w-40">
      <DesktopButton class="whitespace-nowrap" variant="ghost" size="compact" onclick={onback}>Back to workflow</DesktopButton>
    </div>
    <div class="flex min-w-0 flex-1 items-center justify-center">
      <div class="min-w-0 max-w-[60%] truncate text-sm font-semibold text-workspace-text-primary">
        {displayName}
      </div>
    </div>
    <div class="flex w-36 items-center justify-end gap-2">
      <StatusBadge tone="warning" class="shrink-0 whitespace-nowrap px-3 py-1">
        {alignmentConfidenceLabel}
      </StatusBadge>
      {#if ondelete}
        <IconButton
          variant="danger"
          size="compact"
          ariaLabel="Delete submission"
          title={`Delete submission for ${displayName}`}
          disabled={deleteDisabled}
          onclick={() => ondelete?.()}
        >
          <HugeiconsIcon icon={Delete02Icon} size={17} strokeWidth={1.8} aria-hidden="true" />
        </IconButton>
      {/if}
    </div>
  </div>

  {#if alignmentDraftPages.length > 0}
    <div class="relative shrink-0 border-b border-workspace-border bg-workspace-sidebar">
      <div class="flex overflow-x-auto px-6">
        {#each confidenceTabGroups as group, groupIndex (group.band)}
          {#if groupIndex > 0}
            <div class="w-6 shrink-0"></div>
          {/if}
          {#each group.pages as page (page.pageNumber)}
            {@const isActive = page.pageNumber === alignmentPageNumber}
            {@const isReviewed = reviewedPageNumbers.has(page.pageNumber)}
            {@const isExempt = page.reviewExempt === true}
            {@const showUndo = isReviewed && !isExempt && hoveredPageNumber === page.pageNumber}
            <div
              class={[
                pageTabBaseClass,
                tabBorderClass(group.band),
                isActive
                  ? 'text-workspace-text-primary'
                  : 'text-workspace-text-muted hover:text-workspace-text-secondary'
              ]}
              role="group"
              aria-label={`Page ${page.pageNumber} alignment tab`}
              onmouseenter={() => (hoveredPageNumber = page.pageNumber)}
              onmouseleave={() => {
                if (hoveredPageNumber === page.pageNumber) {
                  hoveredPageNumber = null;
                }
              }}
            >
              <span
                class="absolute inset-x-0 bottom-0 h-[3px] rounded-t"
                class:bg-primary={isActive}
                class:bg-transparent={!isActive}
              ></span>
              <button
                type="button"
                class="relative z-10 flex items-center gap-2"
                onclick={() => (alignmentPageNumber = page.pageNumber)}
              >
                <span class:font-semibold={isActive} class:font-medium={!isActive}>
                  P{page.pageNumber}
                </span>
                <span
                  class={[
                    'text-[10px] font-semibold',
                    group.band === 'high'
                      ? 'text-[var(--message-success-text)]'
                      : group.band === 'medium'
                        ? 'text-[var(--message-warning-text)]'
                        : 'text-[var(--message-error-text)]'
                  ]}
                >
                  {confidencePercentLabel(page.confidence)}
                </span>
              </button>
              {#if isExempt}
                <span class="relative z-10 text-[10px] font-semibold text-workspace-text-muted">
                  Exempt
                </span>
              {:else if isActive && pageNeedsManualAcceptance(page, reviewedPageNumbers)}
                <button
                  type="button"
                  class={tabActionButtonClass}
                  disabled={controlsDisabled}
                  onclick={() => acceptPage(page.pageNumber)}
                >
                  Accept
                </button>
              {:else if showUndo}
                <button
                  type="button"
                  class={tabActionButtonClass}
                  disabled={controlsDisabled}
                  onclick={() => clearPageAcceptableMark(page.pageNumber)}
                >
                  Undo
                </button>
              {:else if pageHasCheck(page, reviewedPageNumbers)}
                <span class="relative z-10 text-xs text-workspace-text-muted" aria-label="acceptable alignment">
                  ✓
                </span>
              {/if}
            </div>
          {/each}
        {/each}
      </div>
    </div>
  {/if}

  {#if intakeItem && selectedAlignmentPage}
    <div class="flex min-h-0 min-w-0 flex-1 gap-6 px-6 py-4">
      <div class="min-h-0 min-w-0 flex-1 overflow-hidden">
        <AlignmentOverlayCanvas
          templateImageUrl={selectedTemplateUrl}
          submissionImageUrl={selectedSubmissionUrl}
          pageNumber={alignmentPageNumber}
          transform={selectedAlignmentPage.transform}
          {submissionOpacity}
          {templateTintColor}
          busy={controlsDisabled}
          ontransformchange={(transform) =>
            updatePageTransform(selectedAlignmentPage.pageNumber, transform)}
        />
      </div>

      <aside class="flex w-[15rem] shrink-0 min-h-0 flex-col gap-6 overflow-auto">
        <section>
          <div class="rounded-lg border border-workspace-border bg-card/20 px-3 py-3">
            <div class="text-xs font-semibold uppercase tracking-wide text-workspace-text-muted">
              {selectedAlignmentPage.reviewExempt ? 'Review exempt' : 'Page status'}
            </div>
            <p class="mt-2 text-sm leading-5 text-workspace-text-secondary">
              {#if selectedAlignmentPage.reviewExempt}
                No template questions are on this page, so alignment approval is not required.
              {:else if pageNeedsManualAcceptance(selectedAlignmentPage, reviewedPageNumbers)}
                This page needs alignment approval before workflow automation can continue.
              {:else}
                This page is ready for workflow automation.
              {/if}
            </p>
          </div>
        </section>

        <section>
          <div class="mt-4 space-y-4">
            <div>
              <div class="flex items-center justify-between gap-2">
                <span class="text-xs font-semibold uppercase tracking-wide text-workspace-text-muted">
                  Rotation
                </span>
                <button
                  type="button"
                  class={transformUndoButtonClass}
                  disabled={controlsDisabled ||
                    !selectedOriginalAlignmentPage ||
                    transformFieldMatchesAuto(
                      'rotation',
                      selectedAlignmentPage.transform.rotation,
                      selectedOriginalAlignmentPage.transform.rotation
                    )}
                  aria-label="Reset rotation to auto-align value"
                  onclick={() => restoreAutoAlignField('rotation')}
                >
                  <AlignmentTransformUndoIcon />
                </button>
              </div>
              <input
                type="range"
                min="-10"
                max="10"
                step="0.1"
                class={`mt-2 ${rangeClass}`}
                value={selectedAlignmentPage.transform.rotation}
                oninput={(event) =>
                  updateAlignmentTransform(
                    selectedAlignmentPage.pageNumber,
                    'rotation',
                    Number((event.currentTarget as HTMLInputElement).value)
                  )}
              />
              <input
                type="number"
                step="0.1"
                class={`mt-2 ${fieldClass}`}
                value={selectedAlignmentPage.transform.rotation}
                oninput={(event) =>
                  updateAlignmentTransform(
                    selectedAlignmentPage.pageNumber,
                    'rotation',
                    Number((event.currentTarget as HTMLInputElement).value)
                  )}
              />
            </div>
            <div>
              <div class="flex items-center justify-between gap-2">
                <span class="text-xs font-semibold uppercase tracking-wide text-workspace-text-muted">
                  Scale
                </span>
                <button
                  type="button"
                  class={transformUndoButtonClass}
                  disabled={controlsDisabled ||
                    !selectedOriginalAlignmentPage ||
                    transformFieldMatchesAuto(
                      'scale',
                      selectedAlignmentPage.transform.scale,
                      selectedOriginalAlignmentPage.transform.scale
                    )}
                  aria-label="Reset scale to auto-align value"
                  onclick={() => restoreAutoAlignField('scale')}
                >
                  <AlignmentTransformUndoIcon />
                </button>
              </div>
              <input
                type="range"
                min={minScale}
                max={maxScale}
                step="0.001"
                class={`mt-2 ${rangeClass}`}
                value={selectedAlignmentPage.transform.scale}
                oninput={(event) =>
                  updateAlignmentTransform(
                    selectedAlignmentPage.pageNumber,
                    'scale',
                    Number((event.currentTarget as HTMLInputElement).value)
                  )}
              />
              <input
                type="number"
                min={minScale}
                max={maxScale}
                step="0.001"
                class={`mt-2 ${fieldClass}`}
                value={selectedAlignmentPage.transform.scale}
                oninput={(event) =>
                  updateAlignmentTransform(
                    selectedAlignmentPage.pageNumber,
                    'scale',
                    Number((event.currentTarget as HTMLInputElement).value)
                  )}
              />
            </div>
            <div class="min-w-0">
              <div
                class="grid min-w-0 grid-cols-[1fr_auto_1fr] items-center gap-x-2 text-xs font-semibold uppercase tracking-wide text-workspace-text-muted"
              >
                <div class="flex min-w-0 items-center gap-2 justify-self-start">
                  <span aria-hidden="true">X</span>
                  <button
                    type="button"
                    class={transformUndoButtonClass}
                    disabled={controlsDisabled ||
                      !selectedOriginalAlignmentPage ||
                      transformFieldMatchesAuto(
                        'translateX',
                        selectedAlignmentPage.transform.translateX,
                        selectedOriginalAlignmentPage.transform.translateX
                      )}
                    aria-label="Reset translate X to auto-align value"
                    onclick={() => restoreAutoAlignField('translateX')}
                  >
                    <AlignmentTransformUndoIcon />
                  </button>
                </div>
                <span class="justify-self-center text-center whitespace-nowrap">Translate</span>
                <div class="flex min-w-0 items-center gap-2 justify-self-end">
                  <span aria-hidden="true">Y</span>
                  <button
                    type="button"
                    class={transformUndoButtonClass}
                    disabled={controlsDisabled ||
                      !selectedOriginalAlignmentPage ||
                      transformFieldMatchesAuto(
                        'translateY',
                        selectedAlignmentPage.transform.translateY,
                        selectedOriginalAlignmentPage.transform.translateY
                      )}
                    aria-label="Reset translate Y to auto-align value"
                    onclick={() => restoreAutoAlignField('translateY')}
                  >
                    <AlignmentTransformUndoIcon />
                  </button>
                </div>
              </div>
              <div class="mt-2 grid min-w-0 grid-cols-2 gap-3">
                <input
                  type="number"
                  class="min-w-0 w-full rounded-lg border border-workspace-border bg-card/20 px-3 py-3 text-base text-workspace-text-primary"
                  aria-label="Translate X"
                  title="Translate X"
                  value={selectedAlignmentPage.transform.translateX}
                  oninput={(event) =>
                    updateAlignmentTransform(
                      selectedAlignmentPage.pageNumber,
                      'translateX',
                      Number((event.currentTarget as HTMLInputElement).value)
                    )}
                />
                <input
                  type="number"
                  class="min-w-0 w-full rounded-lg border border-workspace-border bg-card/20 px-3 py-3 text-base text-workspace-text-primary"
                  aria-label="Translate Y"
                  title="Translate Y"
                  value={selectedAlignmentPage.transform.translateY}
                  oninput={(event) =>
                    updateAlignmentTransform(
                      selectedAlignmentPage.pageNumber,
                      'translateY',
                      Number((event.currentTarget as HTMLInputElement).value)
                    )}
                />
              </div>
            </div>
            <button
              type="button"
              class={utilityButtonClass}
              disabled={controlsDisabled}
              onclick={restoreAutoAlignTransform}
            >
              Restore auto-align
            </button>
            <button
              type="button"
              class={utilityButtonClass}
              disabled={controlsDisabled}
              onclick={resetSelectedTransformToZero}
            >
              Zero transform
            </button>
          </div>
        </section>

        <section>
          <div class="mt-4 text-xs font-semibold uppercase tracking-wide text-workspace-text-muted">
            Template shading
            <div class="mt-2 flex items-center gap-3">
              <input
                type="color"
                class="h-9 w-12 cursor-pointer rounded-lg border border-workspace-border bg-transparent p-1"
                aria-label="Template shading color"
                value={templateTintColor}
                oninput={(event) =>
                  updateTemplateTintColor((event.currentTarget as HTMLInputElement).value)}
              />
              <span class="font-mono text-xs font-medium uppercase tracking-normal text-workspace-text-secondary">
                {templateTintColor}
              </span>
            </div>
          </div>
          <label class="mt-4 block text-xs font-semibold uppercase tracking-wide text-workspace-text-muted">
            Submission opacity · {opacityLabel(submissionOpacity)}
            <input
              type="range"
              min="0.2"
              max="1"
              step="0.05"
              class={`mt-2 ${rangeClass}`}
              value={submissionOpacity}
              oninput={(event) =>
                (submissionOpacity = Number((event.currentTarget as HTMLInputElement).value))}
            />
          </label>
        </section>

        <div class="mt-auto flex justify-end">
          <button
            type="button"
            class={allAlignmentPagesReady ? readyConfirmButtonClass : utilityButtonClass}
            disabled={controlsDisabled || !allAlignmentPagesReady}
            onclick={() => void handleConfirm()}
          >
            {saving ? 'Saving…' : 'Confirm alignment → continue'}
          </button>
        </div>
      </aside>
    </div>
  {/if}
</div>

<ConfirmDialog
  open={pendingReuseTransform !== null}
  title="Apply transform to remaining pages?"
  description="Use the accepted transform for the remaining unaccepted alignment-review pages in this submission."
  confirmLabel="Apply All"
  cancelLabel="This Page"
  busy={controlsDisabled}
  onCancel={cancelReuseTransform}
  onConfirm={confirmReuseTransform}
/>
