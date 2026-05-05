// SPDX-License-Identifier: AGPL-3.0-only
use std::path::Path;

use serde_json::Value;

use crate::errors::HostResult;
use crate::models::{JobCompletion, JobProgressRecord, JobRunRecord};
use crate::project_store;
use crate::worker::CompletedWorkerJob;

pub(crate) fn persist_job_submission(
    project_path: &Path,
    command_name: &str,
    request_payload: &Value,
    request_id: &str,
    job_id: &str,
    submitted_at: &str,
) -> HostResult<()> {
    project_store::insert_job_run(
        project_path,
        &JobRunRecord {
            job_id: job_id.to_string(),
            command_name: command_name.to_string(),
            request_id: request_id.to_string(),
            state: "submitted".into(),
            submitted_at: submitted_at.to_string(),
            started_at: None,
            finished_at: None,
            request_json: request_payload.to_string(),
            result_json: None,
            error_json: None,
        },
    )
}

pub(crate) fn persist_job_started(
    project_path: &Path,
    job_id: &str,
    started_at: &str,
) -> HostResult<()> {
    project_store::mark_job_running(project_path, job_id, started_at)
}

pub(crate) fn persist_job_progress(
    project_path: &Path,
    job_id: &str,
    event: &Value,
) -> HostResult<()> {
    project_store::append_job_event(
        project_path,
        job_id,
        &JobProgressRecord {
            sequence: event.get("sequence").and_then(Value::as_i64).unwrap_or(0),
            event_type: event
                .get("event")
                .and_then(Value::as_str)
                .unwrap_or("progress")
                .to_string(),
            progress_json: maybe_json(event.get("progress")),
            scope_json: maybe_json(event.get("scope")),
            data_json: event
                .get("data")
                .cloned()
                .unwrap_or_else(|| Value::Object(Default::default()))
                .to_string(),
            created_at: event
                .get("timestamp")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        },
    )
}

pub(crate) fn persist_job_completion(
    project_path: &Path,
    completed: &CompletedWorkerJob,
) -> HostResult<()> {
    let finished_at = finished_at(&completed.result.envelope);
    let completion = if completed.result.terminal_type == "job_finished" {
        JobCompletion {
            state: "succeeded".into(),
            finished_at,
            result_json: Some(completed.result.envelope.to_string()),
            error_json: None,
        }
    } else {
        JobCompletion {
            state: if completed.result.terminal_type == "job_cancelled" {
                "cancelled".into()
            } else {
                "failed".into()
            },
            finished_at,
            result_json: None,
            error_json: completed
                .result
                .envelope
                .get("error")
                .cloned()
                .map(|value| value.to_string()),
        }
    };
    project_store::complete_job(project_path, &completed.job_id, &completion)
}

fn finished_at(envelope: &Value) -> String {
    envelope
        .get("timing")
        .and_then(Value::as_object)
        .and_then(|timing| timing.get("finished_at"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn maybe_json(value: Option<&Value>) -> Option<String> {
    value.filter(|item| !item.is_null()).map(Value::to_string)
}
