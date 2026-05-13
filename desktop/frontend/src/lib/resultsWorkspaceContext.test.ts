// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  currentResultsAssignmentContextKey,
  resolveStudentDisplayNamesForWorkspace,
  resetStudentDisplayNameCacheForTests
} from './resultsWorkspaceContext';
import type { ExamWorkspaceState, LmsRosterCacheSnapshot } from '$lib/types';

function workspace(): ExamWorkspaceState {
  return {
    project: {
      projectId: 'project_1',
      displayName: 'Midterm 1',
      subject: 'Physics',
      courseCode: 'PHYS 221',
      lmsCourseId: 'course_1',
      projectPath: '/tmp/project_1',
      createdAt: '1',
      updatedAt: '1'
    },
    projectConfig: {
      projectId: 'project_1',
      displayName: 'Midterm 1',
      subject: 'Physics',
      courseCode: 'PHYS 221',
      lmsCourseId: 'course_1',
      lmsAssignmentId: null,
      redactionRequired: true,
      instructorProfile: {
        gradingStrictness: 'balanced',
        syntaxLeniency: 'medium',
        ocrTolerance: 'medium',
        partialCreditStyle: 'balanced',
        feedbackStyle: 'brief',
        enabledTags: {
          gradingStrictness: true,
          syntaxLeniency: false,
          ocrTolerance: false,
          partialCreditStyle: false,
          feedbackStyle: true
        },
        additionalGuidance: '',
        includeMinimumCreditCriterion: false,
        minimumCreditPercent: 10
      },
      traceRefs: {
        setupJobId: null,
        batchAnalyzeJobId: null,
        batchRubricJobId: null,
        intakeJobId: null
      },
      createdAt: '1',
      updatedAt: '1'
    },
    studentRoster: [
      { studentRef: 'student_1', bindingTokenHex: 'token_1' },
      { studentRef: 'student_2', bindingTokenHex: 'token_2' }
    ]
  } as unknown as ExamWorkspaceState;
}

function readySnapshot(overrides: Partial<LmsRosterCacheSnapshot> = {}): LmsRosterCacheSnapshot {
  return {
    status: 'ready',
    projectPath: '/tmp/project_1',
    lmsProvider: 'canvas',
    courseId: 'course_1',
    rows: [
      { userId: 'user_1', displayName: 'Ada Lovelace', sortKey: 'lovelace' },
      { userId: 'user_2', displayName: 'Grace Hopper', sortKey: 'hopper' }
    ],
    lastError: null,
    idleReason: null,
    ...overrides
  };
}

describe('resultsWorkspaceContext', () => {
  beforeEach(() => {
    resetStudentDisplayNameCacheForTests();
  });

  it('builds the assignment loader context key from the project, LMS settings, and course', () => {
    expect(
      currentResultsAssignmentContextKey({
        hasDesktopHost: true,
        currentProjectPath: '/tmp/project_1',
        settings: {
          lmsProvider: 'canvas',
          lmsCanvasBaseUrl: 'https://canvas.example'
        },
        workspace: workspace()
      })
    ).toBe('/tmp/project_1::canvas::https://canvas.example::course_1');
  });

  it('does not build an assignment loader context key for local-only projects', () => {
    const local = workspace();
    local.project.lmsCourseId = null;
    local.projectConfig!.lmsCourseId = null;

    expect(
      currentResultsAssignmentContextKey({
        hasDesktopHost: true,
        currentProjectPath: '/tmp/project_1',
        settings: {
          lmsProvider: 'canvas',
          lmsCanvasBaseUrl: 'https://canvas.example'
        },
        workspace: local
      })
    ).toBeNull();
  });

  it('uses persisted local display names for local-only projects', async () => {
    const local = workspace();
    local.project.lmsCourseId = null;
    local.projectConfig!.lmsCourseId = null;
    local.studentRoster = [];
    local.studentIntake = {
      status: 'ready',
      latestJobId: null,
      unresolvedCount: 0,
      items: [
        {
          studentRef: 'student_1',
          localDisplayName: 'Ada Local',
          canonicalPdfPath: '/tmp/student_1.pdf',
          ingestStatus: 'ready',
          pageCount: 1,
          warnings: []
        }
      ]
    };
    const getLmsRosterCacheState = vi.fn();
    const computeLmsBindingToken = vi.fn();

    await expect(
      resolveStudentDisplayNamesForWorkspace(local, {
        getLmsRosterCacheState,
        computeLmsBindingToken
      })
    ).resolves.toEqual({ student_1: 'Ada Local' });
    expect(getLmsRosterCacheState).not.toHaveBeenCalled();
    expect(computeLmsBindingToken).not.toHaveBeenCalled();
  });

  it('caches roster-token lookups across repeated workspace refreshes for the same snapshot', async () => {
    const snapshot = readySnapshot();
    const getLmsRosterCacheState = vi.fn(async () => snapshot);
    const computeLmsBindingToken = vi.fn(async (_courseId: string, userId: string) =>
      userId === 'user_1' ? 'token_1' : 'token_2'
    );

    const first = await resolveStudentDisplayNamesForWorkspace(workspace(), {
      getLmsRosterCacheState,
      computeLmsBindingToken
    });
    const second = await resolveStudentDisplayNamesForWorkspace(workspace(), {
      getLmsRosterCacheState,
      computeLmsBindingToken
    });

    expect(first).toEqual({
      student_1: 'Ada Lovelace',
      student_2: 'Grace Hopper'
    });
    expect(second).toEqual(first);
    expect(computeLmsBindingToken).toHaveBeenCalledTimes(2);
  });

  it('returns no display names when the roster cache does not match the open project context', async () => {
    const getLmsRosterCacheState = vi.fn(async () =>
      readySnapshot({
        projectPath: '/tmp/other-project'
      })
    );
    const computeLmsBindingToken = vi.fn();

    const resolved = await resolveStudentDisplayNamesForWorkspace(workspace(), {
      getLmsRosterCacheState,
      computeLmsBindingToken
    });

    expect(resolved).toEqual({});
    expect(computeLmsBindingToken).not.toHaveBeenCalled();
  });
});
