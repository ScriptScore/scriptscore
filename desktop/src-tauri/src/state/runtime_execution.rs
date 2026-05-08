// SPDX-License-Identifier: AGPL-3.0-only
use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Value};

use super::super::job_history::{persist_job_completion, persist_job_submission};
use super::super::scheduler::{question_id_from_worker_payload, QueuedRuntimeJob};
use super::super::{current_run_timestamp, AppStateInner, DesktopApp, RuntimeEventSink};
use super::dispatch::dispatch_next_queued_job;
use super::messages::{
    emit_job_submitted, emit_job_submitted_with_payload, emit_queued_job_skipped,
    emit_terminal_runtime_event, handle_worker_message,
};
use super::{ReservedJob, RuntimeJobRequest};
use crate::errors::{HostError, HostResult};
use crate::models::SmokePingResult;
use crate::worker::CompletedWorkerJob;

pub(super) fn start_runtime_job(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    command_name: &str,
) -> HostResult<ReservedJob> {
    let app = state.lock();
    if app.scheduler.has_active_jobs() {
        return Err(HostError::Conflict(
            "A desktop job is already active in this session.".into(),
        ));
    }
    drop(app);
    let reserved = reserve_worker_for_job(state, event_sink, command_name)?;
    {
        let mut app = state.lock();
        let placeholder_job = QueuedRuntimeJob {
            job_id: reserved.job_id.clone(),
            request_id: reserved.request_id.clone(),
            command_name: command_name.to_string(),
            worker_request_payload: serde_json::json!({}),
            persisted_request_payload: serde_json::json!({}),
            output_artifacts_dir: None,
            project_path: None,
            submitted_at: reserved.submitted_at.clone(),
            depends_on_job_id: None,
            skip_if_dependency_failed: false,
            settings: None,
        };
        app.scheduler
            .push_active_job(placeholder_job, reserved.worker.cancel_handle()?);
    }
    Ok(reserved)
}

pub(super) fn reserve_worker_for_job(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    command_name: &str,
) -> HostResult<ReservedJob> {
    let mut worker = {
        let mut app = state.lock();
        match take_worker_for_runtime_job(&mut app) {
            Ok(worker) => {
                if updates_global_runtime_error(command_name) {
                    app.last_runtime_error = None;
                }
                worker
            }
            Err(err) => {
                let message = err.to_string();
                drop(app);
                state.emit_runtime_error(event_sink, command_name, &message);
                return Err(err);
            }
        }
    };
    let (request_id, job_id) = worker.reserve_job_ids();
    let submitted_at = current_run_timestamp();
    emit_job_submitted(event_sink, command_name, &request_id, &job_id);
    super::super::host_workflow::record_nested_worker_job(state, &job_id, command_name);
    Ok(ReservedJob {
        worker,
        request_id,
        job_id,
        submitted_at,
    })
}

pub(super) fn reserve_worker_for_queued_job(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    job: &QueuedRuntimeJob,
) -> HostResult<ReservedJob> {
    let worker = {
        let mut app = state.lock();
        match take_worker_for_runtime_job(&mut app) {
            Ok(worker) => {
                app.last_runtime_error = None;
                worker
            }
            Err(err) => {
                let message = err.to_string();
                drop(app);
                state.emit_runtime_error(event_sink, &job.command_name, &message);
                return Err(err);
            }
        }
    };
    let mut submitted_payload = json!({});
    if let Some(obj) = submitted_payload.as_object_mut() {
        if let Some(qid) = question_id_from_worker_payload(&job.worker_request_payload) {
            obj.insert("questionId".into(), json!(qid));
        }
    }
    emit_job_submitted_with_payload(
        event_sink,
        &job.command_name,
        &job.request_id,
        &job.job_id,
        submitted_payload,
    );
    super::super::host_workflow::record_nested_worker_job(state, &job.job_id, &job.command_name);
    Ok(ReservedJob {
        worker,
        request_id: job.request_id.clone(),
        job_id: job.job_id.clone(),
        submitted_at: job.submitted_at.clone(),
    })
}

fn take_worker_for_runtime_job(
    app: &mut std::sync::MutexGuard<'_, super::super::DesktopApp>,
) -> HostResult<crate::worker::WorkerClient> {
    app.ensure_worker()?;
    let worker = app.take_worker()?;
    if let Err(err) = worker.cancel_handle() {
        app.worker = Some(worker);
        app.last_runtime_error = Some(err.to_string());
        return Err(err);
    }
    Ok(worker)
}

pub(super) fn run_reserved_job(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    reserved: ReservedJob,
    request: RuntimeJobRequest<'_>,
) -> HostResult<CompletedWorkerJob> {
    let request_id = reserved.request_id.clone();
    let job_id = reserved.job_id.clone();
    if let Err(err) = persist_submission_if_requested(&request, &reserved) {
        return finish_runtime_job(
            state,
            event_sink,
            reserved.worker,
            request.command_name,
            &request_id,
            &job_id,
            Err(err),
        );
    }

    let ReservedJob {
        mut worker,
        request_id: reserved_request_id,
        job_id: reserved_job_id,
        submitted_at: _,
    } = reserved;
    let run_result = execute_worker_job(
        state,
        event_sink,
        &mut worker,
        &reserved_request_id,
        &reserved_job_id,
        &request,
    );
    let run_result = persist_successful_run(state, event_sink, &request, run_result);
    finish_runtime_job(
        state,
        event_sink,
        worker,
        request.command_name,
        &request_id,
        &job_id,
        run_result,
    )
}

fn persist_submission_if_requested(
    request: &RuntimeJobRequest<'_>,
    reserved: &ReservedJob,
) -> HostResult<()> {
    let Some(project_path) = request.project_path else {
        return Ok(());
    };
    persist_job_submission(
        project_path,
        request.command_name,
        &request.persisted_request_payload,
        &reserved.request_id,
        &reserved.job_id,
        &reserved.submitted_at,
    )
}

fn execute_worker_job(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    worker: &mut crate::worker::WorkerClient,
    request_id: &str,
    job_id: &str,
    request: &RuntimeJobRequest<'_>,
) -> HostResult<CompletedWorkerJob> {
    let project_path_buf = request.project_path.map(Path::to_path_buf);
    let command_name_owned = request.command_name.to_string();
    let persisted_request_payload = request.persisted_request_payload.clone();
    let state_for_messages = Arc::clone(state);
    worker.run_job(
        request_id.to_string(),
        job_id.to_string(),
        request.command_name,
        request.worker_request_payload.clone(),
        request.output_artifacts_dir,
        request.stdin_bytes,
        move |message| {
            handle_worker_message(
                &state_for_messages,
                event_sink,
                &command_name_owned,
                project_path_buf.as_deref(),
                &persisted_request_payload,
                message,
            )
        },
    )
}

fn persist_successful_run(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    request: &RuntimeJobRequest<'_>,
    run_result: HostResult<CompletedWorkerJob>,
) -> HostResult<CompletedWorkerJob> {
    let completed = run_result?;
    persist_success_outputs_if_needed(request, &completed)?;
    persist_completion_if_requested(state, event_sink, request, &completed)?;
    persist_exam_analyze_failure_state(request, &completed)?;
    Ok(completed)
}

fn persist_exam_analyze_failure_state(
    request: &RuntimeJobRequest<'_>,
    completed: &CompletedWorkerJob,
) -> HostResult<()> {
    if request.command_name != "exam.analyze" {
        return Ok(());
    }
    let terminal = completed.result.terminal_type.as_str();
    if terminal != "job_failed" && terminal != "job_cancelled" {
        return Ok(());
    }
    let Some(project_path) = request.project_path else {
        return Ok(());
    };
    super::super::workspace_actions::persist_exam_analyze_batch_failure(
        project_path,
        completed,
        &request.persisted_request_payload,
    )
}

fn persist_success_outputs_if_needed(
    request: &RuntimeJobRequest<'_>,
    completed: &CompletedWorkerJob,
) -> HostResult<()> {
    if completed.result.terminal_type != "job_finished" {
        return Ok(());
    }
    let Some(project_path) = request.project_path else {
        return Ok(());
    };
    super::super::workspace_actions::persist_host_outputs_after_worker_success(
        request.command_name,
        project_path,
        completed,
        &request.persisted_request_payload,
    )
}

fn persist_completion_if_requested(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    request: &RuntimeJobRequest<'_>,
    completed: &CompletedWorkerJob,
) -> HostResult<()> {
    let Some(project_path) = request.project_path else {
        return Ok(());
    };
    if let Err(err) = persist_job_completion(project_path, completed) {
        state.emit_runtime_error(event_sink, request.command_name, &err.to_string());
        return Err(err);
    }
    Ok(())
}

pub(super) fn finish_runtime_job(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    worker: crate::worker::WorkerClient,
    command_name: &str,
    request_id: &str,
    job_id: &str,
    outcome: HostResult<CompletedWorkerJob>,
) -> HostResult<CompletedWorkerJob> {
    let (has_pending, skipped_jobs) = {
        let mut app = state.lock();
        app.worker = Some(worker);
        let skipped = apply_runtime_job_outcome(&mut app, command_name, job_id, &outcome);
        (app.scheduler.should_dispatch_next(), skipped)
    };

    emit_skipped_dependents(event_sink, job_id, &skipped_jobs);

    if let Ok(completed) = &outcome {
        emit_terminal_runtime_event(
            state,
            event_sink,
            command_name,
            request_id,
            job_id,
            completed,
        );
    }

    if has_pending {
        dispatch_next_queued_job(state, event_sink);
    }

    outcome
}

fn apply_runtime_job_outcome(
    app: &mut DesktopApp,
    command_name: &str,
    job_id: &str,
    outcome: &HostResult<CompletedWorkerJob>,
) -> Vec<QueuedRuntimeJob> {
    let updates_global_error = updates_global_runtime_error(command_name);
    match outcome {
        Ok(completed) => apply_completed_runtime_job(app, job_id, completed, updates_global_error),
        Err(err) => apply_failed_runtime_job(app, job_id, err, updates_global_error),
    }
}

fn apply_completed_runtime_job(
    app: &mut DesktopApp,
    job_id: &str,
    completed: &CompletedWorkerJob,
    updates_global_error: bool,
) -> Vec<QueuedRuntimeJob> {
    if completed.result.terminal_type == "job_finished" {
        apply_finished_runtime_job(app, job_id, updates_global_error);
        return Vec::new();
    }
    apply_failed_worker_result(app, job_id, completed, updates_global_error)
}

fn apply_finished_runtime_job(app: &mut DesktopApp, job_id: &str, updates_global_error: bool) {
    app.scheduler.on_job_complete(job_id);
    if updates_global_error {
        app.last_runtime_error = None;
    }
}

fn apply_failed_worker_result(
    app: &mut DesktopApp,
    job_id: &str,
    completed: &CompletedWorkerJob,
    updates_global_error: bool,
) -> Vec<QueuedRuntimeJob> {
    let skipped = app.scheduler.on_job_failed(job_id);
    if updates_global_error {
        app.last_runtime_error = worker_result_error_message(completed);
    }
    skipped
}

fn apply_failed_runtime_job(
    app: &mut DesktopApp,
    job_id: &str,
    err: &HostError,
    updates_global_error: bool,
) -> Vec<QueuedRuntimeJob> {
    let skipped = app.scheduler.on_job_failed(job_id);
    if updates_global_error {
        app.last_runtime_error = Some(err.to_string());
    }
    skipped
}

fn worker_result_error_message(completed: &CompletedWorkerJob) -> Option<String> {
    completed
        .result
        .envelope
        .get("error")
        .and_then(Value::as_object)
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn updates_global_runtime_error(command_name: &str) -> bool {
    !matches!(
        command_name,
        "runtime.list-llm-models" | "runtime.validate-llm-model"
    )
}

fn emit_skipped_dependents(
    event_sink: &dyn RuntimeEventSink,
    dependency_job_id: &str,
    skipped_jobs: &[QueuedRuntimeJob],
) {
    for job in skipped_jobs {
        persist_skipped_dependent(job, dependency_job_id);
        emit_queued_job_skipped(event_sink, job, dependency_job_id);
    }
}

fn persist_skipped_dependent(job: &QueuedRuntimeJob, dependency_job_id: &str) {
    let Some(project_path) = job.project_path.as_deref() else {
        return;
    };
    let _ = persist_job_submission(
        project_path,
        &job.command_name,
        &job.persisted_request_payload,
        &job.request_id,
        &job.job_id,
        &job.submitted_at,
    );
    let finished_at = current_run_timestamp();
    let completion = crate::models::JobCompletion {
        state: "skipped".into(),
        finished_at,
        result_json: None,
        error_json: Some(
            json!({
                "message": "Queued job skipped because its dependency failed.",
                "code": "dependency_failed",
                "dependencyJobId": dependency_job_id
            })
            .to_string(),
        ),
    };
    let _ = crate::project_store::complete_job(project_path, &job.job_id, &completion);
}

pub(super) fn run_smoke_ping(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<SmokePingResult> {
    let completed = run_smoke_ping_job(
        state,
        event_sink,
        serde_json::json!({"message": "desktop runtime ready", "steps": 1}),
    )?;
    let data = completed
        .result
        .envelope
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            HostError::Protocol("smoke.ping response was missing envelope data.".into())
        })?;
    Ok(SmokePingResult {
        command: completed
            .result
            .envelope
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("smoke.ping")
            .to_string(),
        message: data
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("desktop runtime ready")
            .to_string(),
        steps: data.get("steps").and_then(Value::as_i64).unwrap_or(1),
        event_count: completed.result.events.len(),
    })
}

pub(super) fn run_smoke_ping_job(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    request_payload: Value,
) -> HostResult<CompletedWorkerJob> {
    let project_path = state.lock().current_project_path_optional();
    let reserved = start_runtime_job(state, event_sink, "smoke.ping")?;
    let completed = run_reserved_job(
        state,
        event_sink,
        reserved,
        RuntimeJobRequest {
            command_name: "smoke.ping",
            worker_request_payload: request_payload.clone(),
            persisted_request_payload: request_payload,
            output_artifacts_dir: None,
            project_path: project_path.as_deref(),
            stdin_bytes: None,
        },
    )?;

    if let Some(project_path) = project_path.as_deref() {
        let mut app = state.lock();
        app.refresh_current_project(project_path)?;
    }
    Ok(completed)
}

#[cfg(test)]
mod tests {
    use super::updates_global_runtime_error;

    #[test]
    fn background_provider_probes_do_not_update_global_runtime_error() {
        assert!(!updates_global_runtime_error("runtime.list-llm-models"));
        assert!(!updates_global_runtime_error("runtime.validate-llm-model"));
        assert!(updates_global_runtime_error("exam.analyze"));
    }
}
