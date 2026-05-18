// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::VecDeque;
use std::path::PathBuf;

use serde_json::Value;

use super::{emit_runtime_job_event, RuntimeEventSink};
use crate::errors::{HostError, HostResult};
use crate::models::{AppSettings, RuntimeJobEvent, WorkerJobSummary, WorkerStatus};

#[derive(Clone, Debug)]
pub(crate) struct QueuedRuntimeJob {
    pub job_id: String,
    pub request_id: String,
    pub command_name: String,
    pub worker_request_payload: Value,
    pub persisted_request_payload: Value,
    pub output_artifacts_dir: Option<PathBuf>,
    pub project_path: Option<PathBuf>,
    pub submitted_at: String,
    pub depends_on_job_id: Option<String>,
    pub skip_if_dependency_failed: bool,
    pub settings: Option<AppSettings>,
}

pub(crate) struct ActiveRuntimeJob {
    pub job: QueuedRuntimeJob,
    pub cancel_handle: crate::worker::WorkerCancelHandle,
    pub cancel_requested: bool,
    pub started_at: Option<String>,
}

pub(crate) struct RuntimeScheduler {
    pending_jobs: VecDeque<QueuedRuntimeJob>,
    active_jobs: Vec<ActiveRuntimeJob>,
    max_concurrent_jobs: usize,
    #[cfg(test)]
    test_active_jobs: bool,
}

impl Default for RuntimeScheduler {
    fn default() -> Self {
        Self {
            pending_jobs: VecDeque::new(),
            active_jobs: Vec::new(),
            max_concurrent_jobs: 1,
            #[cfg(test)]
            test_active_jobs: false,
        }
    }
}

pub(crate) fn question_id_from_worker_payload(payload: &Value) -> Option<String> {
    if let Some(question_id) = payload
        .get("question_id")
        .and_then(Value::as_str)
        .map(str::to_string)
    {
        return Some(question_id);
    }
    payload
        .get("question_targets")
        .and_then(Value::as_array)
        .and_then(|targets| match targets.as_slice() {
            [target] => target.get("question_id").and_then(Value::as_str),
            _ => None,
        })
        .map(str::to_string)
}

impl RuntimeScheduler {
    pub fn queue_job(&mut self, job: QueuedRuntimeJob, event_sink: &dyn RuntimeEventSink) {
        let mut payload = serde_json::json!({
            "queue_position": self.pending_jobs.len() + 1,
            "schedulerActiveJobs": self.active_job_count(),
            "schedulerActiveJobDetails": self.active_job_summaries(),
            "schedulerPendingJobs": self.pending_jobs.len() + 1
        });
        if let (Some(obj), Some(qid)) = (
            payload.as_object_mut(),
            question_id_from_worker_payload(&job.worker_request_payload),
        ) {
            obj.insert("questionId".into(), serde_json::json!(qid));
        }
        emit_runtime_job_event(
            event_sink,
            RuntimeJobEvent {
                event_type: "job_queued".into(),
                command_name: job.command_name.clone(),
                worker_status: WorkerStatus::Ready,
                request_id: Some(job.request_id.clone()),
                job_id: Some(job.job_id.clone()),
                payload,
            },
        );
        self.pending_jobs.push_back(job);
    }

    /// True if a `exam.generate-rubric` job for this question is pending or currently running.
    pub(crate) fn has_generate_rubric_for_question(&self, question_id: &str) -> bool {
        let matches_job = |job: &QueuedRuntimeJob| {
            job.command_name == "exam.generate-rubric"
                && question_id_from_worker_payload(&job.worker_request_payload).as_deref()
                    == Some(question_id)
        };
        self.pending_jobs.iter().any(matches_job)
            || self.active_jobs.iter().any(|a| matches_job(&a.job))
    }

    pub fn cancel_job(&mut self, job_id: &str) -> HostResult<()> {
        if let Some(idx) = self.pending_jobs.iter().position(|j| j.job_id == job_id) {
            self.pending_jobs.remove(idx);
            return Ok(());
        }
        if let Some(active) = self.active_jobs.iter_mut().find(|a| a.job.job_id == job_id) {
            if !active.cancel_requested {
                active.cancel_handle.cancel(&active.job.job_id)?;
                active.cancel_requested = true;
            }
            return Ok(());
        }
        Err(HostError::Validation(format!(
            "No active or queued job found with id '{}'.",
            job_id
        )))
    }

    pub fn has_active_jobs(&self) -> bool {
        #[cfg(test)]
        if self.test_active_jobs {
            return true;
        }
        !self.active_jobs.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn __test_set_active_jobs(&mut self, active: bool) {
        self.test_active_jobs = active;
    }

    pub fn worker_status(&self) -> WorkerStatus {
        if self.has_active_jobs() {
            WorkerStatus::Busy
        } else {
            WorkerStatus::Ready
        }
    }

    pub fn active_job_id(&self) -> Option<&str> {
        self.active_jobs.first().map(|a| a.job.job_id.as_str())
    }

    pub fn push_active_job(
        &mut self,
        job: QueuedRuntimeJob,
        cancel_handle: crate::worker::WorkerCancelHandle,
    ) {
        self.active_jobs.push(ActiveRuntimeJob {
            started_at: Some(job.submitted_at.clone()),
            job,
            cancel_handle,
            cancel_requested: false,
        });
    }

    pub fn mark_job_started(&mut self, job_id: &str, started_at: &str) {
        if let Some(active) = self.active_jobs.iter_mut().find(|a| a.job.job_id == job_id) {
            active.started_at = Some(started_at.to_string());
        }
    }

    pub fn on_job_complete(&mut self, job_id: &str) {
        self.active_jobs.retain(|a| a.job.job_id != job_id);
        self.promote_dependents(job_id);
    }

    pub fn on_job_failed(&mut self, job_id: &str) -> Vec<QueuedRuntimeJob> {
        self.active_jobs.retain(|a| a.job.job_id != job_id);
        let mut skipped = Vec::new();
        let mut retained = VecDeque::new();
        while let Some(job) = self.pending_jobs.pop_front() {
            if job.skip_if_dependency_failed && job.depends_on_job_id.as_deref() == Some(job_id) {
                skipped.push(job);
            } else {
                retained.push_back(job);
            }
        }
        self.pending_jobs = retained;
        self.promote_dependents(job_id);
        skipped
    }

    fn promote_dependents(&mut self, completed_job_id: &str) {
        for job in self.pending_jobs.iter_mut() {
            if job.depends_on_job_id.as_deref() == Some(completed_job_id) {
                job.depends_on_job_id = None;
            }
        }
    }

    pub fn should_dispatch_next(&self) -> bool {
        self.active_jobs.len() < self.max_concurrent_jobs
            && self
                .pending_jobs
                .iter()
                .any(|j| j.depends_on_job_id.is_none())
    }

    pub fn pop_next_job(&mut self) -> Option<QueuedRuntimeJob> {
        let idx = self
            .pending_jobs
            .iter()
            .position(|j| j.depends_on_job_id.is_none())?;
        Some(self.pending_jobs.remove(idx).unwrap())
    }

    pub(crate) fn active_job_count(&self) -> usize {
        self.active_jobs.len()
    }

    pub(crate) fn active_job_summaries(&self) -> Vec<WorkerJobSummary> {
        self.active_jobs
            .iter()
            .map(|active| WorkerJobSummary {
                job_id: active.job.job_id.clone(),
                command_name: active.job.command_name.clone(),
                started_at: active.started_at.clone(),
            })
            .collect()
    }

    pub(crate) fn pending_job_count(&self) -> usize {
        self.pending_jobs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::QueuedRuntimeJob;
    use super::RuntimeScheduler;

    fn queued_job(
        job_id: &str,
        depends_on_job_id: Option<&str>,
        skip_if_dependency_failed: bool,
    ) -> QueuedRuntimeJob {
        QueuedRuntimeJob {
            job_id: job_id.into(),
            request_id: format!("req-{job_id}"),
            command_name: "test.command".into(),
            worker_request_payload: serde_json::json!({}),
            persisted_request_payload: serde_json::json!({}),
            output_artifacts_dir: None,
            project_path: None,
            submitted_at: "0".into(),
            depends_on_job_id: depends_on_job_id.map(String::from),
            skip_if_dependency_failed,
            settings: None,
        }
    }

    #[test]
    fn question_id_from_worker_payload_reads_single_question_target() {
        let payload = serde_json::json!({
            "question_targets": [
                {
                    "question_id": "question_2",
                    "template_question_png_path": "/tmp/q2.png"
                }
            ]
        });

        assert_eq!(
            super::question_id_from_worker_payload(&payload).as_deref(),
            Some("question_2")
        );
    }

    #[test]
    fn completed_jobs_promote_dependents() {
        let mut scheduler = RuntimeScheduler::default();
        scheduler
            .pending_jobs
            .push_back(queued_job("analyze", None, false));
        scheduler
            .pending_jobs
            .push_back(queued_job("rubric", Some("analyze"), true));

        let next = scheduler.pop_next_job().expect("root job should dequeue");
        assert_eq!(next.job_id, "analyze");
        assert!(!scheduler.should_dispatch_next());

        scheduler.on_job_complete("analyze");
        assert!(scheduler.should_dispatch_next());

        let promoted = scheduler.pop_next_job().expect("dependent should promote");
        assert_eq!(promoted.job_id, "rubric");
        assert!(promoted.depends_on_job_id.is_none());
    }

    #[test]
    fn failed_jobs_drop_skip_dependents() {
        let mut scheduler = RuntimeScheduler::default();
        scheduler
            .pending_jobs
            .push_back(queued_job("analyze", None, false));
        scheduler
            .pending_jobs
            .push_back(queued_job("rubric", Some("analyze"), true));
        scheduler
            .pending_jobs
            .push_back(queued_job("followup", Some("analyze"), false));

        let next = scheduler.pop_next_job().expect("root job should dequeue");
        assert_eq!(next.job_id, "analyze");

        let skipped = scheduler.on_job_failed("analyze");
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].job_id, "rubric");

        let promoted = scheduler
            .pop_next_job()
            .expect("non-skipping dependent should remain");
        assert_eq!(promoted.job_id, "followup");
        assert!(promoted.depends_on_job_id.is_none());
        assert!(scheduler.pop_next_job().is_none());
    }

    #[test]
    fn failed_parent_drops_all_skip_dependents_that_shared_that_dependency() {
        let mut scheduler = RuntimeScheduler::default();
        scheduler
            .pending_jobs
            .push_back(queued_job("analyze", None, false));
        scheduler.pending_jobs.push_back(QueuedRuntimeJob {
            job_id: "rubric_a".into(),
            request_id: "req-a".into(),
            command_name: "exam.generate-rubric".into(),
            worker_request_payload: serde_json::json!({}),
            persisted_request_payload: serde_json::json!({}),
            output_artifacts_dir: None,
            project_path: None,
            submitted_at: "0".into(),
            depends_on_job_id: Some("analyze".into()),
            skip_if_dependency_failed: true,
            settings: None,
        });
        scheduler.pending_jobs.push_back(QueuedRuntimeJob {
            job_id: "rubric_b".into(),
            request_id: "req-b".into(),
            command_name: "exam.generate-rubric".into(),
            worker_request_payload: serde_json::json!({}),
            persisted_request_payload: serde_json::json!({}),
            output_artifacts_dir: None,
            project_path: None,
            submitted_at: "0".into(),
            depends_on_job_id: Some("analyze".into()),
            skip_if_dependency_failed: true,
            settings: None,
        });

        let next = scheduler.pop_next_job().expect("root");
        assert_eq!(next.job_id, "analyze");
        let skipped = scheduler.on_job_failed("analyze");

        assert_eq!(skipped.len(), 2);
        assert_eq!(skipped[0].job_id, "rubric_a");
        assert_eq!(skipped[1].job_id, "rubric_b");
        assert!(scheduler.pending_jobs.is_empty());
        assert!(scheduler.pop_next_job().is_none());
    }
}
