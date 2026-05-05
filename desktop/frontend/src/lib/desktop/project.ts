// SPDX-License-Identifier: AGPL-3.0-only
import type {
  AppSettings,
  CreateProjectInput,
  ExamWorkspaceState,
  LlmModelValidation,
  ProjectConfig,
  QuestionEdit,
  RubricUpdateInput,
  ShellState,
  SmokePingResult,
  TemplateRedactionRegionInput,
  VisionCapableModel
} from '$lib/types';

import {
  browserShellState,
  currentDesktopSettings,
  invokeDesktopHost,
  invokeDesktopHostOrDefault
} from './shared';

export function getShellState(): Promise<ShellState> {
  return invokeDesktopHostOrDefault(browserShellState, 'get_shell_state');
}

export function createProject(input: CreateProjectInput, settings: AppSettings): Promise<string> {
  return invokeDesktopHost<string>('create_project', { input, settings });
}

export function openProject(
  projectPath: string,
  settings: AppSettings = currentDesktopSettings()
): Promise<ShellState> {
  return invokeDesktopHost<ShellState>('open_project', { projectPath, settings });
}

export function getDefaultProjectsRoot(): Promise<string> {
  return invokeDesktopHostOrDefault('', 'get_default_projects_root');
}

export function projectExists(projectPath: string): Promise<boolean> {
  return invokeDesktopHostOrDefault(false, 'project_exists', { projectPath });
}

export function closeCurrentProject(): Promise<ShellState> {
  return invokeDesktopHost<ShellState>('close_current_project');
}

export function runSmokePing(): Promise<SmokePingResult> {
  return invokeDesktopHost<SmokePingResult>('run_smoke_ping');
}

export function listVisionModels(
  providerName: string,
  baseUrl: string,
  apiKey?: string | null
): Promise<VisionCapableModel[]> {
  return invokeDesktopHost<VisionCapableModel[]>('list_llm_models', {
    providerName,
    baseUrl,
    apiKey: apiKey ?? null
  });
}

export function validateVisionModel(
  providerName: string,
  baseUrl: string,
  model: string,
  apiKey?: string | null
): Promise<LlmModelValidation> {
  return invokeDesktopHost<LlmModelValidation>('validate_llm_model', {
    providerName,
    baseUrl,
    model,
    apiKey: apiKey ?? null
  });
}

export function getExamWorkspaceState(): Promise<ExamWorkspaceState> {
  return invokeDesktopHost<ExamWorkspaceState>('get_exam_workspace_state');
}

export function saveQuestionEdits(edits: QuestionEdit[]): Promise<ExamWorkspaceState> {
  return invokeDesktopHost<ExamWorkspaceState>('save_question_edits', { edits });
}

export function saveRedactionRegions(
  regions: TemplateRedactionRegionInput[]
): Promise<ExamWorkspaceState> {
  return invokeDesktopHost<ExamWorkspaceState>('save_redaction_regions', { regions });
}

export function approveTemplateSetup(settings: AppSettings): Promise<ExamWorkspaceState> {
  return invokeDesktopHost<ExamWorkspaceState>('approve_template_setup', { settings });
}

export function ensureAutomaticRubricJobs(settings: AppSettings): Promise<void> {
  return invokeDesktopHost<void>('ensure_automatic_rubric_jobs', { settings });
}

export function saveProjectConfig(
  config: ProjectConfig,
  settings: AppSettings = currentDesktopSettings()
): Promise<ExamWorkspaceState> {
  return invokeDesktopHost<ExamWorkspaceState>('save_project_config', { config, settings });
}

export function skipTemplateRedaction(): Promise<ExamWorkspaceState> {
  return invokeDesktopHost<ExamWorkspaceState>('skip_template_redaction');
}

export function generateQuestionRubric(
  questionId: string,
  replaceExisting: boolean,
  settings: AppSettings
): Promise<string> {
  return invokeDesktopHost<string>('generate_question_rubric', {
    questionId,
    replaceExisting,
    settings
  });
}

export function reanalyzeQuestion(questionId: string, settings: AppSettings): Promise<string> {
  return invokeDesktopHost<string>('reanalyze_question', {
    questionId,
    settings
  });
}

export function saveRubricUpdate(input: RubricUpdateInput): Promise<ExamWorkspaceState> {
  return invokeDesktopHost<ExamWorkspaceState>('save_rubric_update', { input });
}

export function replaceTemplatePdf(templatePdfPath: string): Promise<string> {
  return invokeDesktopHost<string>('replace_template_pdf', { templatePdfPath });
}

export function exportStampedTemplatePdf(destinationPath: string): Promise<string> {
  return invokeDesktopHost<string>('export_stamped_template_pdf', { destinationPath });
}
