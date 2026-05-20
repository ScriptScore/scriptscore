<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { onMount, tick } from 'svelte';
  import type { BusyAction } from '$lib/stores/workspaceView';
  import type {
    IntakePreviewPage,
    LmsRosterCacheStatus,
    LmsRosterRow,
    PdfPointRect,
    RuntimeJobEvent,
    StudentIntakeRasterSize,
    StudentIntakeRedactionRegionInput,
    StudentIntakeFinalizePayload,
    StudentIntakeFinalizeResult,
    StudentIntakeSummary,
    WorkspaceWarning
  } from '$lib/types';
  import { ensureRuntimeJobBridge, onRuntimeJobEvent, shellState } from '$lib/stores/shell';
  import {
    intakeDefaultPdfRectsFromTemplate,
    priorCanonicalSubmissionExistsForLmsStudent,
    toDesktopAssetUrl,
    transientClipPdfRectsPngBase64,
    transientRenderPdfPagePng,
    transientScansOcrHint
  } from '$lib/desktop';
  import { rosterFuzzyScore } from '$lib/intakeFuzzyMatch';
  import {
    normalizeDesiredPageOrder,
    reorderPageNumbers
  } from '$lib/studentIntakePageOrder';
  import HorizontalProgressBar from './HorizontalProgressBar.svelte';
  import {
    DesktopButton,
    ImageRegionEditor,
    InlineMessage,
    PagePreviewFrame,
    SelectField,
    StatusBadge,
    type ImageRegion,
    type ImageRegionPresentation
  } from './ui';

  // Retained for parent compatibility (disables sidebar upload button, etc).
  // This component no longer renders its own upload controls.
  export let busyAction: BusyAction = null;
  export let prerequisitesMet = false;
  export let lmsCourseId: string | null = null;
  export let sharedRosterStatus: LmsRosterCacheStatus = 'idle';
  export let sharedRosterRows: LmsRosterRow[] = [];
  export let sharedRosterMessage: string | null = null;
  export let onEnsureRosterCache: (() => void | Promise<void>) | null = null;
  export let onFinalizeSubmission: ((
    payload: StudentIntakeFinalizePayload
  ) => Promise<StudentIntakeFinalizeResult | null>) | null = null;
  export let seedPaths: string[] = [];
  export let seedVersion: number = 0;
  export let existingIntakeItems: StudentIntakeSummary[] = [];
  export let expectedPageCount = 0;
  export let onSubmissionCompleted:
    | ((payload: SubmissionCompletedPayload) => void | Promise<void>)
    | null = null;
  export let onActiveFileChange: ((filename: string | null) => void) | null = null;

  type SubmissionCompletedPayload = {
    canvasUserId?: string | null;
    studentRef: string;
  };

  type QueueStatus = 'pending' | 'processing' | 'done';
  type WizardStep = 1 | 2 | 3 | 4 | 5;
  type PreviewProgress = {
    rendered: number;
    total: number | null;
    currentPage: number | null;
  };

  type QueueItem = {
    id: string;
    pdfPath: string;
    status: QueueStatus;
    previewPages?: IntakePreviewPage[];
    previewProgress?: PreviewProgress | null;
    previewError?: string | null;
    rectsPt?: PdfPointRect[];
    rectsPx?: StudentIntakeRedactionRegionInput[];
    cropPngsBase64?: string[];
    ocrHint?: string;
    /** EasyOCR `readtext` row count from transient `scans.ocr` (for verification). */
    ocrSegmentCount?: number | null;
    roster?: LmsRosterRow[];
    bestGuessUserId?: string | null;
    bestGuessScore?: number | null;
    selectedUserId?: string | null;
    localStudentName?: string | null;
    confirmed?: boolean;
    studentRef?: string | null;
    bindingTokenHex?: string | null;
    canonicalPdfPath?: string | null;
    canonicalPageCount?: number | null;
    /** Ingest output PNG paths (page order); from workspace after scans.ingest. */
    examPagePaths?: string[] | null;
    canonicalWarnings?: WorkspaceWarning[];
    /** Shown on step 3 when selected roster student already has a canonical PDF. */
    associationReplaceWarning?: string | null;
    /** Step 2 failures (clip / OCR / roster) are stored per queued item. */
    step2Error?: string | null;
    desiredPageOrder?: number[];
  };

  const stepActiveTextClass = 'text-workspace-text-primary';
  const stepProgressSuccessTextClass = 'text-[var(--message-success-text)]';
  const activeStepPillClass = 'border border-workspace-border-strong bg-card/30 font-medium';
  const stepPillBaseClass = 'inline-flex h-10 shrink-0 items-center rounded-2xl px-4';
  const stepMutedTextClass = 'text-workspace-text-muted';
  const stepConnectorDoneClass = 'bg-[var(--message-success-border)]';
  const stepConnectorPendingClass = 'bg-workspace-border';
  const replaceCanonicalWarningMessage =
    'This student already has a prepared submission for this project. Continuing will replace it.';
  const workerBusyFinalizeMessage =
    'This submission could not start because ScriptScore is already working on another task. Wait for that task to finish, then confirm the student again.';
  const finalizeFailedFallbackMessage =
    'This submission was not prepared. Confirm the student again to retry.';
  const taskRowBaseClass = 'flex items-center gap-3 text-sm';
  const spinnerClass =
    'inline-block size-3 shrink-0 animate-spin rounded-full border-2 border-[var(--message-info-border)] border-r-transparent';
  const INTAKE_PREVIEW_MAX_WIDTH_PX = 1600;

  function taskRowClass(completed: boolean, current: boolean): string {
    if (completed) {
      return `${taskRowBaseClass} ${stepProgressSuccessTextClass}`;
    }
    if (current) {
      return `${taskRowBaseClass} text-[var(--message-info-text)]`;
    }
    return `${taskRowBaseClass} text-workspace-text-secondary`;
  }

  function showTaskIndicator(completed: boolean, current: boolean): 'done' | 'current' | 'pending' {
    if (completed) return 'done';
    if (current) return 'current';
    return 'pending';
  }

  function basename(path: string): string {
    const parts = path.split('/');
    return parts[parts.length - 1] ?? path;
  }

  let queue: QueueItem[] = [];
  let activeId: string | null = null;
  let step: WizardStep = 1;
  let displayStep: WizardStep = 1;

  let step2Progress: { clipped: boolean; ocr: boolean; roster: boolean; fuzzy: boolean } = {
    clipped: false,
    ocr: false,
    roster: false,
    fuzzy: false
  };
  let step4Progress: {
    redactPdf: boolean;
    ingestExam: boolean;
  } = {
    redactPdf: false,
    ingestExam: false
  };
  let step4Percent = {
    redactPdf: 0,
    ingestExam: 0
  };
  let step4Error: string | null = null;
  let currentStep2Task: 'clipped' | 'ocr' | 'roster' | 'fuzzy' | null = 'clipped';
  let step2WaitingForSharedRoster = false;
  let step2PendingItemId: string | null = null;

  /** Latest step for job-event handlers (avoids stale reads vs `$: currentStep = step` in async closures). */
  const intakeStepRef = { current: 1 as WizardStep };
  $: intakeStepRef.current = step;

  let rosterError: string | null = null;
  let activeRegionIndex = 0;
  let activePreviewPageNumber = 1;
  let draggedPreviewPageNumber: number | null = null;
  let dragOverPreviewPageNumber: number | null = null;
  type RectPx = StudentIntakeRedactionRegionInput;
  let showExamView = false;
  let examViewPageNumber = 1;
  let lastActiveFilename: string | null = null;
  const previewLoadTimeoutMs = 12000;
  // Step 2 can include EasyOCR cold start + network roster fetch; allow a longer per-operation timeout.
  const step2OperationTimeoutMs = 180000;

  function enqueuePaths(paths: string[]) {
    const normalized = paths
      .filter((p): p is string => typeof p === 'string' && p.trim().length > 0)
      .map((p) => p.trim());
    if (normalized.length === 0) return;
    const wasEmpty = queue.length === 0;
    const newItems = normalized.map((pdfPath) => ({
      id: crypto.randomUUID(),
      pdfPath,
      status: 'pending' as const
    }));
    queue = [
      ...queue,
      ...newItems
    ];
    ensureActive();
    if (wasEmpty && activeId) {
      void beginItem(activeId);
    }
  }

  $: if (seedVersion > 0) {
    enqueuePaths(seedPaths);
  }
  let associationWarningRequestId = 0;

  $: courseIdTrimmed = (lmsCourseId ?? '').trim();
  $: lmsLinked = courseIdTrimmed.length > 0;
  $: workerBusyForFinalization =
    $shellState.workerStatus === 'busy' || $shellState.workerStatus === 'starting';
  $: if (step4Error === workerBusyFinalizeMessage && !workerBusyForFinalization) {
    step4Error = null;
  }
  $: activeItem = queue.find((q) => q.id === activeId) ?? null;
  $: if (lmsLinked && step2WaitingForSharedRoster && step2PendingItemId && sharedRosterStatus === 'ready') {
    finalizeStep2WithSharedRoster(step2PendingItemId);
  }
  $: displayStep =
    step >= 4
      ? step
      : step === 2 && !activeItem?.step2Error && Array.isArray(activeItem?.roster)
        ? 3
        : step;
  $: currentStep2Task = !step2Progress.clipped
    ? 'clipped'
    : !step2Progress.ocr
      ? 'ocr'
      : !step2Progress.roster
      ? lmsLinked ? 'roster' : null
      : !step2Progress.fuzzy
          ? 'fuzzy'
          : null;
  $: hasNextQueued =
    queue.findIndex((q) => q.id !== activeId && q.status !== 'done') !== -1;

  // Overwrite warning depends on the selected LMS student, not on the visual wizard phase.
  // Recompute whenever the active item / selected user changes.
  $: if (activeItem) {
    void activeItem.selectedUserId;
    void updateAssociationReplaceWarning();
  }

  function setAssociationReplaceWarning(itemId: string, message: string | null): void {
    queue = queue.map((q) =>
      q.id === itemId ? { ...q, associationReplaceWarning: message } : q
    );
  }

  function fallbackHasPriorCanonical(item: QueueItem, userId: string): boolean {
    const rosterSorted = [...(item.roster ?? [])].sort((a, b) => a.sortKey.localeCompare(b.sortKey));
    const selectedIndex = rosterSorted.findIndex((row) => row.userId === userId);
    const fallbackStudentRef = selectedIndex >= 0 ? `student_${selectedIndex + 1}` : null;
    if (fallbackStudentRef === null) {
      return false;
    }
    return existingIntakeItems.some(
      (existing) =>
        existing.studentRef === fallbackStudentRef &&
        existing.canonicalPdfPath.trim().length > 0 &&
        (existing.ingestStatus === 'ok' || existing.pageCount > 0)
    );
  }

  async function updateAssociationReplaceWarning() {
    const item = activeItem;
    if (!item) return;
    const userId = (item.selectedUserId ?? '').trim();
    if (!userId || sharedRosterStatus !== 'ready' || !courseIdTrimmed) {
      if (item.associationReplaceWarning != null) {
        setAssociationReplaceWarning(item.id, null);
      }
      return;
    }
    const req = ++associationWarningRequestId;
    const fallbackHasPrior = fallbackHasPriorCanonical(item, userId);
    try {
      const hostHasPrior = await priorCanonicalSubmissionExistsForLmsStudent(
        courseIdTrimmed,
        userId
      );
      if (req !== associationWarningRequestId) return;
      const hasPrior = fallbackHasPrior || hostHasPrior;
      const msg = hasPrior ? replaceCanonicalWarningMessage : null;
      const cur = queue.find((q) => q.id === item.id)?.associationReplaceWarning ?? null;
      if (cur === msg) return;
      setAssociationReplaceWarning(item.id, msg);
    } catch (err) {
      if (req !== associationWarningRequestId) return;
      if (fallbackHasPrior) {
        setAssociationReplaceWarning(item.id, replaceCanonicalWarningMessage);
        return;
      }
      rosterError = `Unable to check whether this student already has a prepared submission: ${String(err)}`;
      if (item.associationReplaceWarning != null) {
        setAssociationReplaceWarning(item.id, null);
      }
    }
  }

  function ensureActive() {
    if (!activeId && queue.length > 0) {
      activeId = queue[0].id;
    }
  }

  onMount(() => {
    ensureActive();
    void ensureRuntimeJobBridge();
    return onRuntimeJobEvent(handleStep4RuntimeEvent);
  });

  function progressPercentFromEvent(event: RuntimeJobEvent): number | null {
    const progress = event.payload.progress;
    if (typeof progress !== 'object' || progress === null || Array.isArray(progress)) {
      return null;
    }
    const percent = (progress as Record<string, unknown>).percent;
    return typeof percent === 'number' ? percent : null;
  }

  function handleStep4RuntimeEvent(event: RuntimeJobEvent): void {
    if (intakeStepRef.current !== 4) return;
    if (event.commandName !== 'scans.pdf-create-redacted' && event.commandName !== 'scans.ingest') {
      return;
    }
    const key = event.commandName === 'scans.pdf-create-redacted' ? 'redactPdf' : 'ingestExam';
    if (event.eventType === 'job_started' || event.eventType === 'job_submitted') {
      step4Progress = { ...step4Progress, [key]: false };
      step4Percent = { ...step4Percent, [key]: Math.max(0, step4Percent[key]) };
      return;
    }
    if (event.eventType === 'job_progress') {
      const percent = progressPercentFromEvent(event);
      if (percent !== null) {
        step4Percent = { ...step4Percent, [key]: percent };
      }
      return;
    }
    if (event.eventType === 'job_finished') {
      step4Progress = { ...step4Progress, [key]: true };
      step4Percent = { ...step4Percent, [key]: 100 };
    }
  }

  $: activePreview = activeItem?.previewPages?.find((page) => page.pageNumber === activePreviewPageNumber)
    ?? activeItem?.previewPages?.[0]
    ?? null;
  $: renderedPreviewPageNumbers = activeItem?.previewPages?.map((page) => page.pageNumber) ?? [];
  $: previewPageNumbers = activeItem?.previewProgress?.total
    ? Array.from({ length: activeItem.previewProgress.total }, (_, index) => index + 1)
    : renderedPreviewPageNumbers;
  $: orderedPreviewPageNumbers = normalizeDesiredPageOrder(
    previewPageNumbers,
    activeItem?.desiredPageOrder
  );
  $: excludedPreviewPageNumbers = previewPageNumbers.filter(
    (pageNumber) => !orderedPreviewPageNumbers.includes(pageNumber)
  );
  $: displayedPreviewPageNumbers = [
    ...orderedPreviewPageNumbers,
    ...excludedPreviewPageNumbers
  ];
  $: hasExtraPreviewPages =
    expectedPageCount > 0 && previewRenderingComplete && previewPageNumbers.length > expectedPageCount;
  $: hasMissingPreviewPages =
    expectedPageCount > 0 &&
    previewRenderingComplete &&
    orderedPreviewPageNumbers.length < expectedPageCount;
  $: hasUnresolvedPageMismatch =
    expectedPageCount > 0 &&
    previewRenderingComplete &&
    orderedPreviewPageNumbers.length !== expectedPageCount;
  $: step1PageMismatchMessage =
    expectedPageCount <= 0 || !previewRenderingComplete
      ? null
      : orderedPreviewPageNumbers.length < expectedPageCount
        ? `This submission has ${orderedPreviewPageNumbers.length} selected page${orderedPreviewPageNumbers.length === 1 ? '' : 's'}; the template has ${expectedPageCount}. Replace or rescan the PDF before finalizing intake.`
        : previewPageNumbers.length > expectedPageCount
          ? orderedPreviewPageNumbers.length === expectedPageCount
            ? `This submission has ${previewPageNumbers.length} pages; the template has ${expectedPageCount}. Excluded pages will be left out of the prepared submission.`
            : `This submission has ${previewPageNumbers.length} pages; the template has ${expectedPageCount}. Exclude extra pages before finalizing.`
          : null;
  $: previewRenderingComplete =
    !activeItem?.previewProgress ||
    (activeItem.previewProgress.total !== null &&
      activeItem.previewProgress.rendered >= activeItem.previewProgress.total);
  $: previewRenderingPercent = previewProgressPercent(activeItem?.previewProgress);
  $: visibleRectEntries =
    activeItem?.rectsPx
      ?.map((rect, index) => ({ rect, index }))
      .filter((entry) => entry.rect.pageNumber === activePreviewPageNumber) ?? [];
  $: editorRedactionRegions = visibleRectEntries.map(({ rect, index }) => ({
    regionId: editorRegionId(index),
    pageNumber: rect.pageNumber,
    x: rect.x,
    y: rect.y,
    width: rect.width,
    height: rect.height,
    kind: redactionKindForIndex(index)
  }));

  $: examPathsForView = activeItem?.examPagePaths ?? [];
  $: examPageCount = examPathsForView.length;
  $: examImageSrc =
    examPageCount > 0 &&
    examViewPageNumber >= 1 &&
    examViewPageNumber <= examPageCount
      ? toDesktopAssetUrl(examPathsForView[examViewPageNumber - 1]!)
      : '';
  $: activeFilename = activeItem ? basename(activeItem.pdfPath) : null;
  $: if (activeFilename !== lastActiveFilename) {
    lastActiveFilename = activeFilename;
    onActiveFileChange?.(activeFilename);
  }

  function updateQueueItem(id: string, patch: Partial<QueueItem>): void {
    queue = queue.map((item) => (item.id === id ? { ...item, ...patch } : item));
  }

  function updatePreviewProgress(id: string, previewProgress: PreviewProgress | null): void {
    updateQueueItem(id, { previewProgress });
  }

  function previewPageIsRendered(pageNumber: number): boolean {
    return renderedPreviewPageNumbers.includes(pageNumber);
  }

  function previewPagesFromTotal(total: number): number[] {
    return Array.from({ length: Math.max(1, total) }, (_, index) => index + 1);
  }

  function rectsPxFromRenderedPreviews(
    rectsPt: PdfPointRect[],
    previewPages: IntakePreviewPage[]
  ): RectPx[] {
    const previewByPage = new Map(previewPages.map((page) => [page.pageNumber, page]));
    return rectsPt
      .map((r) => {
        const preview = previewByPage.get(r.pageNumber);
        if (!preview) {
          return null;
        }
        const scaleX = preview.pngWidthPx / preview.pageWidthPt;
        const scaleY = preview.pngHeightPx / preview.pageHeightPt;
        return {
          pageNumber: r.pageNumber,
          x: r.xPt * scaleX,
          y: r.yPt * scaleY,
          width: r.widthPt * scaleX,
          height: r.heightPt * scaleY
        };
      })
      .filter((rect): rect is RectPx => rect !== null);
  }

  function mergeDefaultRectsForNewPreviewPages(
    existingRectsPx: RectPx[],
    rectsPt: PdfPointRect[],
    nextPreviewPages: IntakePreviewPage[],
    previousPreviewPages: IntakePreviewPage[] = []
  ): RectPx[] {
    const previousRenderedPages = new Set(previousPreviewPages.map((page) => page.pageNumber));
    const newlyRenderedPages = nextPreviewPages.filter(
      (page) => !previousRenderedPages.has(page.pageNumber)
    );
    return [...existingRectsPx, ...rectsPxFromRenderedPreviews(rectsPt, newlyRenderedPages)];
  }

  function startPreviewPageDrag(pageNumber: number): void {
    draggedPreviewPageNumber = pageNumber;
    dragOverPreviewPageNumber = null;
  }

  function canDragPreviewPage(pageNumber: number): boolean {
    return previewPageIsRendered(pageNumber) && !pageIsExcluded(pageNumber);
  }

  function beginPreviewPagePointerDrag(event: PointerEvent, pageNumber: number): void {
    if (event.button !== 0 || !canDragPreviewPage(pageNumber)) return;
    startPreviewPageDrag(pageNumber);
  }

  function previewPagePointerEnter(pageNumber: number): void {
    if (
      draggedPreviewPageNumber === null ||
      draggedPreviewPageNumber === pageNumber ||
      !canDragPreviewPage(pageNumber)
    ) {
      return;
    }
    dragOverPreviewPageNumber = pageNumber;
  }

  function previewPagePointerLeave(pageNumber: number): void {
    if (dragOverPreviewPageNumber === pageNumber) {
      dragOverPreviewPageNumber = null;
    }
  }

  function finishPreviewPagePointerDrag(): void {
    if (draggedPreviewPageNumber === null) return;
    reorderDraggedPreviewPage(dragOverPreviewPageNumber);
    endPreviewPageDrag();
  }

  function reorderDraggedPreviewPage(targetPageNumber: number | null): boolean {
    if (
      !activeItem ||
      draggedPreviewPageNumber === null ||
      targetPageNumber === null ||
      !previewPageIsRendered(targetPageNumber)
    ) {
      return false;
    }
    const currentOrder = normalizeDesiredPageOrder(
      previewPageNumbers,
      activeItem.desiredPageOrder
    );
    if (
      draggedPreviewPageNumber === targetPageNumber ||
      !currentOrder.includes(draggedPreviewPageNumber) ||
      !currentOrder.includes(targetPageNumber)
    ) {
      return false;
    }
    const nextOrder = reorderPageNumbers(currentOrder, draggedPreviewPageNumber, targetPageNumber);
    updateQueueItem(activeItem.id, { desiredPageOrder: nextOrder });
    return true;
  }

  function endPreviewPageDrag(): void {
    dragOverPreviewPageNumber = null;
    draggedPreviewPageNumber = null;
  }

  async function beginItem(id: string) {
    const item = queue.find((q) => q.id === id) ?? null;
    if (!item) return;
    rosterError = null;
    step4Error = null;
    queue = queue.map((q) =>
      q.id === id ? { ...q, status: 'processing', associationReplaceWarning: null } : q
    );
    step = 1;
    showExamView = false;
    examViewPageNumber = 1;
    activePreviewPageNumber = 1;
    await loadPreviewAndDefaultRects(id);
  }

  async function loadPreviewAndDefaultRects(id: string) {
    const item = queue.find((q) => q.id === id);
    if (!item) return;
    queue = queue.map((q) =>
      q.id === id
        ? {
            ...q,
            previewPages: [],
            previewError: null,
            previewProgress: { rendered: 0, total: null, currentPage: 1 }
          }
        : q
    );
    let previewPages: IntakePreviewPage[];
    let rectsPt: PdfPointRect[];
    try {
      rectsPt = await promiseWithTimeout(
        intakeDefaultPdfRectsFromTemplate(item.pdfPath),
        previewLoadTimeoutMs,
        'Timed out mapping default template regions.'
      );
      updatePreviewProgress(id, { rendered: 0, total: null, currentPage: 1 });
      const firstPreview = await promiseWithTimeout(
        transientRenderPdfPagePng(item.pdfPath, 1, 2.0, INTAKE_PREVIEW_MAX_WIDTH_PX),
        previewLoadTimeoutMs,
        'Timed out rendering PDF preview for page 1.'
      );
      const totalPages = Math.max(1, firstPreview.pageCount);
      previewPages = [firstPreview];
      activePreviewPageNumber = 1;
      const allPageNumbers = previewPagesFromTotal(totalPages);
      queue = queue.map((q) =>
        q.id === id
          ? (() => {
              const nextPreviewPages = [...previewPages];
              return {
                ...q,
                previewPages: nextPreviewPages,
                previewError: null,
                rectsPt,
                rectsPx: mergeDefaultRectsForNewPreviewPages(
                  q.rectsPx ?? [],
                  rectsPt,
                  nextPreviewPages,
                  q.previewPages ?? []
                ),
                desiredPageOrder: normalizeDesiredPageOrder(allPageNumbers, q.desiredPageOrder)
              };
            })()
          : q
      );
      updatePreviewProgress(id, {
        rendered: 1,
        total: totalPages,
        currentPage: totalPages > 1 ? 2 : null
      });
      for (let pageNumber = 2; pageNumber <= totalPages; pageNumber += 1) {
        updatePreviewProgress(id, {
          rendered: previewPages.length,
          total: totalPages,
          currentPage: pageNumber
        });
        previewPages.push(
          await promiseWithTimeout(
            transientRenderPdfPagePng(
              item.pdfPath,
              pageNumber,
              2.0,
              INTAKE_PREVIEW_MAX_WIDTH_PX
            ),
            previewLoadTimeoutMs,
            `Timed out rendering PDF preview for page ${pageNumber}.`
          )
        );
        queue = queue.map((q) =>
          q.id === id
            ? (() => {
                const nextPreviewPages = [...previewPages];
                return {
                  ...q,
                  previewPages: nextPreviewPages,
                  previewError: null,
                  rectsPt,
                  rectsPx: mergeDefaultRectsForNewPreviewPages(
                    q.rectsPx ?? [],
                    rectsPt,
                    nextPreviewPages,
                    q.previewPages ?? []
                  ),
                  desiredPageOrder: normalizeDesiredPageOrder(allPageNumbers, q.desiredPageOrder)
                };
              })()
            : q
        );
        updatePreviewProgress(id, {
          rendered: previewPages.length,
          total: totalPages,
          currentPage: pageNumber < totalPages ? pageNumber + 1 : null
        });
      }
    } catch (err) {
      queue = queue.map((q) =>
        q.id === id ? { ...q, previewPages: [], previewError: String(err) } : q
      );
      return;
    }
    const rectsPx =
      queue.find((q) => q.id === id)?.rectsPx ?? rectsPxFromRenderedPreviews(rectsPt, previewPages);
    activePreviewPageNumber = previewPages[0]?.pageNumber ?? 1;
    activeRegionIndex = rectsPx.findIndex((rect) => rect.pageNumber === activePreviewPageNumber);
    if (activeRegionIndex < 0) {
      activeRegionIndex = 0;
    }
    queue = queue.map((q) =>
      q.id === id
        ? {
            ...q,
            previewPages,
            previewProgress: null,
            previewError: null,
            rectsPt,
            rectsPx,
            desiredPageOrder: normalizeDesiredPageOrder(
              previewPagesFromTotal(previewPages.length),
              q.desiredPageOrder
            )
          }
        : q
    );
  }

  function promiseWithTimeout<T>(
    promise: Promise<T>,
    timeoutMs: number,
    message: string
  ): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const timeoutId = window.setTimeout(() => reject(new Error(message)), timeoutMs);
      promise
        .then((value) => {
          window.clearTimeout(timeoutId);
          resolve(value);
        })
        .catch((error) => {
          window.clearTimeout(timeoutId);
          reject(error);
        });
    });
  }

  function previewUrl(preview: IntakePreviewPage) {
    return `data:image/png;base64,${preview.pngBase64}`;
  }

  function previewProgressLabel(progress: PreviewProgress | null | undefined): string {
    if (!progress) return 'Rendering 0 pages';
    const pageLabel = progress.rendered === 1 ? 'page' : 'pages';
    return `Rendering ${progress.rendered} ${pageLabel}`;
  }

  function previewProgressPercent(progress: PreviewProgress | null | undefined): number {
    if (!progress?.total) return 0;
    return Math.max(0, Math.min(100, Math.round((progress.rendered / progress.total) * 100)));
  }

  function rectPxToPt(item: QueueItem, rectPx: { x: number; y: number; width: number; height: number; pageNumber: number }): PdfPointRect {
    const p = item.previewPages?.find((page) => page.pageNumber === rectPx.pageNumber);
    if (!p) {
      throw new Error(`Preview metadata missing for page ${rectPx.pageNumber}.`);
    }
    const sx = p.pageWidthPt / p.pngWidthPx;
    const sy = p.pageHeightPt / p.pngHeightPx;
    return {
      pageNumber: rectPx.pageNumber,
      xPt: rectPx.x * sx,
      yPt: rectPx.y * sy,
      widthPt: rectPx.width * sx,
      heightPt: rectPx.height * sy
    };
  }

  function redactionRegionsForFinalize(item: QueueItem): StudentIntakeRedactionRegionInput[] {
    return (item.rectsPx ?? []).map((region) => ({
      pageNumber: region.pageNumber,
      x: region.x,
      y: region.y,
      width: region.width,
      height: region.height
    }));
  }

  function editorRegionId(index: number): string {
    return `intake-redaction-${index}`;
  }

  function redactionKindForIndex(index: number): string {
    return index === 0 ? 'name_identification' : 'privacy_protection';
  }

  function intakeRedactionPresentation(region: ImageRegion): ImageRegionPresentation {
    const isName = region.kind === 'name_identification';
    return {
      label: isName ? 'Name' : 'Privacy',
      borderColor: isName
        ? 'var(--workspace-selection-name-border)'
        : 'var(--workspace-selection-border)',
      fillColor: isName
        ? 'var(--workspace-selection-name-fill)'
        : 'var(--workspace-selection-fill)',
      labelBackground: isName
        ? 'var(--workspace-selection-name-border)'
        : 'var(--workspace-selection-handle-fill)',
      labelForeground: isName
        ? 'var(--workspace-selection-name-foreground)'
        : 'var(--workspace-selection-handle-border)',
      labelBorder: isName ? undefined : 'var(--workspace-selection-handle-border)'
    };
  }

  function toRectPx(region: ImageRegion): RectPx {
    return {
      pageNumber: activePreviewPageNumber,
      x: region.x,
      y: region.y,
      width: region.width,
      height: region.height
    };
  }

  function updateActivePreviewRegions(nextRegions: ImageRegion[]): void {
    if (!activeItem) return;
    const previous = activeItem.rectsPx ?? [];
    const visibleIndexById = new Map(
      visibleRectEntries.map(({ index }) => [editorRegionId(index), index])
    );
    const replacements = new Map<number, RectPx>();
    const appended: RectPx[] = [];
    for (const region of nextRegions) {
      const rect = toRectPx(region);
      const previousIndex = region.regionId ? visibleIndexById.get(region.regionId) : undefined;
      if (previousIndex === undefined) {
        appended.push(rect);
      } else {
        replacements.set(previousIndex, rect);
      }
    }
    const rectsPx: RectPx[] = [];
    previous.forEach((rect, index) => {
      if (rect.pageNumber !== activePreviewPageNumber) {
        rectsPx.push(rect);
        return;
      }
      const replacement = replacements.get(index);
      if (replacement) {
        rectsPx.push(replacement);
      }
    });
    const firstAppendedIndex = rectsPx.length;
    rectsPx.push(...appended);
    if (appended.length > 0) {
      activeRegionIndex = firstAppendedIndex;
    } else {
      activeRegionIndex = Math.max(0, Math.min(activeRegionIndex, rectsPx.length - 1));
    }
    queue = queue.map((q) => (q.id === activeItem.id ? { ...q, rectsPx } : q));
  }

  function updateActiveEditorSelection(index: number | null): void {
    if (index === null) return;
    const entry = visibleRectEntries[index];
    if (entry) {
      activeRegionIndex = entry.index;
    }
  }

  function rasterSizesForFinalize(item: QueueItem): Record<number, StudentIntakeRasterSize> {
    const sizes: Record<number, StudentIntakeRasterSize> = {};
    for (const page of item.previewPages ?? []) {
      sizes[page.pageNumber] = {
        widthPx: page.pngWidthPx,
        heightPx: page.pngHeightPx
      };
    }
    return sizes;
  }

  function finalizeStep2WithSharedRoster(id: string) {
    const item = queue.find((q) => q.id === id);
    if (!item) return;
    const roster = [...sharedRosterRows].sort((a, b) => a.sortKey.localeCompare(b.sortKey));
    let bestGuessUserId: string | null = null;
    let bestScore = -1;
    for (const row of roster) {
      const score = rosterFuzzyScore(item.ocrHint ?? '', row);
      if (score > bestScore) {
        bestScore = score;
        bestGuessUserId = row.userId;
      }
    }
    step2Progress = { ...step2Progress, roster: true, fuzzy: true };
    step2WaitingForSharedRoster = false;
    step2PendingItemId = null;
    queue = queue.map((q) =>
      q.id === id
        ? {
            ...q,
            roster,
            bestGuessUserId,
            bestGuessScore: bestGuessUserId ? bestScore : null,
            selectedUserId: bestGuessUserId
          }
        : q
    );
    if (activeId === id) {
      step = 3;
    }
  }

  function finalizeStep2Local(id: string) {
    step2Progress = { ...step2Progress, roster: true, fuzzy: true };
    step2WaitingForSharedRoster = false;
    step2PendingItemId = null;
    queue = queue.map((q) =>
      q.id === id
        ? {
            ...q,
            roster: [],
            localStudentName: q.localStudentName?.trim() || ''
          }
        : q
    );
    if (activeId === id) {
      step = 3;
    }
  }

  async function waitForSharedRoster(id: string) {
    step2WaitingForSharedRoster = true;
    step2PendingItemId = id;
    if (sharedRosterStatus === 'idle' || sharedRosterStatus === 'error') {
      await onEnsureRosterCache?.();
    }
    if (sharedRosterStatus === 'ready') {
      finalizeStep2WithSharedRoster(id);
    }
  }

  async function confirmRegions() {
    if (!previewRenderingComplete) return;
    if (!activeItem?.previewPages?.length || !activeItem.rectsPx?.length) return;
    // Update rectsPt from current px state
    const rectsPt = activeItem.rectsPx.map((r) => rectPxToPt(activeItem, r));
    queue = queue.map((q) => (q.id === activeItem.id ? { ...q, rectsPt } : q));
    step = 2;
    await runStep2Background(activeItem.id);
  }

  // Test hook: lets component tests advance the wizard without relying on jsdom click quirks.
  export async function __testConfirmRegions(): Promise<void> {
    await confirmRegions();
  }

  function selectPreviewPage(pageNumber: number) {
    activePreviewPageNumber = pageNumber;
    const nextIndex = activeItem?.rectsPx?.findIndex((rect) => rect.pageNumber === pageNumber) ?? -1;
    activeRegionIndex = nextIndex >= 0 ? nextIndex : 0;
  }

  function previewPageNumbersForItem(item: QueueItem): number[] {
    const rendered = item.previewPages?.map((page) => page.pageNumber) ?? [];
    return item.previewProgress?.total
      ? previewPagesFromTotal(item.previewProgress.total)
      : rendered;
  }

  function includedPageNumbersForItem(item: QueueItem): number[] {
    return normalizeDesiredPageOrder(previewPageNumbersForItem(item), item.desiredPageOrder);
  }

  function pageIsExcluded(pageNumber: number): boolean {
    return excludedPreviewPageNumbers.includes(pageNumber);
  }

  function canTogglePreviewPage(pageNumber: number): boolean {
    if (!activeItem || !previewPageIsRendered(pageNumber) || !hasExtraPreviewPages) return false;
    return pageIsExcluded(pageNumber) || orderedPreviewPageNumbers.length > expectedPageCount;
  }

  function canDeleteActivePreviewPage(): boolean {
    if (!activePreview) return false;
    return (
      hasExtraPreviewPages &&
      !pageIsExcluded(activePreview.pageNumber) &&
      orderedPreviewPageNumbers.length > expectedPageCount &&
      canTogglePreviewPage(activePreview.pageNumber)
    );
  }

  function deleteActivePreviewPage(): void {
    if (!activePreview || !canDeleteActivePreviewPage()) return;
    togglePreviewPageIncluded(activePreview.pageNumber);
  }

  function togglePreviewPageIncluded(pageNumber: number): void {
    if (!activeItem || !canTogglePreviewPage(pageNumber)) return;
    const currentOrder = normalizeDesiredPageOrder(previewPageNumbers, activeItem.desiredPageOrder);
    const nextOrder = currentOrder.includes(pageNumber)
      ? currentOrder.filter((candidate) => candidate !== pageNumber)
      : [...currentOrder, pageNumber];
    updateQueueItem(activeItem.id, { desiredPageOrder: nextOrder });
    if (!nextOrder.includes(activePreviewPageNumber)) {
      activePreviewPageNumber = nextOrder[0] ?? pageNumber;
    }
  }

  function addRedactionRegion() {
    if (!activeItem || !activePreview) return;
    const w = activePreview.pngWidthPx;
    const h = activePreview.pngHeightPx;
    const rw = Math.max(48, w * 0.22);
    const rh = Math.max(48, h * 0.14);
    const rect: RectPx = {
      pageNumber: activePreviewPageNumber,
      x: Math.max(0, (w - rw) / 2),
      y: Math.max(0, (h - rh) / 2),
      width: rw,
      height: rh
    };
    const nextPx = [...(activeItem.rectsPx ?? []), rect];
    activeRegionIndex = nextPx.length - 1;
    queue = queue.map((q) => (q.id === activeItem.id ? { ...q, rectsPx: nextPx } : q));
  }

  async function runStep2Background(id: string) {
    const item = queue.find((q) => q.id === id);
    if (!item?.rectsPt?.length) return;
    step2Progress = { clipped: false, ocr: false, roster: false, fuzzy: false };
    step2WaitingForSharedRoster = false;
    step2PendingItemId = null;
    queue = queue.map((q) => (q.id === id ? { ...q, step2Error: null } : q));
    try {
      // Clip thumbnails
      const cropPngsBase64 = await promiseWithTimeout(
        transientClipPdfRectsPngBase64(item.pdfPath, item.rectsPt, 2.0),
        step2OperationTimeoutMs,
        'Timed out clipping redaction regions.'
      );
      step2Progress = { ...step2Progress, clipped: true };

      // OCR is only useful as a transient roster hint; local projects use manual entry.
      const ocrResult =
        lmsLinked && cropPngsBase64.length > 0
          ? await promiseWithTimeout(
              transientScansOcrHint(cropPngsBase64[0]),
              step2OperationTimeoutMs,
              'Timed out reading the name region.'
            )
          : null;
      const ocrHint = ocrResult?.hintText ?? '';
      const ocrSegmentCount = ocrResult?.segmentCount ?? null;
      step2Progress = { ...step2Progress, ocr: true };

      queue = queue.map((q) =>
        q.id === id
          ? {
              ...q,
              cropPngsBase64,
              ocrHint,
              ocrSegmentCount,
              roster: [],
              localStudentName: lmsLinked ? q.localStudentName : ''
            }
          : q
      );
      if (lmsLinked) {
        await promiseWithTimeout(
          waitForSharedRoster(id),
          step2OperationTimeoutMs,
          'Timed out waiting for the course roster.'
        );
      } else {
        finalizeStep2Local(id);
      }
    } catch (err) {
      queue = queue.map((q) => (q.id === id ? { ...q, step2Error: String(err) } : q));
      return;
    }
  }

  async function confirmAssociation() {
    if (!activeItem) return;
    const localStudentName = localStudentNameForFinalization(activeItem);
    const validationError = finalizationValidationError(activeItem, localStudentName);
    if (validationError === 'skip') return;
    if (validationError) {
      rosterError = validationError;
      return;
    }
    if (workerBusyForFinalization) {
      step4Error = workerBusyFinalizeMessage;
      return;
    }
    step4Error = null;
    step = 4;
    step4Progress = {
      redactPdf: false,
      ingestExam: false
    };
    step4Percent = {
      redactPdf: 0,
      ingestExam: 0
    };
    await tick();
    // Parent: resolveLmsStudentRef (await), then run_student_intake job id + job_finished — like setup → analyze.
    let finalized: StudentIntakeFinalizeResult | null | undefined;
    try {
      finalized = await onFinalizeSubmission?.(finalizationPayload(activeItem, localStudentName));
    } catch (error) {
      step4Error = String(error);
      return;
    }
    if (!finalized) {
      step4Error = $shellState.lastRuntimeError ?? finalizeFailedFallbackMessage;
      return;
    }
    const { workspaceState, studentRef, bindingTokenHex } = finalized;
    step4Progress = {
      redactPdf: true,
      ingestExam: true
    };
    step4Percent = {
      redactPdf: 100,
      ingestExam: 100
    };
    let canonicalPdfPath: string | null = null;
    let canonicalPageCount: number | null = null;
    let examPagePaths: string[] | null = null;
    let canonicalWarnings: WorkspaceWarning[] = [];
    const intakeRow =
      workspaceState.studentIntake?.items?.find((item) => item.studentRef === studentRef) ?? null;
    canonicalPdfPath = intakeRow?.canonicalPdfPath ?? null;
    canonicalPageCount = typeof intakeRow?.pageCount === 'number' ? intakeRow.pageCount : null;
    const paths = intakeRow?.examPagePaths?.filter((p) => typeof p === 'string' && p.trim().length > 0);
    examPagePaths = paths && paths.length > 0 ? paths : null;
    canonicalWarnings = Array.isArray(intakeRow?.warnings) ? intakeRow.warnings : [];
    queue = queue.map((q) =>
      q.id === activeItem.id
        ? {
            ...q,
            studentRef,
            canonicalPdfPath: canonicalPdfPath ?? null,
            canonicalPageCount,
            examPagePaths,
            bindingTokenHex,
            desiredPageOrder: normalizeDesiredPageOrder(
              previewPageNumbers,
              activeItem.desiredPageOrder
            )
          }
        : q
    );
    queue = queue.map((q) =>
      q.id === activeItem.id ? { ...q, canonicalWarnings } : q
    );
    await onSubmissionCompleted?.({
      canvasUserId: activeItem.selectedUserId ?? null,
      studentRef
    });
    step = 5;
    queue = queue.map((q) => (q.id === activeItem.id ? { ...q, status: 'done' } : q));
  }

  function localStudentNameForFinalization(item: QueueItem): string {
    return item.localStudentName?.trim() || '';
  }

  function finalizationValidationError(item: QueueItem, localStudentName: string): string | 'skip' | null {
    if (!item.confirmed) return 'skip';
    if (lmsLinked && !item.selectedUserId) return 'skip';
    if (!lmsLinked && !localStudentName) {
      return 'Enter a student name before finalizing intake.';
    }
    if (lmsLinked && (sharedRosterStatus !== 'ready' || !courseIdTrimmed)) {
      return sharedRosterMessage ??
        'The course roster is not ready yet. Wait for it to finish loading before preparing this submission.';
    }
    if (expectedPageCount > 0 && includedPageNumbersForItem(item).length < expectedPageCount) {
      return 'This submission has fewer selected pages than the template. Replace or rescan the PDF before finalizing intake.';
    }
    return null;
  }

  function finalizationPayload(item: QueueItem, localStudentName: string): StudentIntakeFinalizePayload {
    return {
      rawPdfPath: item.pdfPath,
      courseId: lmsLinked ? courseIdTrimmed : null,
      canvasUserId: lmsLinked ? item.selectedUserId! : null,
      localStudentName: lmsLinked ? null : localStudentName,
      desiredPageOrder: includedPageNumbersForItem(item),
      redactionRegionsPx: redactionRegionsForFinalize(item),
      rasterSizesByPage: rasterSizesForFinalize(item)
    };
  }

  function nextExam() {
    const idx = queue.findIndex((q) => q.id === activeId);
    const next = queue.slice(idx + 1).find((q) => q.status !== 'done') ?? null;
    activeId = next?.id ?? null;
    step = 1;
    ensureActive();
    if (activeId) void beginItem(activeId);
  }
</script>

<svelte:window
  onpointerup={finishPreviewPagePointerDrag}
  onpointercancel={endPreviewPageDrag}
/>

<section
  class="bg-surface-panel px-8 py-2"
  aria-busy={busyAction !== null}
>

<div class="mx-auto max-w-6xl space-y-6">
    {#if !prerequisitesMet}
      <div class="rounded-3xl border border-workspace-border bg-card/20 px-6 py-6 text-workspace-text-secondary">
        Student submissions become available after privacy regions are set, questions are reviewed, and rubrics are approved.
      </div>
    {:else}
      {#if queue.length === 0}
        <div class="rounded-3xl border border-workspace-border bg-card/10 px-6 py-6 text-workspace-text-secondary">
          Use the left sidebar to upload or drop a student PDF.
        </div>
      {/if}

      {#if activeItem}
          <div class="overflow-x-auto pb-1">
            <div class="flex min-w-max items-center gap-1 text-sm">
              <div
                class={`${stepPillBaseClass} ${
                  displayStep === 1
                    ? `${activeStepPillClass} ${stepActiveTextClass}`
                    : displayStep > 1
                      ? stepProgressSuccessTextClass
                      : stepMutedTextClass
                }`}
              >
                1 Preview & regions
              </div>
              <div
                class={`h-px w-6 shrink-0 ${displayStep > 1 ? stepConnectorDoneClass : stepConnectorPendingClass}`}
              ></div>
              <div
                class={`${stepPillBaseClass} ${
                  displayStep === 2
                    ? `${activeStepPillClass} ${stepActiveTextClass}`
                    : displayStep > 2
                      ? stepProgressSuccessTextClass
                      : stepMutedTextClass
                }`}
              >
                {lmsLinked ? '2 Read name' : '2 Enter name'}
              </div>
              <div
                class={`h-px w-6 shrink-0 ${displayStep > 2 ? stepConnectorDoneClass : stepConnectorPendingClass}`}
              ></div>
              <div
                class={`${stepPillBaseClass} ${
                  displayStep === 3
                    ? `${activeStepPillClass} ${stepActiveTextClass}`
                    : displayStep > 3
                      ? stepProgressSuccessTextClass
                      : stepMutedTextClass
                }`}
              >
                3 Match student
              </div>
              <div
                class={`h-px w-6 shrink-0 ${displayStep > 3 ? stepConnectorDoneClass : stepConnectorPendingClass}`}
              ></div>
              <div
                class={`${stepPillBaseClass} ${
                  displayStep === 4
                    ? `${activeStepPillClass} ${stepActiveTextClass}`
                    : displayStep > 4
                      ? stepProgressSuccessTextClass
                      : stepMutedTextClass
                }`}
              >
                4 Prepare submission
              </div>
              <div
                class={`h-px w-6 shrink-0 ${displayStep > 4 ? stepConnectorDoneClass : stepConnectorPendingClass}`}
              ></div>
              <div
                class={`${stepPillBaseClass} ${
                  displayStep === 5
                    ? `${activeStepPillClass} ${stepActiveTextClass}`
                    : stepMutedTextClass
                }`}
              >
                5 Complete
              </div>
            </div>

          {#if displayStep === 1}
            <p class="mt-4 shell-body text-workspace-text-secondary">
              The first region is used to identify the student name; any additional regions are privacy masks. Use the page pills to place or adjust regions on every sheet of the scan. Drag those same page pills to set the final page order; each pill keeps its original page label so changes stay visible.
            </p>
            {#if step1PageMismatchMessage}
              <InlineMessage
                tone={hasMissingPreviewPages ? 'error' : 'warning'}
                class="mt-4 rounded-2xl px-4 py-3"
              >
                {step1PageMismatchMessage}
              </InlineMessage>
            {/if}
            <div class="mt-6 grid gap-6 lg:grid-cols-2">
              {#if activePreview}
                <div class="min-w-0">
                  <div class="flex flex-wrap items-center justify-between gap-3">
                    <div class="flex flex-wrap items-center gap-2">
                      {#each displayedPreviewPageNumbers as pageNumber (pageNumber)}
                      {@const pageRendered = renderedPreviewPageNumbers.includes(pageNumber)}
                      {@const excluded = excludedPreviewPageNumbers.includes(pageNumber)}
                      <button
                        class={`inline-flex h-9 select-none items-center gap-2 rounded-xl border px-3 text-xs font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
                          excluded
                            ? 'border-workspace-border bg-card/10 text-workspace-text-muted opacity-75 line-through hover:bg-muted/20'
                            : pageNumber === activePreviewPageNumber
                              ? 'border-workspace-border-strong bg-workspace-sidebar-active text-workspace-text-primary'
                              : dragOverPreviewPageNumber === pageNumber
                                ? 'border-[var(--message-info-border)] bg-[var(--message-info-bg)]/30 text-workspace-text-primary'
                                : 'border-workspace-border bg-card/20 text-workspace-text-secondary hover:bg-muted/20'
                        }`}
                        type="button"
                        disabled={!pageRendered}
                        aria-label={excluded ? `Restore page ${pageNumber}` : `Page ${pageNumber}`}
                        title={excluded ? `Click to restore page ${pageNumber}` : undefined}
                        onpointerdown={(event) => beginPreviewPagePointerDrag(event, pageNumber)}
                        onpointerenter={() => previewPagePointerEnter(pageNumber)}
                        onpointerleave={() => previewPagePointerLeave(pageNumber)}
                        onclick={() => {
                          if (!pageRendered) return;
                          if (excluded) {
                            togglePreviewPageIncluded(pageNumber);
                            return;
                          }
                          selectPreviewPage(pageNumber);
                        }}
                      >
                        <span class={excluded ? 'text-workspace-text-muted' : 'cursor-grab text-workspace-text-muted'} aria-hidden="true">::</span>
                        <span>Page {pageNumber}</span>
                      </button>
                      {/each}
                    </div>
                  </div>
                  <div class="mt-3">
                    <ImageRegionEditor
                      imageSrc={previewUrl(activePreview)}
                      imageAlt="Preview page"
                      pageNumber={activePreviewPageNumber}
                      imageWidth={activePreview.pngWidthPx}
                      imageHeight={activePreview.pngHeightPx}
                      regions={editorRedactionRegions}
                      ariaLabel="Redaction region editor"
                      regionPresentation={intakeRedactionPresentation}
                      deleteTitle="Delete redaction region?"
                      deleteDescription="Remove this redaction region from the intake preview."
                      onRegionsChange={updateActivePreviewRegions}
                      onSelectionChange={updateActiveEditorSelection}
                    />
                  </div>
                </div>
                <div class="min-w-0">
                  <div class="flex flex-wrap items-center justify-between gap-2" aria-label="Preview actions">
                    <DesktopButton
                      size="compact"
                      type="button"
                      onclick={() => addRedactionRegion()}
                      disabled={!activePreview}
                    >
                      Add Redaction
                    </DesktopButton>
                    {#if hasUnresolvedPageMismatch}
                      <DesktopButton
                        size="compact"
                        type="button"
                        variant="destructive"
                        onclick={deleteActivePreviewPage}
                        disabled={!canDeleteActivePreviewPage()}
                      >
                        Delete Page {activePreview.pageNumber}
                      </DesktopButton>
                    {:else}
                      <DesktopButton size="compact" type="button" onclick={() => void confirmRegions()} disabled={!activePreview || !previewRenderingComplete}>
                        {previewRenderingComplete ? 'Confirm for All Pages' : `Wait to Confirm - ${previewRenderingPercent}%`}
                      </DesktopButton>
                    {/if}
                  </div>
                  {#if activePreview && visibleRectEntries.length > 0}
                    <div class="mt-3 space-y-3">
                      {#each visibleRectEntries as { rect, index }, idx (index)}
                        <div class="relative">
                          {#if index === 0}
                            <div class="absolute left-2 top-2 z-10 rounded-md bg-[var(--workspace-selection-name-border)] px-2 py-1 text-[11px] font-semibold uppercase leading-none tracking-wider text-[var(--workspace-selection-name-foreground)]">
                              Name identification
                            </div>
                          {/if}
                          <div
                            class={`relative overflow-hidden rounded-lg border bg-surface-page ${
                              index === activeRegionIndex
                                ? 'border-workspace-border-strong'
                                : 'border-workspace-border'
                            }`}
                            style={`aspect-ratio: ${Math.max(1, rect.width)} / ${Math.max(1, rect.height)};`}
                          >
                            <img
                              src={previewUrl(activePreview)}
                              alt={index === 0 ? `Cropped region preview ${idx + 1} - Name identification crop preview` : `Cropped region preview ${idx + 1}`}
                              class="absolute left-0 top-0 block h-auto max-w-none select-none"
                              style={`width: ${(activePreview.pngWidthPx / rect.width) * 100}%; transform: translate(-${(rect.x / activePreview.pngWidthPx) * 100}%, -${(rect.y / activePreview.pngHeightPx) * 100}%);`}
                            />
                          </div>
                        </div>
                      {/each}
                    </div>
                    <div class="mt-3 shell-meta text-workspace-text-muted">
                      Live crops from name and privacy regions on the selected page.
                    </div>
                  {:else if visibleRectEntries.length === 0}
                    <div class="mt-3 shell-body text-workspace-text-secondary">
                      No redaction regions configured on this page.
                    </div>
                  {:else}
                    <div class="mt-3 shell-body text-workspace-text-secondary">
                      Move/resize a region to preview the crop.
                    </div>
                  {/if}
                </div>
              {:else if activeItem.previewError}
                <InlineMessage tone="error" class="rounded-3xl p-5">
                  <div class="shell-section-title">Preview failed</div>
                  <div class="mt-2 shell-body break-words">{activeItem.previewError}</div>
                  <div class="mt-4">
                    <DesktopButton type="button" onclick={() => void loadPreviewAndDefaultRects(activeItem.id)}>
                      Retry preview
                    </DesktopButton>
                  </div>
                </InlineMessage>
              {:else}
                <div class="shell-body text-workspace-text-secondary">{previewProgressLabel(activeItem.previewProgress)}</div>
              {/if}
            </div>
          {:else if displayStep === 2}
            <div class="mt-8 rounded-3xl border border-workspace-border bg-workspace-empty px-6 py-6">
              <div class="shell-body text-workspace-text-secondary">Preparing student match — no action needed</div>
              {#if activeItem?.step2Error}
                <InlineMessage tone="error" class="mt-3 rounded-xl">
                  {activeItem.step2Error}
                </InlineMessage>
                <div class="mt-4">
                  <DesktopButton type="button" onclick={() => void runStep2Background(activeItem.id)}>
                    Retry background step
                  </DesktopButton>
                </div>
              {/if}
              <div class="mt-6 space-y-2 text-sm text-workspace-text-secondary">
                <div class={taskRowClass(step2Progress.clipped, currentStep2Task === 'clipped')}>
                  {#if showTaskIndicator(step2Progress.clipped, currentStep2Task === 'clipped') === 'done'}
                    <span class="inline-flex size-4 shrink-0 items-center justify-center rounded-full border border-[var(--message-success-border)] bg-[var(--message-success-bg)] text-[10px] font-bold">v</span>
                  {:else if showTaskIndicator(step2Progress.clipped, currentStep2Task === 'clipped') === 'current'}
                    <span class={spinnerClass}></span>
                  {:else}
                    <span class="inline-block size-3 shrink-0 rounded-full border border-workspace-border"></span>
                  {/if}
                  <span>Reading selected regions</span>
                </div>
                <div class={taskRowClass(step2Progress.ocr, currentStep2Task === 'ocr')}>
                  {#if showTaskIndicator(step2Progress.ocr, currentStep2Task === 'ocr') === 'done'}
                    <span class="inline-flex size-4 shrink-0 items-center justify-center rounded-full border border-[var(--message-success-border)] bg-[var(--message-success-bg)] text-[10px] font-bold">v</span>
                  {:else if showTaskIndicator(step2Progress.ocr, currentStep2Task === 'ocr') === 'current'}
                    <span class={spinnerClass}></span>
                  {:else}
                    <span class="inline-block size-3 shrink-0 rounded-full border border-workspace-border"></span>
                  {/if}
                  <span>{lmsLinked ? 'Reading the name region' : 'Name reading skipped'}</span>
                </div>
                <div class={taskRowClass(step2Progress.roster, currentStep2Task === 'roster')}>
                  {#if showTaskIndicator(step2Progress.roster, currentStep2Task === 'roster') === 'done'}
                    <span class="inline-flex size-4 shrink-0 items-center justify-center rounded-full border border-[var(--message-success-border)] bg-[var(--message-success-bg)] text-[10px] font-bold">v</span>
                  {:else if showTaskIndicator(step2Progress.roster, currentStep2Task === 'roster') === 'current'}
                    <span class={spinnerClass}></span>
                  {:else}
                    <span class="inline-block size-3 shrink-0 rounded-full border border-workspace-border"></span>
                  {/if}
                  <span>{lmsLinked ? 'Course roster loaded' : 'Name entry ready'}</span>
                </div>
                <div class={taskRowClass(step2Progress.fuzzy, currentStep2Task === 'fuzzy')}>
                  {#if showTaskIndicator(step2Progress.fuzzy, currentStep2Task === 'fuzzy') === 'done'}
                    <span class="inline-flex size-4 shrink-0 items-center justify-center rounded-full border border-[var(--message-success-border)] bg-[var(--message-success-bg)] text-[10px] font-bold">v</span>
                  {:else if showTaskIndicator(step2Progress.fuzzy, currentStep2Task === 'fuzzy') === 'current'}
                    <span class={spinnerClass}></span>
                  {:else}
                    <span class="inline-block size-3 shrink-0 rounded-full border border-workspace-border"></span>
                  {/if}
                  <span>{lmsLinked ? 'Suggested match ready' : 'Typed name ready'}</span>
                </div>
              </div>
              {#if !step2Progress.roster && (step2WaitingForSharedRoster || sharedRosterMessage)}
                <InlineMessage tone="muted" class="mt-4 rounded-xl">
                  {#if step2WaitingForSharedRoster && sharedRosterStatus === 'loading'}
                    Waiting for the course roster…
                  {:else if sharedRosterMessage}
                    {sharedRosterMessage}
                  {:else}
                    Waiting for the course roster…
                  {/if}
                </InlineMessage>
              {/if}
            </div>
          {:else if displayStep === 3}
            <div class="mt-6 space-y-4">
              <div class="shell-body text-workspace-text-secondary">
                {lmsLinked
                  ? 'ScriptScore read a possible student name from the scan. Confirm the right roster entry before preparing the submission.'
                  : 'Type the student name before preparing the submission.'}
              </div>
              {#if rosterError}
                <InlineMessage tone="error" class="rounded-xl">
                  {rosterError}
                </InlineMessage>
              {/if}
              <div class="grid gap-4 lg:grid-cols-2">
                <div class="rounded-3xl border border-workspace-border bg-workspace-empty p-4">
                  <div class="shell-meta text-workspace-text-muted">Name &amp; privacy region(s)</div>
                  <div class="mt-3 space-y-3">
                    {#each activeItem.cropPngsBase64 ?? [] as crop, idx (idx)}
                      <PagePreviewFrame
                        class="w-full border-workspace-border"
                        src={`data:image/png;base64,${crop}`}
                        alt={`Redaction crop ${idx + 1}`}
                      />
                    {:else}
                      <div class="shell-body text-workspace-text-secondary">No crop previews available.</div>
                    {/each}
                  </div>
                </div>
                <div class="rounded-3xl border border-workspace-border bg-workspace-empty p-4">
                  {#if lmsLinked}
                    <div class="shell-meta text-workspace-text-muted">Name read from scan</div>
                    <div class="mt-2 font-mono text-sm text-workspace-text-primary">
                      {activeItem.ocrHint ?? ''}
                      {#if activeItem.ocrSegmentCount != null}
                        <span class="ml-2 text-workspace-text-muted">
                          ({activeItem.ocrSegmentCount} region{activeItem.ocrSegmentCount === 1 ? '' : 's'})
                        </span>
                      {/if}
                    </div>
                    {#if activeItem.bestGuessScore !== null && activeItem.bestGuessScore !== undefined}
                      <div class="mt-3 shell-meta text-workspace-text-muted">
                        suggested match
                        <StatusBadge tone="success" class="ml-2 min-h-0 px-2 py-1 text-[11px]">
                          {activeItem.bestGuessScore}%
                        </StatusBadge>
                      </div>
                    {/if}
                  {/if}
                  {#if lmsLinked}
                    <div class="shell-meta text-workspace-text-muted">Course roster — choose student</div>
                    {@const rosterOptions = (activeItem.roster ?? []).map((row) => ({
                      value: row.userId,
                      label: row.displayName
                    }))}
                    <SelectField
                      class="mt-3"
                      value={activeItem.selectedUserId ?? ''}
                      options={rosterOptions}
                      searchable
                      searchPlaceholder="Search roster"
                      allowEmpty
                      emptyLabel="— select —"
                      placeholder="— select —"
                      noOptionsLabel="No matching students."
                      ariaLabel="Course roster student"
                      popoverPlacement="fixed"
                      optionListClass="mt-2"
                      onChange={async (v) => {
                        queue = queue.map((q) =>
                          q.id === activeItem.id ? { ...q, selectedUserId: v, associationReplaceWarning: null } : q
                        );
                        await tick();
                        void updateAssociationReplaceWarning();
                      }}
                    />
                  {:else}
                    <label class="shell-meta text-workspace-text-muted" for={`local-student-name-${activeItem.id}`}>
                      Student name
                    </label>
                    <input
                      id={`local-student-name-${activeItem.id}`}
                      class="mt-3 min-h-11 w-full rounded-2xl border border-border bg-card/30 px-4 text-sm text-foreground outline-none transition-colors focus:border-ring"
                      value={activeItem.localStudentName ?? ''}
                      placeholder="Type student name"
                      oninput={(e) => {
                        const v = (e.currentTarget as HTMLInputElement).value;
                        rosterError = null;
                        queue = queue.map((q) =>
                          q.id === activeItem.id ? { ...q, localStudentName: v } : q
                        );
                      }}
                    />
                  {/if}
                  {#if activeItem.associationReplaceWarning}
                    <InlineMessage tone="warning" class="mt-3 rounded-2xl px-4 py-3">
                      {activeItem.associationReplaceWarning}
                    </InlineMessage>
                  {/if}
                  {#if step4Error}
                    <InlineMessage tone="error" class="mt-3 rounded-2xl px-4 py-3">
                      {step4Error}
                    </InlineMessage>
                  {/if}
                  <label class="mt-4 flex items-center gap-3 text-sm text-workspace-text-secondary">
                    <input
                      type="checkbox"
                      checked={activeItem.confirmed ?? false}
                      onchange={(e) => {
                        const v = (e.currentTarget as HTMLInputElement).checked;
                        queue = queue.map((q) => (q.id === activeItem.id ? { ...q, confirmed: v } : q));
                      }}
                    />
                    {lmsLinked
                      ? 'I confirm this submission belongs to the selected student'
                      : 'I confirm this submission belongs to the typed student name'}
                  </label>
                </div>
              </div>
              <div class="mt-4 flex justify-end">
                <DesktopButton
                  type="button"
                  disabled={
                    !activeItem.confirmed ||
                    hasMissingPreviewPages ||
                    (lmsLinked
                      ? !activeItem.selectedUserId
                      : !(activeItem.localStudentName?.trim()))
                  }
                  onclick={() => void confirmAssociation()}
                >
                  Confirm student →
                </DesktopButton>
              </div>
            </div>
          {:else if displayStep === 4}
            <div class="mt-8 rounded-3xl border border-workspace-border bg-workspace-empty px-6 py-6">
              <div class="shell-body text-workspace-text-secondary">
                Creating the student PDF and page previews — no action needed
              </div>
              <div class="mt-6 space-y-3">
                <HorizontalProgressBar
                  label="Mask private information"
                  progress={step4Percent.redactPdf}
                  active={!step4Progress.redactPdf}
                  complete={step4Progress.redactPdf}
                  tone={step4Progress.redactPdf ? 'success' : 'info'}
                />
                <HorizontalProgressBar
                  label="Prepare exam pages"
                  progress={step4Percent.ingestExam}
                  active={step4Progress.redactPdf && !step4Progress.ingestExam}
                  complete={step4Progress.ingestExam}
                  tone={step4Progress.ingestExam ? 'success' : step4Progress.redactPdf ? 'info' : 'muted'}
                />
              </div>
              {#if step4Error}
                <InlineMessage tone="error" class="mt-4 rounded-2xl px-4 py-3">
                  {step4Error}
                </InlineMessage>
                <div class="mt-4 flex flex-wrap justify-end gap-3">
                  <DesktopButton
                    type="button"
                    onclick={() => {
                      step4Error = null;
                      step = 3;
                    }}
                  >
                    Back to student match
                  </DesktopButton>
                  <DesktopButton
                    type="button"
                    onclick={() => void confirmAssociation()}
                  >
                    Try again
                  </DesktopButton>
                </div>
              {/if}
            </div>
          {:else if displayStep === 5}
            <p class="mt-4 shell-body text-workspace-text-secondary">
              This submission is ready. The page order you confirmed in Step 1 has been applied.
            </p>
            <div class="mt-6 space-y-4">
                <div class="shell-title-lg text-workspace-text-primary">Submission ready</div>
                <div class="shell-body text-workspace-text-secondary">
                  The prepared PDF and exam page previews are ready for the workflow.
                </div>
                {#if activeItem.bindingTokenHex}
                  <div class="rounded-2xl border border-workspace-border bg-workspace-empty px-4 py-3 font-mono text-sm text-workspace-text-secondary">
                    hmac: {activeItem.bindingTokenHex}
                  </div>
                {/if}
                <div class="flex flex-wrap items-center gap-3">
                  {#if examPageCount > 0}
                    <DesktopButton
                      type="button"
                      onclick={() => {
                        const nextShow = !showExamView;
                        showExamView = nextShow;
                        if (nextShow) examViewPageNumber = 1;
                      }}
                    >
                      {showExamView ? 'Hide exam' : 'View exam'}
                    </DesktopButton>
                  {/if}
                  <DesktopButton
                    type="button"
                    disabled={!hasNextQueued}
                    onclick={() => void nextExam()}
                  >
                    Process next exam →
                  </DesktopButton>
                </div>
                {#if activeItem.canonicalPdfPath}
                  <div class="shell-meta text-workspace-text-muted">
                    {activeItem.canonicalPdfPath.split('/').slice(-1)[0]}
                  </div>
                {/if}
                {#if (activeItem.canonicalWarnings?.filter((w) => w.code !== 'student_intake_replaced').length ?? 0) > 0}
                  <InlineMessage tone="warning" class="rounded-2xl px-4 py-3">
                    {#each (activeItem.canonicalWarnings ?? []).filter((w) => w.code !== 'student_intake_replaced') as warning, idx (idx)}
                      <div>{warning.message}</div>
                    {/each}
                  </InlineMessage>
                {/if}
                {#if showExamView && examPageCount > 0}
                  <div class="rounded-3xl border border-workspace-border bg-workspace-empty p-4">
                    <div class="flex flex-wrap items-center justify-between gap-3">
                      <div class="shell-meta text-workspace-text-muted">Exam pages</div>
                      <div class="flex items-center gap-2">
                        <DesktopButton
                          type="button"
                          disabled={examViewPageNumber <= 1}
                          onclick={() => {
                            examViewPageNumber = Math.max(1, examViewPageNumber - 1);
                          }}
                        >
                          Prev
                        </DesktopButton>
                        <div class="shell-meta text-workspace-text-muted">
                          p.{examViewPageNumber} / {examPageCount}
                        </div>
                        <DesktopButton
                          type="button"
                          disabled={examViewPageNumber >= examPageCount}
                          onclick={() => {
                            examViewPageNumber = Math.min(examPageCount, examViewPageNumber + 1);
                          }}
                        >
                          Next
                        </DesktopButton>
                      </div>
                    </div>
                    {#if examImageSrc}
                      <PagePreviewFrame
                        class="mt-3 w-full border-workspace-border"
                        src={examImageSrc}
                        alt="Exam page {examViewPageNumber}"
                      />
                    {:else}
                      <div class="mt-3 shell-body text-workspace-text-secondary">No image for this page.</div>
                    {/if}
                  </div>
                {/if}
            </div>
          {/if}
        </div>
      {/if}

      {#if queue.length > 0}
        <div class="rounded-[2rem] border border-workspace-border bg-card/10 px-6 py-5">
          <div class="flex items-center justify-between gap-4">
            <div>
              <div class="shell-section-title text-workspace-text-primary">Queued PDFs</div>
              <div class="shell-body text-workspace-text-secondary">
                {queue.length} submission{queue.length === 1 ? '' : 's'} in this intake session
              </div>
            </div>
            <div class="shell-meta text-workspace-text-muted">
              {queue.filter((item) => item.status === 'done').length} complete
            </div>
          </div>
          <div class="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
            {#each queue as item, index (item.id)}
              <div
                class={`rounded-2xl border px-4 py-3 ${
                  item.id === activeId
                    ? 'border-workspace-border-strong bg-workspace-sidebar-active'
                    : 'border-workspace-border bg-workspace-empty'
                }`}
              >
                <div class="flex items-center justify-between gap-3">
                  <div class="min-w-0">
                    <div class="truncate shell-body font-medium text-workspace-text-primary">
                      {basename(item.pdfPath)}
                    </div>
                    <div class="shell-meta text-workspace-text-muted">Queue #{index + 1}</div>
                  </div>
                  <StatusBadge
                    tone={item.status === 'done' ? 'success' : item.id === activeId ? 'info' : 'muted'}
                    class="h-8 px-3 text-[11px] uppercase tracking-[0.12em]"
                  >
                    {item.status === 'done' ? 'Complete' : item.id === activeId ? 'Active' : 'Queued'}
                  </StatusBadge>
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/if}
    {/if}
  </div>
</section>
