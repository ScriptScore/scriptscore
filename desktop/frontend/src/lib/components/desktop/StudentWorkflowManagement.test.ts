// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import StudentWorkflowBoard from './StudentWorkflowBoard.svelte';
import StudentWorkflowSidebar from './StudentWorkflowSidebar.svelte';
import type { SidebarStudentEntry } from './student-workflow-helpers';

const webviewMocks = vi.hoisted(() => ({
  getCurrentWebview: vi.fn(() => ({
    onDragDropEvent: vi.fn().mockResolvedValue(() => {})
  }))
}));

vi.mock('@tauri-apps/api/webview', () => webviewMocks);

function entry(
  studentRef: string,
  displayName: string,
  statusGroup: SidebarStudentEntry['statusGroup'],
  progress: number,
  label: string = statusGroup
): SidebarStudentEntry {
  return {
    studentRef,
    displayName,
    statusGroup,
    progress,
    dotClass: 'bg-muted',
    label,
    debugLine: ''
  };
}

const entries = [
  entry('student_1', 'Grace Hopper', 'graded', 100, 'draft grading ready'),
  entry('student_2', 'Ada Lovelace', 'needsReview', 70, 'alignment review'),
  entry('student_3', 'Katherine Johnson', 'manual', 45, 'manual grading'),
  entry('student_4', 'Dorothy Vaughan', 'ready', 0, 'ready to finalize'),
  entry('student_5', 'Mary Jackson', 'failed', 12, 'workflow failed'),
  entry('student_6', 'Stop Row', 'stopped', 12, 'workflow stopped'),
  entry('student_7', 'No Submission', 'noSubmission', 0, 'no submission')
];

describe('StudentWorkflowSidebar management controls', () => {
  it('opens the student scan guide from the upload area', async () => {
    render(StudentWorkflowSidebar, {
      entries,
      selectedStudentRef: null,
      busyAction: null,
      rosterBusy: false
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Student scan guide' }));

    const guideImage = screen.getByRole('img', {
      name: 'Three-step student submission intake guide showing PDF upload, privacy region review, and roster matching'
    });
    expect(guideImage.getAttribute('src')).toBe('/student-intake-guide.png');
    expect(screen.getByRole('dialog', { name: 'Student submission intake guide' })).toBeTruthy();
  });

  it('renders compact status pills while preserving detailed status titles', () => {
    render(StudentWorkflowSidebar, {
      entries,
      selectedStudentRef: null,
      busyAction: null,
      rosterBusy: false
    });

    expect(screen.getByText('Graded')).toBeTruthy();
    expect(screen.getByText('Review')).toBeTruthy();
    expect(screen.getByText('Manual')).toBeTruthy();
    expect(screen.getByText('Ready')).toBeTruthy();
    expect(screen.getByText('Failed')).toBeTruthy();
    expect(screen.getByText('Stopped')).toBeTruthy();
    expect(screen.getByText('Missing')).toBeTruthy();

    expect(screen.getByTitle('draft grading ready').textContent).toContain('Graded');
    expect(screen.getByTitle('alignment review').textContent).toContain('Review');
    expect(screen.getByTitle('manual grading').textContent).toContain('Manual');
    expect(screen.getByTitle('ready to finalize').textContent).toContain('Ready');
    expect(screen.getByTitle('workflow failed').textContent).toContain('Failed');
    expect(screen.getByTitle('workflow stopped').textContent).toContain('Stopped');
    expect(screen.getByTitle('no submission').textContent).toContain('Missing');

    expect(screen.queryByText('draft grading ready')).toBeNull();
    expect(screen.queryByText('Blocked')).toBeNull();
    expect(screen.queryByText('Failed/stopped')).toBeNull();
  });

  it('filters rows by workflow status group', async () => {
    render(StudentWorkflowSidebar, {
      entries,
      selectedStudentRef: null,
      busyAction: null,
      rosterBusy: false
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Filter students by workflow status' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Needs review' }));

    expect(screen.getByText('Ada Lovelace')).toBeTruthy();
    expect(screen.queryByText('Grace Hopper')).toBeNull();
    expect(screen.queryByText('Katherine Johnson')).toBeNull();
  });

  it('filters missing submissions separately from failed or stopped rows', async () => {
    render(StudentWorkflowSidebar, {
      entries,
      selectedStudentRef: null,
      busyAction: null,
      rosterBusy: false
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Filter students by workflow status' }));
    await fireEvent.click(screen.getByRole('button', { name: 'No submission' }));

    expect(screen.getByText('No Submission')).toBeTruthy();
    expect(screen.queryByText('Mary Jackson')).toBeNull();
    expect(screen.queryByText('Stop Row')).toBeNull();
  });

  it('filters failed and stopped rows independently', async () => {
    render(StudentWorkflowSidebar, {
      entries,
      selectedStudentRef: null,
      busyAction: null,
      rosterBusy: false
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Filter students by workflow status' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Stopped' }));

    expect(screen.getByText('Stop Row')).toBeTruthy();
    expect(screen.queryByText('Mary Jackson')).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Filter students by workflow status' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Failed' }));

    expect(screen.getByText('Mary Jackson')).toBeTruthy();
    expect(screen.queryByText('Stop Row')).toBeNull();
  });

  it('searches rows by student name', async () => {
    render(StudentWorkflowSidebar, {
      entries,
      selectedStudentRef: null,
      busyAction: null,
      rosterBusy: false
    });

    await fireEvent.input(screen.getByRole('searchbox', { name: 'Search students' }), {
      target: { value: 'johnson' }
    });

    expect(screen.getByText('Katherine Johnson')).toBeTruthy();
    expect(screen.queryByText('Ada Lovelace')).toBeNull();
    expect(screen.queryByText('Grace Hopper')).toBeNull();
  });

  it('sorts by name, status, and progress', async () => {
    render(StudentWorkflowSidebar, {
      entries,
      selectedStudentRef: null,
      busyAction: null,
      rosterBusy: false
    });

    const nav = screen.getByRole('navigation');
    expect(nav.textContent?.indexOf('Ada Lovelace')).toBeLessThan(
      nav.textContent?.indexOf('Dorothy Vaughan') ?? 0
    );

    await fireEvent.click(screen.getByRole('button', { name: 'Sort students' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Status' }));
    expect(nav.textContent?.indexOf('Ada Lovelace')).toBeLessThan(
      nav.textContent?.indexOf('Katherine Johnson') ?? 0
    );

    await fireEvent.click(screen.getByRole('button', { name: 'Sort students' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Progress' }));
    expect(nav.textContent?.indexOf('Dorothy Vaughan')).toBeLessThan(
      nav.textContent?.indexOf('Mary Jackson') ?? 0
    );

    await fireEvent.click(screen.getByRole('button', { name: 'Sort descending' }));
    expect(nav.textContent?.indexOf('Grace Hopper')).toBeLessThan(
      nav.textContent?.indexOf('Ada Lovelace') ?? 0
    );
    expect(screen.getByRole('button', { name: 'Sort ascending' })).toBeTruthy();
  });

});

describe('StudentWorkflowBoard workflow trigger', () => {
  function workflowRow(studentRef: string, displayName: string, stageText: string, stageProgress = 0) {
    return {
      student: { studentRef },
      displayName,
      item: {
        studentRef,
        canonicalPdfPath: `/tmp/${studentRef}.pdf`,
        ingestStatus: 'ok',
        pageCount: 1,
        examPagePaths: [`/tmp/${studentRef}.png`],
        warnings: []
      },
      workflowSubmission: null,
      stageText,
      stageTitle: undefined,
      stageProgress,
      stageTone: 'muted' as const,
      stageComplete: false,
      stageActive: false
    };
  }

  function renderBoard(props: Record<string, unknown> = {}) {
    return render(StudentWorkflowBoard, {
      courseCode: 'CHEM 201',
      displayName: 'Midterm',
      intakeComplete: 1,
      processingCount: 0,
      attentionCount: 0,
      gradedCount: 0,
      readyCount: 1,
      canonicalReadyCount: 1,
      busyActionLabel: null,
      stopWorkflowBusy: false,
      attentionItems: [],
      canonicalReadyRows: [workflowRow('student_1', 'Ada Lovelace', 'waiting')],
      onSelectStudent: vi.fn(),
      onBeginWorkflow: vi.fn(),
      ...props
    });
  }

  it('starts workflow from the large play icon', async () => {
    const onBeginWorkflow = vi.fn();
    renderBoard({ onBeginWorkflow });

    await fireEvent.click(screen.getByRole('button', { name: 'Begin student workflow' }));

    expect(onBeginWorkflow).toHaveBeenCalledTimes(1);
  });

  it('disables the large play icon while busy or empty', async () => {
    const { rerender } = renderBoard({ busyActionLabel: 'Running...' });
    expect(screen.getByRole('button', { name: 'Begin student workflow' })).toHaveProperty(
      'disabled',
      true
    );

    await rerender({ canonicalReadyRows: [], canonicalReadyCount: 0, busyActionLabel: null });
    expect(screen.getByRole('button', { name: 'Begin student workflow' })).toHaveProperty(
      'disabled',
      true
    );
  });

  it('shows one-word progress states on student workflow cards', () => {
    renderBoard({
      canonicalReadyRows: [
        workflowRow('student_1', 'Student 1', 'waiting'),
        workflowRow('student_2', 'Student 2', 'stopped'),
        workflowRow('student_3', 'Student 3', 'aligning'),
        workflowRow('student_4', 'Student 4', 'alignment review'),
        workflowRow('student_5', 'Student 5', 'canonicalizing'),
        workflowRow('student_6', 'Student 6', 'detecting'),
        workflowRow('student_7', 'Student 7', 'region review'),
        workflowRow('student_8', 'Student 8', 'cropping'),
        workflowRow('student_9', 'Student 9', 'screening PII'),
        workflowRow('student_10', 'Student 10', 'parsing'),
        workflowRow('student_11', 'Student 11', 'parse review'),
        workflowRow('student_12', 'Student 12', 'grading'),
        workflowRow('student_13', 'Student 13', 'manual grading'),
        workflowRow('student_14', 'Student 14', 'draft grading ready'),
        workflowRow('student_15', 'Student 15', 'failed')
      ],
      canonicalReadyCount: 15
    });

    [
      'Waiting',
      'Stopped',
      'Aligning',
      'Alignment',
      'Canonicalizing',
      'Detecting',
      'Regions',
      'Cropping',
      'Screening',
      'Parsing',
      'Parse',
      'Grading',
      'Manual',
      'Graded',
      'Failed'
    ].forEach((label) => {
      expect(screen.getByRole('progressbar', { name: label })).toBeTruthy();
    });

    expect(screen.getByTitle('draft grading ready')).toBeTruthy();
    expect(screen.queryByText('draft grading ready')).toBeNull();
  });

  it('uses the available pane height as the student card scroll container', () => {
    renderBoard({
      canonicalReadyRows: Array.from({ length: 18 }, (_, index) =>
        workflowRow(`student_${index + 1}`, `Student ${index + 1}`, 'waiting')
      ),
      canonicalReadyCount: 18
    });

    expect(screen.getByLabelText('Student workflow board').className).toContain('min-h-0');

    const submissionsRegion = screen.getByLabelText('Student workflow submissions');
    expect(submissionsRegion.className).toContain('min-h-0');
    expect(submissionsRegion.className).toContain('flex-1');
    expect(submissionsRegion.className).toContain('overflow-y-auto');
    expect(screen.getByRole('button', { name: 'Open Student 18 workflow' })).toBeTruthy();
  });

  it('keeps compact labels when scoped progress details are available', () => {
    renderBoard({
      canonicalReadyRows: [
        {
          ...workflowRow('student_1', 'Student 1', 'detecting', 28),
          stageTitle: 'detecting · 2/12 questions complete · active question Q3 (question_3)',
          stageTone: 'info' as const,
          stageActive: true
        }
      ]
    });

    expect(screen.getByRole('progressbar', { name: 'Detecting' })).toBeTruthy();
    expect(
      screen.getByTitle('detecting · 2/12 questions complete · active question Q3 (question_3)')
    ).toBeTruthy();
  });
});
