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
  entry('student_3', 'Katherine Johnson', 'processing', 45, 'manual grading'),
  entry('student_4', 'Dorothy Vaughan', 'ready', 0, 'ready to finalize'),
  entry('student_5', 'Mary Jackson', 'failedStopped', 12, 'workflow failed'),
  entry('student_6', 'No Submission', 'noSubmission', 0, 'no submission')
];

describe('StudentWorkflowSidebar management controls', () => {
  it('renders compact status pills while preserving detailed status titles', () => {
    render(StudentWorkflowSidebar, {
      entries,
      selectedStudentRef: null,
      busyAction: null,
      rosterBusy: false
    });

    expect(screen.getByText('Graded')).toBeTruthy();
    expect(screen.getByText('Review')).toBeTruthy();
    expect(screen.getByText('Processing')).toBeTruthy();
    expect(screen.getByText('Ready')).toBeTruthy();
    expect(screen.getByText('Blocked')).toBeTruthy();
    expect(screen.getByText('Missing')).toBeTruthy();

    expect(screen.getByTitle('draft grading ready').textContent).toContain('Graded');
    expect(screen.getByTitle('alignment review').textContent).toContain('Review');
    expect(screen.getByTitle('manual grading').textContent).toContain('Processing');
    expect(screen.getByTitle('ready to finalize').textContent).toContain('Ready');
    expect(screen.getByTitle('workflow failed').textContent).toContain('Blocked');
    expect(screen.getByTitle('no submission').textContent).toContain('Missing');

    expect(screen.queryByText('draft grading ready')).toBeNull();
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
        workflowRow('student_6', 'Student 6', 'transforming'),
        workflowRow('student_7', 'Student 7', 'detecting'),
        workflowRow('student_8', 'Student 8', 'region review'),
        workflowRow('student_9', 'Student 9', 'cropping'),
        workflowRow('student_10', 'Student 10', 'screening PII'),
        workflowRow('student_11', 'Student 11', 'parsing'),
        workflowRow('student_12', 'Student 12', 'parse review'),
        workflowRow('student_13', 'Student 13', 'grading'),
        workflowRow('student_14', 'Student 14', 'manual grading'),
        workflowRow('student_15', 'Student 15', 'draft grading ready'),
        workflowRow('student_16', 'Student 16', 'failed')
      ],
      canonicalReadyCount: 16
    });

    [
      'Waiting',
      'Stopped',
      'Aligning',
      'Alignment',
      'Canonicalizing',
      'Transforming',
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
});
