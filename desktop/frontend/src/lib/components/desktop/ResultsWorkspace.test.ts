// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { get } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { ExamWorkspaceState } from '$lib/types';
import { resultsWorkspaceView } from '$lib/stores/resultsWorkspaceView';
import ResultsWorkspace from './ResultsWorkspace.svelte';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((promiseResolve, promiseReject) => {
    resolve = promiseResolve;
    reject = promiseReject;
  });
  return { promise, resolve, reject };
}

function workspaceState(): ExamWorkspaceState {
  return {
    project: {
      projectId: 'proj_1',
      displayName: 'Spring 2026 - CS145 - Midterm',
      subject: 'Computer Science',
      courseCode: 'CS145',
      lmsCourseId: 'course_1',
      projectPath: '/tmp/project',
      createdAt: '1',
      updatedAt: '1',
    },
    status: 'approved',
    statusLabel: 'Approved',
    failureMessage: null,
    templatePreviewArtifacts: [],
    questions: [
      {
        questionId: 'question_1',
        questionNumber: 1,
        pageNumber: 1,
        maxPoints: 5,
        text: 'Explain the difference.',
        baselinePdfText: 'Explain the difference.',
        sourceArtifactId: null,
      },
    ],
    redactionRegions: [],
    warnings: [],
    canApprove: true,
    canApproveRubric: true,
    projectConfig: {
      projectId: 'proj_1',
      displayName: 'Spring 2026 - CS145 - Midterm',
      subject: 'Computer Science',
      courseCode: 'CS145',
      lmsCourseId: 'course_1',
      lmsAssignmentId: 'assignment_1',
      redactionRequired: true,
      instructorProfile: {
        gradingStrictness: 'balanced',
        syntaxLeniency: 'medium',
        ocrTolerance: 'medium',
        partialCreditStyle: 'balanced',
        feedbackStyle: 'brief',
        additionalGuidance: '',
        includeMinimumCreditCriterion: false,
        minimumCreditPercent: 10,
      },
      traceRefs: {
        setupJobId: null,
        batchAnalyzeJobId: null,
        batchRubricJobId: null,
        intakeJobId: null,
      },
      createdAt: '1',
      updatedAt: '1',
    },
    studentRoster: [],
    studentIntake: {
      status: 'ready',
      latestJobId: null,
      items: [],
      unresolvedCount: 0,
    },
    studentWorkflow: {
      status: 'graded',
      latestJobId: null,
      submissions: [],
    },
    moderationState: {
      scoreOverrides: [],
      feedbackOverrides: [],
      questionReviews: [],
    },
    resultsLmsState: {
      selectedTarget: {
        provider: 'canvas',
        courseId: 'course_1',
        assignmentId: 'assignment_1',
      },
      finalizationRecords: [],
      uploadAttempts: [
        {
          attemptId: 'attempt_1',
          mode: 'live',
          provider: 'canvas',
          courseId: 'course_1',
          assignmentId: 'assignment_1',
          startedAt: '1',
          finishedAt: '2',
          attemptedCount: 1,
          successCount: 1,
          failureCount: 0,
          studentResults: [
            {
              studentRef: 'student_2',
              resultFingerprint: 'fp_2',
              status: 'uploaded',
              sanitizedError: null,
            },
          ],
        },
      ],
    },
    resultsLmsRows: [
      {
        studentRef: 'student_1',
        aggregateTotal: 96,
        aggregateComplete: true,
        readyToFinalize: true,
        blockedReasons: [],
        questionRows: [],
        resultFingerprint: 'fp_1',
        finalized: true,
        staleFinalization: false,
        finalizedAt: '1',
        uploaded: false,
        uploadFailed: false,
        latestUploadError: null,
        lastUploadAttemptId: null,
      },
      {
        studentRef: 'student_2',
        aggregateTotal: 71,
        aggregateComplete: true,
        readyToFinalize: true,
        blockedReasons: [],
        questionRows: [],
        resultFingerprint: 'fp_2',
        finalized: true,
        staleFinalization: false,
        finalizedAt: '1',
        uploaded: true,
        uploadFailed: false,
        latestUploadError: null,
        lastUploadAttemptId: 'attempt_1',
      },
    ],
    resultsLmsMetrics: {
      scoredStudentCount: 2,
      averageScore: 83.5,
      medianScore: 83.5,
      minScore: 71,
      maxScore: 96,
      questionMetrics: [
        {
          questionId: 'question_1',
          questionNumber: 1,
          maxPoints: 5,
          reviewed: false,
          sampleSize: 2,
          averagePoints: 4.2,
          averagePercent: 84,
          difficultyPercent: 16,
        },
      ],
    },
    resultsLmsReviewSummary: {
      totalReviewableQuestions: 1,
      unreviewedQuestionCount: 1,
      hasUnreviewedQuestions: true,
    },
    workflowStage: 'results_upload_ready',
    workflowLabel: 'Ready',
  };
}

describe('ResultsWorkspace', () => {
  beforeEach(() => {
    resultsWorkspaceView.reset();
  });

  it('loads the selected student preview and can switch students', async () => {
    const onLoadReportPreview = vi.fn(async (studentRef: string) => ({
      studentRef,
      resultFingerprint: studentRef === 'student_1' ? 'fp_1' : 'fp_2',
      html:
        studentRef === 'student_1'
          ? '<!doctype html><html><body>Preview One</body></html>'
          : '<!doctype html><html><body>Preview Two</body></html>',
    }));

    render(ResultsWorkspace, {
      workspaceState: workspaceState(),
      studentDisplayNamesByRef: {
        student_1: 'Jordan Scott',
        student_2: 'Alex Tobin',
      },
      onLoadReportPreview,
    });

    await waitFor(() => expect(onLoadReportPreview).toHaveBeenCalledWith('student_2'));
    expect(
      (await screen.findByTitle('Alex Tobin uploaded report preview')).getAttribute('srcdoc')
    ).toBe('<!doctype html><html><body>Preview Two</body></html>');

    await fireEvent.click(screen.getByRole('button', { name: /Jordan Scott/i }));

    await waitFor(() => expect(onLoadReportPreview).toHaveBeenCalledWith('student_1'));
    expect(
      (await screen.findByTitle('Jordan Scott uploaded report preview')).getAttribute('srcdoc')
    ).toBe('<!doctype html><html><body>Preview One</body></html>');
  });

  it('surfaces the moderation warning and still runs live upload', async () => {
    const onRunUpload = vi.fn().mockResolvedValue(undefined);

    render(ResultsWorkspace, {
      workspaceState: workspaceState(),
      studentDisplayNamesByRef: {
        student_1: 'Jordan Scott',
        student_2: 'Alex Tobin',
      },
      onLoadReportPreview: vi.fn().mockResolvedValue({
        studentRef: 'student_2',
        resultFingerprint: 'fp_2',
        html: '<!doctype html><html><body>Preview Two</body></html>',
      }),
      onRunUpload,
    });

    expect(
      screen.getByText(/moderation question still need review/i)
    ).toBeTruthy();

    await fireEvent.click(screen.getByLabelText('Select Alex Tobin'));
    await fireEvent.click(screen.getByLabelText('Select Jordan Scott'));
    await fireEvent.click(screen.getByRole('button', { name: 'Upload' }));

    await waitFor(() =>
      expect(onRunUpload).toHaveBeenCalledWith('live', ['student_2', 'student_1'])
    );
  });

  it('does not auto-select students for upload on load', async () => {
    render(ResultsWorkspace, {
      workspaceState: workspaceState(),
      studentDisplayNamesByRef: {
        student_1: 'Jordan Scott',
        student_2: 'Alex Tobin',
      },
      onLoadReportPreview: vi.fn().mockResolvedValue({
        studentRef: 'student_2',
        resultFingerprint: 'fp_2',
        html: '<!doctype html><html><body>Preview Two</body></html>',
      }),
    });

    expect((screen.getByLabelText('Select Alex Tobin') as HTMLInputElement).checked).toBe(false);
    expect((screen.getByLabelText('Select Jordan Scott') as HTMLInputElement).checked).toBe(false);
    expect((screen.getByRole('button', { name: 'Upload' }) as HTMLButtonElement).disabled).toBe(
      true
    );
  });

  it('filters to ready rows and finalizes only the visible ready rows', async () => {
    const onFinalizeReady = vi.fn().mockResolvedValue(undefined);
    const baseWorkspace = workspaceState();

    render(ResultsWorkspace, {
      workspaceState: {
        ...baseWorkspace,
        resultsLmsRows: [
          ...(baseWorkspace.resultsLmsRows ?? []),
          {
            studentRef: 'student_3',
            aggregateTotal: 88,
            aggregateComplete: true,
            readyToFinalize: true,
            blockedReasons: [],
            questionRows: [],
            resultFingerprint: 'fp_3',
            finalized: false,
            staleFinalization: false,
            finalizedAt: null,
            uploaded: false,
            uploadFailed: false,
            latestUploadError: null,
            lastUploadAttemptId: null,
          },
        ],
      },
      studentDisplayNamesByRef: {
        student_1: 'Jordan Scott',
        student_2: 'Alex Tobin',
        student_3: 'Morgan Lee',
      },
      onLoadReportPreview: vi.fn().mockResolvedValue({
        studentRef: 'student_2',
        resultFingerprint: 'fp_2',
        html: '<!doctype html><html><body>Preview Two</body></html>',
      }),
      onFinalizeReady,
    });

    expect(screen.queryByRole('button', { name: 'Finalize filtered' })).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Filter students by result status' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Ready' }));

    expect(screen.queryByRole('button', { name: 'Upload' })).toBeNull();
    expect(screen.getByRole('button', { name: /Morgan Lee/i })).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Alex Tobin/i })).toBeNull();
    expect(screen.getAllByRole('button', { name: 'Finalize' }).length).toBeGreaterThan(0);

    await fireEvent.click(screen.getByLabelText('Select all filtered students'));
    expect((screen.getByLabelText('Select Morgan Lee') as HTMLInputElement).checked).toBe(true);

    const sidebarRegion = screen.getByRole('region', { name: 'Student results list' });
    await fireEvent.click(within(sidebarRegion).getByRole('button', { name: 'Finalize' }));

    await waitFor(() => expect(onFinalizeReady).toHaveBeenCalledWith(['student_3']));
  });

  it('renders Results controls with search, filter, and sort affordances', async () => {
    render(ResultsWorkspace, {
      workspaceState: workspaceState(),
      studentDisplayNamesByRef: {
        student_1: 'Jordan Scott',
        student_2: 'Alex Tobin',
      },
      onLoadReportPreview: vi.fn().mockResolvedValue({
        studentRef: 'student_2',
        resultFingerprint: 'fp_2',
        html: '<!doctype html><html><body>Preview Two</body></html>',
      }),
    });

    expect(screen.queryByText('Students')).toBeNull();
    expect(screen.getByRole('button', { name: 'Upload' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Student display settings' })).toBeTruthy();
    expect(screen.queryByLabelText('Select LMS assignment')).toBeNull();
    expect(screen.getByRole('button', { name: 'Filter students by result status' })).toBeTruthy();
    expect(screen.getByLabelText('Search results students')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Sort students' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Sort descending' })).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Sort students' }));
    expect(screen.getByRole('button', { name: 'Status' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Upload status' })).toBeNull();
  });

  it('searches results by display name and student ref', async () => {
    render(ResultsWorkspace, {
      workspaceState: workspaceState(),
      studentDisplayNamesByRef: {
        student_1: 'Jordan Scott',
        student_2: 'Alex Tobin',
      },
      onLoadReportPreview: vi.fn().mockResolvedValue({
        studentRef: 'student_2',
        resultFingerprint: 'fp_2',
        html: '<!doctype html><html><body>Preview Two</body></html>',
      }),
    });

    await fireEvent.input(screen.getByLabelText('Search results students'), {
      target: { value: 'Jordan' },
    });

    expect(screen.getByRole('button', { name: /Jordan Scott/i })).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Alex Tobin/i })).toBeNull();

    await fireEvent.input(screen.getByLabelText('Search results students'), {
      target: { value: 'student_2' },
    });

    expect(screen.queryByRole('button', { name: /Jordan Scott/i })).toBeNull();
    expect(screen.getByRole('button', { name: /Alex Tobin/i })).toBeTruthy();
  });

  it('select-all only selects visible searched rows', async () => {
    render(ResultsWorkspace, {
      workspaceState: workspaceState(),
      studentDisplayNamesByRef: {
        student_1: 'Jordan Scott',
        student_2: 'Alex Tobin',
      },
      onLoadReportPreview: vi.fn().mockResolvedValue({
        studentRef: 'student_2',
        resultFingerprint: 'fp_2',
        html: '<!doctype html><html><body>Preview Two</body></html>',
      }),
    });

    await fireEvent.input(screen.getByLabelText('Search results students'), {
      target: { value: 'Jordan' },
    });
    await fireEvent.click(screen.getByLabelText('Select all filtered students'));

    expect((screen.getByLabelText('Select Jordan Scott') as HTMLInputElement).checked).toBe(true);
    expect(get(resultsWorkspaceView).selectedStudentRefs).toEqual(['student_1']);
  });

  it('uploads only checked rows that are visible after search filtering', async () => {
    const onRunUpload = vi.fn().mockResolvedValue(undefined);

    render(ResultsWorkspace, {
      workspaceState: workspaceState(),
      studentDisplayNamesByRef: {
        student_1: 'Jordan Scott',
        student_2: 'Alex Tobin',
      },
      onLoadReportPreview: vi.fn().mockResolvedValue({
        studentRef: 'student_2',
        resultFingerprint: 'fp_2',
        html: '<!doctype html><html><body>Preview Two</body></html>',
      }),
      onRunUpload,
    });

    await fireEvent.click(screen.getByLabelText('Select Jordan Scott'));
    await fireEvent.click(screen.getByLabelText('Select Alex Tobin'));
    await fireEvent.input(screen.getByLabelText('Search results students'), {
      target: { value: 'Jordan' },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Upload' }));

    await waitFor(() => expect(onRunUpload).toHaveBeenCalledWith('live', ['student_1']));
  });

  it('advances to finalized only after sidebar finalize succeeds', async () => {
    const finalizeRequest = deferred<void>();
    const onFinalizeReady = vi.fn().mockReturnValue(finalizeRequest.promise);
    const baseWorkspace = workspaceState();

    render(ResultsWorkspace, {
      workspaceState: {
        ...baseWorkspace,
        resultsLmsRows: [
          {
            studentRef: 'student_3',
            aggregateTotal: 88,
            aggregateComplete: true,
            readyToFinalize: true,
            blockedReasons: [],
            questionRows: [],
            resultFingerprint: 'fp_3',
            finalized: false,
            staleFinalization: false,
            finalizedAt: null,
            uploaded: false,
            uploadFailed: false,
            latestUploadError: null,
            lastUploadAttemptId: null,
          },
        ],
      },
      studentDisplayNamesByRef: {
        student_3: 'Morgan Lee',
      },
      onLoadReportPreview: vi.fn().mockResolvedValue({
        studentRef: 'student_3',
        resultFingerprint: 'fp_3',
        html: '<!doctype html><html><body>Preview Three</body></html>',
      }),
      onFinalizeReady,
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Filter students by result status' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Ready' }));
    const sidebarRegion = screen.getByRole('region', { name: 'Student results list' });
    await fireEvent.click(within(sidebarRegion).getByRole('button', { name: 'Finalize' }));

    expect(get(resultsWorkspaceView).statusFilter).toBe('ready');

    finalizeRequest.resolve();

    await waitFor(() => expect(get(resultsWorkspaceView).statusFilter).toBe('finalized'));
  });

  it('does not advance after failed sidebar finalize', async () => {
    const finalizeRequest = deferred<void>();
    const onFinalizeReady = vi.fn().mockReturnValue(finalizeRequest.promise);
    const baseWorkspace = workspaceState();

    render(ResultsWorkspace, {
      workspaceState: {
        ...baseWorkspace,
        resultsLmsRows: [
          {
            studentRef: 'student_3',
            aggregateTotal: 88,
            aggregateComplete: true,
            readyToFinalize: true,
            blockedReasons: [],
            questionRows: [],
            resultFingerprint: 'fp_3',
            finalized: false,
            staleFinalization: false,
            finalizedAt: null,
            uploaded: false,
            uploadFailed: false,
            latestUploadError: null,
            lastUploadAttemptId: null,
          },
        ],
      },
      studentDisplayNamesByRef: {
        student_3: 'Morgan Lee',
      },
      onLoadReportPreview: vi.fn().mockResolvedValue({
        studentRef: 'student_3',
        resultFingerprint: 'fp_3',
        html: '<!doctype html><html><body>Preview Three</body></html>',
      }),
      onFinalizeReady,
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Filter students by result status' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Ready' }));
    const sidebarRegion = screen.getByRole('region', { name: 'Student results list' });
    await fireEvent.click(within(sidebarRegion).getByRole('button', { name: 'Finalize' }));

    finalizeRequest.reject(new Error('finalize failed'));

    await waitFor(() => expect(onFinalizeReady).toHaveBeenCalled());
    expect(get(resultsWorkspaceView).statusFilter).toBe('ready');
  });

  it('advances to uploaded only after live upload succeeds', async () => {
    const uploadRequest = deferred<void>();
    const onRunUpload = vi.fn().mockReturnValue(uploadRequest.promise);

    render(ResultsWorkspace, {
      workspaceState: workspaceState(),
      studentDisplayNamesByRef: {
        student_1: 'Jordan Scott',
        student_2: 'Alex Tobin',
      },
      onLoadReportPreview: vi.fn().mockResolvedValue({
        studentRef: 'student_2',
        resultFingerprint: 'fp_2',
        html: '<!doctype html><html><body>Preview Two</body></html>',
      }),
      onRunUpload,
    });

    await fireEvent.click(screen.getByLabelText('Select Jordan Scott'));
    await fireEvent.click(screen.getByRole('button', { name: 'Upload' }));

    expect(get(resultsWorkspaceView).statusFilter).toBe('all');

    uploadRequest.resolve();

    await waitFor(() => expect(get(resultsWorkspaceView).statusFilter).toBe('uploaded'));
  });

  it('does not advance after failed live upload', async () => {
    const uploadRequest = deferred<void>();
    const onRunUpload = vi.fn().mockReturnValue(uploadRequest.promise);

    render(ResultsWorkspace, {
      workspaceState: workspaceState(),
      studentDisplayNamesByRef: {
        student_1: 'Jordan Scott',
        student_2: 'Alex Tobin',
      },
      onLoadReportPreview: vi.fn().mockResolvedValue({
        studentRef: 'student_2',
        resultFingerprint: 'fp_2',
        html: '<!doctype html><html><body>Preview Two</body></html>',
      }),
      onRunUpload,
    });

    await fireEvent.click(screen.getByLabelText('Select Jordan Scott'));
    await fireEvent.click(screen.getByRole('button', { name: 'Upload' }));

    uploadRequest.reject(new Error('upload failed'));

    await waitFor(() => expect(onRunUpload).toHaveBeenCalled());
    expect(get(resultsWorkspaceView).statusFilter).toBe('all');
  });

  it('shows transient uploading and uploaded states before the workspace refresh returns', async () => {
    render(ResultsWorkspace, {
      workspaceState: workspaceState(),
      studentDisplayNamesByRef: {
        student_1: 'Jordan Scott',
        student_2: 'Alex Tobin',
      },
      onLoadReportPreview: vi.fn().mockResolvedValue({
        studentRef: 'student_1',
        resultFingerprint: 'fp_1',
        html: '<!doctype html><html><body>Preview One</body></html>',
      }),
    });

    resultsWorkspaceView.startUploadBatch('batch_1', ['student_1']);
    resultsWorkspaceView.setSelectedStudentRef('student_1');
    await waitFor(() => {
      expect(screen.getAllByText('Uploading').length).toBeGreaterThan(0);
    });

    resultsWorkspaceView.setStudentUploadProgress('batch_1', 'student_1', 'uploaded');
    await waitFor(() => {
      expect(screen.getAllByText('Uploaded').length).toBeGreaterThan(0);
    });
  });

  it('opens upload history details with per-student statuses', async () => {
    render(ResultsWorkspace, {
      workspaceState: workspaceState(),
      studentDisplayNamesByRef: {
        student_1: 'Jordan Scott',
        student_2: 'Alex Tobin',
      },
      onLoadReportPreview: vi.fn().mockResolvedValue({
        studentRef: 'student_2',
        resultFingerprint: 'fp_2',
        html: '<!doctype html><html><body>Preview Two</body></html>',
      }),
    });

    const uploadAttemptButton = screen.getByRole('button', { name: /Upload · 1\/1/i });
    await fireEvent.click(uploadAttemptButton);

    expect(await screen.findByRole('dialog', { name: 'Upload attempt details' })).toBeTruthy();
    expect(screen.getAllByText('Alex Tobin').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Uploaded').length).toBeGreaterThan(0);

    await fireEvent.keyDown(document, { key: 'Escape' });

    expect(screen.queryByRole('dialog', { name: 'Upload attempt details' })).toBeNull();
    expect(document.activeElement).toBe(uploadAttemptButton);
  });

  it('hides LMS controls for local-only projects', async () => {
    const local = workspaceState();
    local.project.lmsCourseId = null;
    local.projectConfig!.lmsCourseId = null;
    const onRunExport = vi.fn().mockResolvedValue(true);

    render(ResultsWorkspace, {
      workspaceState: local,
      studentDisplayNamesByRef: {
        student_1: 'Jordan Scott',
        student_2: 'Alex Tobin',
      },
      onLoadReportPreview: vi.fn().mockResolvedValue({
        studentRef: 'student_2',
        resultFingerprint: 'fp_2',
        html: '<!doctype html><html><body>Preview Two</body></html>',
      }),
      onRunExport,
    });

    expect(screen.queryByText(/Local-only project/i)).toBeNull();
    expect(screen.queryByRole('button', { name: 'Upload' })).toBeNull();
    const exportButton = screen.getByRole('button', { name: 'Export' }) as HTMLButtonElement;
    expect(exportButton.disabled).toBe(true);
    expect(screen.queryByLabelText('Select LMS assignment')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Finalize' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Unfinalize' })).toBeNull();
    expect(screen.queryByText('Show Export action')).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Filter students by result status' }));
    const filterDialog = screen.getByRole('dialog', { name: 'Filter students by result status' });
    expect(within(filterDialog).getByRole('button', { name: 'All' })).toBeTruthy();
    expect(within(filterDialog).getByRole('button', { name: 'Ready' })).toBeTruthy();
    expect(within(filterDialog).queryByRole('button', { name: 'Finalized' })).toBeNull();
    expect(within(filterDialog).queryByRole('button', { name: 'Uploaded' })).toBeNull();

    expect(screen.queryByText('Upload history')).toBeNull();
  });

  it('hides LMS export by default and reveals it from display settings', async () => {
    const onRunExport = vi.fn().mockResolvedValue(true);

    render(ResultsWorkspace, {
      workspaceState: workspaceState(),
      studentDisplayNamesByRef: {
        student_1: 'Jordan Scott',
        student_2: 'Alex Tobin',
      },
      onLoadReportPreview: vi.fn().mockResolvedValue({
        studentRef: 'student_2',
        resultFingerprint: 'fp_2',
        html: '<!doctype html><html><body>Preview Two</body></html>',
      }),
      onRunExport,
    });

    expect(screen.getByRole('button', { name: 'Upload' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Export' })).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Student display settings' }));
    await fireEvent.click(screen.getByRole('button', { name: /Show Export action/i }));

    expect(screen.getByRole('button', { name: 'Upload' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Export' })).toBeTruthy();
  });

  it('exports only visible checked ready rows after choosing a format', async () => {
    const onRunExport = vi.fn().mockResolvedValue(true);

    render(ResultsWorkspace, {
      workspaceState: workspaceState(),
      studentDisplayNamesByRef: {
        student_1: 'Jordan Scott',
        student_2: 'Alex Tobin',
      },
      onLoadReportPreview: vi.fn().mockResolvedValue({
        studentRef: 'student_2',
        resultFingerprint: 'fp_2',
        html: '<!doctype html><html><body>Preview Two</body></html>',
      }),
      onRunExport,
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Student display settings' }));
    await fireEvent.click(screen.getByRole('button', { name: /Show Export action/i }));
    await fireEvent.click(screen.getByLabelText('Select Alex Tobin'));
    await fireEvent.click(screen.getByRole('button', { name: 'Export' }));
    await fireEvent.click(screen.getByRole('button', { name: 'CSV' }));

    await waitFor(() => expect(onRunExport).toHaveBeenCalledWith('csv', ['student_2']));
  });

  it('uses the HTML choice for zipped report export', async () => {
    const onRunExport = vi.fn().mockResolvedValue(true);
    const local = workspaceState();
    local.project.lmsCourseId = null;
    local.projectConfig!.lmsCourseId = null;

    render(ResultsWorkspace, {
      workspaceState: local,
      studentDisplayNamesByRef: {
        student_1: 'Jordan Scott',
        student_2: 'Alex Tobin',
      },
      onLoadReportPreview: vi.fn().mockResolvedValue({
        studentRef: 'student_2',
        resultFingerprint: 'fp_2',
        html: '<!doctype html><html><body>Preview Two</body></html>',
      }),
      onRunExport,
    });

    await fireEvent.click(screen.getByLabelText('Select Alex Tobin'));
    await fireEvent.click(screen.getByRole('button', { name: 'Export' }));
    await fireEvent.click(screen.getByRole('button', { name: 'HTML' }));

    await waitFor(() => expect(onRunExport).toHaveBeenCalledWith('html_zip', ['student_2']));
  });

  it('does not invoke export when the format dialog is cancelled', async () => {
    const onRunExport = vi.fn().mockResolvedValue(true);
    const local = workspaceState();
    local.project.lmsCourseId = null;
    local.projectConfig!.lmsCourseId = null;

    render(ResultsWorkspace, {
      workspaceState: local,
      studentDisplayNamesByRef: {
        student_1: 'Jordan Scott',
        student_2: 'Alex Tobin',
      },
      onLoadReportPreview: vi.fn().mockResolvedValue({
        studentRef: 'student_2',
        resultFingerprint: 'fp_2',
        html: '<!doctype html><html><body>Preview Two</body></html>',
      }),
      onRunExport,
    });

    await fireEvent.click(screen.getByLabelText('Select Alex Tobin'));
    await fireEvent.click(screen.getByRole('button', { name: 'Export' }));
    expect(screen.getByRole('dialog', { name: 'Export results' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    expect(onRunExport).not.toHaveBeenCalled();
  });
});
