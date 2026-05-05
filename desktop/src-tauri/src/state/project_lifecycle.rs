// SPDX-License-Identifier: AGPL-3.0-only
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;

use super::runtime::{run_reserved_job, start_runtime_job, RuntimeJobRequest};
use super::scheduler::QueuedRuntimeJob;
use super::template_setup::{exam_setup_failure_message, parse_exam_setup_success, run_token};
use super::{AppStateInner, RuntimeEventSink};
use crate::errors::{HostError, HostResult};
use crate::models::ExamWorkspaceState;
use crate::project_store;

pub(crate) fn create_project(
    state_arc: &Arc<AppStateInner>,
    input: crate::models::CreateProjectInput,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<crate::models::ShellState> {
    let template_pdf_path = PathBuf::from(input.template_pdf_path.clone());
    if !template_pdf_path.is_file() {
        return Err(HostError::Validation(format!(
            "Template PDF was not found at '{}'.",
            template_pdf_path.display()
        )));
    }
    let instructor_profile = input.instructor_profile.unwrap_or_default();
    let project_root = input.project_root.as_ref().map(PathBuf::from);
    let summary = match project_root {
        Some(root) => project_store::create_project_in_root(
            root,
            &input.display_name,
            input.subject.clone(),
            input.course_code.clone(),
            input.lms_course_id.clone(),
            &instructor_profile,
        )?,
        None => project_store::create_project(
            &input.display_name,
            input.subject.clone(),
            input.course_code.clone(),
            input.lms_course_id.clone(),
            &instructor_profile,
        )?,
    };
    let project_path = PathBuf::from(&summary.project_path);
    {
        let mut app = state_arc.lock();
        app.ensure_no_active_job()?;
        app.current_project = Some(summary);
    }
    run_template_setup(state_arc, &project_path, &template_pdf_path, event_sink)?;

    {
        let mut app = state_arc.lock();
        app.refresh_current_project(&project_path)?;
        Ok(app.shell_state())
    }
}

pub(crate) fn enqueue_post_setup_jobs(
    state_arc: &Arc<AppStateInner>,
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    settings: &crate::models::AppSettings,
) -> HostResult<()> {
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    if workspace.questions.is_empty() {
        return Ok(());
    }

    let analyze_job_id = uuid::Uuid::new_v4().to_string();
    let analyze_request_id = uuid::Uuid::new_v4().to_string();

    let question_targets =
        super::workspace_actions::analyze_question_targets_for_workspace(&workspace);
    let output_artifacts_dir =
        project_store::command_output_dir(project_path, "exam.analyze", &analyze_job_id);
    fs::create_dir_all(&output_artifacts_dir)?;

    let analyze_job = QueuedRuntimeJob {
        job_id: analyze_job_id,
        request_id: analyze_request_id,
        command_name: "exam.analyze".to_string(),
        worker_request_payload: super::workspace_actions::analyze_worker_request_payload(
            settings,
            &question_targets,
        ),
        persisted_request_payload: super::workspace_actions::analyze_persisted_request_payload(
            settings,
            &question_targets,
        ),
        output_artifacts_dir: Some(output_artifacts_dir),
        project_path: Some(project_path.to_path_buf()),
        submitted_at: super::current_run_timestamp(),
        depends_on_job_id: None,
        skip_if_dependency_failed: false,
        settings: None,
    };

    {
        let mut app = state_arc.lock();
        app.scheduler.queue_job(analyze_job, event_sink);
    }

    super::runtime::dispatch_next_queued_job(state_arc, event_sink);

    Ok(())
}

/// Enqueues `exam.generate-rubric` for questions that need an auto-generated rubric when analysis is
/// ready (no dependency on the batch analyze job id).
///
/// Eligibility matches rubric authoring in the UI: template setup must be **draft or approved**
/// (same as `can_approve_rubric`), not gated on `ai_assist_enabled` — manual `exam.generate-rubric`
/// does not use that flag, and the default app setting left it off so enqueue was effectively a no-op.
pub(crate) fn enqueue_auto_rubric_jobs_if_eligible(
    state_arc: &Arc<AppStateInner>,
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    settings: &crate::models::AppSettings,
) -> HostResult<()> {
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    if !template_status_allows_auto_rubric_enqueue(&workspace) {
        return Ok(());
    }

    let candidate_ids = eligible_auto_rubric_question_ids(&workspace);

    if candidate_ids.is_empty() {
        return Ok(());
    }

    {
        let mut app = state_arc.lock();
        for qid in candidate_ids {
            if app.scheduler.has_generate_rubric_for_question(qid.as_str()) {
                continue;
            }
            app.scheduler.queue_job(
                queued_generate_rubric_job(project_path, settings, &qid),
                event_sink,
            );
        }
    }

    super::runtime::dispatch_next_queued_job(state_arc, event_sink);
    Ok(())
}

fn template_status_allows_auto_rubric_enqueue(workspace: &ExamWorkspaceState) -> bool {
    matches!(workspace.status.as_str(), "draft" | "approved")
}

fn eligible_auto_rubric_question_ids(workspace: &ExamWorkspaceState) -> Vec<String> {
    workspace
        .questions
        .iter()
        .filter(|question| question.image_path.is_some())
        .filter(|question| question.analysis.is_complete())
        .filter(|question| question.rubric.criteria.is_empty())
        .map(|question| question.question_id.clone())
        .collect()
}

fn queued_generate_rubric_job(
    project_path: &Path,
    settings: &crate::models::AppSettings,
    question_id: &str,
) -> QueuedRuntimeJob {
    QueuedRuntimeJob {
        job_id: uuid::Uuid::new_v4().to_string(),
        request_id: uuid::Uuid::new_v4().to_string(),
        command_name: "exam.generate-rubric".to_string(),
        worker_request_payload: json!({
            "question_id": question_id,
            "replace_existing": true,
        }),
        persisted_request_payload: json!({
            "question_id": question_id,
            "replace_existing": true,
        }),
        output_artifacts_dir: None,
        project_path: Some(project_path.to_path_buf()),
        submitted_at: super::current_run_timestamp(),
        depends_on_job_id: None,
        skip_if_dependency_failed: false,
        settings: Some(settings.clone()),
    }
}

pub(crate) fn replace_template_pdf(
    state_arc: &Arc<AppStateInner>,
    template_pdf_path: String,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<ExamWorkspaceState> {
    let template_pdf_path = PathBuf::from(template_pdf_path);
    let project_path = {
        let app = state_arc.lock();
        app.current_project_path()?
    };
    run_template_setup(state_arc, &project_path, &template_pdf_path, event_sink)?;
    let workspace = project_store::load_exam_workspace_state(&project_path)?;
    let mut app = state_arc.lock();
    app.current_project = Some(workspace.project.clone());
    Ok(workspace)
}

pub(crate) fn run_template_setup(
    state_arc: &Arc<AppStateInner>,
    project_path: &Path,
    template_pdf_path: &Path,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<()> {
    ensure_template_pdf_exists(template_pdf_path)?;

    let reserved =
        match reserve_template_setup_job(state_arc, project_path, template_pdf_path, event_sink)? {
            Some(reserved) => reserved,
            None => return Ok(()),
        };
    let mut payload = match prepare_setup_payload(project_path, template_pdf_path, &reserved.job_id)
    {
        Ok(payload) => payload,
        Err(err) => {
            return super::runtime::finish_runtime_job(
                state_arc,
                event_sink,
                reserved.worker,
                "exam.setup",
                &reserved.request_id,
                &reserved.job_id,
                Err(err),
            )
            .map(|_| ());
        }
    };
    update_setup_trace_ref(project_path, &reserved.job_id)?;
    let output_artifacts_dir = match create_setup_output_dir(project_path, &payload, &reserved) {
        Ok(dir) => dir,
        Err(err) => {
            return super::runtime::finish_runtime_job(
                state_arc,
                event_sink,
                reserved.worker,
                "exam.setup",
                &reserved.request_id,
                &reserved.job_id,
                Err(err),
            )
            .map(|_| ());
        }
    };
    let completed = match execute_template_setup_job(
        state_arc,
        project_path,
        template_pdf_path,
        &payload,
        reserved,
        &output_artifacts_dir,
        event_sink,
    ) {
        Ok(completed) => completed,
        Err(err) => {
            project_store::persist_template_setup_failure(
                project_path,
                &payload,
                &err.to_string(),
            )?;
            return Ok(());
        }
    };

    persist_completed_template_setup(project_path, &mut payload, &completed)?;
    Ok(())
}

fn ensure_template_pdf_exists(template_pdf_path: &Path) -> HostResult<()> {
    if template_pdf_path.is_file() {
        return Ok(());
    }
    Err(HostError::Validation(format!(
        "Template PDF was not found at '{}'.",
        template_pdf_path.display()
    )))
}

fn reserve_template_setup_job(
    state_arc: &Arc<AppStateInner>,
    project_path: &Path,
    template_pdf_path: &Path,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<Option<super::runtime::ReservedJob>> {
    match start_runtime_job(state_arc, event_sink, "exam.setup") {
        Ok(reserved) => Ok(Some(reserved)),
        Err(err @ HostError::Conflict(_)) => Err(err),
        Err(err) => {
            persist_setup_start_failure(project_path, template_pdf_path, &err)?;
            Ok(None)
        }
    }
}

fn persist_setup_start_failure(
    project_path: &Path,
    template_pdf_path: &Path,
    error: &HostError,
) -> HostResult<()> {
    let payload =
        project_store::prepare_template_setup(project_path, template_pdf_path, &run_token())?;
    project_store::persist_template_setup_failure(project_path, &payload, &error.to_string())
}

fn prepare_setup_payload(
    project_path: &Path,
    template_pdf_path: &Path,
    job_id: &str,
) -> HostResult<crate::models::TemplateSetupPayload> {
    project_store::prepare_template_setup(project_path, template_pdf_path, job_id)
}

fn create_setup_output_dir(
    project_path: &Path,
    payload: &crate::models::TemplateSetupPayload,
    reserved: &super::runtime::ReservedJob,
) -> HostResult<std::path::PathBuf> {
    let output_artifacts_dir =
        project_store::command_output_dir(project_path, "exam.setup", &reserved.job_id);
    fs::create_dir_all(&output_artifacts_dir).inspect_err(|err| {
        let _ =
            project_store::persist_template_setup_failure(project_path, payload, &err.to_string());
    })?;
    Ok(output_artifacts_dir)
}

fn execute_template_setup_job(
    state_arc: &Arc<AppStateInner>,
    project_path: &Path,
    template_pdf_path: &Path,
    payload: &crate::models::TemplateSetupPayload,
    reserved: super::runtime::ReservedJob,
    output_artifacts_dir: &Path,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<crate::worker::CompletedWorkerJob> {
    run_reserved_job(
        state_arc,
        event_sink,
        reserved,
        RuntimeJobRequest {
            command_name: "exam.setup",
            worker_request_payload: json!({
                "template_pdf_path": template_pdf_path.to_string_lossy().into_owned(),
            }),
            persisted_request_payload: json!({
                "template_artifact_id": payload.template_artifact_id.clone(),
                "template_source_name": payload.template_source_name.clone(),
            }),
            output_artifacts_dir: Some(output_artifacts_dir),
            project_path: Some(project_path),
            stdin_bytes: None,
        },
    )
}

fn persist_completed_template_setup(
    project_path: &Path,
    payload: &mut crate::models::TemplateSetupPayload,
    completed: &crate::worker::CompletedWorkerJob,
) -> HostResult<()> {
    payload.last_setup_job_id = Some(completed.job_id.clone());
    if completed.result.terminal_type == "job_finished" {
        persist_successful_template_setup(project_path, payload, completed)
    } else {
        project_store::persist_template_setup_failure(
            project_path,
            payload,
            &exam_setup_failure_message(&completed.result.envelope),
        )
    }
}

fn persist_successful_template_setup(
    project_path: &Path,
    payload: &mut crate::models::TemplateSetupPayload,
    completed: &crate::worker::CompletedWorkerJob,
) -> HostResult<()> {
    let (page_artifacts, artifact_records, questions, warnings) =
        parse_exam_setup_success(project_path, completed)?;
    payload.warnings = warnings;
    project_store::persist_template_setup_success(
        project_path,
        payload,
        &page_artifacts,
        &artifact_records,
        &questions,
    )
}

fn update_setup_trace_ref(project_path: &Path, job_id: &str) -> HostResult<()> {
    let mut connection =
        rusqlite::Connection::open(project_store::schema::project_db_path(project_path))?;
    let project_config = project_store::load_project_config(&connection)?;
    let transaction = connection.transaction()?;
    project_store::update_setup_trace_ref(&transaction, &project_config.project_id, job_id)?;
    transaction.commit()?;
    Ok(())
}
