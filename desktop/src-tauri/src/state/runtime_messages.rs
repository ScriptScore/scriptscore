// SPDX-License-Identifier: AGPL-3.0-only
use std::path::Path;
use std::sync::Arc;

use serde_json::Value;

use super::super::job_history::{persist_job_progress, persist_job_started};
use super::super::{emit_runtime_job_event, AppStateInner, RuntimeEventSink};
use crate::errors::{HostError, HostResult};
use crate::models::{RuntimeJobEvent, WorkerStatus};
use crate::protocol::ProtocolMessage;
use crate::worker::CompletedWorkerJob;

pub(super) fn emit_job_submitted(
    event_sink: &dyn RuntimeEventSink,
    command_name: &str,
    request_id: &str,
    job_id: &str,
) {
    emit_job_submitted_with_payload(
        event_sink,
        command_name,
        request_id,
        job_id,
        serde_json::json!({}),
    );
}

pub(super) fn emit_job_submitted_with_payload(
    event_sink: &dyn RuntimeEventSink,
    command_name: &str,
    request_id: &str,
    job_id: &str,
    payload: Value,
) {
    emit_runtime_job_event(
        event_sink,
        RuntimeJobEvent {
            event_type: "job_submitted".into(),
            command_name: command_name.to_string(),
            worker_status: WorkerStatus::Busy,
            request_id: Some(request_id.to_string()),
            job_id: Some(job_id.to_string()),
            payload,
        },
    );
}

pub(super) fn terminal_worker_status(terminal_type: &str) -> WorkerStatus {
    match terminal_type {
        "job_finished" | "job_failed" | "job_cancelled" | "job_skipped" => WorkerStatus::Ready,
        _ => WorkerStatus::Ready,
    }
}

pub(super) fn emit_queued_job_skipped(
    event_sink: &dyn RuntimeEventSink,
    job: &super::super::scheduler::QueuedRuntimeJob,
    dependency_job_id: &str,
) {
    let mut payload = serde_json::json!({
        "resultKind": "error",
        "message": "Queued job skipped because its dependency failed.",
        "code": "dependency_failed",
        "dependencyJobId": dependency_job_id,
        "error": {
            "message": "Queued job skipped because its dependency failed.",
            "code": "dependency_failed",
            "dependencyJobId": dependency_job_id
        }
    });
    merge_question_id_from_request(&mut payload, &job.worker_request_payload);
    merge_request_scope(&mut payload, &job.persisted_request_payload);
    emit_runtime_job_event(
        event_sink,
        RuntimeJobEvent {
            event_type: "job_skipped".into(),
            command_name: job.command_name.clone(),
            worker_status: WorkerStatus::Ready,
            request_id: Some(job.request_id.clone()),
            job_id: Some(job.job_id.clone()),
            payload,
        },
    );
}

pub(super) fn emit_terminal_runtime_event(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    command_name: &str,
    request_id: &str,
    job_id: &str,
    completed: &CompletedWorkerJob,
) {
    let mut payload = completed.result.terminal_payload.clone();
    merge_scheduler_snapshot(state, &mut payload);
    emit_runtime_job_event(
        event_sink,
        RuntimeJobEvent {
            event_type: completed.result.terminal_type.clone(),
            command_name: command_name.to_string(),
            worker_status: terminal_worker_status(&completed.result.terminal_type),
            request_id: Some(request_id.to_string()),
            job_id: Some(job_id.to_string()),
            payload,
        },
    );
}

pub(super) fn merge_scheduler_snapshot(state: &Arc<AppStateInner>, payload: &mut Value) {
    let (active, active_jobs, pending, intake_active, intake_redact_total, intake_redact_index) = {
        let app = state.lock();
        (
            app.scheduler.active_job_count(),
            app.scheduler.active_job_summaries(),
            app.scheduler.pending_job_count(),
            app.intake_pipeline_active,
            app.intake_pipeline_redact_total,
            app.intake_pipeline_redact_index,
        )
    };
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("schedulerActiveJobs".into(), serde_json::json!(active));
        obj.insert(
            "schedulerActiveJobDetails".into(),
            serde_json::json!(active_jobs),
        );
        obj.insert("schedulerPendingJobs".into(), serde_json::json!(pending));
        if intake_active {
            obj.insert("intakePipelineActive".into(), serde_json::json!(true));
            obj.insert(
                "intakePipelineRedactTotal".into(),
                serde_json::json!(intake_redact_total),
            );
            obj.insert(
                "intakePipelineRedactIndex".into(),
                serde_json::json!(intake_redact_index),
            );
        }
    }
}

fn merge_question_id_from_request(payload: &mut Value, persisted_request_payload: &Value) {
    if let Some(qid) = persisted_request_payload
        .get("question_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        if let Some(object) = payload.as_object_mut() {
            object.insert("questionId".into(), serde_json::json!(qid));
        }
    }
}

fn merge_request_scope(payload: &mut Value, persisted_request_payload: &Value) {
    merge_question_id_from_request(payload, persisted_request_payload);
    let Some(student_ref) = persisted_request_payload
        .get("student_ref")
        .or_else(|| persisted_request_payload.get("studentRef"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return;
    };
    let Some(object) = payload.as_object_mut() else {
        return;
    };
    object.insert("studentRef".into(), serde_json::json!(student_ref));
    object.insert("student_ref".into(), serde_json::json!(student_ref));
    if let Some(scope) = object
        .entry("scope")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
    {
        scope.insert("student_ref".into(), serde_json::json!(student_ref));
    }
}

pub(super) fn handle_worker_message(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    command_name: &str,
    project_path: Option<&Path>,
    persisted_request_payload: &Value,
    message: &ProtocolMessage,
) -> HostResult<()> {
    match message.message_type.as_str() {
        "job_started" => {
            let job_id = message.job_id.as_deref().ok_or_else(|| {
                HostError::Protocol("Worker job_started message was missing job_id.".into())
            })?;
            let started_at = message
                .payload
                .get("timestamp")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    HostError::Protocol("Worker job_started message was missing timestamp.".into())
                })?;
            state.lock().scheduler.mark_job_started(job_id, started_at);
            if let Some(project_path) = project_path {
                persist_job_started(project_path, job_id, started_at)?;
            }
        }
        "job_progress" => {
            if let Some(project_path) = project_path {
                let job_id = message.job_id.as_deref().ok_or_else(|| {
                    HostError::Protocol("Worker job_progress message was missing job_id.".into())
                })?;
                persist_job_progress(project_path, job_id, &message.payload)?;
            }
        }
        _ => {}
    }

    let worker_status = match message.message_type.as_str() {
        "job_started" | "job_progress" => WorkerStatus::Busy,
        "job_finished" | "job_failed" | "job_cancelled" | "job_skipped" => WorkerStatus::Ready,
        _ => WorkerStatus::Ready,
    };
    let mut payload = message.payload.clone();
    merge_scheduler_snapshot(state, &mut payload);
    merge_request_scope(&mut payload, persisted_request_payload);
    emit_runtime_job_event(
        event_sink,
        RuntimeJobEvent {
            event_type: message.message_type.clone(),
            command_name: command_name.to_string(),
            worker_status,
            request_id: message.request_id.clone(),
            job_id: message.job_id.clone(),
            payload,
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{InstructorProfile, WorkerJobResult};
    use crate::project_store;
    use crate::state::current_run_timestamp;
    use crate::state::job_history::persist_job_submission;
    use crate::state::scheduler::RuntimeScheduler;
    use crate::state::DesktopApp;
    use crate::test_support::{lock_env_vars, EnvVarGuard};
    use rusqlite::Connection;
    use std::sync::Mutex;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[derive(Clone, Default)]
    struct RecordingEventSink {
        events: Arc<Mutex<Vec<RuntimeJobEvent>>>,
    }

    impl RecordingEventSink {
        fn snapshot(&self) -> Vec<RuntimeJobEvent> {
            self.events.lock().expect("event sink lock").clone()
        }
    }

    impl RuntimeEventSink for RecordingEventSink {
        fn emit_runtime_event(&self, event: RuntimeJobEvent) {
            self.events.lock().expect("event sink lock").push(event);
        }
    }

    fn temp_root(prefix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_millis()
        ))
    }

    fn create_project_root(prefix: &str) -> std::path::PathBuf {
        let test_root = temp_root(prefix);
        std::fs::create_dir_all(&test_root).expect("test root should exist");
        test_root
    }

    fn create_project_summary(test_root: &std::path::Path) -> crate::models::ProjectSummary {
        let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", test_root);
        project_store::create_project(
            "Runtime Test",
            None,
            None,
            None,
            &InstructorProfile::default(),
        )
        .expect("project should be created")
    }

    fn state_without_worker() -> Arc<AppStateInner> {
        Arc::new(AppStateInner {
            inner: Mutex::new(DesktopApp {
                current_project: None,
                debug_features: Default::default(),
                lms_roster_cache: Default::default(),
                worker_status: WorkerStatus::Ready,
                last_runtime_error: None,
                worker: None,
                app_handle: None,
                scheduler: RuntimeScheduler::default(),
                host_workflow_children: Default::default(),
                intake_pipeline_active: false,
                intake_pipeline_redact_total: 0,
                intake_pipeline_redact_index: 0,
            }),
        })
    }

    #[test]
    fn merge_scheduler_snapshot_adds_scheduler_and_intake_fields() {
        let state = state_without_worker();
        {
            let mut app = state.lock();
            app.intake_pipeline_active = true;
            app.intake_pipeline_redact_total = 3;
            app.intake_pipeline_redact_index = 1;
        }
        let mut payload = serde_json::json!({});
        merge_scheduler_snapshot(&state, &mut payload);

        assert_eq!(payload["schedulerActiveJobs"], 0);
        assert_eq!(payload["schedulerPendingJobs"], 0);
        assert_eq!(payload["intakePipelineActive"], true);
        assert_eq!(payload["intakePipelineRedactTotal"], 3);
        assert_eq!(payload["intakePipelineRedactIndex"], 1);
    }

    #[test]
    fn handle_worker_message_persists_started_and_emits_snapshot() {
        let _guard = lock_env_vars();
        let test_root = create_project_root("scriptscore-runtime-messages");
        let created = create_project_summary(&test_root);
        let project_path = std::path::PathBuf::from(&created.project_path);
        let state = state_without_worker();
        let sink = RecordingEventSink::default();

        persist_job_submission(
            &project_path,
            "smoke.ping",
            &serde_json::json!({}),
            "req-1",
            "job-1",
            &current_run_timestamp(),
        )
        .expect("job submission should persist");

        let message = ProtocolMessage {
            message_type: "job_started".into(),
            request_id: Some("req-1".into()),
            job_id: Some("job-1".into()),
            payload: serde_json::json!({
                "timestamp": "12345",
            }),
        };

        handle_worker_message(
            &state,
            &sink,
            "smoke.ping",
            Some(project_path.as_path()),
            &serde_json::json!({ "student_ref": "student_1" }),
            &message,
        )
        .expect("message should handle");

        let connection = Connection::open(project_store::schema::project_db_path(&project_path))
            .expect("project db should open");
        let started_at: String = connection
            .query_row(
                "SELECT started_at FROM job_run WHERE job_id = ?1",
                ["job-1"],
                |row| row.get(0),
            )
            .expect("started_at should query");
        assert_eq!(started_at, "12345");

        let events = sink.snapshot();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "job_started");
        assert_eq!(events[0].payload["schedulerActiveJobs"], 0);
        assert_eq!(events[0].payload["studentRef"], "student_1");
        assert_eq!(events[0].payload["scope"]["student_ref"], "student_1");

        drop(connection);
        std::fs::remove_dir_all(&test_root).expect("test project should clean up");
    }

    #[test]
    fn handle_worker_message_merges_question_id_from_persisted_payload() {
        let state = state_without_worker();
        let sink = RecordingEventSink::default();
        let message = ProtocolMessage {
            message_type: "job_started".into(),
            request_id: Some("req-gr".into()),
            job_id: Some("job-gr".into()),
            payload: serde_json::json!({ "timestamp": "99" }),
        };
        handle_worker_message(
            &state,
            &sink,
            "exam.generate-rubric",
            None,
            &serde_json::json!({ "question_id": "q-a" }),
            &message,
        )
        .expect("message should handle");
        let events = sink.snapshot();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload["questionId"], "q-a");
    }

    #[test]
    fn emit_terminal_runtime_event_uses_ready_status_for_failed_and_cancelled() {
        let state = state_without_worker();
        let sink = RecordingEventSink::default();
        for terminal_type in ["job_failed", "job_cancelled"] {
            emit_terminal_runtime_event(
                &state,
                &sink,
                "scans.parse",
                "req-1",
                "job-1",
                &CompletedWorkerJob {
                    job_id: "job-1".into(),
                    result: WorkerJobResult {
                        terminal_type: terminal_type.into(),
                        terminal_payload: serde_json::json!({}),
                        envelope: serde_json::json!({}),
                        events: Vec::new(),
                    },
                },
            );
        }
        let events = sink.snapshot();
        assert_eq!(events.len(), 2);
        assert!(events
            .iter()
            .all(|event| matches!(event.worker_status, WorkerStatus::Ready)));
    }
}
