// SPDX-License-Identifier: AGPL-3.0-only
import type {
  AppSettings,
  DeleteStudentSubmissionInput,
  ExamWorkspaceState,
  IntakePreviewPage,
  PdfPointRect,
  SaveCriterionScoreInput,
  SaveModeratedFeedbackInput,
  SaveModeratedScoreInput,
  ScansOcrHintResult,
  SetModerationQuestionReviewedInput,
  StudentIntakeInput,
  StudentWorkflowAlignmentPage,
  StudentWorkflowDetectReviewResolutionInput
} from '$lib/types';

import { sanitizeStudentIntakeInputsForHost } from '$lib/studentIntakeHostPayload';

import { invokeDesktopHost } from './shared';

export function transientPdfClipText(
  pdfPath: string,
  pageNumber: number,
  xPt: number,
  yPt: number,
  widthPt: number,
  heightPt: number
): Promise<string> {
  return invokeDesktopHost<string>('transient_pdf_clip_text', {
    input: {
      pdfPath,
      pageNumber,
      xPt,
      yPt,
      widthPt,
      heightPt
    }
  });
}

export function transientRenderPdfPagePng(
  pdfPath: string,
  pageNumber: number,
  zoom: number,
  maxPreviewWidthPx?: number
): Promise<IntakePreviewPage> {
  return invokeDesktopHost<IntakePreviewPage>('transient_render_pdf_page_png', {
    pdfPath,
    pageNumber,
    zoom,
    ...(maxPreviewWidthPx === undefined ? {} : { maxWidthPx: maxPreviewWidthPx })
  });
}

export function transientClipPdfRectsPngBase64(
  pdfPath: string,
  rects: PdfPointRect[],
  zoom: number
): Promise<string[]> {
  return invokeDesktopHost<string[]>('transient_clip_pdf_rects_png_base64', {
    pdfPath,
    rects,
    zoom
  });
}

export function intakeDefaultPdfRectsFromTemplate(pdfPath: string): Promise<PdfPointRect[]> {
  return invokeDesktopHost<PdfPointRect[]>('intake_default_pdf_rects_from_template', {
    pdfPath
  });
}

export function transientScansOcrHint(pngBytesBase64: string): Promise<ScansOcrHintResult> {
  return invokeDesktopHost<ScansOcrHintResult>('transient_scans_ocr_hint', {
    pngBytesBase64
  });
}

export function beginStudentWorkflow(settings: AppSettings): Promise<string> {
  return invokeDesktopHost<string>('begin_student_workflow', { settings });
}

export function confirmStudentAlignment(
  studentRef: string,
  pages: StudentWorkflowAlignmentPage[],
  settings: AppSettings
): Promise<string> {
  return invokeDesktopHost<string>('confirm_student_alignment', {
    input: { studentRef, pages },
    settings
  });
}

export function saveStudentAlignmentReview(
  studentRef: string,
  pages: StudentWorkflowAlignmentPage[]
): Promise<ExamWorkspaceState> {
  return invokeDesktopHost<ExamWorkspaceState>('save_student_alignment_review', {
    input: { studentRef, pages }
  });
}

export function confirmStudentParseReview(
  studentRef: string,
  questionId: string,
  correctedText: string,
  settings: AppSettings
): Promise<string> {
  return invokeDesktopHost<string>('confirm_student_parse_review', {
    input: { studentRef, questionId, correctedText },
    settings
  });
}

export function saveStudentParseReview(
  studentRef: string,
  questionId: string,
  correctedText: string
): Promise<ExamWorkspaceState> {
  return invokeDesktopHost<ExamWorkspaceState>('save_student_parse_review', {
    input: { studentRef, questionId, correctedText }
  });
}

export function confirmStudentDetectReview(
  studentRef: string,
  resolutions: StudentWorkflowDetectReviewResolutionInput[],
  settings: AppSettings
): Promise<string> {
  return invokeDesktopHost<string>('confirm_student_detect_review', {
    input: { studentRef, resolutions },
    settings
  });
}

export function saveStudentDetectReview(
  studentRef: string,
  resolutions: StudentWorkflowDetectReviewResolutionInput[]
): Promise<ExamWorkspaceState> {
  return invokeDesktopHost<ExamWorkspaceState>('save_student_detect_review', {
    input: { studentRef, resolutions }
  });
}

export function runStudentIntake(inputs: StudentIntakeInput[]): Promise<string> {
  return invokeDesktopHost<string>('run_student_intake', {
    inputs: sanitizeStudentIntakeInputsForHost(inputs)
  });
}

export function saveStudentIntakePageOrder(
  studentRef: string,
  examPagePaths: string[]
): Promise<ExamWorkspaceState> {
  return invokeDesktopHost<ExamWorkspaceState>('save_student_intake_page_order', {
    input: { studentRef, examPagePaths }
  });
}

export function deleteStudentSubmission(
  input: DeleteStudentSubmissionInput
): Promise<ExamWorkspaceState> {
  return invokeDesktopHost<ExamWorkspaceState>('delete_student_submission', { input });
}

export function saveCriterionScore(
  input: SaveCriterionScoreInput
): Promise<ExamWorkspaceState> {
  return invokeDesktopHost<ExamWorkspaceState>('save_criterion_score', { input });
}

export function saveModeratedScore(
  input: SaveModeratedScoreInput
): Promise<ExamWorkspaceState> {
  return invokeDesktopHost<ExamWorkspaceState>('save_moderated_score', { input });
}

export function saveModeratedFeedback(
  input: SaveModeratedFeedbackInput
): Promise<ExamWorkspaceState> {
  return invokeDesktopHost<ExamWorkspaceState>('save_moderated_feedback', { input });
}

export function setModerationQuestionReviewed(
  input: SetModerationQuestionReviewedInput
): Promise<ExamWorkspaceState> {
  return invokeDesktopHost<ExamWorkspaceState>('set_moderation_question_reviewed', { input });
}
