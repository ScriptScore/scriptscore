<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { AlertCircleIcon, CheckListIcon, Download01Icon } from '@hugeicons/core-free-icons';
  import { HugeiconsIcon } from '@hugeicons/svelte';
  import RedactionCanvas from '$lib/RedactionCanvas.svelte';
  import { listCanvasCourses, listLmsAssignmentsForCourse, toDesktopAssetUrl } from '$lib/desktop';
  import { appSettings, isCanvasLmsReady } from '$lib/stores/appSettings';
  import type {
    ExamWorkspaceState,
    LmsAssignmentSummary,
    LmsCourseSummary,
    ProjectConfig,
    TemplateRedactionRegionInput
  } from '$lib/types';
  import { REDACTION_LABEL_NAME_IDENTIFICATION } from '$lib/types';
  import {
    DesktopButton,
    DesktopPopover,
    InlineMessage,
    SelectField,
    Surface,
    TextareaField,
    TextField,
    ToggleRow
  } from './ui';

  export let workspaceState: ExamWorkspaceState;
  export let projectConfig: ProjectConfig;
  export let selectedPageNumber = 1;
  export let busy = false;
  export let alignmentMarksPending = false;
  export let onSelectPage: ((pageNumber: number) => void) | null = null;
  export let onReplaceTemplatePdf: (() => void | Promise<void>) | null = null;
  export let onExportTemplatePdf: (() => void | Promise<void>) | null = null;
  export let onRegionsChange:
    | ((regions: TemplateRedactionRegionInput[]) => void | Promise<void>)
    | null = null;
  export let onConfirmContinue: (() => void) | null = null;
  export let onSaveSetup: (() => void | Promise<void>) | null = null;
  export let onDiscardChanges: (() => void) | null = null;
  export let debugRedactionToggle = false;

  let lmsCourses: LmsCourseSummary[] = [];
  let lmsCoursesError: string | null = null;
  let lmsCoursesLoading = false;
  let lmsCoursesSettingsKey: string | null = null;
  let lmsAssignments: LmsAssignmentSummary[] = [];
  let lmsAssignmentsError: string | null = null;
  let lmsAssignmentsLoading = false;
  let lmsAssignmentsLoadedForCourseId: string | null = null;

  let selectedPageNaturalWidth = 0;
  let selectedPageNaturalHeight = 0;
  let centerViewportHeight = 0;
  let workspaceViewportWidth = currentViewportWidth();
  let alignmentMarksPopoverOpen = false;
  let redactionGuidePopoverOpen = false;
  let localAlignmentMarksPending = false;

  const compactPageSelectorMaxWidth = 1280;
  const stackedSetupMaxWidth = 960;

  const strictnessOptions = [
    { value: 'strict', label: 'strict' },
    { value: 'balanced', label: 'balanced' },
    { value: 'generous', label: 'generous' }
  ];
  const toleranceOptions = [
    { value: 'low', label: 'low' },
    { value: 'medium', label: 'medium' },
    { value: 'high', label: 'high' }
  ];
  const feedbackStyleOptions = [
    { value: 'brief', label: 'brief' },
    { value: 'balanced', label: 'balanced' },
    { value: 'detailed', label: 'detailed' }
  ];

  $: selectedPage =
    workspaceState.templatePreviewArtifacts.find((page) => page.pageNumber === selectedPageNumber) ??
    workspaceState.templatePreviewArtifacts[0] ??
    null;
  $: selectedPageUrl = selectedPage ? toDesktopAssetUrl(selectedPage.imagePath) : '';
  $: pageRegions = workspaceState.redactionRegions.filter(
    (region) => region.pageNumber === selectedPageNumber
  );
  $: redactionPageCount = new Set(workspaceState.redactionRegions.map((region) => region.pageNumber))
    .size;
  $: redactionRegionCount = workspaceState.redactionRegions.length;
  $: nameRegionCount = workspaceState.redactionRegions.filter(
    (region) => region.label === REDACTION_LABEL_NAME_IDENTIFICATION
  ).length;
  $: privacyRegionCount = redactionRegionCount - nameRegionCount;
  function formatRedactionSummary(nameCount: number, privacyCount: number, pageCount: number): string {
    if (nameCount + privacyCount === 0) {
      return 'No Privacy Regions Created';
    }
    const namePart = `${nameCount} Name${nameCount === 1 ? '' : 's'}`;
    const privacyPart = privacyCount > 0 ? `, ${privacyCount} Privacy` : '';
    const pagePart = `Across ${pageCount} page${pageCount === 1 ? '' : 's'}`;
    return `${namePart}${privacyPart} \u00b7 ${pagePart}`;
  }
  $: redactionSummary = formatRedactionSummary(nameRegionCount, privacyRegionCount, redactionPageCount);
  $: if (redactionRegionCount > 0 && !projectConfig.redactionRequired) {
    projectConfig.redactionRequired = true;
  }
  $: selectedPageAspectRatio =
    selectedPageNaturalWidth > 0 && selectedPageNaturalHeight > 0
      ? selectedPageNaturalWidth / selectedPageNaturalHeight
      : 0.77;
  $: centerColumnWidth = Math.max(
    420,
    Math.min(920, Math.round(Math.max(320, centerViewportHeight - 48) * selectedPageAspectRatio + 48))
  );
  $: compactPageSelector = workspaceViewportWidth < compactPageSelectorMaxWidth;
  $: stackedSetupLayout = workspaceViewportWidth < stackedSetupMaxWidth;
  $: gridStyle = setupGridStyle(compactPageSelector, stackedSetupLayout, centerColumnWidth);
  $: canContinue = !busy && (!projectConfig.redactionRequired || workspaceState.redactionRegions.length > 0);
  $: continueDisabledReason =
    !busy && projectConfig.redactionRequired && workspaceState.redactionRegions.length === 0
      ? 'Create at least one privacy region before continuing.'
      : null;
  $: arucoStatus = workspaceState.arucoStatus ?? {
    state: 'unknown',
    totalMarkerCount: 0,
    pages: []
  };
  $: arucoMarkersDetected =
    arucoStatus.state === 'detected' || (arucoStatus.totalMarkerCount ?? 0) > 0;
  $: effectiveAlignmentMarksPending = alignmentMarksPending || localAlignmentMarksPending;
  $: if (arucoMarkersDetected && localAlignmentMarksPending) {
    localAlignmentMarksPending = false;
  }
  $: alignmentMarksLabel = arucoMarkersDetected
    ? 'Has Alignment Marks'
    : effectiveAlignmentMarksPending
      ? 'Creating Alignment Marks...'
      : arucoStatus.state === 'not_detected'
      ? 'No Alignment Marks'
      : 'Alignment Marks Unknown';
  $: canExportTemplatePdf = !busy && workspaceState.templatePreviewArtifacts.length > 0;
  $: replaceTemplateLabel = busy
    ? 'Replacing...'
    : workspaceState.templatePreviewArtifacts.length > 0
      ? 'Replace Template PDF'
      : 'Choose Template PDF';

  function currentViewportWidth(): number {
    if (typeof window === 'undefined') {
      return 1440;
    }
    return window.innerWidth;
  }

  function handleWindowResize() {
    workspaceViewportWidth = currentViewportWidth();
  }

  function setupGridStyle(compact: boolean, stacked: boolean, canvasWidth: number): string {
    if (stacked) {
      return [
        'grid-template-columns: 5rem minmax(0, 1fr);',
        'grid-template-rows: minmax(18rem, 42vh) auto;'
      ].join(' ');
    }
    if (compact) {
      return 'grid-template-columns: 5.25rem minmax(24rem, 1fr) minmax(18rem, 23rem);';
    }
    return `grid-template-columns: minmax(11rem, 15.5rem) ${canvasWidth}px minmax(20rem, 1fr);`;
  }

  async function requestAlignmentStampExport() {
    alignmentMarksPopoverOpen = false;
    if (!arucoMarkersDetected && arucoStatus.state === 'not_detected') {
      localAlignmentMarksPending = true;
    }
    try {
      await onExportTemplatePdf?.();
    } finally {
      if (!alignmentMarksPending) {
        localAlignmentMarksPending = false;
      }
    }
  }

  function syntheticLinkedCourse(config: ProjectConfig): LmsCourseSummary {
    const id = config.lmsCourseId?.trim() ?? '';
    const code = config.courseCode?.trim() ?? '';
    return {
      lmsCourseId: id,
      name: code.length > 0 ? code : `LMS course (${id})`,
      courseCode: code.length > 0 ? code : null
    };
  }

  function mergeCoursesWithCurrent(
    rows: LmsCourseSummary[],
    config: ProjectConfig
  ): LmsCourseSummary[] {
    const currentId = config.lmsCourseId?.trim() ?? '';
    if (!currentId) {
      return rows;
    }
    if (rows.some((r) => r.lmsCourseId === currentId)) {
      return rows;
    }
    return [syntheticLinkedCourse(config), ...rows];
  }

  function courseSettingsKey(): string | null {
    const settings = $appSettings;
    if (!isCanvasLmsReady(settings)) {
      return null;
    }
    return `${settings.lmsCanvasBaseUrl.trim()}|${(settings.lmsCanvasApiKey ?? '').trim()}`;
  }

  async function refreshSetupLmsCourses(settingsKey: string | null) {
    if (!settingsKey) {
      lmsCoursesLoading = false;
      lmsCoursesError = null;
      lmsCourses = projectConfig.lmsCourseId?.trim() ? [syntheticLinkedCourse(projectConfig)] : [];
      return;
    }
    const settings = $appSettings;
    const url = settings.lmsCanvasBaseUrl.trim();
    const token = (settings.lmsCanvasApiKey ?? '').trim();
    lmsCoursesLoading = true;
    lmsCoursesError = null;
    try {
      const rows = await listCanvasCourses(url, token);
      lmsCourses = mergeCoursesWithCurrent(rows, projectConfig);
    } catch (error) {
      lmsCourses = [syntheticLinkedCourse(projectConfig)];
      lmsCoursesError = String(error);
    } finally {
      lmsCoursesLoading = false;
    }
  }

  $: {
    const nextSettingsKey = courseSettingsKey();
    if (nextSettingsKey !== lmsCoursesSettingsKey) {
      lmsCoursesSettingsKey = nextSettingsKey;
      void refreshSetupLmsCourses(nextSettingsKey);
    }
  }

  $: lmsCourseSelectorAvailable =
    isCanvasLmsReady($appSettings);
  $: courseOptions = lmsCourses.map((course) => ({
    value: course.lmsCourseId,
    label: course.name,
    subitem: course.courseCode
  }));

  function selectSetupLmsCourse(lmsCourseId: string) {
    if (!lmsCourseId) {
      projectConfig.lmsCourseId = null;
      projectConfig.lmsAssignmentId = null;
      return;
    }
    const row = lmsCourses.find((c) => c.lmsCourseId === lmsCourseId);
    if (row) {
      if (projectConfig.lmsCourseId !== row.lmsCourseId) {
        projectConfig.lmsAssignmentId = null;
        lmsAssignmentsLoadedForCourseId = null;
      }
      projectConfig.lmsCourseId = row.lmsCourseId;
      projectConfig.courseCode = row.courseCode ?? '';
    }
  }

  function assignmentLabel(assignment: LmsAssignmentSummary): string {
    if (typeof assignment.pointsPossible === 'number') {
      return `${assignment.name} (${assignment.pointsPossible} pts)`;
    }
    return assignment.name;
  }

  $: selectedSetupAssignment = projectConfig.lmsAssignmentId?.trim()
    ? lmsAssignments.find((a) => a.assignmentId === projectConfig.lmsAssignmentId?.trim()) ?? null
    : null;
  $: assignmentTriggerLabel = selectedSetupAssignment
    ? assignmentLabel(selectedSetupAssignment)
    : 'Select assignment';
  $: assignmentOptions = lmsAssignments.map((assignment) => ({
    value: assignment.assignmentId,
    label: assignment.name,
    subitem:
      assignment.pointsPossible == null
        ? assignment.assignmentId
        : `${assignment.assignmentId} · ${assignment.pointsPossible} pts`
  }));

  function selectSetupAssignment(assignmentId: string) {
    projectConfig.lmsAssignmentId = assignmentId || null;
  }

  async function refreshSetupLmsAssignments(courseId: string | null) {
    if (!courseId) {
      lmsAssignments = [];
      lmsAssignmentsError = null;
      lmsAssignmentsLoading = false;
      lmsAssignmentsLoadedForCourseId = null;
      return;
    }
    const requestedCourseId = courseId;
    lmsAssignmentsLoading = true;
    lmsAssignmentsError = null;
    try {
      const assignments = await listLmsAssignmentsForCourse(requestedCourseId);
      if (currentAssignmentCourseId !== requestedCourseId) {
        return;
      }
      lmsAssignments = assignments;
    } catch (error) {
      if (currentAssignmentCourseId !== requestedCourseId) {
        return;
      }
      lmsAssignments = [];
      lmsAssignmentsError = String(error);
    } finally {
      if (currentAssignmentCourseId === requestedCourseId) {
        lmsAssignmentsLoading = false;
      }
    }
  }

  $: currentAssignmentCourseId = lmsCourseSelectorAvailable
    ? projectConfig.lmsCourseId?.trim() || null
    : null;
  $: if (!currentAssignmentCourseId && projectConfig.lmsAssignmentId) {
    projectConfig.lmsAssignmentId = null;
  }
  $: if (currentAssignmentCourseId !== lmsAssignmentsLoadedForCourseId) {
    lmsAssignmentsLoadedForCourseId = currentAssignmentCourseId;
    void refreshSetupLmsAssignments(currentAssignmentCourseId);
  }

  $: hasUnsavedChanges = workspaceState.projectConfig
    ? JSON.stringify(projectConfig) !== JSON.stringify(workspaceState.projectConfig)
    : false;

  function handleMetricsChange(metrics: { naturalWidth: number; naturalHeight: number }) {
    selectedPageNaturalWidth = metrics.naturalWidth;
    selectedPageNaturalHeight = metrics.naturalHeight;
  }

  function setMinimumPointsEnabled(enabled: boolean) {
    projectConfig.instructorProfile.includeMinimumCreditCriterion = enabled;
    projectConfig = { ...projectConfig, instructorProfile: projectConfig.instructorProfile };
  }

  function setMinimumPointsPercent(input: HTMLInputElement) {
    const value = input.valueAsNumber;
    if (!Number.isFinite(value) || value < 0 || value > 100) {
      input.value = String(projectConfig.instructorProfile.minimumCreditPercent);
      return;
    }
    projectConfig.instructorProfile.minimumCreditPercent = value;
    input.value = String(value);
    projectConfig = { ...projectConfig, instructorProfile: projectConfig.instructorProfile };
  }
</script>

<svelte:window on:resize={handleWindowResize} />

<div
  class={[
    'grid h-full min-h-0',
    stackedSetupLayout ? 'overflow-x-hidden overflow-y-auto' : 'overflow-hidden'
  ]}
  style={gridStyle}
  data-layout={stackedSetupLayout ? 'stacked' : compactPageSelector ? 'compact' : 'wide'}
>
  <aside
    class={[
      'flex min-h-0 flex-col border-r border-workspace-sidebar-border bg-surface-sidebar',
      stackedSetupLayout ? 'col-start-1 row-start-1' : ''
    ]}
  >
    <div class="px-3 py-3">
      <DesktopButton
        class={[
          'h-10 w-full justify-center border-workspace-border bg-workspace-sidebar-active text-foreground hover:bg-muted/40',
          compactPageSelector ? 'px-2 text-xs' : ''
        ].join(' ')}
        aria-label={replaceTemplateLabel}
        disabled={busy}
        onclick={() => void onReplaceTemplatePdf?.()}
      >
        {compactPageSelector ? 'PDF' : replaceTemplateLabel}
      </DesktopButton>
    </div>
    <div class="min-h-0 flex-1 space-y-3 overflow-y-auto px-3 pb-4" aria-label="Template pages">
      {#if workspaceState.templatePreviewArtifacts.length > 0}
        {#each workspaceState.templatePreviewArtifacts as page (page.artifactId)}
          <button
            class={[
              'w-full transition-colors',
              compactPageSelector ? 'flex justify-center px-2 py-2 text-center' : 'px-3 py-3 text-left',
              selectedPageNumber === page.pageNumber
                ? 'bg-workspace-sidebar-active text-workspace-sidebar-foreground'
                : 'text-workspace-sidebar-muted hover:bg-workspace-sidebar-hover'
            ]}
            type="button"
            aria-label={`Select ${page.label}`}
            aria-current={selectedPageNumber === page.pageNumber ? 'page' : undefined}
            onclick={() => {
              onSelectPage?.(page.pageNumber);
            }}
          >
            {#if compactPageSelector}
              <span class="block text-base font-semibold text-workspace-sidebar-foreground">
                {page.pageNumber}
              </span>
            {:else}
              <div class="overflow-hidden bg-workspace-thumbnail-bg">
                <img
                  src={toDesktopAssetUrl(page.imagePath)}
                  alt={page.label}
                  class="block w-full bg-surface-page"
                />
              </div>
              <div class="pt-2 text-center text-sm font-medium text-workspace-sidebar-foreground">{page.label}</div>
            {/if}
          </button>
        {/each}
      {:else}
        <div class="px-2 py-4 text-sm text-workspace-text-muted">No template pages are available yet.</div>
      {/if}
    </div>
  </aside>

  <section
    class={[
      'flex min-h-0 min-w-0 flex-col bg-surface-panel',
      stackedSetupLayout ? 'col-start-2 row-start-1' : 'border-r border-workspace-border'
    ]}
  >
    <div class="flex-1 overflow-auto px-4 py-4 lg:px-6 lg:py-6" bind:clientHeight={centerViewportHeight}>
      <RedactionCanvas
        imageUrl={selectedPageUrl}
        pageNumber={selectedPageNumber}
        regions={pageRegions}
        examRegionCount={redactionRegionCount}
        busy={busy}
        squarePageCorners
        editorMinHeightClass={stackedSetupLayout ? 'min-h-[18rem]' : 'min-h-[28rem]'}
        editorConstrainHeight={!compactPageSelector}
        onMetricsChange={handleMetricsChange}
        onRegionsChange={(regions) => void onRegionsChange?.(regions)}
      />
    </div>
  </section>

  <aside
    class={[
      'flex min-h-0 min-w-0 flex-col bg-surface-sidebar',
      stackedSetupLayout
        ? 'col-span-2 col-start-1 row-start-2 border-t border-workspace-border'
        : ''
    ]}
  >
    <div class="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-6">
      <Surface variant="card" bordered radius="3xl" class="px-5 py-5">
        <div class="space-y-5">
          <div class="flex flex-wrap items-start justify-between gap-4">
            <div class="shell-eyebrow text-workspace-text-muted">Exam details</div>
            <div class="flex flex-wrap items-center justify-end gap-x-6 gap-y-3">
              {#if redactionRegionCount === 0}
                <DesktopPopover
                  bind:open={redactionGuidePopoverOpen}
                  rootClass="relative inline-block"
                  triggerClass="inline-flex items-center gap-1.5 shell-body font-medium text-message-warning-text underline underline-offset-4 transition-colors hover:text-workspace-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring"
                  triggerLabel={redactionSummary}
                  triggerAriaHaspopup="dialog"
                  title="No privacy regions have been created. Click to learn how redaction regions protect copied student submissions."
                  panelRole="dialog"
                  panelAriaLabel="Redaction regions"
                  panelClass="right-0 left-auto w-80 max-w-[calc(100vw-3rem)] space-y-3 p-3"
                  align="end"
                >
                  <svelte:fragment slot="trigger">
                    <HugeiconsIcon icon={AlertCircleIcon} size={16} strokeWidth={1.8} aria-hidden="true" />
                    <span>{redactionSummary}</span>
                  </svelte:fragment>
                  <img
                    class="block h-auto w-full rounded-lg border border-border-subtle object-cover"
                    src="/redaction-regions-infographic.png"
                    alt="Exam page with red boxes marking name and privacy regions"
                  />
                  <p class="text-sm leading-5 text-workspace-text-secondary">
                    Review exam details and use your mouse to draw rectangular redaction regions before continuing. The first region placed is designated for name identification; additional regions are pre-emptive privacy masks. All regions apply to this template and will mask every student copy.
                  </p>
                </DesktopPopover>
              {:else}
                <div class="shell-body text-workspace-text-secondary">{redactionSummary}</div>
              {/if}
              <div class="flex items-center gap-1.5 shell-body text-workspace-text-secondary">
                {#if arucoMarkersDetected}
                  <span>{alignmentMarksLabel}</span>
                  <button
                    type="button"
                    class="inline-flex size-7 items-center justify-center rounded-full text-primary transition-colors hover:bg-workspace-empty hover:text-workspace-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring disabled:cursor-not-allowed disabled:text-workspace-text-muted"
                    title="Export template PDF"
                    aria-label="Export template PDF"
                    disabled={!canExportTemplatePdf}
                    onclick={() => void requestAlignmentStampExport()}
                  >
                    <HugeiconsIcon icon={Download01Icon} size={17} strokeWidth={1.8} aria-hidden="true" />
                  </button>
                {:else if effectiveAlignmentMarksPending}
                  <span class="inline-flex items-center gap-1.5 font-medium text-workspace-text-primary">
                    <HugeiconsIcon icon={AlertCircleIcon} size={16} strokeWidth={1.8} aria-hidden="true" />
                    <span>{alignmentMarksLabel}</span>
                  </span>
                {:else if arucoStatus.state === 'not_detected'}
                  <DesktopPopover
                    bind:open={alignmentMarksPopoverOpen}
                    rootClass="relative inline-block"
                    triggerClass="inline-flex items-center gap-1.5 shell-body font-medium text-message-warning-text underline underline-offset-4 transition-colors hover:text-workspace-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring"
                    triggerLabel={alignmentMarksLabel}
                    triggerAriaHaspopup="dialog"
                    title="No alignment marks were detected. Click to learn how alignment marks improve scanned-page matching."
                    panelRole="dialog"
                    panelAriaLabel="Alignment marks"
                    panelClass="right-0 left-auto w-80 max-w-[calc(100vw-3rem)] space-y-3 p-3"
                    align="end"
                  >
                    <svelte:fragment slot="trigger">
                      <HugeiconsIcon icon={AlertCircleIcon} size={16} strokeWidth={1.8} aria-hidden="true" />
                      <span>{alignmentMarksLabel}</span>
                    </svelte:fragment>
                    <img
                      class="block h-auto w-full rounded-lg border border-border-subtle object-cover"
                      src="/alignment-marks-infographic.png"
                      alt="Exam page with a square alignment mark in one corner"
                    />
                    <p class="text-sm leading-5 text-workspace-text-secondary">
                      Alignment marks are small square codes printed near page corners. They help ScriptScore line up scanned student copies with the original template.
                    </p>
                    <div class="space-y-2">
                      <p class="text-sm font-medium text-workspace-text-primary">
                        Add alignment stamps to this template?
                      </p>
                      <DesktopButton
                        class="w-full justify-center"
                        variant="primary"
                        disabled={!canExportTemplatePdf}
                        onclick={() => void requestAlignmentStampExport()}
                      >
                        Add Alignment Stamps & Export PDF
                      </DesktopButton>
                    </div>
                  </DesktopPopover>
                {:else}
                  <span>{alignmentMarksLabel}</span>
                {/if}
              </div>
              {#if debugRedactionToggle}
                <div class="flex items-center gap-3">
                  <ToggleRow
                    class="items-center rounded-xl bg-workspace-empty p-2"
                    title="Redactions Required"
                    checked={projectConfig.redactionRequired}
                    disabled={redactionRegionCount > 0}
                    onToggle={(checked: boolean) => {
                      projectConfig.redactionRequired = checked;
                    }}
                  />
                </div>
              {/if}
            </div>
          </div>
          <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
            <div class="grid gap-3">
              <TextField
                id="setupExamName"
                label="Exam Name"
                bind:value={projectConfig.displayName}
                placeholder="Midterm 1"
              />
              <TextField
                id="setupSubject"
                label="Subject"
                value={projectConfig.subject ?? ''}
                placeholder="History"
                oninput={(event: Event) => {
                  projectConfig.subject = (event.currentTarget as HTMLInputElement).value;
                }}
              />
            </div>
            <div class="grid content-start gap-3">
              {#if lmsCourseSelectorAvailable}
                <div class="grid gap-1.5 shell-body text-workspace-text-secondary">
                  <SelectField
                    label="Course"
                    value={projectConfig.lmsCourseId ?? ''}
                    options={courseOptions}
                    searchable
                    searchPlaceholder="Search courses"
                    showSubitems
                    allowEmpty
                    emptyLabel="No LMS course"
                    placeholder="No LMS course"
                    loading={lmsCoursesLoading}
                    loadingLabel="Loading courses..."
                    noOptionsLabel="No matching courses."
                    ariaLabel="Course"
                    onChange={selectSetupLmsCourse}
                  />
                  {#if lmsCoursesError}
                    <InlineMessage tone="warning" message={lmsCoursesError} />
                  {/if}
                  {#if !isCanvasLmsReady($appSettings)}
                    <p class="text-xs text-workspace-text-muted">
                      Turn on Canvas in Settings with a base URL and API token to refresh the full course list.
                    </p>
                  {/if}
                </div>
              {:else}
                <TextField
                  id="setupCourseCode"
                  label="Course Code"
                  value={projectConfig.courseCode ?? ''}
                  placeholder="HIST 201"
                  oninput={(event: Event) => {
                    projectConfig.courseCode = (event.currentTarget as HTMLInputElement).value;
                  }}
                />
              {/if}
              {#if currentAssignmentCourseId}
                <div class="grid gap-1.5 shell-body text-workspace-text-secondary">
                  <div class="relative">
                    <span class="absolute right-3 top-0 z-10">
                      {#if !projectConfig.lmsAssignmentId?.trim()}
                        <span
                          class="inline-flex size-5 items-center justify-center rounded-full text-message-warning-text"
                          title="You cannot upload results to the LMS without first selecting an assignment."
                          aria-label="You cannot upload results to the LMS without first selecting an assignment."
                        >
                          <HugeiconsIcon icon={AlertCircleIcon} size={16} strokeWidth={1.7} aria-hidden="true" />
                        </span>
                      {/if}
                    </span>
                    <SelectField
                      label="Assignment"
                      value={projectConfig.lmsAssignmentId ?? ''}
                      options={assignmentOptions}
                      searchable
                      searchPlaceholder="Search assignments"
                      showSubitems
                      allowEmpty
                      emptyLabel="No assignment"
                      placeholder={assignmentTriggerLabel}
                      disabled={!lmsAssignmentsLoading && lmsAssignments.length === 0}
                      loading={lmsAssignmentsLoading}
                      loadingLabel="Loading assignments..."
                      noOptionsLabel="No matching assignments."
                      ariaLabel="Assignment"
                      onChange={selectSetupAssignment}
                    />
                  </div>
                  {#if lmsAssignmentsError}
                    <InlineMessage tone="warning" message={lmsAssignmentsError} />
                  {/if}
                </div>
              {/if}
            </div>
          </div>
        </div>
      </Surface>

      <Surface variant="card" bordered radius="3xl" class="px-5 py-5">
        <div class="shell-eyebrow text-workspace-text-muted">Instructor profile</div>
        <div class="mt-4 grid gap-3 md:grid-cols-2">
          <SelectField
            label="Grading strictness"
            bind:value={projectConfig.instructorProfile.gradingStrictness}
            options={strictnessOptions}
          />
          <SelectField
            label="Syntax leniency"
            bind:value={projectConfig.instructorProfile.syntaxLeniency}
            options={toleranceOptions}
          />
          <SelectField
            label="OCR tolerance"
            bind:value={projectConfig.instructorProfile.ocrTolerance}
            options={toleranceOptions}
          />
          <SelectField
            label="Partial credit style"
            bind:value={projectConfig.instructorProfile.partialCreditStyle}
            options={strictnessOptions}
          />
          <SelectField
            label="Feedback style"
            bind:value={projectConfig.instructorProfile.feedbackStyle}
            options={feedbackStyleOptions}
          />
          <TextareaField
            class="md:col-span-2"
            label="Additional guidance"
            bind:value={projectConfig.instructorProfile.additionalGuidance}
          />
          <div class="md:col-span-2 mt-4">
            <div class="shell-eyebrow mb-3 text-workspace-text-muted">Non-blank answer minimum</div>
            <div class="space-y-3">
              <div class="flex flex-wrap items-center gap-x-4 gap-y-3">
                <button
                  type="button"
                  role="switch"
                  aria-checked={projectConfig.instructorProfile.includeMinimumCreditCriterion}
                  aria-label="Award minimum points for non-blank answers"
                  class={[
                    'relative inline-flex h-7 w-12 shrink-0 items-center rounded-full border transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring',
                    projectConfig.instructorProfile.includeMinimumCreditCriterion
                      ? 'border-toggle-active-border bg-toggle-active-bg'
                      : 'border-border-default bg-workspace-empty'
                  ]}
                  onclick={() => {
                    setMinimumPointsEnabled(!projectConfig.instructorProfile.includeMinimumCreditCriterion);
                  }}
                >
                  <span
                    class={[
                      'pointer-events-none inline-flex size-5 rounded-full shadow-sm transition-transform',
                      projectConfig.instructorProfile.includeMinimumCreditCriterion
                        ? 'translate-x-6 bg-toggle-active-text'
                        : 'translate-x-1 bg-text-primary'
                    ]}
                    aria-hidden="true"
                  ></span>
                </button>

                {#if projectConfig.instructorProfile.includeMinimumCreditCriterion}
                  <div class="flex min-w-0 flex-1 flex-wrap items-center gap-x-3 gap-y-2 text-sm font-medium text-text-primary">
                    <span>Award at least</span>
                    <div class="inline-flex h-9 shrink-0 items-stretch overflow-hidden rounded-xl border border-border-default bg-workspace-empty shadow-[var(--surface-shadow-inset)] transition-colors hover:border-border-strong focus-within:border-border-focus focus-within:ring-2 focus-within:ring-focus-ring/60">
                      <input
                        class="w-14 bg-transparent px-3 text-center text-sm font-medium text-text-primary outline-none [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
                        type="number"
                        min="0"
                        max="100"
                        step="1"
                        value={projectConfig.instructorProfile.minimumCreditPercent}
                        aria-label="Minimum percentage of question points for non-blank answers"
                        oninput={(event: Event) => {
                          setMinimumPointsPercent(event.currentTarget as HTMLInputElement);
                        }}
                      />
                      <span class="inline-flex min-w-9 items-center justify-center border-l border-border-default px-3 text-sm font-medium text-text-primary">
                        %
                      </span>
                    </div>
                    <span>of question points for any non-blank answer</span>
                  </div>
                {:else}
                  <div class="min-w-0 flex-1">
                    <div class="text-sm font-medium text-text-primary">
                      Award minimum points for non-blank answers
                    </div>
                    <div class="mt-1 text-sm leading-6 text-text-secondary">
                      Off — no minimum is guaranteed and no rubric criterion will be added.
                    </div>
                  </div>
                {/if}
              </div>

              {#if projectConfig.instructorProfile.includeMinimumCreditCriterion}
                <div class="mt-3 flex items-center gap-3 text-sm leading-6 text-text-secondary">
                  <span class="inline-flex size-8 shrink-0 items-center justify-center rounded-full bg-message-success-bg text-message-success-text" aria-hidden="true">
                    <HugeiconsIcon icon={CheckListIcon} size={18} strokeWidth={1.8} />
                  </span>
                  <span>Automatically adds a rubric criterion for non-blank attempts.</span>
                </div>
              {/if}
            </div>
          </div>
        </div>
      </Surface>
    </div>

    <div class="border-t border-workspace-border bg-surface-bottom-bar px-6 py-5">
      <div class="flex flex-wrap gap-3">
        <DesktopButton
          class="min-w-32 flex-1 justify-center"
          disabled={busy}
          onclick={() => void onDiscardChanges?.()}
        >
          Discard
        </DesktopButton>
        <DesktopButton
          class="min-w-32 flex-1 justify-center"
          variant={hasUnsavedChanges ? 'primary' : 'secondary'}
          disabled={busy}
          onclick={() => void onSaveSetup?.()}
        >
          Save
        </DesktopButton>
        <span class="min-w-32 flex-1" title={continueDisabledReason ?? undefined}>
          <DesktopButton
            class="w-full justify-center"
            variant="primary"
            disabled={!canContinue}
            title={continueDisabledReason ?? undefined}
            onclick={() => {
              onConfirmContinue?.();
            }}
          >
            Continue
          </DesktopButton>
        </span>
      </div>
    </div>
  </aside>

</div>
