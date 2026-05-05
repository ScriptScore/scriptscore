// SPDX-License-Identifier: AGPL-3.0-only
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use super::super::scheduler::QueuedRuntimeJob;
use super::super::{AppStateInner, RuntimeEventSink};
use super::execution::{reserve_worker_for_queued_job, run_reserved_job};
use super::messages::emit_queued_job_skipped;
use super::RuntimeJobRequest;

pub(super) fn dispatch_next_queued_job(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
) {
    let job = {
        let mut app = state.lock();
        if !app.scheduler.should_dispatch_next() {
            return;
        }
        match app.scheduler.pop_next_job() {
            Some(job) => job,
            None => return,
        }
    };

    if job.command_name == "exam.generate-rubric" {
        dispatch_queued_generate_rubric(state, event_sink, job);
        return;
    }

    let job_id = job.job_id.clone();
    let command_name = job.command_name.clone();

    let reserved = match reserve_worker_for_queued_job(state, event_sink, &job) {
        Ok(r) => r,
        Err(err) => {
            let msg = err.to_string();
            fail_popped_queued_job(state, event_sink, &command_name, &job_id, &msg);
            return;
        }
    };

    let cancel_handle = match reserved.cancel_handle() {
        Ok(h) => h,
        Err(err) => {
            let msg = err.to_string();
            fail_popped_queued_job(state, event_sink, &command_name, &job_id, &msg);
            return;
        }
    };
    state
        .lock()
        .scheduler
        .push_active_job(job.clone(), cancel_handle);

    let outcome = run_reserved_job(
        state,
        event_sink,
        reserved,
        RuntimeJobRequest {
            command_name: &job.command_name,
            worker_request_payload: job.worker_request_payload,
            persisted_request_payload: job.persisted_request_payload,
            output_artifacts_dir: job.output_artifacts_dir.as_deref(),
            project_path: job.project_path.as_deref(),
            stdin_bytes: None,
        },
    );

    if let Err(err) = &outcome {
        state.emit_runtime_error(event_sink, &job.command_name, &err.to_string());
    }

    dispatch_next_if_ready(state, event_sink);
}

fn dispatch_next_if_ready(state: &Arc<AppStateInner>, event_sink: &dyn RuntimeEventSink) {
    if state.lock().scheduler.should_dispatch_next() {
        dispatch_next_queued_job(state, event_sink);
    }
}

fn fail_popped_queued_job(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    command_name: &str,
    job_id: &str,
    message: &str,
) {
    state.emit_runtime_error(event_sink, command_name, message);
    let skipped = state.lock().scheduler.on_job_failed(job_id);
    for skipped_job in skipped {
        emit_queued_job_skipped(event_sink, &skipped_job, job_id);
    }
    dispatch_next_if_ready(state, event_sink);
}

fn parse_queued_generate_rubric_inputs(
    job: &QueuedRuntimeJob,
) -> Result<(PathBuf, crate::models::AppSettings, String, bool), &'static str> {
    let project_path = job
        .project_path
        .as_ref()
        .ok_or("No project path for rubric generation.")?
        .clone();
    let settings = job
        .settings
        .as_ref()
        .ok_or("No settings for rubric generation.")?
        .clone();
    let question_id = job
        .worker_request_payload
        .get("question_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if question_id.is_empty() {
        return Err("Queued rubric job was missing question_id.");
    }
    let replace_existing = job
        .worker_request_payload
        .get("replace_existing")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    Ok((
        project_path,
        settings,
        question_id.to_string(),
        replace_existing,
    ))
}

fn dispatch_queued_generate_rubric(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    job: QueuedRuntimeJob,
) {
    let job_id_owned = job.job_id.clone();
    let command_name = job.command_name.clone();
    let (project_path, settings, question_id, replace_existing) =
        match parse_queued_generate_rubric_inputs(&job) {
            Ok(parts) => parts,
            Err(msg) => {
                fail_popped_queued_job(state, event_sink, &command_name, &job_id_owned, msg);
                return;
            }
        };

    let reserved = match reserve_worker_for_queued_job(state, event_sink, &job) {
        Ok(r) => r,
        Err(err) => {
            let msg = err.to_string();
            fail_popped_queued_job(state, event_sink, &command_name, &job_id_owned, &msg);
            return;
        }
    };

    let cancel_handle = match reserved.cancel_handle() {
        Ok(h) => h,
        Err(err) => {
            let msg = err.to_string();
            fail_popped_queued_job(state, event_sink, &command_name, &job_id_owned, &msg);
            return;
        }
    };
    state.lock().scheduler.push_active_job(job, cancel_handle);

    let prepared = match crate::state::workspace_actions::prepare_generate_rubric_for_job(
        &project_path,
        &question_id,
        replace_existing,
        &settings,
        &reserved.job_id,
    ) {
        Ok(p) => p,
        Err(err) => {
            fail_popped_queued_job(
                state,
                event_sink,
                &command_name,
                &job_id_owned,
                &err.to_string(),
            );
            return;
        }
    };

    if let Err(err) = fs::create_dir_all(&prepared.output_artifacts_dir) {
        fail_popped_queued_job(
            state,
            event_sink,
            &command_name,
            &job_id_owned,
            &err.to_string(),
        );
        return;
    }

    let outcome = run_reserved_job(
        state,
        event_sink,
        reserved,
        RuntimeJobRequest {
            command_name: "exam.generate-rubric",
            worker_request_payload: prepared.worker_request_payload,
            persisted_request_payload: prepared.persisted_request_payload,
            output_artifacts_dir: Some(prepared.output_artifacts_dir.as_path()),
            project_path: Some(project_path.as_path()),
            stdin_bytes: None,
        },
    );

    if let Err(err) = &outcome {
        state.emit_runtime_error(event_sink, &command_name, &err.to_string());
    }

    dispatch_next_if_ready(state, event_sink);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AppSettings;

    fn queued_rubric_job(worker_request_payload: serde_json::Value) -> QueuedRuntimeJob {
        QueuedRuntimeJob {
            job_id: "job-1".into(),
            request_id: "req-1".into(),
            command_name: "exam.generate-rubric".into(),
            worker_request_payload,
            persisted_request_payload: serde_json::json!({}),
            output_artifacts_dir: None,
            project_path: Some(PathBuf::from("/tmp/project")),
            submitted_at: "0".into(),
            depends_on_job_id: None,
            skip_if_dependency_failed: false,
            settings: Some(AppSettings::default()),
        }
    }

    #[test]
    fn queued_generate_rubric_requires_question_id() {
        let err = parse_queued_generate_rubric_inputs(&queued_rubric_job(serde_json::json!({})))
            .expect_err("missing question id should fail");
        assert_eq!(err, "Queued rubric job was missing question_id.");
    }

    #[test]
    fn queued_generate_rubric_defaults_replace_existing_true() {
        let (_, _, question_id, replace_existing) = parse_queued_generate_rubric_inputs(
            &queued_rubric_job(serde_json::json!({ "question_id": "q1" })),
        )
        .expect("queued rubric inputs should parse");
        assert_eq!(question_id, "q1");
        assert!(replace_existing);
    }
}
