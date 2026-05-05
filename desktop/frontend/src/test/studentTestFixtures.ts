// SPDX-License-Identifier: AGPL-3.0-only
import { defaultAppSettings } from '$lib/stores/appSettings';
import type { AppSettings, ExamWorkspaceState, IntakePreviewPage, PdfPointRect } from '$lib/types';

export function configuredCanvasSettings(): AppSettings {
  return {
    ...defaultAppSettings,
    lmsProvider: 'canvas',
    lmsCanvasBaseUrl: 'https://canvas.example.test',
    lmsCanvasApiKey: 'token'
  };
}

export function previewPage(
  pageNumber = 1,
  pageCount = 1
): IntakePreviewPage {
  return {
    pageNumber,
    pageCount,
    pageWidthPt: 600,
    pageHeightPt: 800,
    pngWidthPx: 600,
    pngHeightPx: 800,
    pngBase64: 'ZmFrZS1wcmV2aWV3'
  };
}

export function defaultRects(): PdfPointRect[] {
  return [
    {
      pageNumber: 1,
      xPt: 10,
      yPt: 10,
      widthPt: 120,
      heightPt: 30
    }
  ];
}

export function baseWorkspaceState(
  overrides: Partial<ExamWorkspaceState> = {}
): ExamWorkspaceState {
  return {
    project: {
      projectId: 'proj_1',
      displayName: 'Midterm 1',
      subject: 'Chemistry',
      courseCode: 'CHEM 201',
      lmsCourseId: 'project-course-id',
      projectPath: '/tmp/project',
      createdAt: '1',
      updatedAt: '1'
    },
    status: 'approved',
    statusLabel: 'Ready for student intake',
    failureMessage: null,
    templatePreviewArtifacts: [],
    questions: [
      {
        questionId: 'question_1',
        questionNumber: 1,
        pageNumber: 1,
        maxPoints: 5,
        text: 'Question text',
        baselinePdfText: 'Question text',
        sourceArtifactId: null,
        analysis: {
          status: 'ok',
          questionTextClean: 'Question text',
          questionContext: '',
          warnings: [],
          latestJobId: 'job_analyze_1'
        }
      }
    ],
    redactionRegions: [
      {
        regionId: 'region_1',
        pageNumber: 1,
        x: 10,
        y: 10,
        width: 120,
        height: 30,
        label: 'student name',
        sortOrder: 0
      }
    ],
    warnings: [],
    canApprove: true,
    canApproveRubric: true,
    projectConfig: {
      projectId: 'proj_1',
      displayName: 'Midterm 1',
      subject: 'Chemistry',
      courseCode: 'CHEM 201',
      lmsCourseId: 'persisted-course-id',
      redactionRequired: true,
      instructorProfile: structuredClone(defaultAppSettings.instructorProfile),
      traceRefs: {
        setupJobId: null,
        batchAnalyzeJobId: null,
        batchRubricJobId: null,
        intakeJobId: null
      },
      createdAt: '1',
      updatedAt: '1'
    },
    studentIntake: {
      status: 'ready',
      latestJobId: null,
      items: [],
      unresolvedCount: 0
    },
    studentRoster: [
      {
        studentRef: 'student_1',
        bindingTokenHex: 'token_42'
      }
    ],
    workflowStage: 'student_intake_ready',
    workflowLabel: 'Ready for student intake',
    ...overrides
  };
}

export function finalizeResult(
  workspaceState: ExamWorkspaceState,
  studentRef = 'student_1',
  bindingTokenHex = 'token_42'
) {
  return {
    workspaceState,
    studentRef,
    bindingTokenHex
  };
}
