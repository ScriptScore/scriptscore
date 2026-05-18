// SPDX-License-Identifier: AGPL-3.0-only
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use base64::Engine;

use crate::models::{
    AppSettings, CreateProjectInput, DeleteStudentSubmissionInput, ExamWorkspaceState,
    JobTraceState, JobTraceSummary, LegalDisclosure, LlmModelValidation, LmsAssignmentSummary,
    LmsRosterCacheSnapshot, ProjectConfig, QuestionEdit, ResultsExportResponse,
    ResultsLmsReportPreview, ResultsLmsUploadResponse, RetryResultsLmsUploadInput,
    RubricUpdateInput, RunResultsExportInput, RunResultsLmsUploadInput, SaveCriterionScoreInput,
    SaveModeratedFeedbackInput, SaveModeratedScoreInput, SaveResultsLmsAssignmentInput,
    SetModerationQuestionReviewedInput, SetSubmissionResultFinalizedInput, ShellState,
    SmokePingResult, StudentIntakeInput, TemplateRedactionRegionInput, VisionCapableModel,
};
use crate::project_store;
use crate::state::{
    student_workflow::{
        StudentWorkflowAlignmentUpdateInput, StudentWorkflowDetectReviewInput,
        StudentWorkflowParseReviewInput,
    },
    workspace_actions::PdfTextClipInput,
    AppHandleRuntimeEventSink, AppState,
};

// Shell lifecycle and local-state commands complete synchronously in the host.
#[tauri::command]
pub fn get_shell_state(state: State<'_, AppState>, app: AppHandle) -> Result<ShellState, String> {
    let shell = state.shell_state().map_err(|err| err.to_string())?;
    sync_main_window_title(&app, &shell);
    Ok(shell)
}

#[tauri::command]
pub fn create_project(
    input: CreateProjectInput,
    settings: AppSettings,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    let event_sink = Arc::new(AppHandleRuntimeEventSink::new(app.clone()));
    state
        .start_create_project_job(&input, settings, event_sink)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn open_project(
    project_path: String,
    settings: AppSettings,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ShellState, String> {
    let shell = state
        .open_project(PathBuf::from(project_path), &settings)
        .map_err(|err| err.to_string())?;
    sync_main_window_title(&app, &shell);
    Ok(shell)
}

#[tauri::command]
pub fn get_default_projects_root() -> Result<String, String> {
    project_store::default_projects_root()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn project_exists(project_path: String) -> Result<bool, String> {
    let path = PathBuf::from(&project_path);
    Ok(path.is_dir() && path.join("scriptscore.db").is_file())
}

#[tauri::command]
pub fn get_legal_disclosure(app: AppHandle) -> Result<LegalDisclosure, String> {
    let resource_dir = app.path().resource_dir().map_err(|err| err.to_string())?;
    let legal_dir = resource_dir.join("legal");
    let notices_path = legal_dir.join("THIRD_PARTY_NOTICES.md");
    let report_path = legal_dir.join("license-policy-report.json");
    let third_party_notices = std::fs::read_to_string(&notices_path)
        .map_err(|err| format!("Could not read third-party notices: {err}"))?;
    let policy_report_json = std::fs::read_to_string(&report_path)
        .map_err(|err| format!("Could not read license policy report: {err}"))?;
    let source_url = serde_json::from_str::<Value>(&policy_report_json)
        .ok()
        .and_then(|report| {
            report
                .get("sourceOffer")
                .and_then(|offer| offer.get("url"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "https://github.com/ScriptScore/scriptscore".to_string());
    Ok(LegalDisclosure {
        license_expression: "AGPL-3.0-only".into(),
        source_url,
        local_notices_path: "legal/THIRD_PARTY_NOTICES.md".into(),
        third_party_notices,
        policy_report_json,
        artifact_status: "bundled".into(),
    })
}

#[tauri::command]
pub fn close_current_project(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ShellState, String> {
    let shell = state
        .close_current_project()
        .map_err(|err| err.to_string())?;
    sync_main_window_title(&app, &shell);
    Ok(shell)
}

// Short CLI-backed runtime commands still run off the UI thread, but return their
// result directly because the caller consumes that result immediately.
#[tauri::command]
pub async fn run_smoke_ping(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<SmokePingResult, String> {
    let state_inner = state.clone_inner();
    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let event_sink = AppHandleRuntimeEventSink::new(app_clone);
        state_inner.run_smoke_ping(&event_sink)
    })
    .await
    .map_err(|err| format!("Task panicked: {err}"))?
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn list_llm_models(
    provider_name: String,
    base_url: String,
    api_key: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<VisionCapableModel>, String> {
    let state_inner = state.clone_inner();
    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let event_sink = AppHandleRuntimeEventSink::new(app_clone);
        state_inner.list_llm_models(&event_sink, provider_name, base_url, api_key)
    })
    .await
    .map_err(|err| format!("Task panicked: {err}"))?
    .map_err(runtime_settings_error_message)
}

fn runtime_settings_error_message(err: crate::errors::HostError) -> String {
    let message = err.to_string();
    if message == "A desktop job is already active in this session." {
        "The desktop runtime is busy with another job. Wait for it to finish, then try the Ollama connection again.".into()
    } else {
        message
    }
}

#[tauri::command]
pub async fn validate_llm_model(
    provider_name: String,
    base_url: String,
    model: String,
    api_key: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<LlmModelValidation, String> {
    let state_inner = state.clone_inner();
    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let event_sink = AppHandleRuntimeEventSink::new(app_clone);
        state_inner.validate_llm_model(&event_sink, provider_name, base_url, model, api_key)
    })
    .await
    .map_err(|err| format!("Task panicked: {err}"))?
    .map_err(runtime_settings_error_message)
}

#[tauri::command]
pub fn cancel_active_job(
    job_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<ShellState, String> {
    state
        .cancel_active_job(job_id)
        .map_err(|err| err.to_string())
}

// Generic passthrough for callers that already understand the runtime job/event model.
#[tauri::command]
pub fn start_job(
    command_name: String,
    request_payload: Value,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    let state_inner = state.clone_inner();
    let app_clone = app.clone();
    let command_name_clone = command_name.clone();
    let event_sink = AppHandleRuntimeEventSink::new(app.clone());
    let reserved = state_inner
        .start_job(&command_name, &event_sink)
        .map_err(|err| err.to_string())?;
    let job_id = reserved.job_id.clone();
    tauri::async_runtime::spawn(async move {
        let _ = state_inner.run_reserved_job(
            reserved,
            command_name_clone,
            request_payload,
            &AppHandleRuntimeEventSink::new(app_clone),
        );
    });
    Ok(job_id)
}

// Project.db edits and workspace reads remain synchronous local host operations.
#[tauri::command]
pub fn get_exam_workspace_state(state: State<'_, AppState>) -> Result<ExamWorkspaceState, String> {
    state.workspace_state().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn recover_interrupted_student_workflow(
    state: State<'_, AppState>,
) -> Result<ExamWorkspaceState, String> {
    state
        .recover_interrupted_student_workflow()
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn save_question_edits(
    edits: Vec<QuestionEdit>,
    state: State<'_, AppState>,
) -> Result<ExamWorkspaceState, String> {
    state
        .save_question_edits(edits)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn save_redaction_regions(
    regions: Vec<TemplateRedactionRegionInput>,
    state: State<'_, AppState>,
) -> Result<ExamWorkspaceState, String> {
    state
        .save_redaction_regions(regions)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn approve_template_setup(
    settings: AppSettings,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ExamWorkspaceState, String> {
    let workspace = state
        .approve_template_setup(&settings)
        .map_err(|err| err.to_string())?;
    let event_sink = AppHandleRuntimeEventSink::new(app);
    state
        .ensure_automatic_rubric_jobs(settings, &event_sink)
        .map_err(|err| err.to_string())?;
    Ok(workspace)
}

/// Runs enqueue work off the UI thread so navigating to Review does not block the webview.
#[tauri::command]
pub async fn ensure_automatic_rubric_jobs(
    settings: AppSettings,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let app_state = AppState::from_inner(state.clone_inner());
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let event_sink = AppHandleRuntimeEventSink::new(app_handle);
        app_state.ensure_automatic_rubric_jobs(settings, &event_sink)
    })
    .await
    .map_err(|err| format!("Blocking task join error: {err}"))?
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn save_project_config(
    config: ProjectConfig,
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<ExamWorkspaceState, String> {
    state
        .save_project_config(config, &settings)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn skip_template_redaction(state: State<'_, AppState>) -> Result<ExamWorkspaceState, String> {
    state
        .skip_template_redaction()
        .map_err(|err| err.to_string())
}

// Long-running CLI-backed workspace mutations always return a background job id so
// the frontend stays responsive and refreshes from terminal runtime events.
#[tauri::command]
pub fn generate_question_rubric(
    question_id: String,
    replace_existing: bool,
    settings: AppSettings,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    let event_sink = Arc::new(AppHandleRuntimeEventSink::new(app));
    state
        .start_generate_question_rubric_job(question_id, replace_existing, settings, event_sink)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn reanalyze_question(
    question_id: String,
    settings: AppSettings,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    let event_sink = Arc::new(AppHandleRuntimeEventSink::new(app));
    state
        .start_reanalyze_question_job(question_id, settings, event_sink)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn save_rubric_update(
    input: RubricUpdateInput,
    state: State<'_, AppState>,
) -> Result<ExamWorkspaceState, String> {
    state
        .save_rubric_update(input)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn save_criterion_score(
    input: SaveCriterionScoreInput,
    state: State<'_, AppState>,
) -> Result<ExamWorkspaceState, String> {
    state
        .save_criterion_score(input)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn save_moderated_score(
    input: SaveModeratedScoreInput,
    state: State<'_, AppState>,
) -> Result<ExamWorkspaceState, String> {
    state
        .save_moderated_score(input)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn save_moderated_feedback(
    input: SaveModeratedFeedbackInput,
    state: State<'_, AppState>,
) -> Result<ExamWorkspaceState, String> {
    state
        .save_moderated_feedback(input)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn set_moderation_question_reviewed(
    input: SetModerationQuestionReviewedInput,
    state: State<'_, AppState>,
) -> Result<ExamWorkspaceState, String> {
    state
        .set_moderation_question_reviewed(input)
        .map_err(|err| err.to_string())
}

/// Background worker job: returns job id immediately; completion is delivered via `job_finished`
/// (same pattern as `replace_template_pdf`, `reanalyze_question`, etc.).
#[tauri::command]
pub fn run_student_intake(
    inputs: Vec<StudentIntakeInput>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    let event_sink = Arc::new(AppHandleRuntimeEventSink::new(app));
    state
        .start_student_intake_job(inputs, event_sink)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn save_student_intake_page_order(
    input: crate::models::StudentIntakePageOrderUpdateInput,
    state: State<'_, AppState>,
) -> Result<ExamWorkspaceState, String> {
    state
        .save_student_intake_page_order(input)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn delete_student_submission(
    input: DeleteStudentSubmissionInput,
    state: State<'_, AppState>,
) -> Result<ExamWorkspaceState, String> {
    state
        .delete_student_submission(input)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn begin_student_workflow(
    settings: AppSettings,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    let event_sink = Arc::new(AppHandleRuntimeEventSink::new(app));
    state
        .start_student_workflow_job(settings, event_sink)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn regrade_question_answers(
    question_id: String,
    settings: AppSettings,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    let event_sink = Arc::new(AppHandleRuntimeEventSink::new(app));
    state
        .start_regrade_question_answers_job(question_id, settings, event_sink)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn confirm_student_alignment(
    input: StudentWorkflowAlignmentUpdateInput,
    settings: AppSettings,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    let event_sink = Arc::new(AppHandleRuntimeEventSink::new(app));
    state
        .start_confirm_student_alignment_job(input, settings, event_sink)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn save_student_alignment_review(
    input: StudentWorkflowAlignmentUpdateInput,
    state: State<'_, AppState>,
) -> Result<ExamWorkspaceState, String> {
    state
        .save_student_alignment_review(input)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn confirm_student_parse_review(
    input: StudentWorkflowParseReviewInput,
    settings: AppSettings,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    let event_sink = Arc::new(AppHandleRuntimeEventSink::new(app));
    state
        .start_confirm_student_parse_review_job(input, settings, event_sink)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn save_student_parse_review(
    input: StudentWorkflowParseReviewInput,
    state: State<'_, AppState>,
) -> Result<ExamWorkspaceState, String> {
    state
        .save_student_parse_review(input)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn confirm_student_detect_review(
    input: StudentWorkflowDetectReviewInput,
    settings: AppSettings,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    let event_sink = Arc::new(AppHandleRuntimeEventSink::new(app));
    state
        .start_confirm_student_detect_review_job(input, settings, event_sink)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn save_student_detect_review(
    input: StudentWorkflowDetectReviewInput,
    state: State<'_, AppState>,
) -> Result<ExamWorkspaceState, String> {
    state
        .save_student_detect_review(input)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn get_job_trace(
    job_id: Option<String>,
    command_name: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<JobTraceState>, String> {
    state
        .load_job_trace(job_id, command_name)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn list_job_traces(state: State<'_, AppState>) -> Result<Vec<JobTraceSummary>, String> {
    state.list_job_traces().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn replace_template_pdf(
    template_pdf_path: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    let event_sink = Arc::new(AppHandleRuntimeEventSink::new(app));
    state
        .start_replace_template_pdf_job(template_pdf_path, event_sink)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn export_stamped_template_pdf(
    destination_path: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    let event_sink = Arc::new(AppHandleRuntimeEventSink::new(app));
    state
        .start_export_stamped_template_pdf_job(destination_path, event_sink)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn list_canvas_courses(
    base_url: String,
    access_token: String,
) -> Result<Vec<crate::lms::LmsCourseSummary>, String> {
    crate::lms::canvas::list_teacher_courses(&base_url, &access_token)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_canvas_course_roster(
    base_url: String,
    access_token: String,
    course_id: String,
) -> Result<Vec<crate::lms::LmsRosterRow>, String> {
    crate::lms::canvas::list_course_roster(&base_url, &access_token, &course_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn compute_lms_binding_token(
    course_id: String,
    canvas_user_id: String,
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<String, String> {
    state
        .compute_lms_binding_token(course_id, canvas_user_id, &settings)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn prior_canonical_submission_exists_for_lms_student(
    course_id: String,
    canvas_user_id: String,
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    state
        .prior_canonical_submission_exists_for_lms_student(course_id, canvas_user_id, &settings)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resolve_lms_student_ref(
    course_id: String,
    canvas_user_id: String,
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<crate::models::StudentRosterMatchResult, String> {
    state
        .resolve_lms_student_ref(course_id, canvas_user_id, &settings)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_lms_roster_cache_state(
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<LmsRosterCacheSnapshot, String> {
    state
        .lms_roster_cache_snapshot(&settings)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn ensure_lms_roster_preload(
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<LmsRosterCacheSnapshot, String> {
    state
        .ensure_lms_roster_preload(&settings)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn list_lms_assignments(
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<Vec<LmsAssignmentSummary>, String> {
    let project_path = state
        .current_project_path()
        .map_err(|err| err.to_string())?;
    crate::state::results_lms::list_lms_assignments(&project_path, &settings)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn list_lms_assignments_for_course(
    course_id: String,
    settings: AppSettings,
) -> Result<Vec<LmsAssignmentSummary>, String> {
    let course_id = course_id.trim();
    if course_id.is_empty() {
        return Err("Link an LMS course in Template setup before selecting an assignment.".into());
    }
    match settings.lms_provider.trim() {
        "canvas" => {
            let base_url = settings.lms_canvas_base_url.trim();
            let access_token = settings.lms_canvas_api_key.as_deref().unwrap_or("").trim();
            if base_url.is_empty() || access_token.is_empty() {
                return Err(
                    "Canvas LMS settings are incomplete. Add the base URL and access token in Settings."
                        .into(),
                );
            }
            crate::lms::canvas::list_course_assignments(base_url, access_token, course_id)
                .await
                .map_err(|err| err.to_string())
        }
        "" | "none" => {
            Err("Choose an LMS provider in Settings before selecting an assignment.".into())
        }
        provider => Err(format!(
            "The '{provider}' LMS provider is not supported for assignment loading."
        )),
    }
}

#[tauri::command]
pub async fn save_results_lms_assignment(
    input: SaveResultsLmsAssignmentInput,
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<ExamWorkspaceState, String> {
    let app_state = AppState::from_inner(state.clone_inner());
    tauri::async_runtime::spawn_blocking(move || {
        app_state.save_results_lms_assignment(&settings, input)
    })
    .await
    .map_err(|err| format!("Blocking task join error: {err}"))?
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn set_submission_result_finalized(
    input: SetSubmissionResultFinalizedInput,
    state: State<'_, AppState>,
) -> Result<ExamWorkspaceState, String> {
    let app_state = AppState::from_inner(state.clone_inner());
    tauri::async_runtime::spawn_blocking(move || app_state.set_submission_result_finalized(input))
        .await
        .map_err(|err| format!("Blocking task join error: {err}"))?
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn finalize_ready_results(
    input: crate::models::FinalizeReadyResultsInput,
    state: State<'_, AppState>,
) -> Result<ExamWorkspaceState, String> {
    let app_state = AppState::from_inner(state.clone_inner());
    tauri::async_runtime::spawn_blocking(move || app_state.finalize_ready_results(input))
        .await
        .map_err(|err| format!("Blocking task join error: {err}"))?
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn preview_results_lms_report(
    student_ref: String,
    state: State<'_, AppState>,
) -> Result<ResultsLmsReportPreview, String> {
    let project_path = state
        .current_project_path()
        .map_err(|err| err.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::state::results_lms::preview_results_lms_report(&project_path, &student_ref)
    })
    .await
    .map_err(|err| format!("Blocking task join error: {err}"))?
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn run_results_lms_upload(
    input: RunResultsLmsUploadInput,
    settings: AppSettings,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ResultsLmsUploadResponse, String> {
    let project_path = state
        .current_project_path()
        .map_err(|err| err.to_string())?;
    let roster_cache = state
        .lms_roster_cache_snapshot(&settings)
        .map_err(|err| err.to_string())?;
    let event_sink = AppHandleRuntimeEventSink::new(app);
    crate::state::results_lms::run_results_lms_upload(
        &project_path,
        &settings,
        &roster_cache,
        input,
        &event_sink,
    )
    .await
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn retry_results_lms_upload(
    input: RetryResultsLmsUploadInput,
    settings: AppSettings,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ResultsLmsUploadResponse, String> {
    let project_path = state
        .current_project_path()
        .map_err(|err| err.to_string())?;
    let roster_cache = state
        .lms_roster_cache_snapshot(&settings)
        .map_err(|err| err.to_string())?;
    let event_sink = AppHandleRuntimeEventSink::new(app);
    crate::state::results_lms::retry_results_lms_upload(
        &project_path,
        &settings,
        &roster_cache,
        input,
        &event_sink,
    )
    .await
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn run_results_export(
    input: RunResultsExportInput,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ResultsExportResponse, String> {
    let state_inner = state.clone_inner();
    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let event_sink = AppHandleRuntimeEventSink::new(app_clone);
        crate::state::results_export::run_results_export(&state_inner, input, &event_sink)
    })
    .await
    .map_err(|err| format!("Task panicked: {err}"))?
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn transient_pdf_clip_text(
    input: PdfTextClipInput,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    let state_inner = state.clone_inner();
    tauri::async_runtime::spawn_blocking(move || {
        let event_sink = AppHandleRuntimeEventSink::new(app);
        state_inner.transient_pdf_clip_text(input, &event_sink)
    })
    .await
    .map_err(|err| format!("Task panicked: {err}"))?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn transient_scans_ocr_hint(
    png_bytes_base64: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<crate::models::ScansOcrHintResult, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(png_bytes_base64.trim())
        .map_err(|_| "png_bytes_base64 must be valid base64.".to_string())?;
    let state_inner = state.clone_inner();
    tauri::async_runtime::spawn_blocking(move || {
        let event_sink = AppHandleRuntimeEventSink::new(app);
        state_inner.transient_scans_ocr_hint(bytes, &event_sink)
    })
    .await
    .map_err(|err| format!("Task panicked: {err}"))?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn transient_render_pdf_page_png(
    pdf_path: String,
    page_number: i64,
    zoom: f64,
    max_width_px: Option<i64>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<crate::state::workspace_actions::IntakePreviewPage, String> {
    let state_inner = state.clone_inner();
    tauri::async_runtime::spawn_blocking(move || {
        let event_sink = AppHandleRuntimeEventSink::new(app);
        state_inner.transient_render_pdf_page_png(
            pdf_path,
            page_number,
            zoom,
            max_width_px,
            &event_sink,
        )
    })
    .await
    .map_err(|err| format!("Task panicked: {err}"))?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn transient_clip_pdf_rects_png_base64(
    pdf_path: String,
    rects: Vec<crate::state::workspace_actions::PdfPointRect>,
    zoom: f64,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<String>, String> {
    let state_inner = state.clone_inner();
    tauri::async_runtime::spawn_blocking(move || {
        let event_sink = AppHandleRuntimeEventSink::new(app);
        state_inner.transient_clip_pdf_rects_png_base64(pdf_path, rects, zoom, &event_sink)
    })
    .await
    .map_err(|err| format!("Task panicked: {err}"))?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn intake_default_pdf_rects_from_template(
    pdf_path: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<crate::state::workspace_actions::PdfPointRect>, String> {
    let state_inner = state.clone_inner();
    tauri::async_runtime::spawn_blocking(move || {
        let event_sink = AppHandleRuntimeEventSink::new(app);
        state_inner.intake_default_pdf_rects_from_template(pdf_path, &event_sink)
    })
    .await
    .map_err(|err| format!("Task panicked: {err}"))?
    .map_err(|e| e.to_string())
}

fn sync_main_window_title(app: &AppHandle, shell: &ShellState) {
    let title = window_title(shell);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_title(&title);
    }
}

pub(crate) fn window_title(shell: &ShellState) -> String {
    shell
        .current_project
        .as_ref()
        .map(|project| format!("ScriptScore - [{}]", project.project_path))
        .unwrap_or_else(|| "ScriptScore".to_string())
}

#[cfg(test)]
mod tests {
    use crate::models::{ProjectSummary, ShellState, WorkerStatus};

    use super::window_title;

    #[test]
    fn window_title_reflects_shell_project_state() {
        assert_eq!(
            window_title(&ShellState {
                current_project: None,
                worker_status: WorkerStatus::Ready,
                worker_activity: Default::default(),
                last_runtime_error: None,
                debug_features: Default::default(),
            }),
            "ScriptScore"
        );

        assert_eq!(
            window_title(&ShellState {
                current_project: Some(ProjectSummary {
                    project_id: "proj_1".into(),
                    display_name: "Midterm 1".into(),
                    subject: Some("Physics".into()),
                    course_code: Some("PHYS 221".into()),
                    lms_course_id: None,
                    project_path: "/tmp/midterm-1".into(),
                    created_at: "1".into(),
                    updated_at: "1".into(),
                }),
                worker_status: WorkerStatus::Ready,
                worker_activity: Default::default(),
                last_runtime_error: None,
                debug_features: Default::default(),
            }),
            "ScriptScore - [/tmp/midterm-1]"
        );
    }
}
