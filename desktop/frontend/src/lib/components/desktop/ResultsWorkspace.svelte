<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import {
    resultsPreviewEntry,
    resultsWorkspaceView,
  } from '$lib/stores/resultsWorkspaceView';
  import type {
    LmsUploadMode,
    ResultsExportFormat,
    ResultStudentRow,
    ResultsLmsReportPreview,
    ExamWorkspaceState,
  } from '$lib/types';
  import ResultsMetricsSidebar from './ResultsMetricsSidebar.svelte';
  import ResultsReportPreview from './ResultsReportPreview.svelte';
  import ResultsStudentsSidebar from './ResultsStudentsSidebar.svelte';
  import {
    displayNameForStudent,
    examMaxPointsFromQuestions,
    filterResultsRows,
    sortResultsRows,
  } from './results-workspace-helpers';

  export let workspaceState: ExamWorkspaceState;
  export let studentDisplayNamesByRef: Record<string, string> = {};
  export let busy = false;
  export let onSetResultFinalized:
    | ((studentRef: string, finalized: boolean) => Promise<void>)
    | null = null;
  export let onFinalizeReady: ((studentRefs: string[]) => Promise<boolean | void>) | null = null;
  export let onRunUpload:
    | ((mode: LmsUploadMode, studentRefs: string[]) => Promise<boolean | void>)
    | null = null;
  export let onRunExport:
    | ((format: ResultsExportFormat, studentRefs: string[]) => Promise<boolean | void>)
    | null = null;
  export let onRetryUpload: ((attemptId: string) => Promise<void>) | null = null;
  export let onLoadReportPreview:
    | ((studentRef: string) => Promise<ResultsLmsReportPreview>)
    | null = null;

  let lastProjectPath: string | null = null;
  let lastRowsReference: ResultStudentRow[] | undefined;

  $: rows = workspaceState.resultsLmsRows ?? [];
  $: uploadAttempts = workspaceState.resultsLmsState?.uploadAttempts ?? [];
  $: currentAssignmentId =
    workspaceState.resultsLmsState?.selectedTarget?.assignmentId ??
    workspaceState.projectConfig?.lmsAssignmentId ??
    null;
  $: lmsLinked = Boolean(
    (workspaceState.projectConfig?.lmsCourseId ?? workspaceState.project.lmsCourseId ?? '').trim()
  );
  $: metrics = workspaceState.resultsLmsMetrics ?? null;
  $: reviewSummary = workspaceState.resultsLmsReviewSummary ?? null;
  $: examMaxPoints = examMaxPointsFromQuestions(workspaceState.questions);

  $: {
    const projectPath = workspaceState.project.projectPath;
    if (projectPath !== lastProjectPath) {
      lastProjectPath = projectPath;
      resultsWorkspaceView.syncProject(projectPath);
    }
  }

  $: if (rows !== lastRowsReference) {
    lastRowsReference = rows;
    resultsWorkspaceView.resetPreviewCache();
    resultsWorkspaceView.clearUploadProgress();
  }

  $: if (
    !lmsLinked &&
    ($resultsWorkspaceView.statusFilter === 'finalized' ||
      $resultsWorkspaceView.statusFilter === 'uploaded')
  ) {
    resultsWorkspaceView.setStatusFilter('all');
  }

  $: sortedRows = sortResultsRows(
    rows,
    studentDisplayNamesByRef,
    $resultsWorkspaceView.sortKey,
    $resultsWorkspaceView.sortDirection
  );
  $: filteredRows = filterResultsRows(
    sortedRows,
    $resultsWorkspaceView.statusFilter,
    $resultsWorkspaceView.searchTerm,
    studentDisplayNamesByRef
  );

  $: resultsWorkspaceView.syncRows(sortedRows.map((row) => row.studentRef));
  $: resultsWorkspaceView.syncAttempts(uploadAttempts.map((attempt) => attempt.attemptId));
  $: if (filteredRows.length === 0 && $resultsWorkspaceView.selectedStudentRef !== null) {
    resultsWorkspaceView.setSelectedStudentRef(null);
  }
  $: if (
    filteredRows.length > 0 &&
    !filteredRows.some((row) => row.studentRef === $resultsWorkspaceView.selectedStudentRef)
  ) {
    resultsWorkspaceView.setSelectedStudentRef(filteredRows[0]?.studentRef ?? null);
  }

  $: selectedRow =
    filteredRows.find((row) => row.studentRef === $resultsWorkspaceView.selectedStudentRef) ?? null;
  $: selectedRows = filteredRows.filter((row) =>
    $resultsWorkspaceView.selectedStudentRefs.includes(row.studentRef)
  );
  $: selectedUploadableRows = selectedRows.filter(
    (row) => row.finalized && !row.staleFinalization
  );
  $: selectedExportableRows = selectedRows.filter(
    (row) => row.readyToFinalize && row.aggregateComplete
  );
  $: readyFinalizeCount = filteredRows.filter((row) => row.readyToFinalize && !row.finalized).length;
  $: selectedPreviewEntry = resultsPreviewEntry($resultsWorkspaceView.selectedStudentRef);
  $: selectedAttempt =
    uploadAttempts.find((attempt) => attempt.attemptId === $resultsWorkspaceView.selectedAttemptId) ??
    null;
  $: retryableFailureCount = selectedAttempt
    ? selectedAttempt.studentResults.filter((result) => result.status === 'failed').length
    : 0;
  $: selectedStudentDisplayName = selectedRow
    ? displayNameForStudent(selectedRow.studentRef, studentDisplayNamesByRef)
    : '';

  $: if (selectedRow && onLoadReportPreview) {
    const requestedStudentRef = selectedRow.studentRef;
    const resultFingerprint = selectedRow.resultFingerprint ?? null;
    const cached = $resultsWorkspaceView.previewByStudentRef[requestedStudentRef];
    if (!cached || cached.resultFingerprint !== resultFingerprint) {
      resultsWorkspaceView.startPreview(requestedStudentRef, resultFingerprint);
      void onLoadReportPreview(requestedStudentRef)
        .then((preview) => {
          resultsWorkspaceView.savePreview(
            requestedStudentRef,
            preview.resultFingerprint ?? null,
            preview.html
          );
        })
        .catch((error) => {
          resultsWorkspaceView.setPreviewError(
            requestedStudentRef,
            resultFingerprint,
            String(error)
          );
        });
    }
  }

  async function handleRunUpload(mode: LmsUploadMode) {
    if (!onRunUpload) {
      return;
    }
    if (mode === 'live' && reviewSummary?.hasUnreviewedQuestions) {
      resultsWorkspaceView.flashUploadWarning();
    }
    const succeeded = await onRunUpload(
      mode,
      selectedUploadableRows.map((row) => row.studentRef)
    ).catch(() => false);
    if (mode === 'live' && succeeded !== false) {
      resultsWorkspaceView.setStatusFilter('uploaded');
    }
  }

  async function handleFinalizeReady() {
    if (!onFinalizeReady) {
      return;
    }
    const succeeded = await onFinalizeReady(
      filteredRows
        .filter((row) => row.readyToFinalize && !row.finalized)
        .map((row) => row.studentRef)
    ).catch(() => false);
    if (succeeded !== false) {
      resultsWorkspaceView.setStatusFilter('finalized');
    }
  }

  async function handleRunExport(format: ResultsExportFormat) {
    if (!onRunExport || selectedExportableRows.length === 0) {
      return false;
    }
    return await onRunExport(
      format,
      selectedExportableRows.map((row) => row.studentRef)
    ).catch(() => false);
  }
</script>

<section class="flex h-full overflow-hidden bg-surface-panel">
  <ResultsStudentsSidebar
    rows={filteredRows}
    {studentDisplayNamesByRef}
    selectedStudentRef={$resultsWorkspaceView.selectedStudentRef}
    selectedStudentRefs={$resultsWorkspaceView.selectedStudentRefs}
    sortKey={$resultsWorkspaceView.sortKey}
    sortDirection={$resultsWorkspaceView.sortDirection}
    statusFilter={$resultsWorkspaceView.statusFilter}
    searchTerm={$resultsWorkspaceView.searchTerm}
    scoreDisplayMode={$resultsWorkspaceView.scoreDisplayMode}
    uploadProgressByStudentRef={$resultsWorkspaceView.uploadProgressByStudentRef}
    {currentAssignmentId}
    selectedUploadableRowsCount={selectedUploadableRows.length}
    selectedExportableRowsCount={selectedExportableRows.length}
    {lmsLinked}
    showExportActionForLms={$resultsWorkspaceView.showExportActionForLms}
    {readyFinalizeCount}
    {busy}
    onSelect={(studentRef) => resultsWorkspaceView.setSelectedStudentRef(studentRef)}
    onToggleSelection={(studentRef) => resultsWorkspaceView.toggleStudentSelection(studentRef)}
    onToggleFilteredSelection={(selected) =>
      resultsWorkspaceView.setStudentSelectionForScope(
        filteredRows.map((row) => row.studentRef),
        selected
      )}
    onChangeSortKey={(sortKey) => resultsWorkspaceView.setSortKey(sortKey)}
    onToggleSortDirection={() =>
      resultsWorkspaceView.setSortDirection(
        $resultsWorkspaceView.sortDirection === 'asc' ? 'desc' : 'asc'
      )}
    onChangeStatusFilter={(statusFilter) => resultsWorkspaceView.setStatusFilter(statusFilter)}
    onChangeSearchTerm={(searchTerm) => resultsWorkspaceView.setSearchTerm(searchTerm)}
    onChangeScoreDisplayMode={(mode) => resultsWorkspaceView.setScoreDisplayMode(mode)}
    onChangeShowExportActionForLms={(enabled) =>
      resultsWorkspaceView.setShowExportActionForLms(enabled)}
    onFinalizeReady={handleFinalizeReady}
    onRunUpload={() => handleRunUpload('live')}
    onRunExport={handleRunExport}
  />

  <ResultsReportPreview
    {selectedRow}
    studentDisplayName={selectedStudentDisplayName}
    previewEntry={selectedPreviewEntry}
    {busy}
    {lmsLinked}
    scoreDisplayMode={$resultsWorkspaceView.scoreDisplayMode}
    uploadProgressStatus={
      selectedRow
        ? $resultsWorkspaceView.uploadProgressByStudentRef[selectedRow.studentRef] ?? null
        : null
    }
    onFinalize={(studentRef) => onSetResultFinalized?.(studentRef, true) ?? Promise.resolve()}
    onUnfinalize={(studentRef) => onSetResultFinalized?.(studentRef, false) ?? Promise.resolve()}
  />

  <ResultsMetricsSidebar
    {metrics}
    reviewSummary={reviewSummary}
    scoreDisplayMode={$resultsWorkspaceView.scoreDisplayMode}
    {examMaxPoints}
    {studentDisplayNamesByRef}
    {uploadAttempts}
    {lmsLinked}
    selectedAttemptId={$resultsWorkspaceView.selectedAttemptId}
    {retryableFailureCount}
    {busy}
    onSelectAttempt={(attemptId) => resultsWorkspaceView.setSelectedAttemptId(attemptId)}
    {onRetryUpload}
  />
</section>
