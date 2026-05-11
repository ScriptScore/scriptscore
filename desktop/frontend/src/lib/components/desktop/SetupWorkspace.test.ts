// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { appSettings, defaultAppSettings } from '$lib/stores/appSettings';

const desktopMocks = vi.hoisted(() => ({
  listCanvasCourses: vi.fn(),
  listLmsAssignmentsForCourse: vi.fn(),
  toDesktopAssetUrl: vi.fn((path: string) => path)
}));

vi.mock('$lib/desktop', () => desktopMocks);

import SetupWorkspace from './SetupWorkspace.svelte';
import type { ExamWorkspaceState, ProjectConfig } from '$lib/types';

function projectConfig(overrides: Partial<ProjectConfig> = {}): ProjectConfig {
  return {
    projectId: 'proj_setup',
    displayName: 'Midterm 1',
    subject: 'Physics',
    courseCode: 'PHYS 221',
    lmsCourseId: null,
    lmsAssignmentId: null,
    redactionRequired: true,
    instructorProfile: {
      gradingStrictness: 'balanced',
      syntaxLeniency: 'medium',
      ocrTolerance: 'medium',
      partialCreditStyle: 'balanced',
      feedbackStyle: 'brief',
      additionalGuidance: '',
      includeMinimumCreditCriterion: false,
      minimumCreditPercent: 20
    },
    traceRefs: {
      setupJobId: null,
      batchAnalyzeJobId: null,
      batchRubricJobId: null,
      intakeJobId: null
    },
    createdAt: '1',
    updatedAt: '1',
    ...overrides
  };
}

function workspaceState(config: ProjectConfig): ExamWorkspaceState {
  return {
    project: {
      projectId: config.projectId,
      displayName: config.displayName,
      subject: config.subject,
      courseCode: config.courseCode,
      lmsCourseId: config.lmsCourseId,
      projectPath: '/tmp/proj_setup',
      createdAt: config.createdAt,
      updatedAt: config.updatedAt
    },
    status: 'setup',
    statusLabel: 'Setup',
    failureMessage: null,
    templatePreviewArtifacts: [],
    questions: [],
    redactionRegions: [],
    warnings: [],
    canApprove: false,
    canApproveRubric: false,
    projectConfig: config
  };
}

function workspaceStateWithPages(config: ProjectConfig): ExamWorkspaceState {
  return {
    ...workspaceState(config),
    templatePreviewArtifacts: [
      {
        artifactId: 'artifact_page_1',
        pageNumber: 1,
        imagePath: '/tmp/page-1.png',
        label: 'Page 1'
      },
      {
        artifactId: 'artifact_page_2',
        pageNumber: 2,
        imagePath: '/tmp/page-2.png',
        label: 'Page 2'
      }
    ]
  };
}

function setViewportWidth(width: number) {
  Object.defineProperty(window, 'innerWidth', {
    configurable: true,
    writable: true,
    value: width
  });
  window.dispatchEvent(new Event('resize'));
}

describe('SetupWorkspace', () => {
  beforeEach(() => {
    setViewportWidth(1440);
    desktopMocks.listCanvasCourses.mockReset();
    desktopMocks.listLmsAssignmentsForCourse.mockReset();
    desktopMocks.toDesktopAssetUrl.mockImplementation((path: string) => path);
    appSettings.save(defaultAppSettings);
  });

  it('hides LMS course and assignment controls when LMS is disabled', () => {
    appSettings.save({
      ...defaultAppSettings,
      lmsProvider: 'none',
      lmsCanvasBaseUrl: '',
      lmsCanvasApiKey: null
    });
    const config = projectConfig({
      lmsCourseId: 'stale-course-id',
      lmsAssignmentId: 'stale-assignment-id'
    });

    render(SetupWorkspace, {
      workspaceState: workspaceState(config),
      projectConfig: config
    });

    expect(screen.queryByRole('combobox', { name: 'Course' })).toBeNull();
    expect(screen.queryByRole('combobox', { name: 'Assignment' })).toBeNull();
    const courseCode = screen.getByLabelText('Course Code');
    expect(courseCode).toBeTruthy();
    expect(courseCode.className).toContain('bg-workspace-empty');
    expect(desktopMocks.listCanvasCourses).not.toHaveBeenCalled();
    expect(desktopMocks.listLmsAssignmentsForCourse).not.toHaveBeenCalled();
  });

  it('uses compact page buttons without sidebar thumbnails on constrained viewports', async () => {
    setViewportWidth(1100);
    desktopMocks.listCanvasCourses.mockResolvedValue([
      { lmsCourseId: 'course_1', name: 'Physics 221', courseCode: 'PHYS 221' }
    ]);
    desktopMocks.listLmsAssignmentsForCourse.mockResolvedValue([
      { assignmentId: 'assignment_1', name: 'Midterm 1', pointsPossible: 100 }
    ]);
    appSettings.save({
      ...defaultAppSettings,
      lmsProvider: 'canvas',
      lmsCanvasBaseUrl: 'https://canvas.example.test',
      lmsCanvasApiKey: 'canvas-token'
    });
    const config = projectConfig({ lmsCourseId: 'course_1' });
    const onSelectPage = vi.fn();

    render(SetupWorkspace, {
      workspaceState: workspaceStateWithPages(config),
      projectConfig: config,
      selectedPageNumber: 1,
      onSelectPage
    });

    expect(screen.queryByAltText('Page 1')).toBeNull();
    expect(screen.getByAltText('Template page 1')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Select Page 1' }).getAttribute('aria-current')).toBe(
      'page'
    );

    await fireEvent.click(screen.getByRole('button', { name: 'Select Page 2' }));
    expect(onSelectPage).toHaveBeenCalledWith(2);

    expect(screen.getByLabelText('Exam Name')).toBeTruthy();
    expect(await screen.findByRole('combobox', { name: 'Course' })).toBeTruthy();
    expect(await screen.findByRole('combobox', { name: 'Assignment' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Discard' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Save' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Continue' })).toBeTruthy();
  });

  it('keeps thumbnail page selection on wide viewports', async () => {
    setViewportWidth(1440);
    const config = projectConfig();
    const onSelectPage = vi.fn();

    render(SetupWorkspace, {
      workspaceState: workspaceStateWithPages(config),
      projectConfig: config,
      selectedPageNumber: 2,
      onSelectPage
    });

    expect(screen.getByAltText('Page 1')).toBeTruthy();
    expect(screen.getByAltText('Page 2')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Select Page 2' }).getAttribute('aria-current')).toBe(
      'page'
    );

    await fireEvent.click(screen.getByRole('button', { name: 'Select Page 1' }));
    expect(onSelectPage).toHaveBeenCalledWith(1);
  });

  it('moves redaction guidance into the no-regions popover', async () => {
    const config = projectConfig();

    render(SetupWorkspace, {
      workspaceState: workspaceStateWithPages(config),
      projectConfig: config
    });

    expect(
      screen.queryByText(/Review exam details and use your mouse to draw rectangular redaction regions/)
    ).toBeNull();
    const noRegionsTrigger = screen.getByRole('button', { name: 'No Privacy Regions Created' });
    expect(noRegionsTrigger.querySelector('svg')).toBeTruthy();
    expect(noRegionsTrigger.getAttribute('title')).toBe(
      'No privacy regions have been created. Click to learn how redaction regions protect copied student submissions.'
    );

    await fireEvent.click(noRegionsTrigger);

    expect(
      screen.getByText(/Review exam details and use your mouse to draw rectangular redaction regions/)
    ).toBeTruthy();
    expect(screen.getByAltText('Exam page with red boxes marking name and privacy regions')).toBeTruthy();

    const continueButton = screen.getByRole('button', { name: 'Continue' });
    expect(continueButton.getAttribute('title')).toBe(
      'Create at least one privacy region before continuing.'
    );
  });

  it('offers alignment stamp export from the compact no-marks popover', async () => {
    const config = projectConfig();
    let finishExport!: () => void;
    const exportPromise = new Promise<void>((resolve) => {
      finishExport = resolve;
    });
    const onExportTemplatePdf = vi.fn(() => exportPromise);

    render(SetupWorkspace, {
      workspaceState: {
        ...workspaceStateWithPages(config),
        arucoStatus: { state: 'not_detected', totalMarkerCount: 0, pages: [] }
      },
      projectConfig: config,
      onExportTemplatePdf
    });

    expect(screen.queryByText('Print alignment')).toBeNull();
    const noMarksTrigger = screen.getByRole('button', { name: 'No Alignment Marks' });
    expect(noMarksTrigger.querySelector('svg')).toBeTruthy();
    expect(noMarksTrigger.getAttribute('title')).toBe(
      'No alignment marks were detected. Click to learn how alignment marks improve scanned-page matching.'
    );
    await fireEvent.click(noMarksTrigger);

    expect(screen.getByText('Add alignment stamps to this template?')).toBeTruthy();
    expect(screen.getByAltText('Exam page with a square alignment mark in one corner')).toBeTruthy();
    const action = screen.getByRole('button', { name: 'Add Alignment Stamps & Export PDF' });
    expect(action.hasAttribute('disabled')).toBe(false);
    await fireEvent.click(action);
    expect(onExportTemplatePdf).toHaveBeenCalledTimes(1);
    expect(await screen.findByText('Creating Alignment Marks...')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'No Alignment Marks' })).toBeNull();

    finishExport();
    expect(await screen.findByRole('button', { name: 'No Alignment Marks' })).toBeTruthy();
  });

  it('shows compact export affordance when alignment marks already exist', async () => {
    const config = projectConfig();
    const onExportTemplatePdf = vi.fn();

    render(SetupWorkspace, {
      workspaceState: {
        ...workspaceStateWithPages(config),
        arucoStatus: {
          state: 'detected',
          totalMarkerCount: 4,
          pages: [{ pageNumber: 1, markerCount: 4, markerIds: [0, 1, 2, 3] }]
        }
      },
      projectConfig: config,
      onExportTemplatePdf
    });

    expect(screen.getByText('Has Alignment Marks')).toBeTruthy();
    expect(screen.queryByText(/ArUco markers detected/)).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Export template PDF' }));
    expect(onExportTemplatePdf).toHaveBeenCalledTimes(1);
  });

  it('shows transient creation copy while alignment stamps are being generated', () => {
    const config = projectConfig();

    render(SetupWorkspace, {
      workspaceState: {
        ...workspaceStateWithPages(config),
        arucoStatus: { state: 'not_detected', totalMarkerCount: 0, pages: [] }
      },
      projectConfig: config,
      alignmentMarksPending: true
    });

    expect(screen.getByText('Creating Alignment Marks...')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'No Alignment Marks' })).toBeNull();
  });

  it('renders the non-blank answer minimum disabled state clearly', () => {
    const config = projectConfig();

    render(SetupWorkspace, {
      workspaceState: workspaceState(config),
      projectConfig: config
    });

    const switchControl = screen.getByRole('switch', {
      name: 'Award minimum points for non-blank answers'
    });
    expect(switchControl.getAttribute('aria-checked')).toBe('false');
    expect(screen.getByText('Non-blank answer minimum')).toBeTruthy();
    expect(screen.getByText('Award minimum points for non-blank answers')).toBeTruthy();
    expect(
      screen.getByText('Off — no minimum is guaranteed and no rubric criterion will be added.')
    ).toBeTruthy();
    expect(screen.queryByText('Non-blank answers may receive zero points.')).toBeNull();
    expect(
      screen.queryByLabelText('Minimum percentage of question points for non-blank answers')
    ).toBeNull();
  });

  it('edits and validates the non-blank answer minimum percentage inline', async () => {
    const config = projectConfig();

    render(SetupWorkspace, {
      workspaceState: workspaceState(config),
      projectConfig: config
    });

    await fireEvent.click(screen.getByRole('switch', { name: 'Award minimum points for non-blank answers' }));

    expect(config.instructorProfile.includeMinimumCreditCriterion).toBe(true);
    expect(screen.getByText('Award at least')).toBeTruthy();
    expect(screen.getByText('of question points for any non-blank answer')).toBeTruthy();
    expect(
      screen.getByText('Automatically adds a rubric criterion for non-blank attempts.')
    ).toBeTruthy();
    expect(
      screen.queryByText("The minimum is calculated as a percentage of this question's total point value.")
    ).toBeNull();

    const input = screen.getByLabelText(
      'Minimum percentage of question points for non-blank answers'
    ) as HTMLInputElement;
    expect(input.value).toBe('20');

    await fireEvent.input(input, { target: { value: '42' } });
    expect(config.instructorProfile.minimumCreditPercent).toBe(42);

    await fireEvent.input(input, { target: { value: '-1' } });
    expect(config.instructorProfile.minimumCreditPercent).toBe(42);
    expect(input.value).toBe('42');

    await fireEvent.input(input, { target: { value: '0' } });
    expect(config.instructorProfile.minimumCreditPercent).toBe(0);
  });
});
