// SPDX-License-Identifier: AGPL-3.0-only
use std::path::PathBuf;
use std::sync::Arc;

use crate::errors::HostResult;
use crate::models::{AppSettings, RuntimeJobEvent, ShellState, WorkerStatus};

use super::{
    emit_runtime_job_event, project_lifecycle, AppStateInner, DesktopApp, RuntimeEventSink,
};

pub(super) fn current_worker_status(app: &DesktopApp) -> WorkerStatus {
    if app.worker.is_none() {
        if app.last_runtime_error.is_some() {
            WorkerStatus::Error
        } else {
            WorkerStatus::Busy
        }
    } else {
        app.scheduler.worker_status()
    }
}

pub(super) fn start_create_project_job(
    state: &Arc<AppStateInner>,
    input: &crate::models::CreateProjectInput,
    settings: AppSettings,
    event_sink: Arc<dyn RuntimeEventSink>,
) -> HostResult<String> {
    let job_id = uuid::Uuid::new_v4().to_string();
    let request_id = uuid::Uuid::new_v4().to_string();
    let job_id_clone = job_id.clone();
    let input_clone = input.clone();
    let settings_clone = settings.clone();
    let self_clone = Arc::clone(state);
    tauri::async_runtime::spawn_blocking(move || {
        emit_create_project_started(&*event_sink, &request_id, &job_id_clone);
        let result = project_lifecycle::create_project(&self_clone, input_clone, &*event_sink);
        match result {
            Ok(shell) => finish_create_project_success(
                &self_clone,
                &shell,
                &settings_clone,
                &event_sink,
                &request_id,
                &job_id_clone,
            ),
            Err(err) => emit_create_project_failed(
                &self_clone,
                &*event_sink,
                &request_id,
                &job_id_clone,
                &err.to_string(),
            ),
        }
    });
    Ok(job_id)
}

fn emit_create_project_started(event_sink: &dyn RuntimeEventSink, request_id: &str, job_id: &str) {
    emit_runtime_job_event(
        event_sink,
        RuntimeJobEvent {
            event_type: "job_started".into(),
            command_name: "create_project".into(),
            worker_status: WorkerStatus::Busy,
            request_id: Some(request_id.to_string()),
            job_id: Some(job_id.to_string()),
            payload: serde_json::json!({
                "resultKind": "shell",
                "data": null,
                "workspaceChanged": true,
                "nestedJobIds": []
            }),
        },
    );
}

fn finish_create_project_success(
    state: &Arc<AppStateInner>,
    shell: &ShellState,
    settings: &AppSettings,
    event_sink: &Arc<dyn RuntimeEventSink>,
    request_id: &str,
    job_id: &str,
) {
    emit_create_project_finished(&**event_sink, shell, request_id, job_id);
    let _ = super::lms_roster_cache::ensure_project_preload(state, settings);
    maybe_spawn_post_setup_background_jobs(state, shell, settings, event_sink);
}

fn emit_create_project_finished(
    event_sink: &dyn RuntimeEventSink,
    shell: &ShellState,
    request_id: &str,
    job_id: &str,
) {
    emit_runtime_job_event(
        event_sink,
        RuntimeJobEvent {
            event_type: "job_finished".into(),
            command_name: "create_project".into(),
            worker_status: WorkerStatus::Ready,
            request_id: Some(request_id.to_string()),
            job_id: Some(job_id.to_string()),
            payload: serde_json::json!({
                "resultKind": "shell",
                "data": serde_json::to_value(shell).unwrap_or_default(),
                "workspaceChanged": true,
                "nestedJobIds": []
            }),
        },
    );
}

fn maybe_spawn_post_setup_background_jobs(
    state: &Arc<AppStateInner>,
    shell: &ShellState,
    settings: &AppSettings,
    event_sink: &Arc<dyn RuntimeEventSink>,
) {
    let Some(project) = shell.current_project.as_ref() else {
        return;
    };
    spawn_post_setup_enqueue_job(
        Arc::clone(state),
        PathBuf::from(project.project_path.clone()),
        settings.clone(),
        Arc::clone(event_sink),
    );
}

fn emit_create_project_failed(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    request_id: &str,
    job_id: &str,
    message: &str,
) {
    let worker_status = {
        let app = state.lock();
        app.worker_status.clone()
    };
    emit_runtime_job_event(
        event_sink,
        RuntimeJobEvent {
            event_type: "job_failed".into(),
            command_name: "create_project".into(),
            worker_status,
            request_id: Some(request_id.to_string()),
            job_id: Some(job_id.to_string()),
            payload: serde_json::json!({
                "resultKind": "error",
                "message": message,
                "workspaceChanged": true,
                "nestedJobIds": [],
                "error": { "message": message, "nestedJobIds": [] }
            }),
        },
    );
}

pub(super) fn emit_runtime_error(
    event_sink: &dyn RuntimeEventSink,
    command_name: &str,
    message: &str,
) {
    emit_runtime_job_event(
        event_sink,
        RuntimeJobEvent {
            event_type: "runtime_error".into(),
            command_name: command_name.to_string(),
            worker_status: WorkerStatus::Error,
            request_id: None,
            job_id: None,
            payload: serde_json::json!({ "message": message }),
        },
    );
}

pub(super) fn spawn_post_setup_enqueue_job(
    state: Arc<AppStateInner>,
    project_path: PathBuf,
    settings: AppSettings,
    event_sink: Arc<dyn RuntimeEventSink>,
) {
    let state_work = Arc::clone(&state);
    let sink_work = Arc::clone(&event_sink);
    let sink_err = Arc::clone(&event_sink);
    tauri::async_runtime::spawn(async move {
        let join = tauri::async_runtime::spawn_blocking(move || {
            let _ = refresh_template_aruco_detection(&state_work, &project_path, &sink_work);
            if settings.ai_assist_categories.question_analysis {
                project_lifecycle::enqueue_post_setup_jobs(
                    &state_work,
                    project_path.as_path(),
                    &*sink_work,
                    &settings,
                )?;
            }
            Ok::<(), crate::errors::HostError>(())
        })
        .await;
        match join {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                emit_runtime_error(&*sink_err, "enqueue_post_setup_jobs", &err.to_string());
            }
            Err(_) => {}
        }
    });
}

pub(super) fn spawn_template_aruco_detection_job(
    state: Arc<AppStateInner>,
    project_path: PathBuf,
    event_sink: Arc<dyn RuntimeEventSink>,
) {
    tauri::async_runtime::spawn_blocking(move || {
        let _ = refresh_template_aruco_detection(&state, &project_path, &event_sink);
    });
}

fn refresh_template_aruco_detection(
    state: &Arc<AppStateInner>,
    project_path: &std::path::Path,
    event_sink: &Arc<dyn RuntimeEventSink>,
) -> HostResult<()> {
    super::template_export::refresh_template_aruco_detection_if_ready(
        state,
        project_path,
        &**event_sink,
    )
}
