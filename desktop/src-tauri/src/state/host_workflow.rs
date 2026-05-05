// SPDX-License-Identifier: AGPL-3.0-only
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::{json, Value};

use crate::errors::HostResult;
use crate::models::{RuntimeJobEvent, WorkerStatus};
use crate::project_store;

use super::{current_run_timestamp, emit_runtime_job_event, AppStateInner, RuntimeEventSink};

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostWorkflowResultKind {
    Workspace,
    Shell,
    Export,
    None,
}

impl HostWorkflowResultKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Shell => "shell",
            Self::Export => "export",
            Self::None => "none",
        }
    }
}

#[derive(Clone)]
struct CurrentHostWorkflow {
    workflow_id: String,
    nested_job_ids: Arc<Mutex<Vec<String>>>,
}

thread_local! {
    static CURRENT_HOST_WORKFLOW: RefCell<Option<CurrentHostWorkflow>> = const { RefCell::new(None) };
}

pub(crate) fn record_nested_worker_job(
    state: &Arc<AppStateInner>,
    child_job_id: &str,
    command_name: &str,
) {
    CURRENT_HOST_WORKFLOW.with(|slot| {
        let Some(current) = slot.borrow().clone() else {
            return;
        };
        current
            .nested_job_ids
            .lock()
            .expect("host workflow child lock")
            .push(child_job_id.to_string());
        let project_path = {
            let mut app = state.lock();
            app.host_workflow_children
                .entry(current.workflow_id.clone())
                .or_default()
                .push(child_job_id.to_string());
            app.current_project_path_optional()
        };
        if let Some(project_path) = project_path {
            let _ = project_store::insert_host_workflow_child_job(
                &project_path,
                &current.workflow_id,
                child_job_id,
                command_name,
            );
        }
    });
}

pub(crate) fn host_workflow_child_for_cancel(
    state: &Arc<AppStateInner>,
    workflow_id: &str,
) -> Option<String> {
    let app = state.lock();
    app.host_workflow_children
        .get(workflow_id)
        .and_then(|children| children.last())
        .cloned()
}

pub(crate) fn start_host_workflow_job<T, F>(
    state: Arc<AppStateInner>,
    command_name: &'static str,
    result_kind: HostWorkflowResultKind,
    workspace_changed: bool,
    event_sink: Arc<dyn RuntimeEventSink>,
    action: F,
) -> String
where
    T: Serialize + Send + 'static,
    F: FnOnce() -> HostResult<T> + Send + 'static,
{
    let workflow_id = uuid::Uuid::new_v4().to_string();
    let request_id = uuid::Uuid::new_v4().to_string();
    let submitted_at = current_run_timestamp();
    let nested_job_ids = Arc::new(Mutex::new(Vec::new()));
    persist_workflow_started(
        &state,
        &workflow_id,
        command_name,
        result_kind,
        &submitted_at,
    );
    emit_workflow_started(
        &*event_sink,
        command_name,
        result_kind,
        workspace_changed,
        &request_id,
        &workflow_id,
    );

    let workflow_id_clone = workflow_id.clone();
    let request_id_clone = request_id.clone();
    let nested_job_ids_clone = Arc::clone(&nested_job_ids);
    tauri::async_runtime::spawn_blocking(move || {
        let current = CurrentHostWorkflow {
            workflow_id: workflow_id_clone.clone(),
            nested_job_ids: Arc::clone(&nested_job_ids_clone),
        };
        let result = CURRENT_HOST_WORKFLOW.with(|slot| {
            let previous = slot.replace(Some(current));
            let result = action();
            slot.replace(previous);
            result
        });
        let nested = nested_snapshot(&nested_job_ids_clone);
        match result {
            Ok(data) => finish_workflow_success(
                WorkflowFinishContext {
                    state: &state,
                    event_sink: &*event_sink,
                    command_name,
                    result_kind,
                    workspace_changed,
                    request_id: &request_id_clone,
                    workflow_id: &workflow_id_clone,
                    nested_job_ids: &nested,
                },
                data,
            ),
            Err(err) => finish_workflow_failure(
                WorkflowFinishContext {
                    state: &state,
                    event_sink: &*event_sink,
                    command_name,
                    result_kind,
                    workspace_changed,
                    request_id: &request_id_clone,
                    workflow_id: &workflow_id_clone,
                    nested_job_ids: &nested,
                },
                &err.to_string(),
            ),
        }
    });
    workflow_id
}

struct WorkflowFinishContext<'a> {
    state: &'a Arc<AppStateInner>,
    event_sink: &'a dyn RuntimeEventSink,
    command_name: &'a str,
    result_kind: HostWorkflowResultKind,
    workspace_changed: bool,
    request_id: &'a str,
    workflow_id: &'a str,
    nested_job_ids: &'a [String],
}

fn nested_snapshot(nested_job_ids: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    nested_job_ids
        .lock()
        .expect("host workflow child lock")
        .clone()
}

fn current_project_path(state: &Arc<AppStateInner>) -> Option<PathBuf> {
    state.lock().current_project_path_optional()
}

fn persist_workflow_started(
    state: &Arc<AppStateInner>,
    workflow_id: &str,
    command_name: &str,
    result_kind: HostWorkflowResultKind,
    submitted_at: &str,
) {
    if let Some(project_path) = current_project_path(state) {
        let _ = project_store::insert_host_workflow(
            &project_path,
            workflow_id,
            command_name,
            result_kind.as_str(),
            submitted_at,
        );
    }
}

fn persist_workflow_completion(
    context: &WorkflowFinishContext<'_>,
    state_name: &str,
    result_json: Option<&str>,
    error_json: Option<&str>,
) {
    if let Some(project_path) = current_project_path(context.state) {
        let finished_at = current_run_timestamp();
        let _ = project_store::insert_host_workflow(
            &project_path,
            context.workflow_id,
            context.command_name,
            context.result_kind.as_str(),
            &finished_at,
        );
        let _ = project_store::complete_host_workflow(
            &project_path,
            context.workflow_id,
            state_name,
            &finished_at,
            context.workspace_changed,
            result_json,
            error_json,
        );
    }
    context
        .state
        .lock()
        .host_workflow_children
        .remove(context.workflow_id);
}

fn workflow_payload(
    result_kind: HostWorkflowResultKind,
    workspace_changed: bool,
    nested_job_ids: &[String],
    data: Value,
) -> Value {
    json!({
        "resultKind": result_kind.as_str(),
        "data": data,
        "workspaceChanged": workspace_changed,
        "nestedJobIds": nested_job_ids
    })
}

fn workflow_error_payload(
    workspace_changed: bool,
    nested_job_ids: &[String],
    message: &str,
) -> Value {
    json!({
        "resultKind": "error",
        "message": message,
        "workspaceChanged": workspace_changed,
        "nestedJobIds": nested_job_ids,
        "error": {
            "message": message,
            "nestedJobIds": nested_job_ids
        }
    })
}

fn emit_workflow_started(
    event_sink: &dyn RuntimeEventSink,
    command_name: &str,
    result_kind: HostWorkflowResultKind,
    workspace_changed: bool,
    request_id: &str,
    workflow_id: &str,
) {
    emit_runtime_job_event(
        event_sink,
        RuntimeJobEvent {
            event_type: "job_started".into(),
            command_name: command_name.into(),
            worker_status: WorkerStatus::Busy,
            request_id: Some(request_id.to_string()),
            job_id: Some(workflow_id.to_string()),
            payload: workflow_payload(result_kind, workspace_changed, &[], Value::Null),
        },
    );
}

fn finish_workflow_success<T: Serialize>(context: WorkflowFinishContext<'_>, data: T) {
    let data = serde_json::to_value(data).unwrap_or_default();
    let payload = workflow_payload(
        context.result_kind,
        context.workspace_changed,
        context.nested_job_ids,
        data,
    );
    persist_workflow_completion(&context, "completed", Some(&payload.to_string()), None);
    emit_runtime_job_event(
        context.event_sink,
        RuntimeJobEvent {
            event_type: "job_finished".into(),
            command_name: context.command_name.into(),
            worker_status: WorkerStatus::Ready,
            request_id: Some(context.request_id.to_string()),
            job_id: Some(context.workflow_id.to_string()),
            payload,
        },
    );
}

fn finish_workflow_failure(context: WorkflowFinishContext<'_>, message: &str) {
    let payload =
        workflow_error_payload(context.workspace_changed, context.nested_job_ids, message);
    persist_workflow_completion(&context, "failed", None, Some(&payload.to_string()));
    let worker_status = {
        let app = context.state.lock();
        app.worker_status.clone()
    };
    emit_runtime_job_event(
        context.event_sink,
        RuntimeJobEvent {
            event_type: "job_failed".into(),
            command_name: context.command_name.into(),
            worker_status,
            request_id: Some(context.request_id.to_string()),
            job_id: Some(context.workflow_id.to_string()),
            payload,
        },
    );
}
