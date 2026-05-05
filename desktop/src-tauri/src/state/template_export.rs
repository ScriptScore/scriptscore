// SPDX-License-Identifier: AGPL-3.0-only
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::runtime::{run_reserved_job, start_runtime_job, RuntimeJobRequest};
use super::template_setup::exam_setup_failure_message;
use super::{AppStateInner, RuntimeEventSink};
use crate::errors::{HostError, HostResult};
use crate::models::{ExamWorkspaceState, TemplateArucoPageStatus, TemplateArucoStatus};
use crate::project_store;

const TEMPLATE_EXPORT_RUNTIME_JOB_WAIT_TIMEOUT: Duration = Duration::from_secs(1800);
const TEMPLATE_EXPORT_RUNTIME_JOB_POLL_INTERVAL: Duration = Duration::from_millis(100);

pub(crate) fn export_stamped_template_pdf(
    state_arc: &Arc<AppStateInner>,
    destination_path: String,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<ExamWorkspaceState> {
    let destination_path = PathBuf::from(destination_path);
    validate_pdf_destination(&destination_path)?;
    let project_path = {
        let app = state_arc.lock();
        app.current_project_path()?
    };
    let mut workspace = project_store::load_exam_workspace_state(&project_path)?;
    if workspace.aruco_status.state == "unknown" {
        refresh_template_aruco_detection(state_arc, &project_path, event_sink)?;
        workspace = project_store::load_exam_workspace_state(&project_path)?;
    }

    if workspace.aruco_status.total_marker_count <= 0 {
        stamp_current_template(state_arc, &project_path, event_sink)?;
        workspace = project_store::load_exam_workspace_state(&project_path)?;
    }

    let canonical = project_store::load_canonical_template_pdf_path(&project_path)?
        .ok_or_else(|| HostError::Project("No canonical template PDF is available.".into()))?;
    fs::copy(canonical, &destination_path)?;
    let mut app = state_arc.lock();
    app.current_project = Some(workspace.project.clone());
    Ok(workspace)
}

pub(crate) fn refresh_template_aruco_detection_if_ready(
    state_arc: &Arc<AppStateInner>,
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<()> {
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    if workspace.status == "failed" || workspace.template_preview_artifacts.is_empty() {
        return Ok(());
    }
    refresh_template_aruco_detection(state_arc, project_path, event_sink).map(|_| ())
}

fn validate_pdf_destination(destination_path: &Path) -> HostResult<()> {
    if destination_path
        .extension()
        .and_then(|value| value.to_str())
        != Some("pdf")
    {
        return Err(HostError::Validation(
            "Template export destination must use a .pdf extension.".into(),
        ));
    }
    if let Some(parent) = destination_path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn refresh_template_aruco_detection(
    state_arc: &Arc<AppStateInner>,
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<TemplateArucoStatus> {
    let template_pdf_path = project_store::load_canonical_template_pdf_path(project_path)?
        .ok_or_else(|| HostError::Project("No canonical template PDF is available.".into()))?;
    let completed = run_aruco_detect_job(state_arc, project_path, &template_pdf_path, event_sink)?;
    let status = parse_aruco_detect_status(&completed)?;
    project_store::update_template_aruco_status(project_path, status.clone())?;
    Ok(status)
}

fn run_aruco_detect_job(
    state_arc: &Arc<AppStateInner>,
    project_path: &Path,
    template_pdf_path: &Path,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<crate::worker::CompletedWorkerJob> {
    let reserved = start_runtime_job_when_idle(state_arc, event_sink, "scans.pdf-detect-aruco")?;
    let output_artifacts_dir =
        project_store::command_output_dir(project_path, "scans.pdf-detect-aruco", &reserved.job_id);
    fs::create_dir_all(&output_artifacts_dir)?;
    run_reserved_job(
        state_arc,
        event_sink,
        reserved,
        RuntimeJobRequest {
            command_name: "scans.pdf-detect-aruco",
            worker_request_payload: json!({
                "pdf_path": template_pdf_path.to_string_lossy().into_owned(),
            }),
            persisted_request_payload: json!({
                "template_artifact": "canonical_template_pdf",
            }),
            output_artifacts_dir: Some(&output_artifacts_dir),
            project_path: Some(project_path),
            stdin_bytes: None,
        },
    )
}

fn stamp_current_template(
    state_arc: &Arc<AppStateInner>,
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<()> {
    let template_pdf_path = project_store::load_canonical_template_pdf_path(project_path)?
        .ok_or_else(|| HostError::Project("No canonical template PDF is available.".into()))?;
    let completed = run_aruco_stamp_job(state_arc, project_path, &template_pdf_path, event_sink)?;
    let (stamped_pdf_path, rendered_page_paths) = parse_aruco_stamp_artifacts(&completed)?;
    let status = refresh_status_from_pdf(state_arc, project_path, &stamped_pdf_path, event_sink)?;
    project_store::replace_template_pdf_and_pages(
        project_path,
        &completed.job_id,
        &stamped_pdf_path,
        &rendered_page_paths,
        status,
    )
}

fn refresh_status_from_pdf(
    state_arc: &Arc<AppStateInner>,
    project_path: &Path,
    pdf_path: &Path,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<TemplateArucoStatus> {
    let completed = run_aruco_detect_job(state_arc, project_path, pdf_path, event_sink)?;
    parse_aruco_detect_status(&completed)
}

fn run_aruco_stamp_job(
    state_arc: &Arc<AppStateInner>,
    project_path: &Path,
    template_pdf_path: &Path,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<crate::worker::CompletedWorkerJob> {
    let reserved = start_runtime_job_when_idle(state_arc, event_sink, "scans.pdf-stamp-aruco")?;
    let output_artifacts_dir =
        project_store::command_output_dir(project_path, "scans.pdf-stamp-aruco", &reserved.job_id);
    fs::create_dir_all(&output_artifacts_dir)?;
    run_reserved_job(
        state_arc,
        event_sink,
        reserved,
        RuntimeJobRequest {
            command_name: "scans.pdf-stamp-aruco",
            worker_request_payload: json!({
                "input_pdf_path": template_pdf_path.to_string_lossy().into_owned(),
            }),
            persisted_request_payload: json!({
                "template_artifact": "canonical_template_pdf",
            }),
            output_artifacts_dir: Some(&output_artifacts_dir),
            project_path: Some(project_path),
            stdin_bytes: None,
        },
    )
}

fn start_runtime_job_when_idle(
    state_arc: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    command_name: &str,
) -> HostResult<super::runtime::ReservedJob> {
    let started_waiting = Instant::now();
    loop {
        match start_runtime_job(state_arc, event_sink, command_name) {
            Ok(reserved) => return Ok(reserved),
            Err(HostError::Conflict(_))
                if started_waiting.elapsed() < TEMPLATE_EXPORT_RUNTIME_JOB_WAIT_TIMEOUT =>
            {
                thread::sleep(TEMPLATE_EXPORT_RUNTIME_JOB_POLL_INTERVAL);
            }
            Err(HostError::Conflict(_)) => {
                return Err(HostError::Conflict(format!(
                    "Timed out waiting for the desktop worker before starting '{}'.",
                    command_name
                )));
            }
            Err(err) => return Err(err),
        }
    }
}

fn parse_aruco_detect_status(
    completed: &crate::worker::CompletedWorkerJob,
) -> HostResult<TemplateArucoStatus> {
    if completed.result.terminal_type != "job_finished" {
        return Err(HostError::Project(exam_setup_failure_message(
            &completed.result.envelope,
        )));
    }
    let data = completed
        .result
        .envelope
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| HostError::Protocol("ArUco detection response was missing data.".into()))?;
    let total_marker_count = data
        .get("total_marker_count")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let pages = data
        .get("pages")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .map(parse_aruco_page_status)
                .collect::<HostResult<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(TemplateArucoStatus {
        state: if total_marker_count > 0 {
            "detected".into()
        } else {
            "not_detected".into()
        },
        total_marker_count,
        pages,
    })
}

fn parse_aruco_page_status(value: &Value) -> HostResult<TemplateArucoPageStatus> {
    let object = value
        .as_object()
        .ok_or_else(|| HostError::Protocol("ArUco page status must be an object.".into()))?;
    let marker_ids = object
        .get("marker_ids")
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();
    Ok(TemplateArucoPageStatus {
        page_number: object
            .get("page_number")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        marker_count: object
            .get("marker_count")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        marker_ids,
    })
}

fn parse_aruco_stamp_artifacts(
    completed: &crate::worker::CompletedWorkerJob,
) -> HostResult<(PathBuf, Vec<PathBuf>)> {
    if completed.result.terminal_type != "job_finished" {
        return Err(HostError::Project(exam_setup_failure_message(
            &completed.result.envelope,
        )));
    }
    let artifacts = completed
        .result
        .envelope
        .get("artifacts")
        .and_then(Value::as_array)
        .ok_or_else(|| HostError::Protocol("ArUco stamp response was missing artifacts.".into()))?;
    let mut stamped_pdf_path = None;
    let mut rendered_pages: Vec<(i64, PathBuf)> = Vec::new();
    for artifact in artifacts {
        let role = artifact
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let path = artifact
            .get("path")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .ok_or_else(|| HostError::Protocol("ArUco stamp artifact was missing path.".into()))?;
        if role == "stamped_template_pdf" {
            stamped_pdf_path = Some(path);
        } else if role == "rendered_stamped_template_page" {
            let page_number = artifact
                .get("entity_scope")
                .and_then(Value::as_object)
                .and_then(|scope| scope.get("page_number"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            rendered_pages.push((page_number, path));
        }
    }
    rendered_pages.sort_by_key(|(page_number, _path)| *page_number);
    let stamped_pdf_path = stamped_pdf_path
        .ok_or_else(|| HostError::Protocol("ArUco stamp response did not include a PDF.".into()))?;
    Ok((
        stamped_pdf_path,
        rendered_pages
            .into_iter()
            .map(|(_page, path)| path)
            .collect(),
    ))
}
