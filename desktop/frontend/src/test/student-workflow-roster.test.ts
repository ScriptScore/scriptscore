// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { appSettings } from '$lib/stores/appSettings';
import { baseWorkspaceState, configuredCanvasSettings } from './studentTestFixtures';

const desktopMocks = vi.hoisted(() => ({
  computeLmsBindingToken: vi.fn(),
  ensureLmsRosterPreload: vi.fn(),
  getExamWorkspaceState: vi.fn(),
  getLmsRosterCacheState: vi.fn(),
  intakeDefaultPdfRectsFromTemplate: vi.fn(),
  listCanvasCourseRoster: vi.fn(),
  priorCanonicalSubmissionExistsForLmsStudent: vi.fn(),
  toDesktopAssetUrl: vi.fn((path: string) => `asset://${path}`),
  transientClipPdfRectsPngBase64: vi.fn(),
  transientRenderPdfPagePng: vi.fn(),
  transientScansOcrHint: vi.fn()
}));

const dialogMocks = vi.hoisted(() => ({
  open: vi.fn()
}));

const webviewMocks = vi.hoisted(() => ({
  getCurrentWebview: vi.fn(() => ({
    onDragDropEvent: vi.fn().mockResolvedValue(() => {})
  }))
}));

const shellMocks = vi.hoisted(() => ({
  ensureRuntimeJobBridge: vi.fn().mockResolvedValue(undefined),
  onRuntimeJobEvent: vi.fn((handler: unknown) => {
    void handler;
    return () => {};
  })
}));

vi.mock('$lib/desktop', async () => {
  const actual = await vi.importActual<typeof import('$lib/desktop')>('$lib/desktop');
  return {
    ...actual,
    ...desktopMocks
  };
});

vi.mock('@tauri-apps/plugin-dialog', () => dialogMocks);
vi.mock('@tauri-apps/api/webview', () => webviewMocks);

vi.mock('$lib/stores/shell', async () => {
  const { writable } = await vi.importActual<typeof import('svelte/store')>('svelte/store');
  return {
    shellState: writable({
      currentProject: null,
      workerStatus: 'ready',
      lastRuntimeError: null
    }),
    jobProgress: writable<number | null>(null),
    ensureRuntimeJobBridge: shellMocks.ensureRuntimeJobBridge,
    onRuntimeJobEvent: shellMocks.onRuntimeJobEvent
  };
});

import StudentWorkflowWorkspace from '$lib/components/desktop/StudentWorkflowWorkspace.svelte';
import { shellState } from '$lib/stores/shell';

describe('StudentWorkflowWorkspace roster and stage state', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    shellState.set({
      currentProject: null,
      workerStatus: 'ready',
      lastRuntimeError: null,
      debugFeatures: { redactionToggle: false }
    });
    appSettings.save(configuredCanvasSettings());
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([]);
    desktopMocks.getLmsRosterCacheState.mockImplementation(async () => ({
      status: 'ready',
      projectPath: '/tmp/project',
      lmsProvider: 'canvas',
      courseId: 'persisted-course-id',
      rows: await desktopMocks.listCanvasCourseRoster(),
      lastError: null,
      idleReason: null
    }));
    desktopMocks.ensureLmsRosterPreload.mockImplementation(async () =>
      await desktopMocks.getLmsRosterCacheState()
    );
  });

  it('uses the persisted project LMS course id for overwrite checks inside the workflow shell', async () => {
    desktopMocks.priorCanonicalSubmissionExistsForLmsStudent.mockResolvedValue(false);
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      {
        userId: 'canvas_42',
        displayName: 'Jordan Rivera',
        sortKey: 'rivera, jordan'
      }
    ]);
    desktopMocks.computeLmsBindingToken.mockResolvedValue('token_42');
    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_42' }],
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 4,
              examPagePaths: ['/tmp/page_001.png'],
              warnings: []
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'draft-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    await waitFor(() => {
      expect(desktopMocks.ensureLmsRosterPreload).toHaveBeenCalled();
      expect(desktopMocks.computeLmsBindingToken).toHaveBeenCalledWith(
        'persisted-course-id',
        'canvas_42'
      );
    });
  });

  it('shows a loading student label instead of Unknown student while the roster warms', async () => {
    desktopMocks.getLmsRosterCacheState.mockResolvedValue({
      status: 'loading',
      projectPath: '/tmp/project',
      lmsProvider: 'canvas',
      courseId: 'persisted-course-id',
      rows: [],
      lastError: null,
      idleReason: null
    });
    desktopMocks.ensureLmsRosterPreload.mockResolvedValue({
      status: 'loading',
      projectPath: '/tmp/project',
      lmsProvider: 'canvas',
      courseId: 'persisted-course-id',
      rows: [],
      lastError: null,
      idleReason: null
    });

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_42' }],
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 4,
              examPagePaths: ['/tmp/page_001.png'],
              warnings: []
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    expect((await screen.findAllByText('Loading student')).length).toBeGreaterThan(0);
    expect(screen.queryByText('Unknown student')).toBeNull();
  });

  it('shows local-only canonical intake rows as workflow eligible', async () => {
    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        project: {
          ...baseWorkspaceState().project,
          lmsCourseId: null
        },
        projectConfig: {
          ...baseWorkspaceState().projectConfig!,
          lmsCourseId: null
        },
        studentRoster: [],
        studentIntake: {
          status: 'ok',
          latestJobId: 'job_intake_1',
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              localDisplayName: 'Ada Local',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: []
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: null,
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    expect(await screen.findByText(/1 submission ready/)).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Begin Workflow' }) as HTMLButtonElement).disabled).toBe(false);
    expect(screen.queryByText('No submissions are ready yet. Upload a student PDF to prepare it for grading.')).toBeNull();
    expect(screen.getAllByText('Ada Local').length).toBeGreaterThan(0);
    expect(
      screen.queryByText('Link an LMS course in Template setup before loading the roster.')
    ).toBeNull();
    expect(screen.queryByText('No LMS course linked.')).toBeNull();
    expect(desktopMocks.ensureLmsRosterPreload).not.toHaveBeenCalled();
  });

  it('does not show roster warnings or retry actions when LMS provider is disabled', async () => {
    appSettings.save({
      ...configuredCanvasSettings(),
      lmsProvider: 'none',
      lmsCanvasBaseUrl: '',
      lmsCanvasApiKey: null
    });
    desktopMocks.getLmsRosterCacheState.mockResolvedValue({
      status: 'idle',
      projectPath: '/tmp/project',
      lmsProvider: null,
      courseId: null,
      rows: [],
      lastError: null,
      idleReason: 'Choose an LMS provider in Settings before loading the roster.'
    });

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState(),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    const sidebar = screen.getByRole('region', { name: 'Student roster and submission upload' });
    expect(
      screen.queryByText('Choose an LMS provider in Settings before loading the roster.')
    ).toBeNull();
    expect(within(sidebar).queryByRole('button', { name: 'Retry' })).toBeNull();
    expect(desktopMocks.ensureLmsRosterPreload).not.toHaveBeenCalled();
  });

  it('confirms deletion from the selected student panel', async () => {
    const onDeleteSubmission = vi.fn().mockResolvedValue(undefined);
    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        project: {
          ...baseWorkspaceState().project,
          lmsCourseId: null
        },
        projectConfig: {
          ...baseWorkspaceState().projectConfig!,
          lmsCourseId: null
        },
        studentRoster: [],
        studentIntake: {
          status: 'ok',
          latestJobId: 'job_intake_1',
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              localDisplayName: 'Ada Local',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 1,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: []
            },
            {
              studentRef: 'student_2',
              localDisplayName: 'Grace Local',
              canonicalPdfPath: '/tmp/student_2.pdf',
              ingestStatus: 'ok',
              pageCount: 1,
              examPagePaths: ['/tmp/student_2_p1.png'],
              warnings: []
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: null,
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onDeleteSubmission
    });

    const sidebar = screen.getByRole('region', { name: 'Student roster and submission upload' });
    await fireEvent.click(within(sidebar).getByRole('button', { name: /Ada Local/i }));
    await fireEvent.click(screen.getByTitle('Delete submission for Ada Local'));

    const dialog = screen.getByRole('dialog', { name: 'Delete student submission?' });
    expect(within(dialog).getByText(/Ada Local/)).toBeTruthy();
    expect(onDeleteSubmission).not.toHaveBeenCalled();

    await fireEvent.click(within(dialog).getByRole('button', { name: 'Delete submission' }));

    expect(onDeleteSubmission).toHaveBeenCalledWith('student_1', 'student_2');
  });

  it('does not offer deletion for a roster student without submission state', async () => {
    const onDeleteSubmission = vi.fn().mockResolvedValue(undefined);
    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_42' }],
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: []
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onDeleteSubmission
    });

    const sidebar = screen.getByRole('region', { name: 'Student roster and submission upload' });
    await fireEvent.click(await within(sidebar).findByRole('button', { name: /(?:Unknown|Loading) student/i }));

    expect(screen.getByText('No submission on file for this student.')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Delete submission' })).toBeNull();
    expect(onDeleteSubmission).not.toHaveBeenCalled();
  });

  it('highlights intake page count mismatches and removes extra pages before workflow', async () => {
    const onSaveStudentIntakePageOrder = vi.fn().mockResolvedValue(undefined);
    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        project: {
          ...baseWorkspaceState().project,
          lmsCourseId: null
        },
        projectConfig: {
          ...baseWorkspaceState().projectConfig!,
          lmsCourseId: null
        },
        templatePreviewArtifacts: [
          { artifactId: 'template_1', pageNumber: 1, imagePath: '/tmp/template_1.png', label: 'Page 1' },
          { artifactId: 'template_2', pageNumber: 2, imagePath: '/tmp/template_2.png', label: 'Page 2' }
        ],
        studentRoster: [],
        studentIntake: {
          status: 'ok',
          latestJobId: 'job_intake_1',
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              localDisplayName: 'Ada Local',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 3,
              examPagePaths: [
                '/tmp/student_1_p1.png',
                '/tmp/student_1_p2.png',
                '/tmp/student_1_p3.png'
              ],
              warnings: []
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: null,
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onSaveStudentIntakePageOrder
    });

    const sidebar = screen.getByRole('region', { name: 'Student roster and submission upload' });
    await fireEvent.click(within(sidebar).getByRole('button', { name: /Ada Local/i }));
    expect(screen.getByText('Page count mismatch: this submission has 3 pages; the template has 2.')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Next →' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Next →' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Remove page' }));
    await fireEvent.click(
      within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove page' })
    );

    expect(onSaveStudentIntakePageOrder).toHaveBeenCalledWith('student_1', [
      '/tmp/student_1_p1.png',
      '/tmp/student_1_p2.png'
    ]);
  });

  it('offers a stop workflow action while workflow is running', async () => {
    const onStopWorkflow = vi.fn().mockResolvedValue(undefined);
    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState(),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: 'studentWorkflow',
      onFinalizeSubmission: vi.fn(),
      onStopWorkflow
    });

    const stop = screen.getByRole('button', { name: 'Stop Workflow' });
    expect((stop as HTMLButtonElement).disabled).toBe(false);
    await stop.click();

    expect(onStopWorkflow).toHaveBeenCalledTimes(1);
    expect((screen.getByRole('button', { name: 'Running…' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('disables workflow start controls when a recovered workflow session is still running', async () => {
    const onBeginWorkflow = vi.fn().mockResolvedValue(undefined);
    const onRecoverWorkflow = vi.fn().mockResolvedValue(undefined);
    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentIntake: {
          status: 'ready',
          latestJobId: 'job_intake_1',
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: [],
              bindingTokenHex: 'token_42'
            }
          ]
        },
        studentWorkflow: {
          status: 'running',
          latestJobId: 'job_workflow_1',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 2,
              stage: 'parse',
              latestJobId: 'job_stage_1',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: []
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow,
      onRecoverWorkflow
    });

    const headerBegin = await screen.findByRole('button', { name: 'Running…' });
    const playBegin = screen.getByRole('button', { name: 'Begin student workflow' });
    const recover = screen.getByRole('button', { name: 'Recover Workflow' });

    expect((headerBegin as HTMLButtonElement).disabled).toBe(true);
    expect((playBegin as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText(/the desktop runtime has no active job/i)).toBeTruthy();

    await fireEvent.click(headerBegin);
    await fireEvent.click(playBegin);
    expect(onBeginWorkflow).not.toHaveBeenCalled();

    await fireEvent.click(recover);
    expect(onRecoverWorkflow).toHaveBeenCalledTimes(1);
  });

  it('disables workflow start controls when project workflow stage is actively grading', async () => {
    const onBeginWorkflow = vi.fn().mockResolvedValue(undefined);
    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        workflowStage: 'student_grading',
        studentIntake: {
          status: 'ready',
          latestJobId: 'job_intake_1',
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: [],
              bindingTokenHex: 'token_42'
            }
          ]
        },
        studentWorkflow: {
          status: 'ready',
          latestJobId: 'job_workflow_1',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 2,
              stage: 'grading',
              latestJobId: 'job_stage_1',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: []
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow
    });

    const headerBegin = await screen.findByRole('button', { name: 'Running…' });
    const playBegin = screen.getByRole('button', { name: 'Begin student workflow' });

    expect((headerBegin as HTMLButtonElement).disabled).toBe(true);
    expect((playBegin as HTMLButtonElement).disabled).toBe(true);

    await fireEvent.click(headerBegin);
    await fireEvent.click(playBegin);
    expect(onBeginWorkflow).not.toHaveBeenCalled();
  });

  it('disables workflow start controls after live reload while worker is busy and cards are active', async () => {
    shellState.set({
      currentProject: null,
      workerStatus: 'busy',
      lastRuntimeError: null,
      debugFeatures: { redactionToggle: false }
    });
    const onBeginWorkflow = vi.fn().mockResolvedValue(undefined);
    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        workflowStage: 'student_intake_ready',
        studentIntake: {
          status: 'ready',
          latestJobId: 'job_intake_1',
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: [],
              bindingTokenHex: 'token_42'
            }
          ]
        },
        studentWorkflow: {
          status: 'ready',
          latestJobId: 'job_workflow_1',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 2,
              stage: 'parse',
              latestJobId: 'job_stage_1',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: []
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn(),
      onBeginWorkflow
    });

    const headerBegin = await screen.findByRole('button', { name: 'Running…' });
    const playBegin = screen.getByRole('button', { name: 'Begin student workflow' });

    expect((headerBegin as HTMLButtonElement).disabled).toBe(true);
    expect((playBegin as HTMLButtonElement).disabled).toBe(true);

    await fireEvent.click(headerBegin);
    await fireEvent.click(playBegin);
    expect(onBeginWorkflow).not.toHaveBeenCalled();
  });

  it('updates the workflow stage labels when the workspace state changes', async () => {
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      {
        userId: 'canvas_42',
        displayName: 'Jordan Rivera',
        sortKey: 'rivera, jordan'
      }
    ]);
    desktopMocks.computeLmsBindingToken.mockResolvedValue('token_canvas_1');

    const view = render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_canvas_1' }],
        studentIntake: {
          status: 'ok',
          latestJobId: 'job_intake_1',
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: [],
              bindingTokenHex: 'token_canvas_1'
            }
          ]
        },
        studentWorkflow: {
          status: 'ready',
          latestJobId: 'job_workflow_1',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 2,
              stage: 'intake_ready',
              latestJobId: 'job_stage_1',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: []
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    await waitFor(() => {
      expect(screen.getAllByText('Waiting').length).toBeGreaterThan(0);
    });

    await view.rerender({
      workspaceState: baseWorkspaceState({
        studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_canvas_1' }],
        studentIntake: {
          status: 'ok',
          latestJobId: 'job_intake_1',
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: [],
              bindingTokenHex: 'token_canvas_1'
            }
          ]
        },
        studentWorkflow: {
          status: 'running',
          latestJobId: 'job_workflow_1',
          submissions: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              pageCount: 2,
              stage: 'parse',
              latestJobId: 'job_stage_2',
              failureMessage: null,
              warnings: [],
              pageArtifacts: [],
              alignmentPages: [],
              answers: []
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    await waitFor(() => {
      expect(screen.getAllByText('Parsing').length).toBeGreaterThan(0);
    });
  });

  it('does not project one intake row onto multiple students when roster binding tokens collide', async () => {
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Jordan Rivera', sortKey: 'rivera, jordan' },
      { userId: 'canvas_2', displayName: 'Another Student', sortKey: 'student, another' },
      { userId: 'canvas_3', displayName: 'Test Student', sortKey: 'student, test' }
    ]);
    desktopMocks.computeLmsBindingToken
      .mockResolvedValueOnce('token_dup')
      .mockResolvedValueOnce('token_dup')
      .mockResolvedValueOnce('token_3');

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [
          { studentRef: 'student_1', bindingTokenHex: 'token_dup' },
          { studentRef: 'student_2', bindingTokenHex: 'token_other' },
          { studentRef: 'student_3', bindingTokenHex: 'token_3' }
        ],
        studentIntake: {
          status: 'ok',
          latestJobId: 'job_intake_1',
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: [],
              bindingTokenHex: 'token_dup'
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    const sidebar = screen.getByRole('region', { name: 'Student roster and submission upload' });
    await waitFor(() => {
      expect(within(sidebar).getByText('Course Roster')).toBeTruthy();
    });

    const studentOneRow = within(sidebar).getByTitle('canonical · ready').closest('button')!;
    const noSubmissionRows = within(sidebar)
      .getAllByTitle('no submission')
      .map((element) => element.closest('button')!);

    expect(studentOneRow.textContent).toContain('Ready');
    expect(studentOneRow.textContent).not.toContain('student_1');
    expect(studentOneRow.textContent).not.toContain('token_dup');
    expect(noSubmissionRows).toHaveLength(2);
    expect(noSubmissionRows[0].textContent).not.toContain('student_2');
    expect(noSubmissionRows[0].textContent).not.toContain('token_other');
    expect(noSubmissionRows[0].textContent).not.toContain('Ready');
  });

  it('does not warn that the live roster changed when duplicate live rows collapse to the same token', async () => {
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Jordan Rivera', sortKey: 'rivera, jordan' },
      {
        userId: 'canvas_1_duplicate',
        displayName: 'Jordan Rivera',
        sortKey: 'rivera, jordan'
      }
    ]);
    desktopMocks.computeLmsBindingToken.mockResolvedValue('token_dup');

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_dup' }],
        studentIntake: {
          status: 'ready',
          latestJobId: null,
          unresolvedCount: 0,
          items: []
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    expect(await screen.findByText('Course Roster')).toBeTruthy();
    expect(screen.queryByText(/Roster verification mismatch:/)).toBeNull();
  });

  it('reports the specific roster verification diff when live and persisted tokens differ', async () => {
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Jordan Rivera', sortKey: 'rivera, jordan' },
      { userId: 'canvas_2', displayName: 'Another Student', sortKey: 'student, another' }
    ]);
    desktopMocks.computeLmsBindingToken.mockImplementation(async (_courseId: string, userId: string) =>
      userId === 'canvas_1' ? 'token_live_1' : 'token_live_2'
    );

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [
          { studentRef: 'student_1', bindingTokenHex: 'token_live_1' },
          { studentRef: 'student_2', bindingTokenHex: 'token_persisted_only' }
        ]
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    expect(await screen.findByText('Course Roster')).toBeTruthy();
    expect(
      await screen.findByText(/Roster verification mismatch: matched 1\/2 persisted students\./)
    ).toBeTruthy();
    expect(screen.getByText(/Persisted only: student_2 \(token_persis\)\./)).toBeTruthy();
    expect(screen.getByText(/Live LMS only: token_live_2\./)).toBeTruthy();
  });

  it('recomputes roster verification when persisted project bindings change', async () => {
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Jordan Rivera', sortKey: 'rivera, jordan' },
      { userId: 'canvas_2', displayName: 'Another Student', sortKey: 'student, another' }
    ]);
    desktopMocks.computeLmsBindingToken.mockImplementation(async (_courseId: string, userId: string) =>
      userId === 'canvas_1' ? 'token_live_1' : 'token_live_2'
    );

    const view = render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [{ studentRef: 'student_1', bindingTokenHex: 'token_live_1' }]
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    expect(await screen.findByText('Course Roster')).toBeTruthy();
    expect(screen.queryByText(/Roster verification mismatch:/)).toBeNull();

    await view.rerender({
      workspaceState: baseWorkspaceState({
        studentRoster: [
          { studentRef: 'student_1', bindingTokenHex: 'token_live_1' },
          { studentRef: 'student_2', bindingTokenHex: 'token_persisted_only' }
        ]
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    expect(
      await screen.findByText(/Roster verification mismatch: matched 1\/2 persisted students\./)
    ).toBeTruthy();
    expect(screen.getByText(/Persisted only: student_2 \(token_persis\)\./)).toBeTruthy();
  });

  it('calls out likely HMAC secret or course drift when zero roster tokens overlap', async () => {
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_1', displayName: 'Jordan Rivera', sortKey: 'rivera, jordan' },
      { userId: 'canvas_2', displayName: 'Another Student', sortKey: 'student, another' }
    ]);
    desktopMocks.computeLmsBindingToken.mockImplementation(async (_courseId: string, userId: string) =>
      userId === 'canvas_1' ? 'token_live_1' : 'token_live_2'
    );

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [
          { studentRef: 'student_1', bindingTokenHex: 'token_persisted_1' },
          { studentRef: 'student_2', bindingTokenHex: 'token_persisted_2' }
        ]
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    expect(await screen.findByText('Course Roster')).toBeTruthy();
    expect(
      await screen.findByText(
        /Zero tokens overlapped\. This usually means the LMS binding HMAC secret or linked course id changed/
      )
    ).toBeTruthy();
  });

  it('does not use student_ref ordering fallback to attach one processed exam to the wrong roster row', async () => {
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      { userId: 'canvas_jordan', displayName: 'Jordan Rivera', sortKey: 'rivera, jordan' },
      { userId: 'canvas_other', displayName: 'Another Student', sortKey: 'student, another' }
    ]);
    desktopMocks.computeLmsBindingToken
      .mockResolvedValueOnce('token_jordan')
      .mockResolvedValueOnce('token_other');

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState({
        studentRoster: [
          { studentRef: 'student_1', bindingTokenHex: 'token_jordan' },
          { studentRef: 'student_2', bindingTokenHex: 'token_other' }
        ],
        studentIntake: {
          status: 'ok',
          latestJobId: 'job_intake_1',
          unresolvedCount: 0,
          items: [
            {
              studentRef: 'student_1',
              canonicalPdfPath: '/tmp/student_1.pdf',
              ingestStatus: 'ok',
              pageCount: 2,
              examPagePaths: ['/tmp/student_1_p1.png'],
              warnings: [],
              bindingTokenHex: 'token_other'
            }
          ]
        }
      }),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    const sidebar = screen.getByRole('region', { name: 'Student roster and submission upload' });
    await waitFor(() => {
      expect(within(sidebar).getByText('Course Roster')).toBeTruthy();
    });

    const jordanRow = within(sidebar).getByTitle('canonical · ready').closest('button')!;
    const otherRow = within(sidebar).getByTitle('no submission').closest('button')!;

    expect(jordanRow.textContent).toContain('Ready');
    expect(jordanRow.textContent).not.toContain('student_1');
    expect(jordanRow.textContent).not.toContain('token_jordan');

    expect(otherRow.textContent).toContain('Missing');
    expect(otherRow.textContent).not.toContain('student_2');
    expect(otherRow.textContent).not.toContain('token_other');
  });
});
