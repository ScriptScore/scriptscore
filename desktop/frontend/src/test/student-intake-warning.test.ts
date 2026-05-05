// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { appSettings } from '$lib/stores/appSettings';
import { shellState } from '$lib/stores/shell';
import type { RuntimeJobEvent } from '$lib/types';
import {
  baseWorkspaceState,
  configuredCanvasSettings,
  defaultRects,
  finalizeResult,
  previewPage
} from './studentTestFixtures';

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
  onRuntimeJobEvent: vi.fn((handler: (event: RuntimeJobEvent) => void) => {
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

import StudentIntakeWorkspace from '$lib/components/desktop/StudentIntakeWorkspace.svelte';
import StudentWorkflowWorkspace from '$lib/components/desktop/StudentWorkflowWorkspace.svelte';

const readySharedRosterProps = {
  sharedRosterStatus: 'ready' as const,
  sharedRosterRows: [
    {
      userId: 'canvas_42',
      displayName: 'Jordan Rivera',
      sortKey: 'rivera, jordan'
    }
  ],
  sharedRosterMessage: null
};

async function advanceToAssociationStep(component: { __testConfirmRegions: () => Promise<void> }): Promise<void> {
  await screen.findByAltText('Preview page');
  await screen.findAllByAltText(/Cropped region preview/);
  await component.__testConfirmRegions();
  await waitFor(() => {
    expect(desktopMocks.transientScansOcrHint).toHaveBeenCalled();
  });
  await screen.findByRole('combobox');
}

function runtimeEvent(
  eventType: RuntimeJobEvent['eventType'],
  commandName: string,
  percent: number
): RuntimeJobEvent {
  return {
    eventType,
    commandName,
    workerStatus: 'busy',
    requestId: 'request_1',
    jobId: 'job_1',
    payload: {
      progress: { percent }
    }
  };
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function requireRuntimeHandler(
  handler: ((event: RuntimeJobEvent) => void) | null
): (event: RuntimeJobEvent) => void {
  if (!handler) {
    throw new Error('Runtime event handler was not registered.');
  }
  return handler;
}

function workerBusyMessage(): string {
  return 'The previous intake attempt did not start because the desktop worker was already running another job. No intake record was created. Wait for that job to finish, then confirm the student again.';
}

describe('student intake overwrite warning', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    appSettings.save(configuredCanvasSettings());
    shellState.set({
      currentProject: null,
      workerStatus: 'ready',
      lastRuntimeError: null,
      debugFeatures: { redactionToggle: false }
    });

    desktopMocks.transientRenderPdfPagePng.mockResolvedValue(previewPage());
    desktopMocks.intakeDefaultPdfRectsFromTemplate.mockResolvedValue(defaultRects());
    desktopMocks.transientClipPdfRectsPngBase64.mockResolvedValue(['ZmFrZS1jcm9w']);
    desktopMocks.transientScansOcrHint.mockResolvedValue({
      hintText: 'Jordan Rivera',
      segmentCount: 1
    });
    desktopMocks.listCanvasCourseRoster.mockResolvedValue([
      {
        userId: 'canvas_42',
        displayName: 'Jordan Rivera',
        sortKey: 'rivera, jordan'
      }
    ]);
    desktopMocks.priorCanonicalSubmissionExistsForLmsStudent.mockResolvedValue(true);
    desktopMocks.computeLmsBindingToken.mockResolvedValue('token_42');
    desktopMocks.getLmsRosterCacheState.mockResolvedValue({
      status: 'ready',
      projectPath: '/tmp/project',
      lmsProvider: 'canvas',
      courseId: 'persisted-course-id',
      rows: readySharedRosterProps.sharedRosterRows,
      lastError: null,
      idleReason: null
    });
    desktopMocks.ensureLmsRosterPreload.mockResolvedValue({
      status: 'ready',
      projectPath: '/tmp/project',
      lmsProvider: 'canvas',
      courseId: 'persisted-course-id',
      rows: readySharedRosterProps.sharedRosterRows,
      lastError: null,
      idleReason: null
    });
    dialogMocks.open.mockReset();
  });

  it('shows the replace warning when workflow state already maps the selected roster student to a canonical PDF', async () => {
    desktopMocks.priorCanonicalSubmissionExistsForLmsStudent.mockResolvedValue(false);
    const view = render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [
        {
          studentRef: 'student_1',
          canonicalPdfPath: '/tmp/student_1.pdf',
          ingestStatus: 'ok',
          pageCount: 4,
          examPagePaths: ['/tmp/page_001.png'],
          warnings: []
        }
      ],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    await advanceToAssociationStep(view.component as { __testConfirmRegions: () => Promise<void> });

    await waitFor(() => {
      expect(desktopMocks.priorCanonicalSubmissionExistsForLmsStudent).toHaveBeenCalledWith(
        'persisted-course-id',
        'canvas_42'
      );
    });
    expect(
      await screen.findByText(
        'This student already has a canonical PDF for this project. Finalizing will replace that submission.'
      )
    ).toBeTruthy();
  });

  it('lets the roster selection list layer beyond the OCR card without its own scrollbar', async () => {
    const view = render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    await advanceToAssociationStep(view.component as { __testConfirmRegions: () => Promise<void> });
    await fireEvent.click(screen.getByRole('combobox', { name: /Course roster student/ }));

    const listbox = screen.getByRole('listbox', { name: 'Course roster student' });
    expect(listbox.className).not.toContain('overflow-y-auto');
    expect(listbox.parentElement?.className).toContain('fixed');
    expect(screen.getByRole('option', { name: 'Jordan Rivera' })).toBeTruthy();
  });

  it('shows Step 4 command progress bars and updates them from runtime events', async () => {
    let runtimeHandler: ((event: RuntimeJobEvent) => void) | null = null;
    shellMocks.onRuntimeJobEvent.mockImplementation((handler: (event: RuntimeJobEvent) => void) => {
      runtimeHandler = handler;
      return () => {};
    });
    const finalizeDeferred = new Promise<null>(() => {});
    const onFinalizeSubmission = vi.fn(async () => await finalizeDeferred);

    const view = render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission
    });

    await advanceToAssociationStep(view.component as { __testConfirmRegions: () => Promise<void> });

    const confirmCheckbox = screen.getByRole('checkbox');
    await fireEvent.click(confirmCheckbox);
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm student →' }));

    expect(onFinalizeSubmission).toHaveBeenCalled();
    expect(await screen.findByText('Redacting PDF (worker) and ingesting exam pages — no action needed')).toBeTruthy();

    expect(screen.getByRole('progressbar', { name: 'Redact PDF' }).getAttribute('aria-valuenow')).toBe('0');
    expect(screen.getByRole('progressbar', { name: 'Ingest Exam' }).getAttribute('aria-valuenow')).toBe('0');
    expect(screen.queryByText('Student ref assigned')).toBeNull();
    expect(screen.queryByText('LMS binding (HMAC)')).toBeNull();
    expect(screen.queryByText(/Worker busy/i)).toBeNull();

    const emitRuntimeEvent = requireRuntimeHandler(runtimeHandler);
    emitRuntimeEvent(runtimeEvent('job_progress', 'scans.pdf-create-redacted', 35));
    await waitFor(() => {
      expect(screen.getByRole('progressbar', { name: 'Redact PDF' }).getAttribute('aria-valuenow')).toBe('35');
    });
    emitRuntimeEvent(runtimeEvent('job_finished', 'scans.pdf-create-redacted', 100));
    emitRuntimeEvent(runtimeEvent('job_progress', 'scans.ingest', 68));
    await waitFor(() => {
      expect(screen.getByRole('progressbar', { name: 'Redact PDF' }).getAttribute('aria-valuenow')).toBe('100');
      expect(screen.getByRole('progressbar', { name: 'Ingest Exam' }).getAttribute('aria-valuenow')).toBe('68');
    });
  });

  it('keeps the wizard on step 4 when finalization returns null and offers retry controls', async () => {
    const onFinalizeSubmission = vi.fn().mockResolvedValue(null);

    const view = render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission
    });

    await advanceToAssociationStep(view.component as { __testConfirmRegions: () => Promise<void> });

    await fireEvent.click(screen.getByRole('checkbox'));
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm student →' }));

    await waitFor(() => {
      expect(onFinalizeSubmission).toHaveBeenCalled();
      expect(
        screen.getByText('Student intake did not complete. Confirm the student again to retry.')
      ).toBeTruthy();
    });
    expect(
      screen.getByText('Redacting PDF (worker) and ingesting exam pages — no action needed')
    ).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Back to association' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Retry finalization' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Confirm student →' })).toBeNull();
    expect(screen.queryByText('Submission ready')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Process next exam →' })).toBeNull();
  });

  it('keeps the wizard on step 4 and shows the concrete finalization error', async () => {
    const onFinalizeSubmission = vi
      .fn()
      .mockRejectedValue('Desktop worker is not available.');

    const view = render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission
    });

    await advanceToAssociationStep(view.component as { __testConfirmRegions: () => Promise<void> });

    await fireEvent.click(screen.getByRole('checkbox'));
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm student →' }));

    await waitFor(() => {
      expect(onFinalizeSubmission).toHaveBeenCalled();
      expect(screen.getByText('Desktop worker is not available.')).toBeTruthy();
    });
    expect(
      screen.getByText('Redacting PDF (worker) and ingesting exam pages — no action needed')
    ).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Back to association' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Retry finalization' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Confirm student →' })).toBeNull();
  });

  it('shows all queued PDF filenames during intake processing', async () => {
    render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam-a.pdf', '/tmp/exam-b.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    expect(await screen.findByText('Queued PDFs')).toBeTruthy();
    expect(screen.getByText('exam-a.pdf')).toBeTruthy();
    expect(screen.getByText('exam-b.pdf')).toBeTruthy();
    expect(screen.getByText('2 submissions in this intake session')).toBeTruthy();
  });

  it('loads and switches between multiple redaction pages during intake preview', async () => {
    desktopMocks.intakeDefaultPdfRectsFromTemplate.mockResolvedValue([
      {
        pageNumber: 1,
        xPt: 10,
        yPt: 10,
        widthPt: 120,
        heightPt: 30
      },
      {
        pageNumber: 2,
        xPt: 20,
        yPt: 20,
        widthPt: 100,
        heightPt: 24
      }
    ]);
    desktopMocks.transientRenderPdfPagePng.mockImplementation(async (_pdfPath, pageNumber) => ({
      ...previewPage(),
      pageNumber,
      pageCount: 2
    }));

    render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    expect(await screen.findByRole('button', { name: 'Page 1' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Page 2' })).toBeTruthy();
    expect(screen.getByText('Name')).toBeTruthy();
    await waitFor(() => {
      expect((screen.getByRole('button', { name: 'Page 2' }) as HTMLButtonElement).disabled).toBe(false);
    });
    expect(desktopMocks.transientRenderPdfPagePng).toHaveBeenCalledWith(
      '/tmp/exam.pdf',
      1,
      2.0,
      1600
    );
    expect(desktopMocks.transientRenderPdfPagePng).toHaveBeenCalledWith(
      '/tmp/exam.pdf',
      2,
      2.0,
      1600
    );

    await fireEvent.click(screen.getByRole('button', { name: 'Page 2' }));
    expect(screen.queryByText('p.2')).toBeNull();
    expect(screen.getByRole('button', { name: 'Page 2' })).toBeTruthy();
    expect(await screen.findByText('Privacy')).toBeTruthy();
    await waitFor(() => {
      expect(screen.queryByText('Name')).toBeNull();
    });
  });

  it('shows the Step 1 preview actions and adds an extra redaction region on the current preview page', async () => {
    desktopMocks.intakeDefaultPdfRectsFromTemplate.mockResolvedValue([
      {
        pageNumber: 1,
        xPt: 10,
        yPt: 10,
        widthPt: 120,
        heightPt: 30
      }
    ]);
    desktopMocks.transientRenderPdfPagePng.mockImplementation(async (_pdfPath, pageNumber) => ({
      ...previewPage(),
      pageNumber,
      pageCount: 1
    }));

    render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    const redactionEditor = await screen.findByRole('button', { name: 'Redaction region editor' });
    expect(screen.getByRole('button', { name: 'Confirm for All Pages' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Confirm regions →' })).toBeNull();
    expect(screen.queryByText('Region crops')).toBeNull();
    expect(await screen.findByAltText(/Name identification crop preview/)).toBeTruthy();
    expect(screen.getByText('Name identification')).toBeTruthy();
    expect(screen.getAllByText('Name')).toHaveLength(1);

    await fireEvent.click(screen.getByRole('button', { name: 'Add Redaction' }));

    expect(screen.getAllByText('Name')).toHaveLength(1);
    expect(screen.getAllByText('Privacy')).toHaveLength(1);

    await fireEvent.pointerDown(redactionEditor, {
      button: 2,
      pointerId: 20,
      clientX: 300,
      clientY: 400
    });
    await fireEvent.contextMenu(redactionEditor, { clientX: 300, clientY: 400 });
    expect(screen.getByRole('dialog', { name: 'Delete redaction region?' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    expect(screen.getAllByText('Name')).toHaveLength(1);
    expect(screen.getAllByText('Privacy')).toHaveLength(1);
    expect(screen.getByAltText(/Name identification crop preview/)).toBeTruthy();

    await fireEvent.pointerDown(redactionEditor, {
      button: 0,
      pointerId: 21,
      clientX: 450,
      clientY: 650
    });
    await fireEvent.pointerMove(redactionEditor, {
      pointerId: 21,
      clientX: 560,
      clientY: 760
    });
    await fireEvent.pointerUp(redactionEditor, {
      pointerId: 21,
      clientX: 560,
      clientY: 760
    });

    await waitFor(() => {
      expect(screen.getAllByText('Privacy')).toHaveLength(2);
    });

    await fireEvent.contextMenu(redactionEditor, { clientX: 300, clientY: 400 });
    expect(screen.getByRole('dialog', { name: 'Delete redaction region?' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Delete' }));

    await waitFor(() => {
      expect(screen.getAllByText('Name')).toHaveLength(1);
      expect(screen.getAllByText('Privacy')).toHaveLength(1);
    });
  });

  it('clears stale crop previews after deleting the final visible redaction region', async () => {
    desktopMocks.intakeDefaultPdfRectsFromTemplate.mockResolvedValue([
      {
        pageNumber: 1,
        xPt: 10,
        yPt: 10,
        widthPt: 120,
        heightPt: 30
      }
    ]);

    render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    const redactionEditor = await screen.findByRole('button', { name: 'Redaction region editor' });
    expect(await screen.findByAltText(/Name identification crop preview/)).toBeTruthy();

    await fireEvent.contextMenu(redactionEditor, { clientX: 50, clientY: 25 });
    expect(screen.getByRole('dialog', { name: 'Delete redaction region?' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Delete' }));

    expect(screen.queryByText('Name')).toBeNull();
    await waitFor(() => {
      expect(screen.queryByAltText(/Name identification crop preview/)).toBeNull();
    });
    expect(screen.getByText('No redaction regions configured on this page.')).toBeTruthy();

    await fireEvent.pointerDown(redactionEditor, {
      button: 0,
      pointerId: 12,
      clientX: 20,
      clientY: 20
    });
    await fireEvent.pointerMove(redactionEditor, {
      pointerId: 12,
      clientX: 140,
      clientY: 80
    });
    await fireEvent.pointerUp(redactionEditor, {
      pointerId: 12,
      clientX: 140,
      clientY: 80
    });

    expect(await screen.findByText('Name')).toBeTruthy();
  });

  it('shows page 1 while later preview pages render and unlocks page buttons as each page completes', async () => {
    const pageTwo = deferred<ReturnType<typeof previewPage>>();
    desktopMocks.transientRenderPdfPagePng.mockImplementation((_pdfPath: string, pageNumber: number) => {
      if (pageNumber === 1) {
        return Promise.resolve(previewPage(1, 3));
      }
      if (pageNumber === 2) {
        return pageTwo.promise;
      }
      return Promise.resolve(previewPage(pageNumber, 3));
    });

    render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    expect(await screen.findByAltText('Preview page')).toBeTruthy();
    expect(screen.queryByText('Rendering 1 page')).toBeNull();
    expect(screen.getByRole('button', { name: 'Redaction region editor' }).className).toContain('cursor-crosshair');
    const cropPreview = await screen.findByAltText(/Cropped region preview 1/);
    expect(cropPreview).toBeTruthy();
    expect(cropPreview.getAttribute('style')).toContain('transform: translate');
    expect(desktopMocks.transientClipPdfRectsPngBase64).not.toHaveBeenCalled();
    expect((screen.getByRole('button', { name: 'Page 1' }) as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByRole('button', { name: 'Page 2' }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole('button', { name: 'Page 3' }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole('button', { name: 'Add Redaction' }) as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByRole('button', { name: 'Wait to Confirm - 33%' }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(screen.getByRole('button', { name: 'Add Redaction' }));
    expect(screen.getByText('Privacy')).toBeTruthy();

    pageTwo.resolve(previewPage(2, 3));
    await waitFor(() => {
      expect((screen.getByRole('button', { name: 'Page 2' }) as HTMLButtonElement).disabled).toBe(false);
      expect((screen.getByRole('button', { name: 'Page 3' }) as HTMLButtonElement).disabled).toBe(false);
      expect((screen.getByRole('button', { name: 'Confirm for All Pages' }) as HTMLButtonElement).disabled).toBe(false);
      expect(screen.getByRole('button', { name: 'Redaction region editor' }).className).toContain('cursor-crosshair');
    });
    expect(screen.getByText('Privacy')).toBeTruthy();
    expect(desktopMocks.transientRenderPdfPagePng).toHaveBeenCalledWith(
      '/tmp/exam.pdf',
      3,
      2.0,
      1600
    );
  });

  it('shows a retryable preview render failure and reloads preview on retry', async () => {
    desktopMocks.transientRenderPdfPagePng
      .mockRejectedValueOnce(new Error('Renderer crashed.'))
      .mockImplementation(async (_pdfPath: string, pageNumber: number) =>
        previewPage(pageNumber, 1)
      );

    render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    expect(await screen.findByText('Preview failed')).toBeTruthy();
    expect(screen.getByText('Error: Renderer crashed.')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Retry preview' }));

    expect(await screen.findByAltText('Preview page')).toBeTruthy();
    expect(desktopMocks.transientRenderPdfPagePng).toHaveBeenCalledTimes(2);
  });

  it('shows shared roster loading and error messages while Step 2 waits for the roster cache', async () => {
    const rosterPreload = deferred<void>();
    const view = render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      sharedRosterStatus: 'idle',
      sharedRosterRows: [],
      sharedRosterMessage: 'Canvas roster cache is warming up.',
      onEnsureRosterCache: vi.fn(() => rosterPreload.promise),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    await screen.findByAltText('Preview page');
    await screen.findByAltText(/Cropped region preview/);
    const confirmRegionsPromise = (
      view.component as { __testConfirmRegions: () => Promise<void> }
    ).__testConfirmRegions();

    expect(await screen.findByText('Background processing — no action needed')).toBeTruthy();
    expect(await screen.findByText('Canvas roster cache is warming up.')).toBeTruthy();

    await view.rerender({
      ...readySharedRosterProps,
      sharedRosterStatus: 'error',
      sharedRosterRows: [],
      sharedRosterMessage: 'Canvas roster failed to load.',
      onEnsureRosterCache: vi.fn(() => rosterPreload.promise),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    expect(await screen.findByText('Canvas roster failed to load.')).toBeTruthy();
    rosterPreload.resolve();
    await confirmRegionsPromise;
  });

  it('surfaces a retryable worker-busy message without disabling confirmation', async () => {
    shellState.set({
      currentProject: null,
      workerStatus: 'busy',
      lastRuntimeError: null,
      debugFeatures: { redactionToggle: false }
    });
    const onFinalizeSubmission = vi.fn();

    const view = render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission
    });

    await advanceToAssociationStep(view.component as { __testConfirmRegions: () => Promise<void> });

    await fireEvent.click(screen.getByRole('checkbox'));
    const confirmButton = screen.getByRole('button', { name: 'Confirm student →' });
    expect(confirmButton.hasAttribute('disabled')).toBe(false);
    await fireEvent.click(confirmButton);
    expect(
      screen.getByText(workerBusyMessage())
    ).toBeTruthy();
    expect(onFinalizeSubmission).not.toHaveBeenCalled();
  });

  it('skips OCR and requires a typed name for non-LMS intake', async () => {
    const view = render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      prerequisitesMet: true,
      lmsCourseId: null,
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    await screen.findByAltText('Preview page');
    await screen.findByAltText(/Cropped region preview/);
    await (view.component as { __testConfirmRegions: () => Promise<void> }).__testConfirmRegions();

    expect(await screen.findByText('2 Manual identity')).toBeTruthy();
    expect(
      await screen.findByText(
        'Manual name entry is ready. Type the student name before generating the canonical PDF.'
      )
    ).toBeTruthy();
    expect(desktopMocks.transientClipPdfRectsPngBase64).toHaveBeenCalled();
    expect(desktopMocks.transientScansOcrHint).not.toHaveBeenCalled();

    const nameInput = screen.getByLabelText('Student name') as HTMLInputElement;
    expect(nameInput.value).toBe('');

    await fireEvent.click(screen.getByRole('checkbox'));
    const confirmButton = screen.getByRole('button', { name: 'Confirm student →' }) as HTMLButtonElement;
    expect(confirmButton.disabled).toBe(true);

    await fireEvent.input(nameInput, { target: { value: 'Local Student' } });
    expect(confirmButton.disabled).toBe(false);
  });

  it('advances to the completion step when finalization succeeds', async () => {
    const finalizedWorkspace = baseWorkspaceState({
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
            examPagePaths: ['/tmp/page_001.png', '/tmp/page_002.png'],
            warnings: [],
            bindingTokenHex: 'token_42'
          }
        ]
      }
    });
    const onFinalizeSubmission = vi.fn().mockResolvedValue(
      finalizeResult(finalizedWorkspace)
    );

    const view = render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission
    });

    await advanceToAssociationStep(view.component as { __testConfirmRegions: () => Promise<void> });

    await fireEvent.click(screen.getByRole('checkbox'));
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm student →' }));

    expect(await screen.findByText('Submission ready')).toBeTruthy();
    expect(screen.getByText('hmac: token_42')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'View exam' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Up' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Down' })).toBeNull();
  });

  it('sends the dragged Step 1 page order to finalization and shows the ordered ingest output', async () => {
    desktopMocks.transientRenderPdfPagePng.mockImplementation(async (_pdfPath: string, pageNumber: number) =>
      previewPage(pageNumber, 2)
    );
    const finalizedWorkspace = baseWorkspaceState({
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
            examPagePaths: ['/tmp/page_002.png', '/tmp/page_001.png'],
            warnings: [],
            bindingTokenHex: 'token_42'
          }
        ]
      }
    });
    const onFinalizeSubmission = vi.fn().mockResolvedValue(finalizeResult(finalizedWorkspace));

    const view = render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission
    });

    await screen.findByAltText('Preview page');
    const pageTwoButton = await screen.findByRole('button', { name: 'Page 2' });
    await waitFor(() => {
      expect((pageTwoButton as HTMLButtonElement).disabled).toBe(false);
    });
    await fireEvent.dragStart(pageTwoButton);
    await fireEvent.dragOver(screen.getByRole('button', { name: 'Page 1' }));
    await fireEvent.drop(screen.getByRole('button', { name: 'Page 1' }));
    await fireEvent.dragEnd(pageTwoButton);

    const pagePills = screen.getAllByRole('button', { name: /Page [12]/ });
    expect(pagePills[0]?.textContent).toContain('Page 2');
    expect(pagePills[1]?.textContent).toContain('Page 1');

    await advanceToAssociationStep(view.component as { __testConfirmRegions: () => Promise<void> });

    await fireEvent.click(screen.getByRole('checkbox'));
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm student →' }));

    await waitFor(() => {
      expect(onFinalizeSubmission).toHaveBeenCalledWith(
        expect.objectContaining({
          desiredPageOrder: [2, 1],
          redactionRegionsPx: [
            {
              pageNumber: 1,
              x: 10,
              y: 10,
              width: 120,
              height: 30
            }
          ],
          rasterSizesByPage: {
            1: {
              widthPx: 600,
              heightPx: 800
            },
            2: {
              widthPx: 600,
              heightPx: 800
            }
          }
        })
      );
    });
    expect(await screen.findByText('Submission ready')).toBeTruthy();
    expect(screen.queryByText('Applied page order')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Up' })).toBeNull();
  });

  it('preserves ordered pages, default redactions, ad-hoc redactions, and raster sizes in finalization', async () => {
    desktopMocks.intakeDefaultPdfRectsFromTemplate.mockResolvedValue([
      {
        pageNumber: 1,
        xPt: 10,
        yPt: 10,
        widthPt: 120,
        heightPt: 30
      },
      {
        pageNumber: 2,
        xPt: 20,
        yPt: 20,
        widthPt: 100,
        heightPt: 24
      }
    ]);
    desktopMocks.transientRenderPdfPagePng.mockImplementation(async (_pdfPath: string, pageNumber: number) =>
      previewPage(pageNumber, 2)
    );
    const finalizedWorkspace = baseWorkspaceState({
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
            examPagePaths: ['/tmp/page_002.png', '/tmp/page_001.png'],
            warnings: [],
            bindingTokenHex: 'token_42'
          }
        ]
      }
    });
    const onFinalizeSubmission = vi.fn().mockResolvedValue(finalizeResult(finalizedWorkspace));

    const view = render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission
    });

    await screen.findByAltText('Preview page');
    const pageTwoButton = await screen.findByRole('button', { name: 'Page 2' });
    await waitFor(() => {
      expect((pageTwoButton as HTMLButtonElement).disabled).toBe(false);
    });
    await fireEvent.dragStart(pageTwoButton);
    await fireEvent.dragOver(screen.getByRole('button', { name: 'Page 1' }));
    await fireEvent.drop(screen.getByRole('button', { name: 'Page 1' }));
    await fireEvent.dragEnd(pageTwoButton);
    await fireEvent.click(pageTwoButton);
    await fireEvent.click(screen.getByRole('button', { name: 'Add Redaction' }));

    await advanceToAssociationStep(view.component as { __testConfirmRegions: () => Promise<void> });
    await fireEvent.click(screen.getByRole('checkbox'));
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm student →' }));

    await waitFor(() => {
      expect(onFinalizeSubmission).toHaveBeenCalledWith(
        expect.objectContaining({
          desiredPageOrder: [2, 1],
          rasterSizesByPage: {
            1: {
              widthPx: 600,
              heightPx: 800
            },
            2: {
              widthPx: 600,
              heightPx: 800
            }
          },
          redactionRegionsPx: [
            {
              pageNumber: 1,
              x: 10,
              y: 10,
              width: 120,
              height: 30
            },
            {
              pageNumber: 2,
              x: 20,
              y: 20,
              width: 100,
              height: 24
            },
            expect.objectContaining({
              pageNumber: 2,
              x: expect.any(Number),
              y: expect.any(Number),
              width: expect.any(Number),
              height: expect.any(Number)
            })
          ]
        })
      );
    });
  });

  it('refreshes and highlights the processed student in the workflow sidebar after completion', async () => {
    const finalizedWorkspace = baseWorkspaceState({
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
            examPagePaths: ['/tmp/page_001.png'],
            warnings: [],
            bindingTokenHex: 'token_42'
          }
        ]
      }
    });
    dialogMocks.open.mockResolvedValue('/tmp/exam.pdf');

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState(),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn().mockResolvedValue(
        finalizeResult(finalizedWorkspace)
      )
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Upload Submission' }));
    await screen.findByAltText('Preview page');
    await screen.findByAltText(/Cropped region preview/);
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm for All Pages' }));
    await waitFor(() => {
      expect(desktopMocks.transientScansOcrHint).toHaveBeenCalled();
      expect(desktopMocks.ensureLmsRosterPreload).toHaveBeenCalled();
    });
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Filter students by workflow status' })).toBeTruthy();
      expect(screen.getByRole('searchbox', { name: 'Search students' })).toBeTruthy();
      expect(screen.getByRole('button', { name: 'Sort students' })).toBeTruthy();
    });

    await fireEvent.click(screen.getByRole('checkbox'));
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm student →' }));

    expect(await screen.findByText('Submission ready')).toBeTruthy();
    const rosterRow = screen.getByRole('button', { name: /Jordan Rivera/i });
    expect(rosterRow.closest('.group')?.className).toContain('bg-workspace-sidebar-active');
  });

  it('surfaces a step-2 error inline and does not advance to student association', async () => {
    desktopMocks.transientScansOcrHint.mockRejectedValue(new Error('Timed out extracting OCR hint.'));

    const view = render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    await screen.findByAltText('Preview page');
    await screen.findByAltText(/Cropped region preview/);
    await (view.component as { __testConfirmRegions: () => Promise<void> }).__testConfirmRegions();

    expect(await screen.findByText('Background processing — no action needed')).toBeTruthy();
    expect(await screen.findByText('Error: Timed out extracting OCR hint.')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Retry background step' })).toBeTruthy();
    expect(screen.queryByRole('combobox')).toBeNull();
    expect(screen.queryByText('Submission ready')).toBeNull();
  });

  it('reruns Step 2 when retrying after an OCR failure', async () => {
    desktopMocks.transientScansOcrHint
      .mockRejectedValueOnce(new Error('Timed out extracting OCR hint.'))
      .mockResolvedValueOnce({
        hintText: 'Jordan Rivera',
        segmentCount: 1
      });

    const view = render(StudentIntakeWorkspace, {
      ...readySharedRosterProps,
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      existingIntakeItems: [],
      seedPaths: ['/tmp/exam.pdf'],
      seedVersion: 1,
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    await screen.findByAltText('Preview page');
    await screen.findByAltText(/Cropped region preview/);
    await (view.component as { __testConfirmRegions: () => Promise<void> }).__testConfirmRegions();

    expect(await screen.findByText('Error: Timed out extracting OCR hint.')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Retry background step' }));

    expect(await screen.findByRole('combobox')).toBeTruthy();
    expect(desktopMocks.transientScansOcrHint).toHaveBeenCalledTimes(2);
  });

  it('queues all PDFs returned by the multi-file intake picker', async () => {
    dialogMocks.open.mockResolvedValue(['/tmp/exam-a.pdf', '/tmp/exam-b.pdf']);

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState(),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Upload Submission' }));

    expect(dialogMocks.open).toHaveBeenCalledWith(
      expect.objectContaining({
        multiple: true,
        title: 'Choose Student PDFs'
      })
    );
    expect(await screen.findByText('Queued PDFs')).toBeTruthy();
    expect(screen.getByText('exam-a.pdf')).toBeTruthy();
    expect(screen.getByText('exam-b.pdf')).toBeTruthy();
    expect(screen.getByText('2 submissions in this intake session')).toBeTruthy();
  });

  it('scrolls the whole intake main panel including the queued PDF summary', async () => {
    dialogMocks.open.mockResolvedValue(['/tmp/exam-a.pdf', '/tmp/exam-b.pdf']);

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState(),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      onFinalizeSubmission: vi.fn()
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Upload Submission' }));

    const header = await screen.findByText('Student Intake Processor');
    const mainPanel = header.closest('section');
    expect(mainPanel?.className).toContain('overflow-y-auto');
    expect(within(mainPanel as HTMLElement).getByText('Queued PDFs')).toBeTruthy();
    expect(within(mainPanel as HTMLElement).getByText('exam-a.pdf')).toBeTruthy();
    expect(within(mainPanel as HTMLElement).getByText('exam-b.pdf')).toBeTruthy();
  });

  it('does not enter intake mode when the file picker is cancelled', async () => {
    dialogMocks.open.mockResolvedValue(null);

    render(StudentWorkflowWorkspace, {
      workspaceState: baseWorkspaceState(),
      prerequisitesMet: true,
      lmsCourseId: 'persisted-course-id',
      busyAction: null,
      onFinalizeSubmission: vi.fn()
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Upload Submission' }));

    expect(dialogMocks.open).toHaveBeenCalledWith(expect.objectContaining({ multiple: true }));
    expect(screen.queryByText('Student Intake Processor')).toBeNull();
    expect(screen.queryByText('Queued PDFs')).toBeNull();
  });
});
