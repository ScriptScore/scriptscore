// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::path::PathBuf;

use rusqlite::Connection;
use serde_json::json;

use crate::binding_token::{canvas_course_context, compute_binding_token_hex, TOKEN_VERSION};
use crate::errors::{HostError, HostResult};
use crate::lms;
use crate::models::{
    AppSettings, ExamWorkspaceState, FinalizeReadyResultsInput, LmsAssignmentSummary,
    LmsRosterCacheSnapshot, LmsUploadAttemptResult, LmsUploadMode, LmsUploadPreparationRow,
    LmsUploadPublishOutcome, LmsUploadStudentResult, LmsUploadStudentStatus, ProjectConfig,
    ResultFinalizationRecord, ResultsLmsAssetBinding, ResultsLmsReportPreview, ResultsLmsState,
    ResultsLmsTarget, ResultsLmsUploadResponse, RetryResultsLmsUploadInput,
    RunResultsLmsUploadInput, SaveResultsLmsAssignmentInput, SetSubmissionResultFinalizedInput,
    WorkerStatus,
};
use crate::project_store;
use crate::state::{results_lms_report, RuntimeEventSink};

const MAX_UPLOAD_ATTEMPTS: usize = 12;
pub(crate) const RESULTS_LMS_UPLOAD_COMMAND_NAME: &str = "results.lms-upload";
pub(crate) const RESULTS_LMS_UPLOAD_BATCH_STARTED_EVENT: &str = "results_lms_upload_batch_started";
pub(crate) const RESULTS_LMS_UPLOAD_STUDENT_STARTED_EVENT: &str =
    "results_lms_upload_student_started";
pub(crate) const RESULTS_LMS_UPLOAD_STUDENT_FINISHED_EVENT: &str =
    "results_lms_upload_student_finished";
pub(crate) const RESULTS_LMS_UPLOAD_BATCH_FINISHED_EVENT: &str =
    "results_lms_upload_batch_finished";
pub(crate) const RESULTS_LMS_UPLOAD_BATCH_FAILED_EVENT: &str = "results_lms_upload_batch_failed";

struct UploadExecutionPlan {
    workspace: ExamWorkspaceState,
    selected_target: ResultsLmsTarget,
    publisher: lms::ActiveLmsResultsPublisher,
    preparation_rows: Vec<LmsUploadPreparationRow>,
    preflight_failures: Vec<LmsUploadStudentResult>,
}

struct UploadPreparationContext<'a> {
    project_path: &'a Path,
    exam_title: &'a str,
    selected_target: &'a ResultsLmsTarget,
    existing_asset_bindings: &'a [ResultsLmsAssetBinding],
    questions_by_id: &'a HashMap<&'a str, &'a crate::models::QuestionRecord>,
    transient_user_ids: &'a HashMap<String, String>,
}

pub(crate) fn sync_results_lms_assignment_context(
    project_path: &Path,
    settings: &AppSettings,
) -> HostResult<bool> {
    let connection = Connection::open(project_store::schema::project_db_path(project_path))?;
    project_store::schema::initialize_schema(&connection)?;
    let mut project_config = project_store::load_project_config(&connection)?;
    let mut results_state = project_store::load_results_lms_state(project_path)?;

    let active_provider = settings.lms_provider.trim().to_ascii_lowercase();
    let active_provider = if active_provider.is_empty() {
        None
    } else {
        Some(active_provider)
    };
    let active_course_id = project_config
        .lms_course_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let should_clear = match results_state.selected_target.as_ref() {
        Some(target) => {
            Some(target.provider.trim().to_ascii_lowercase()) != active_provider
                || Some(target.course_id.trim().to_string()) != active_course_id
                || project_config.lms_assignment_id.as_deref().map(str::trim)
                    != Some(target.assignment_id.trim())
        }
        None => project_config
            .lms_assignment_id
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty()),
    };

    if !should_clear {
        return Ok(false);
    }

    project_config.lms_assignment_id = None;
    results_state.selected_target = None;
    save_project_config_and_results_state(project_path, &project_config, &results_state)?;
    Ok(true)
}

pub(crate) async fn list_lms_assignments(
    project_path: &Path,
    settings: &AppSettings,
) -> HostResult<Vec<LmsAssignmentSummary>> {
    let _ = sync_results_lms_assignment_context(project_path, settings)?;
    let connection = Connection::open(project_store::schema::project_db_path(project_path))?;
    project_store::schema::initialize_schema(&connection)?;
    let project_config = project_store::load_project_config(&connection)?;
    let loader = lms::resolve_active_assignment_loader(&project_config, settings)
        .map_err(|idle| HostError::Validation(idle.reason))?;
    lms::load_course_assignments(&loader).await
}

pub(crate) fn save_results_lms_assignment(
    project_path: &Path,
    settings: &AppSettings,
    input: SaveResultsLmsAssignmentInput,
) -> HostResult<ExamWorkspaceState> {
    let _ = sync_results_lms_assignment_context(project_path, settings)?;
    let connection = Connection::open(project_store::schema::project_db_path(project_path))?;
    project_store::schema::initialize_schema(&connection)?;
    let mut project_config = project_store::load_project_config(&connection)?;
    let mut results_state = project_store::load_results_lms_state(project_path)?;

    let assignment_id = input
        .assignment_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    if let Some(assignment_id) = assignment_id {
        let loader = lms::resolve_active_assignment_loader(&project_config, settings)
            .map_err(|idle| HostError::Validation(idle.reason))?;
        results_state.selected_target = Some(ResultsLmsTarget {
            provider: loader.provider_id().to_string(),
            course_id: loader.course_id().to_string(),
            assignment_id: assignment_id.clone(),
        });
        project_config.lms_assignment_id = Some(assignment_id);
    } else {
        results_state.selected_target = None;
        project_config.lms_assignment_id = None;
    }

    save_project_config_and_results_state(project_path, &project_config, &results_state)?;
    project_store::load_exam_workspace_state(project_path)
}

pub(crate) fn set_submission_result_finalized(
    project_path: &Path,
    input: SetSubmissionResultFinalizedInput,
) -> HostResult<ExamWorkspaceState> {
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    let row = result_row_for_student(&workspace, &input.student_ref)?;

    let mut results_state = workspace.results_lms_state.clone();
    results_state
        .finalization_records
        .retain(|record| record.student_ref != input.student_ref);

    if input.finalized {
        if !row.ready_to_finalize {
            return Err(HostError::Validation(
                "Only ready results can be finalized.".into(),
            ));
        }
        let fingerprint = row.result_fingerprint.clone().ok_or_else(|| {
            HostError::Validation("Ready results must have a current fingerprint.".into())
        })?;
        results_state
            .finalization_records
            .push(ResultFinalizationRecord {
                student_ref: input.student_ref,
                result_fingerprint: fingerprint,
                finalized_at: crate::project_store::schema::current_timestamp(),
            });
    }

    project_store::save_results_lms_state(project_path, &results_state)?;
    project_store::load_exam_workspace_state(project_path)
}

pub(crate) fn finalize_ready_results(
    project_path: &Path,
    input: FinalizeReadyResultsInput,
) -> HostResult<ExamWorkspaceState> {
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    let requested_set =
        normalize_requested_student_refs(&input.student_refs, &workspace.results_lms_rows)?;
    if requested_set.is_empty() {
        return Ok(workspace);
    }
    let now = crate::project_store::schema::current_timestamp();
    let mut records_by_ref = workspace
        .results_lms_state
        .finalization_records
        .iter()
        .map(|record| (record.student_ref.clone(), record.clone()))
        .collect::<HashMap<_, _>>();

    for row in &workspace.results_lms_rows {
        if let Some(record) = finalized_record_for_requested_row(row, &requested_set, &now) {
            records_by_ref.insert(row.student_ref.clone(), record);
        }
    }

    let mut results_state = workspace.results_lms_state.clone();
    results_state.finalization_records = records_by_ref.into_values().collect();
    results_state
        .finalization_records
        .sort_by(|left, right| left.student_ref.cmp(&right.student_ref));
    project_store::save_results_lms_state(project_path, &results_state)?;
    project_store::load_exam_workspace_state(project_path)
}

fn finalized_record_for_requested_row(
    row: &crate::models::ResultStudentRow,
    requested_set: &HashSet<&str>,
    finalized_at: &str,
) -> Option<ResultFinalizationRecord> {
    if !requested_set.contains(row.student_ref.as_str()) || !row.ready_to_finalize {
        return None;
    }

    row.result_fingerprint
        .as_ref()
        .map(|fingerprint| ResultFinalizationRecord {
            student_ref: row.student_ref.clone(),
            result_fingerprint: fingerprint.clone(),
            finalized_at: finalized_at.to_string(),
        })
}

pub(crate) fn preview_results_lms_report(
    project_path: &Path,
    student_ref: &str,
) -> HostResult<ResultsLmsReportPreview> {
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    results_lms_report::build_report_preview(&workspace, student_ref)
}

pub(crate) async fn run_results_lms_upload(
    project_path: &Path,
    settings: &AppSettings,
    roster_cache: &LmsRosterCacheSnapshot,
    input: RunResultsLmsUploadInput,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<ResultsLmsUploadResponse> {
    run_upload_for_student_refs(
        project_path,
        settings,
        roster_cache,
        input.mode,
        &input.student_refs,
        event_sink,
    )
    .await
}

pub(crate) async fn retry_results_lms_upload(
    project_path: &Path,
    settings: &AppSettings,
    roster_cache: &LmsRosterCacheSnapshot,
    input: RetryResultsLmsUploadInput,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<ResultsLmsUploadResponse> {
    let _ = sync_results_lms_assignment_context(project_path, settings)?;
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    let results_state = project_store::load_results_lms_state(project_path)?;
    let prior_attempt = results_state
        .upload_attempts
        .iter()
        .find(|attempt| attempt.attempt_id == input.attempt_id)
        .cloned()
        .ok_or_else(|| {
            HostError::Validation("The selected upload attempt was not found.".into())
        })?;
    validate_retry_target(
        workspace.results_lms_state.selected_target.as_ref(),
        &prior_attempt,
    )?;

    let student_refs = prior_attempt
        .student_results
        .iter()
        .filter(|result| result.status == LmsUploadStudentStatus::Failed)
        .map(|result| result.student_ref.clone())
        .collect::<Vec<_>>();

    if student_refs.is_empty() {
        return Err(HostError::Validation(
            "The selected upload attempt has no failed rows to retry.".into(),
        ));
    }

    run_upload_for_student_refs(
        project_path,
        settings,
        roster_cache,
        prior_attempt.mode,
        &student_refs,
        event_sink,
    )
    .await
}

async fn run_upload_for_student_refs(
    project_path: &Path,
    settings: &AppSettings,
    roster_cache: &LmsRosterCacheSnapshot,
    mode: LmsUploadMode,
    requested_student_refs: &[String],
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<ResultsLmsUploadResponse> {
    let project_path = PathBuf::from(project_path);
    let settings = settings.clone();
    let roster_cache = roster_cache.clone();
    let requested_student_refs = requested_student_refs.to_vec();
    let upload_plan = tauri::async_runtime::spawn_blocking(move || {
        prepare_upload_execution(
            &project_path,
            &settings,
            &roster_cache,
            &requested_student_refs,
        )
    })
    .await
    .map_err(|err| HostError::Project(format!("Blocking task join error: {err}")))??;

    let UploadExecutionPlan {
        workspace,
        selected_target,
        publisher,
        preparation_rows,
        preflight_failures,
    } = upload_plan;
    if preparation_rows.is_empty() && preflight_failures.is_empty() {
        return Err(HostError::Validation(
            "Select at least one finalized result before upload.".into(),
        ));
    }

    let batch_id = format!("results_upload_{}", uuid::Uuid::new_v4());
    let tracked_student_refs = tracked_upload_student_refs(&preparation_rows, &preflight_failures);
    emit_results_lms_upload_batch_started(event_sink, &batch_id, mode, &tracked_student_refs);
    for failure in &preflight_failures {
        emit_results_lms_upload_student_finished(event_sink, &batch_id, failure);
    }

    let started_at = crate::project_store::schema::current_timestamp();
    let published_outcomes = if preparation_rows.is_empty() {
        Vec::new()
    } else {
        match lms::publish_results(&publisher, mode, &preparation_rows, event_sink, &batch_id).await
        {
            Ok(results) => results,
            Err(error) => {
                emit_results_lms_upload_batch_failed(event_sink, &batch_id, &error.to_string());
                return Err(error);
            }
        }
    };
    let finished_at = crate::project_store::schema::current_timestamp();

    let mut student_results = preflight_failures;
    student_results.extend(
        published_outcomes
            .iter()
            .map(|outcome| outcome.student_result.clone()),
    );
    student_results.sort_by(|left, right| left.student_ref.cmp(&right.student_ref));
    let attempt = build_upload_attempt(
        &selected_target,
        mode,
        started_at,
        finished_at,
        student_results,
    );

    let project_path = PathBuf::from(workspace.project.project_path.as_str());
    let attempt_to_save = attempt.clone();
    let outcomes_to_save = published_outcomes.clone();
    let selected_target_to_save = selected_target.clone();
    let refreshed_workspace = tauri::async_runtime::spawn_blocking(move || {
        persist_upload_attempt(
            &project_path,
            &workspace,
            &selected_target_to_save,
            attempt_to_save,
            outcomes_to_save,
        )
    })
    .await
    .map_err(|err| HostError::Project(format!("Blocking task join error: {err}")))??;
    emit_results_lms_upload_batch_finished(event_sink, &batch_id);
    Ok(ResultsLmsUploadResponse {
        attempt,
        workspace: refreshed_workspace,
    })
}

fn tracked_upload_student_refs(
    preparation_rows: &[LmsUploadPreparationRow],
    preflight_failures: &[LmsUploadStudentResult],
) -> Vec<String> {
    let mut student_refs = preparation_rows
        .iter()
        .map(|row| row.student_ref.clone())
        .collect::<Vec<_>>();
    student_refs.extend(
        preflight_failures
            .iter()
            .map(|result| result.student_ref.clone()),
    );
    student_refs.sort();
    student_refs.dedup();
    student_refs
}

fn emit_results_lms_upload_batch_started(
    event_sink: &dyn RuntimeEventSink,
    batch_id: &str,
    mode: LmsUploadMode,
    student_refs: &[String],
) {
    event_sink.emit_runtime_event(crate::models::RuntimeJobEvent {
        event_type: RESULTS_LMS_UPLOAD_BATCH_STARTED_EVENT.into(),
        command_name: RESULTS_LMS_UPLOAD_COMMAND_NAME.into(),
        worker_status: WorkerStatus::Busy,
        request_id: None,
        job_id: None,
        payload: json!({
            "batchId": batch_id,
            "mode": mode,
            "studentRefs": student_refs,
        }),
    });
}

pub(crate) fn emit_results_lms_upload_student_started(
    event_sink: &dyn RuntimeEventSink,
    batch_id: &str,
    student_ref: &str,
) {
    event_sink.emit_runtime_event(crate::models::RuntimeJobEvent {
        event_type: RESULTS_LMS_UPLOAD_STUDENT_STARTED_EVENT.into(),
        command_name: RESULTS_LMS_UPLOAD_COMMAND_NAME.into(),
        worker_status: WorkerStatus::Busy,
        request_id: None,
        job_id: None,
        payload: json!({
            "batchId": batch_id,
            "studentRef": student_ref,
        }),
    });
}

pub(crate) fn emit_results_lms_upload_student_finished(
    event_sink: &dyn RuntimeEventSink,
    batch_id: &str,
    result: &LmsUploadStudentResult,
) {
    event_sink.emit_runtime_event(crate::models::RuntimeJobEvent {
        event_type: RESULTS_LMS_UPLOAD_STUDENT_FINISHED_EVENT.into(),
        command_name: RESULTS_LMS_UPLOAD_COMMAND_NAME.into(),
        worker_status: WorkerStatus::Busy,
        request_id: None,
        job_id: None,
        payload: json!({
            "batchId": batch_id,
            "studentRef": result.student_ref,
            "status": result.status,
            "error": result.sanitized_error,
        }),
    });
}

fn emit_results_lms_upload_batch_finished(event_sink: &dyn RuntimeEventSink, batch_id: &str) {
    event_sink.emit_runtime_event(crate::models::RuntimeJobEvent {
        event_type: RESULTS_LMS_UPLOAD_BATCH_FINISHED_EVENT.into(),
        command_name: RESULTS_LMS_UPLOAD_COMMAND_NAME.into(),
        worker_status: WorkerStatus::Ready,
        request_id: None,
        job_id: None,
        payload: json!({
            "batchId": batch_id,
        }),
    });
}

fn emit_results_lms_upload_batch_failed(
    event_sink: &dyn RuntimeEventSink,
    batch_id: &str,
    error_message: &str,
) {
    event_sink.emit_runtime_event(crate::models::RuntimeJobEvent {
        event_type: RESULTS_LMS_UPLOAD_BATCH_FAILED_EVENT.into(),
        command_name: RESULTS_LMS_UPLOAD_COMMAND_NAME.into(),
        worker_status: WorkerStatus::Error,
        request_id: None,
        job_id: None,
        payload: json!({
            "batchId": batch_id,
            "error": error_message,
        }),
    });
}

fn prepare_upload_execution(
    project_path: &Path,
    settings: &AppSettings,
    roster_cache: &LmsRosterCacheSnapshot,
    requested_student_refs: &[String],
) -> HostResult<UploadExecutionPlan> {
    let (workspace, selected_target, publisher) = resolve_upload_context(project_path, settings)?;
    validate_roster_cache(roster_cache, &workspace, &selected_target)?;
    let transient_user_ids =
        resolve_transient_user_ids(settings, &selected_target, &workspace, roster_cache)?;
    let requested_set =
        normalize_requested_student_refs(requested_student_refs, &workspace.results_lms_rows)?;
    let (preparation_rows, preflight_failures) = build_upload_preparation_rows(
        &workspace,
        &selected_target,
        &requested_set,
        &transient_user_ids,
    )?;
    Ok(UploadExecutionPlan {
        workspace,
        selected_target,
        publisher,
        preparation_rows,
        preflight_failures,
    })
}

fn persist_upload_attempt(
    project_path: &Path,
    workspace: &ExamWorkspaceState,
    selected_target: &ResultsLmsTarget,
    attempt: LmsUploadAttemptResult,
    published_outcomes: Vec<LmsUploadPublishOutcome>,
) -> HostResult<ExamWorkspaceState> {
    let mut results_state = workspace.results_lms_state.clone();
    results_state.upload_attempts.push(attempt);
    apply_published_asset_bindings(&mut results_state, selected_target, &published_outcomes);
    trim_upload_attempt_history(&mut results_state);
    project_store::save_results_lms_state(project_path, &results_state)?;
    project_store::load_exam_workspace_state(project_path)
}

fn resolve_upload_context(
    project_path: &Path,
    settings: &AppSettings,
) -> HostResult<(
    ExamWorkspaceState,
    ResultsLmsTarget,
    lms::ActiveLmsResultsPublisher,
)> {
    let _ = sync_results_lms_assignment_context(project_path, settings)?;
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    let selected_target = workspace
        .results_lms_state
        .selected_target
        .clone()
        .ok_or_else(|| HostError::Validation("Choose an LMS assignment before upload.".into()))?;
    let publisher = lms::resolve_active_results_publisher(&workspace.project_config, settings)
        .map_err(|idle| HostError::Validation(idle.reason))?;
    validate_active_upload_target(&selected_target, &publisher)?;
    Ok((workspace, selected_target, publisher))
}

fn validate_active_upload_target(
    selected_target: &ResultsLmsTarget,
    publisher: &lms::ActiveLmsResultsPublisher,
) -> HostResult<()> {
    if publisher.provider_id() == selected_target.provider
        && publisher.course_id() == selected_target.course_id
        && publisher.assignment_id() == selected_target.assignment_id
    {
        return Ok(());
    }

    Err(HostError::Validation(
        "The saved LMS assignment no longer matches the active LMS configuration. Choose the assignment again before upload.".into(),
    ))
}

fn build_upload_preparation_rows(
    workspace: &ExamWorkspaceState,
    selected_target: &ResultsLmsTarget,
    requested_set: &HashSet<&str>,
    transient_user_ids: &HashMap<String, String>,
) -> HostResult<(Vec<LmsUploadPreparationRow>, Vec<LmsUploadStudentResult>)> {
    let mut preparation_rows = Vec::new();
    let mut preflight_failures = Vec::new();
    let project_path = Path::new(workspace.project.project_path.as_str());
    let questions_by_id = workspace
        .questions
        .iter()
        .map(|question| (question.question_id.as_str(), question))
        .collect::<HashMap<_, _>>();
    let submissions_by_student = workspace
        .student_workflow
        .submissions
        .iter()
        .map(|submission| (submission.student_ref.as_str(), submission))
        .collect::<HashMap<_, _>>();

    for row in workspace
        .results_lms_rows
        .iter()
        .filter(|row| requested_set.contains(row.student_ref.as_str()))
    {
        let context = UploadPreparationContext {
            project_path,
            exam_title: &workspace.project.display_name,
            selected_target,
            existing_asset_bindings: &workspace.results_lms_state.asset_bindings,
            questions_by_id: &questions_by_id,
            transient_user_ids,
        };
        match build_upload_preparation_row(
            &context,
            row,
            submissions_by_student
                .get(row.student_ref.as_str())
                .copied(),
        )? {
            Ok(preparation_row) => preparation_rows.push(preparation_row),
            Err(preflight_failure) => preflight_failures.push(preflight_failure),
        }
    }

    Ok((preparation_rows, preflight_failures))
}

fn build_upload_preparation_row(
    context: &UploadPreparationContext<'_>,
    row: &crate::models::ResultStudentRow,
    submission: Option<&crate::models::StudentWorkflowSubmission>,
) -> HostResult<Result<LmsUploadPreparationRow, LmsUploadStudentResult>> {
    if !row.finalized || row.stale_finalization {
        return Ok(Err(build_preflight_failure(
            row,
            row.result_fingerprint.clone().unwrap_or_default(),
            "Only currently finalized results can be uploaded.",
        )));
    }

    let fingerprint = row.result_fingerprint.clone().ok_or_else(|| {
        HostError::Validation("Finalized results must have a current fingerprint.".into())
    })?;
    let Some(user_id) = context.transient_user_ids.get(row.student_ref.as_str()) else {
        return Ok(Err(build_preflight_failure(
            row,
            fingerprint,
            "Could not resolve this student in the current LMS roster cache.",
        )));
    };
    let Some(submission) = submission else {
        return Ok(Err(build_preflight_failure(
            row,
            fingerprint,
            "Could not find this student's graded submission while preparing the HTML report.",
        )));
    };
    let (report_html_template, report_assets) = results_lms_report::build_report_upload_materials(
        context.project_path,
        context.exam_title,
        row,
        submission,
        context.questions_by_id,
    )?;
    let existing_asset_bindings = context
        .existing_asset_bindings
        .iter()
        .filter(|binding| {
            binding.provider == context.selected_target.provider
                && binding.course_id == context.selected_target.course_id
                && binding.assignment_id == context.selected_target.assignment_id
                && binding.student_ref == row.student_ref
        })
        .cloned()
        .collect::<Vec<_>>();

    Ok(Ok(LmsUploadPreparationRow {
        student_ref: row.student_ref.clone(),
        user_id: user_id.clone(),
        result_fingerprint: fingerprint,
        score: row.aggregate_total as f64,
        report_html_template,
        report_assets,
        existing_asset_bindings,
    }))
}

fn build_preflight_failure(
    row: &crate::models::ResultStudentRow,
    result_fingerprint: String,
    message: &str,
) -> LmsUploadStudentResult {
    LmsUploadStudentResult {
        student_ref: row.student_ref.clone(),
        result_fingerprint,
        status: LmsUploadStudentStatus::Failed,
        sanitized_error: Some(message.into()),
    }
}

fn build_upload_attempt(
    selected_target: &ResultsLmsTarget,
    mode: LmsUploadMode,
    started_at: String,
    finished_at: String,
    student_results: Vec<LmsUploadStudentResult>,
) -> LmsUploadAttemptResult {
    let success_count = student_results
        .iter()
        .filter(|result| {
            matches!(
                result.status,
                LmsUploadStudentStatus::Ready | LmsUploadStudentStatus::Uploaded
            )
        })
        .count() as i64;
    let failure_count = student_results
        .iter()
        .filter(|result| result.status == LmsUploadStudentStatus::Failed)
        .count() as i64;

    LmsUploadAttemptResult {
        attempt_id: format!("attempt_{}", crate::project_store::schema::unique_suffix()),
        mode,
        provider: selected_target.provider.clone(),
        course_id: selected_target.course_id.clone(),
        assignment_id: selected_target.assignment_id.clone(),
        started_at,
        finished_at,
        attempted_count: student_results.len() as i64,
        success_count,
        failure_count,
        student_results,
    }
}

fn apply_published_asset_bindings(
    results_state: &mut ResultsLmsState,
    selected_target: &ResultsLmsTarget,
    published_outcomes: &[LmsUploadPublishOutcome],
) {
    for outcome in published_outcomes {
        if outcome.student_result.status != LmsUploadStudentStatus::Uploaded {
            continue;
        }
        results_state.asset_bindings.retain(|binding| {
            !binding_matches_target_student(
                binding,
                &selected_target.provider,
                &selected_target.course_id,
                &selected_target.assignment_id,
                &outcome.student_result.student_ref,
            )
        });
        results_state
            .asset_bindings
            .extend(outcome.active_asset_bindings.iter().cloned());
    }
}

fn binding_matches_target_student(
    binding: &ResultsLmsAssetBinding,
    provider: &str,
    course_id: &str,
    assignment_id: &str,
    student_ref: &str,
) -> bool {
    binding.provider == provider
        && binding.course_id == course_id
        && binding.assignment_id == assignment_id
        && binding.student_ref == student_ref
}

fn save_project_config_and_results_state(
    project_path: &Path,
    project_config: &ProjectConfig,
    results_state: &ResultsLmsState,
) -> HostResult<()> {
    let mut connection = Connection::open(project_store::schema::project_db_path(project_path))?;
    project_store::schema::initialize_schema(&connection)?;
    let transaction = connection.transaction()?;
    project_store::save_project_config(&transaction, project_config)?;
    transaction.commit()?;
    project_store::save_results_lms_state(project_path, results_state)?;
    Ok(())
}

fn result_row_for_student<'a>(
    workspace: &'a ExamWorkspaceState,
    student_ref: &str,
) -> HostResult<&'a crate::models::ResultStudentRow> {
    workspace
        .results_lms_rows
        .iter()
        .find(|row| row.student_ref == student_ref)
        .ok_or_else(|| {
            HostError::Validation(format!("Result row '{}' was not found.", student_ref))
        })
}

fn validate_roster_cache(
    roster_cache: &LmsRosterCacheSnapshot,
    workspace: &ExamWorkspaceState,
    selected_target: &ResultsLmsTarget,
) -> HostResult<()> {
    if roster_cache.status != crate::models::LmsRosterCacheStatus::Ready {
        return Err(HostError::Validation(
            "The LMS roster cache is not ready. Refresh the roster before upload.".into(),
        ));
    }
    if roster_cache.project_path.as_deref() != Some(workspace.project.project_path.as_str()) {
        return Err(HostError::Validation(
            "The LMS roster cache belongs to a different project. Refresh the roster before upload.".into(),
        ));
    }
    if roster_cache.course_id.as_deref() != Some(selected_target.course_id.as_str()) {
        return Err(HostError::Validation(
            "The LMS roster cache does not match the selected course. Refresh the roster before upload.".into(),
        ));
    }
    if roster_cache.lms_provider.as_deref() != Some(selected_target.provider.as_str()) {
        return Err(HostError::Validation(
            "The LMS roster cache does not match the selected provider. Refresh the roster before upload.".into(),
        ));
    }
    Ok(())
}

fn resolve_transient_user_ids(
    settings: &AppSettings,
    selected_target: &ResultsLmsTarget,
    workspace: &ExamWorkspaceState,
    roster_cache: &LmsRosterCacheSnapshot,
) -> HostResult<HashMap<String, String>> {
    let secret = crate::secrets::binding_hmac_secret_bytes(settings)?;
    let token_context = canvas_course_context(&selected_target.course_id);
    let roster_by_token = roster_cache
        .rows
        .iter()
        .map(|row| {
            let token = compute_binding_token_hex(
                &secret,
                &token_context,
                row.user_id.trim(),
                TOKEN_VERSION,
            )?;
            Ok((token, row.user_id.clone()))
        })
        .collect::<HostResult<HashMap<_, _>>>()?;

    Ok(workspace
        .student_roster
        .iter()
        .filter_map(|row| {
            roster_by_token
                .get(row.binding_token_hex.as_str())
                .map(|user_id| (row.student_ref.clone(), user_id.clone()))
        })
        .collect())
}

fn normalize_requested_student_refs<'a>(
    requested_student_refs: &'a [String],
    rows: &'a [crate::models::ResultStudentRow],
) -> HostResult<HashSet<&'a str>> {
    if requested_student_refs.is_empty() {
        return Ok(HashSet::new());
    }
    let available = rows
        .iter()
        .map(|row| row.student_ref.as_str())
        .collect::<HashSet<_>>();
    let mut out = HashSet::new();
    for student_ref in requested_student_refs {
        let trimmed = student_ref.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !available.contains(trimmed) {
            return Err(HostError::Validation(format!(
                "Result row '{}' was not found.",
                trimmed
            )));
        }
        out.insert(trimmed);
    }
    Ok(out)
}

fn trim_upload_attempt_history(state: &mut ResultsLmsState) {
    if state.upload_attempts.len() <= MAX_UPLOAD_ATTEMPTS {
        return;
    }
    let remove_count = state.upload_attempts.len() - MAX_UPLOAD_ATTEMPTS;
    state.upload_attempts.drain(0..remove_count);
}

fn validate_retry_target(
    selected_target: Option<&ResultsLmsTarget>,
    prior_attempt: &LmsUploadAttemptResult,
) -> HostResult<()> {
    let Some(selected_target) = selected_target else {
        return Err(HostError::Validation(
            "Re-select the LMS assignment used for this upload attempt before retrying failed rows."
                .into(),
        ));
    };

    if selected_target.provider != prior_attempt.provider
        || selected_target.course_id != prior_attempt.course_id
        || selected_target.assignment_id != prior_attempt.assignment_id
    {
        return Err(HostError::Validation(
            "The selected LMS assignment no longer matches the chosen upload attempt. Re-select the original assignment before retrying failed rows.".into(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use rusqlite::Connection;

    use crate::models::{
        AppSettings, ExamWorkspaceState, InstructorProfile, LmsRosterCacheSnapshot,
        LmsRosterCacheStatus, LmsUploadAttemptResult, LmsUploadMode, LmsUploadPublishOutcome,
        LmsUploadStudentResult, LmsUploadStudentStatus, ProjectConfig, ProjectSummary,
        ResultsLmsState, ResultsLmsTarget, RuntimeJobEvent, StudentIntakeState, StudentRosterRow,
        StudentWorkflowState, WorkerStatus,
    };
    use crate::test_support::{lock_env_vars, EnvVarGuard};

    use super::{
        apply_published_asset_bindings, build_upload_attempt, build_upload_preparation_row,
        emit_results_lms_upload_batch_failed, emit_results_lms_upload_batch_started,
        emit_results_lms_upload_student_finished, emit_results_lms_upload_student_started,
        normalize_requested_student_refs, resolve_transient_user_ids,
        sync_results_lms_assignment_context, tracked_upload_student_refs,
        trim_upload_attempt_history, validate_retry_target, validate_roster_cache,
        UploadPreparationContext,
    };
    use std::collections::HashMap;

    #[derive(Clone, Default)]
    struct RecordingEventSink {
        events: Arc<Mutex<Vec<RuntimeJobEvent>>>,
    }

    impl RecordingEventSink {
        fn snapshot(&self) -> Vec<RuntimeJobEvent> {
            self.events.lock().expect("event sink lock").clone()
        }
    }

    impl crate::state::RuntimeEventSink for RecordingEventSink {
        fn emit_runtime_event(&self, event: RuntimeJobEvent) {
            self.events.lock().expect("event sink lock").push(event);
        }
    }

    fn temp_root(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_millis()
        ))
    }

    fn create_project_with_course(test_root: &std::path::Path, course_id: &str) -> PathBuf {
        let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", test_root);
        let created = crate::project_store::create_project(
            "Results LMS",
            None,
            None,
            Some(course_id.into()),
            &InstructorProfile::default(),
        )
        .expect("project should be created");
        PathBuf::from(created.project_path)
    }

    fn save_assignment_binding(project_path: &std::path::Path, assignment_id: &str) {
        let mut connection =
            Connection::open(crate::project_store::schema::project_db_path(project_path))
                .expect("project db should open");
        crate::project_store::schema::initialize_schema(&connection)
            .expect("schema should initialize");
        let mut project_config =
            crate::project_store::load_project_config(&connection).expect("config should load");
        project_config.lms_assignment_id = Some(assignment_id.into());
        let transaction = connection.transaction().expect("transaction should open");
        crate::project_store::save_project_config(&transaction, &project_config)
            .expect("config should save");
        transaction.commit().expect("transaction should commit");
    }

    fn workspace_for_lms(project_path: &str, course_id: &str) -> ExamWorkspaceState {
        ExamWorkspaceState {
            project: ProjectSummary {
                project_id: "proj_1".into(),
                display_name: "Results LMS".into(),
                subject: None,
                course_code: None,
                lms_course_id: Some(course_id.into()),
                project_path: project_path.into(),
                created_at: "1".into(),
                updated_at: "1".into(),
            },
            status: "ready".into(),
            status_label: "Ready".into(),
            failure_message: None,
            template_preview_artifacts: Vec::new(),
            aruco_status: Default::default(),
            questions: Vec::new(),
            redaction_regions: Vec::new(),
            warnings: Vec::new(),
            can_approve: false,
            can_approve_rubric: false,
            project_config: ProjectConfig {
                lms_course_id: Some(course_id.into()),
                ..ProjectConfig::default()
            },
            student_roster: Vec::new(),
            student_intake: StudentIntakeState::not_started(),
            student_workflow: StudentWorkflowState::not_started(),
            moderation_state: Default::default(),
            results_lms_state: ResultsLmsState::default(),
            results_lms_rows: Vec::new(),
            results_lms_metrics: None,
            results_lms_review_summary: None,
            workflow_stage: "ready".into(),
            workflow_label: "Ready".into(),
        }
    }

    fn ready_roster_cache(
        project_path: &str,
        provider: &str,
        course_id: &str,
    ) -> LmsRosterCacheSnapshot {
        LmsRosterCacheSnapshot {
            status: LmsRosterCacheStatus::Ready,
            project_path: Some(project_path.into()),
            lms_provider: Some(provider.into()),
            course_id: Some(course_id.into()),
            rows: Vec::new(),
            last_error: None,
            idle_reason: None,
        }
    }

    #[test]
    fn sync_results_lms_assignment_context_clears_assignment_when_course_changes() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-results-lms-sync-course");
        let project_path = create_project_with_course(&test_root, "course-a");
        save_assignment_binding(&project_path, "assignment-1");
        crate::project_store::save_results_lms_state(
            &project_path,
            &ResultsLmsState {
                selected_target: Some(ResultsLmsTarget {
                    provider: "canvas".into(),
                    course_id: "course-a".into(),
                    assignment_id: "assignment-1".into(),
                }),
                ..ResultsLmsState::default()
            },
        )
        .expect("results state should save");

        let settings = AppSettings {
            lms_provider: "canvas".into(),
            ..AppSettings::default()
        };

        let initial_sync = sync_results_lms_assignment_context(&project_path, &settings)
            .expect("initial sync should succeed");
        assert!(
            !initial_sync,
            "matching provider/course should not clear state"
        );

        let mut connection =
            Connection::open(crate::project_store::schema::project_db_path(&project_path))
                .expect("project db should open");
        let mut project_config =
            crate::project_store::load_project_config(&connection).expect("config should load");
        project_config.lms_course_id = Some("course-b".into());
        let transaction = connection.transaction().expect("transaction should open");
        crate::project_store::save_project_config(&transaction, &project_config)
            .expect("config should save");
        transaction.commit().expect("transaction should commit");

        let cleared = sync_results_lms_assignment_context(&project_path, &settings)
            .expect("sync should succeed after course change");
        assert!(
            cleared,
            "course drift should clear the saved assignment context"
        );

        let connection =
            Connection::open(crate::project_store::schema::project_db_path(&project_path))
                .expect("project db should reopen");
        let project_config =
            crate::project_store::load_project_config(&connection).expect("config should load");
        let results_state =
            crate::project_store::load_results_lms_state(&project_path).expect("state should load");
        assert_eq!(project_config.lms_course_id.as_deref(), Some("course-b"));
        assert_eq!(project_config.lms_assignment_id, None);
        assert!(results_state.selected_target.is_none());

        std::fs::remove_dir_all(&test_root).expect("test project should clean up");
    }

    #[test]
    fn validate_retry_target_rejects_assignment_mismatch() {
        let error = validate_retry_target(
            Some(&ResultsLmsTarget {
                provider: "canvas".into(),
                course_id: "course-a".into(),
                assignment_id: "assignment-2".into(),
            }),
            &LmsUploadAttemptResult {
                attempt_id: "attempt-1".into(),
                mode: LmsUploadMode::Live,
                provider: "canvas".into(),
                course_id: "course-a".into(),
                assignment_id: "assignment-1".into(),
                started_at: "1".into(),
                finished_at: "2".into(),
                attempted_count: 1,
                success_count: 0,
                failure_count: 1,
                student_results: vec![LmsUploadStudentResult {
                    student_ref: "student_1".into(),
                    result_fingerprint: "fp".into(),
                    status: LmsUploadStudentStatus::Failed,
                    sanitized_error: Some("failed".into()),
                }],
            },
        )
        .expect_err("retry should fail when the selected assignment changed");

        assert!(error
            .to_string()
            .contains("selected LMS assignment no longer matches"));
    }

    #[test]
    fn validate_retry_target_rejects_missing_selected_assignment() {
        let error = validate_retry_target(None, &upload_attempt("attempt-1", "student_1"))
            .expect_err("retry should fail without selected assignment");

        assert!(error.to_string().contains("Re-select the LMS assignment"));
    }

    #[test]
    fn upload_preparation_returns_preflight_failures_before_publish() {
        let selected_target = ResultsLmsTarget {
            provider: "canvas".into(),
            course_id: "course-a".into(),
            assignment_id: "assignment-1".into(),
        };
        let mut transient_user_ids = HashMap::new();
        transient_user_ids.insert("student_1".to_string(), "42".to_string());
        let questions_by_id = HashMap::new();
        let context = UploadPreparationContext {
            project_path: std::path::Path::new("/tmp/unused"),
            exam_title: "Exam",
            selected_target: &selected_target,
            existing_asset_bindings: &[],
            questions_by_id: &questions_by_id,
            transient_user_ids: &transient_user_ids,
        };
        let unfinalized = result_row("student_1", false, false, Some("fp-1"));
        let stale = result_row("student_1", true, true, Some("fp-1"));
        let missing_roster = result_row("student_2", true, false, Some("fp-2"));
        let missing_submission = result_row("student_1", true, false, Some("fp-1"));

        let unfinalized_failure = build_upload_preparation_row(&context, &unfinalized, None)
            .expect("preflight should not fail host validation")
            .expect_err("unfinalized result should be preflight failure");
        let stale_failure = build_upload_preparation_row(&context, &stale, None)
            .expect("preflight should not fail host validation")
            .expect_err("stale result should be preflight failure");
        let roster_failure = build_upload_preparation_row(&context, &missing_roster, None)
            .expect("preflight should not fail host validation")
            .expect_err("missing roster user should be preflight failure");
        let submission_failure = build_upload_preparation_row(&context, &missing_submission, None)
            .expect("preflight should not fail host validation")
            .expect_err("missing submission should be preflight failure");

        assert!(unfinalized_failure
            .sanitized_error
            .as_deref()
            .unwrap()
            .contains("currently finalized"));
        assert!(stale_failure
            .sanitized_error
            .as_deref()
            .unwrap()
            .contains("currently finalized"));
        assert!(roster_failure
            .sanitized_error
            .as_deref()
            .unwrap()
            .contains("current LMS roster cache"));
        assert!(submission_failure
            .sanitized_error
            .as_deref()
            .unwrap()
            .contains("graded submission"));
    }

    #[test]
    fn validate_roster_cache_rejects_not_ready_project_course_and_provider_mismatches() {
        let workspace = workspace_for_lms("/tmp/project-a", "course-a");
        let ready = ready_roster_cache("/tmp/project-a", "canvas", "course-a");
        let selected_target = ResultsLmsTarget {
            provider: "canvas".into(),
            course_id: "course-a".into(),
            assignment_id: "assignment-1".into(),
        };
        validate_roster_cache(&ready, &workspace, &selected_target)
            .expect("matching ready roster cache should validate");

        let mut not_ready = ready.clone();
        not_ready.status = LmsRosterCacheStatus::Loading;
        assert!(
            validate_roster_cache(&not_ready, &workspace, &selected_target)
                .unwrap_err()
                .to_string()
                .contains("not ready")
        );

        let wrong_project = ready_roster_cache("/tmp/project-b", "canvas", "course-a");
        assert!(
            validate_roster_cache(&wrong_project, &workspace, &selected_target)
                .unwrap_err()
                .to_string()
                .contains("different project")
        );

        let wrong_course = ready_roster_cache("/tmp/project-a", "canvas", "course-b");
        assert!(
            validate_roster_cache(&wrong_course, &workspace, &selected_target)
                .unwrap_err()
                .to_string()
                .contains("selected course")
        );

        let wrong_provider = ready_roster_cache("/tmp/project-a", "schoology", "course-a");
        assert!(
            validate_roster_cache(&wrong_provider, &workspace, &selected_target)
                .unwrap_err()
                .to_string()
                .contains("selected provider")
        );
    }

    #[test]
    fn resolve_transient_user_ids_maps_roster_tokens_to_persisted_student_refs() {
        let _guard = lock_env_vars();
        let secret = vec![7_u8; 32];
        crate::secrets::__test_set_binding_hmac_secret_bytes(secret.clone());
        let selected_target = ResultsLmsTarget {
            provider: "canvas".into(),
            course_id: "course-a".into(),
            assignment_id: "assignment-1".into(),
        };
        let token_context = crate::binding_token::canvas_course_context(&selected_target.course_id);
        let matched_token = crate::binding_token::compute_binding_token_hex(
            &secret,
            &token_context,
            "42",
            crate::binding_token::TOKEN_VERSION,
        )
        .expect("binding token should compute");
        let mut workspace = workspace_for_lms("/tmp/project-a", "course-a");
        workspace.student_roster = vec![
            StudentRosterRow {
                student_ref: "student_1".into(),
                binding_token_hex: matched_token,
            },
            StudentRosterRow {
                student_ref: "student_2".into(),
                binding_token_hex: "unmatched".into(),
            },
        ];
        let mut roster_cache = ready_roster_cache("/tmp/project-a", "canvas", "course-a");
        roster_cache.rows = vec![crate::lms::LmsRosterRow {
            user_id: "42".into(),
            display_name: "Amy First".into(),
            sort_key: "first, amy".into(),
            email: None,
            login_id: None,
        }];

        let resolved = resolve_transient_user_ids(
            &AppSettings::default(),
            &selected_target,
            &workspace,
            &roster_cache,
        )
        .expect("transient user ids should resolve");

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved.get("student_1").map(String::as_str), Some("42"));
        assert!(!resolved.contains_key("student_2"));
        crate::secrets::__test_clear_binding_hmac_secret_cache();
    }

    #[test]
    fn normalize_requested_student_refs_deduplicates_trims_and_rejects_unknown_rows() {
        let rows = vec![
            result_row("student_1", true, false, Some("fp-1")),
            result_row("student_2", true, false, Some("fp-2")),
        ];

        let all = normalize_requested_student_refs(&[], &rows).expect("empty means all selected");
        assert!(all.is_empty());

        let requested_refs = vec![
            " student_1 ".to_string(),
            "".to_string(),
            "student_1".to_string(),
            "student_2".to_string(),
        ];
        let requested = normalize_requested_student_refs(&requested_refs, &rows)
            .expect("known requested refs should normalize");

        assert_eq!(requested.len(), 2);
        assert!(requested.contains("student_1"));
        assert!(requested.contains("student_2"));
        assert!(
            normalize_requested_student_refs(&["missing".to_string()], &rows)
                .unwrap_err()
                .to_string()
                .contains("Result row 'missing' was not found")
        );
    }

    #[test]
    fn upload_attempt_counts_ready_uploaded_and_failed_results() {
        let selected_target = ResultsLmsTarget {
            provider: "canvas".into(),
            course_id: "course-a".into(),
            assignment_id: "assignment-1".into(),
        };
        let attempt = build_upload_attempt(
            &selected_target,
            LmsUploadMode::DryRun,
            "1".into(),
            "2".into(),
            vec![
                LmsUploadStudentResult {
                    student_ref: "student_1".into(),
                    result_fingerprint: "fp-1".into(),
                    status: LmsUploadStudentStatus::Ready,
                    sanitized_error: None,
                },
                LmsUploadStudentResult {
                    student_ref: "student_2".into(),
                    result_fingerprint: "fp-2".into(),
                    status: LmsUploadStudentStatus::Uploaded,
                    sanitized_error: None,
                },
                LmsUploadStudentResult {
                    student_ref: "student_3".into(),
                    result_fingerprint: "fp-3".into(),
                    status: LmsUploadStudentStatus::Failed,
                    sanitized_error: Some("failed".into()),
                },
            ],
        );

        assert_eq!(attempt.mode, LmsUploadMode::DryRun);
        assert_eq!(attempt.attempted_count, 3);
        assert_eq!(attempt.success_count, 2);
        assert_eq!(attempt.failure_count, 1);
    }

    #[test]
    fn tracked_upload_student_refs_merges_sort_and_deduplicates_publish_and_preflight_refs() {
        let preparation_rows = vec![
            crate::models::LmsUploadPreparationRow {
                student_ref: "student_2".into(),
                ..Default::default()
            },
            crate::models::LmsUploadPreparationRow {
                student_ref: "student_1".into(),
                ..Default::default()
            },
        ];
        let preflight_failures = vec![
            LmsUploadStudentResult {
                student_ref: "student_2".into(),
                result_fingerprint: "fp-2".into(),
                status: LmsUploadStudentStatus::Failed,
                sanitized_error: Some("failed".into()),
            },
            LmsUploadStudentResult {
                student_ref: "student_3".into(),
                result_fingerprint: "fp-3".into(),
                status: LmsUploadStudentStatus::Failed,
                sanitized_error: Some("failed".into()),
            },
        ];

        assert_eq!(
            tracked_upload_student_refs(&preparation_rows, &preflight_failures),
            vec!["student_1", "student_2", "student_3"]
        );
    }

    #[test]
    fn trim_upload_attempt_history_keeps_most_recent_attempts() {
        let mut state = ResultsLmsState {
            upload_attempts: (0..15)
                .map(|index| upload_attempt(&format!("attempt-{index}"), "student_1"))
                .collect(),
            ..ResultsLmsState::default()
        };

        trim_upload_attempt_history(&mut state);

        assert_eq!(state.upload_attempts.len(), 12);
        assert_eq!(state.upload_attempts[0].attempt_id, "attempt-3");
        assert_eq!(state.upload_attempts[11].attempt_id, "attempt-14");
    }

    #[test]
    fn published_asset_bindings_replace_only_matching_target_student() {
        let selected_target = ResultsLmsTarget {
            provider: "canvas".into(),
            course_id: "course-a".into(),
            assignment_id: "assignment-1".into(),
        };
        let mut state = ResultsLmsState {
            asset_bindings: vec![
                asset_binding("canvas", "course-a", "assignment-1", "student_1", "old"),
                asset_binding(
                    "canvas",
                    "course-a",
                    "assignment-1",
                    "student_2",
                    "keep-student",
                ),
                asset_binding(
                    "canvas",
                    "course-a",
                    "assignment-2",
                    "student_1",
                    "keep-assignment",
                ),
                asset_binding(
                    "canvas",
                    "course-b",
                    "assignment-1",
                    "student_1",
                    "keep-course",
                ),
            ],
            ..ResultsLmsState::default()
        };
        let replacement = asset_binding("canvas", "course-a", "assignment-1", "student_1", "new");

        apply_published_asset_bindings(
            &mut state,
            &selected_target,
            &[LmsUploadPublishOutcome {
                student_result: LmsUploadStudentResult {
                    student_ref: "student_1".into(),
                    result_fingerprint: "fp".into(),
                    status: LmsUploadStudentStatus::Uploaded,
                    sanitized_error: None,
                },
                active_asset_bindings: vec![replacement],
            }],
        );

        let file_ids = state
            .asset_bindings
            .iter()
            .map(|binding| binding.provider_file_id.as_str())
            .collect::<Vec<_>>();
        assert!(!file_ids.contains(&"old"));
        assert!(file_ids.contains(&"new"));
        assert!(file_ids.contains(&"keep-student"));
        assert!(file_ids.contains(&"keep-assignment"));
        assert!(file_ids.contains(&"keep-course"));
    }

    #[test]
    fn upload_runtime_events_include_privacy_safe_payloads() {
        let sink = RecordingEventSink::default();
        let student_refs = vec!["student_1".to_string(), "student_2".to_string()];
        let finished = LmsUploadStudentResult {
            student_ref: "student_1".into(),
            result_fingerprint: "fp".into(),
            status: LmsUploadStudentStatus::Failed,
            sanitized_error: Some("Canvas API error 422 while publishing this score.".into()),
        };

        emit_results_lms_upload_batch_started(&sink, "batch-1", LmsUploadMode::Live, &student_refs);
        emit_results_lms_upload_student_started(&sink, "batch-1", "student_1");
        emit_results_lms_upload_student_finished(&sink, "batch-1", &finished);
        emit_results_lms_upload_batch_failed(&sink, "batch-1", "Upload failed.");

        let events = sink.snapshot();
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].payload["batchId"], "batch-1");
        assert_eq!(events[0].payload["mode"], "live");
        assert_eq!(
            events[0].payload["studentRefs"],
            serde_json::json!(student_refs)
        );
        assert!(matches!(events[0].worker_status, WorkerStatus::Busy));
        assert_eq!(events[1].payload["studentRef"], "student_1");
        assert!(events[1].payload.get("resultFingerprint").is_none());
        assert_eq!(events[2].payload["status"], "failed");
        assert_eq!(
            events[2].payload["error"],
            "Canvas API error 422 while publishing this score."
        );
        assert!(events[2].payload.get("userId").is_none());
        assert_eq!(events[3].payload["error"], "Upload failed.");
        assert!(matches!(events[3].worker_status, WorkerStatus::Error));
    }

    fn upload_attempt(attempt_id: &str, student_ref: &str) -> LmsUploadAttemptResult {
        LmsUploadAttemptResult {
            attempt_id: attempt_id.into(),
            mode: LmsUploadMode::Live,
            provider: "canvas".into(),
            course_id: "course-a".into(),
            assignment_id: "assignment-1".into(),
            started_at: "1".into(),
            finished_at: "2".into(),
            attempted_count: 1,
            success_count: 0,
            failure_count: 1,
            student_results: vec![LmsUploadStudentResult {
                student_ref: student_ref.into(),
                result_fingerprint: "fp".into(),
                status: LmsUploadStudentStatus::Failed,
                sanitized_error: Some("failed".into()),
            }],
        }
    }

    fn asset_binding(
        provider: &str,
        course_id: &str,
        assignment_id: &str,
        student_ref: &str,
        provider_file_id: &str,
    ) -> crate::models::ResultsLmsAssetBinding {
        crate::models::ResultsLmsAssetBinding {
            provider: provider.into(),
            course_id: course_id.into(),
            assignment_id: assignment_id.into(),
            student_ref: student_ref.into(),
            local_asset_name: "q1.jpg".into(),
            asset_fingerprint: "asset-fp".into(),
            provider_file_id: provider_file_id.into(),
        }
    }

    fn result_row(
        student_ref: &str,
        finalized: bool,
        stale_finalization: bool,
        result_fingerprint: Option<&str>,
    ) -> crate::models::ResultStudentRow {
        crate::models::ResultStudentRow {
            student_ref: student_ref.into(),
            aggregate_total: 10,
            aggregate_complete: true,
            ready_to_finalize: true,
            result_fingerprint: result_fingerprint.map(ToOwned::to_owned),
            finalized,
            stale_finalization,
            ..crate::models::ResultStudentRow::default()
        }
    }
}
