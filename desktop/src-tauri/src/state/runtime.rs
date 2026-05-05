// SPDX-License-Identifier: AGPL-3.0-only
use std::path::Path;
use std::sync::Arc;

use serde_json::Value;

use crate::errors::HostResult;
use crate::models::SmokePingResult;
use crate::worker::CompletedWorkerJob;

#[path = "runtime_dispatch.rs"]
mod dispatch;
#[path = "runtime_execution.rs"]
mod execution;
#[path = "runtime_messages.rs"]
mod messages;

pub(crate) struct ReservedJob {
    pub(crate) worker: crate::worker::WorkerClient,
    pub(crate) request_id: String,
    pub(crate) job_id: String,
    pub(crate) submitted_at: String,
}

impl ReservedJob {
    pub(crate) fn cancel_handle(&self) -> HostResult<crate::worker::WorkerCancelHandle> {
        self.worker.cancel_handle()
    }
}

pub(crate) struct RuntimeJobRequest<'a> {
    pub(crate) command_name: &'a str,
    pub(crate) worker_request_payload: Value,
    pub(crate) persisted_request_payload: Value,
    pub(crate) output_artifacts_dir: Option<&'a Path>,
    pub(crate) project_path: Option<&'a Path>,
    pub(crate) stdin_bytes: Option<&'a [u8]>,
}

pub(crate) fn start_runtime_job(
    state: &Arc<super::AppStateInner>,
    event_sink: &dyn super::RuntimeEventSink,
    command_name: &str,
) -> HostResult<ReservedJob> {
    execution::start_runtime_job(state, event_sink, command_name)
}

pub(crate) fn run_reserved_job(
    state: &Arc<super::AppStateInner>,
    event_sink: &dyn super::RuntimeEventSink,
    reserved: ReservedJob,
    request: RuntimeJobRequest<'_>,
) -> HostResult<CompletedWorkerJob> {
    execution::run_reserved_job(state, event_sink, reserved, request)
}

pub(crate) fn finish_runtime_job(
    state: &Arc<super::AppStateInner>,
    event_sink: &dyn super::RuntimeEventSink,
    worker: crate::worker::WorkerClient,
    command_name: &str,
    request_id: &str,
    job_id: &str,
    outcome: HostResult<CompletedWorkerJob>,
) -> HostResult<CompletedWorkerJob> {
    execution::finish_runtime_job(
        state,
        event_sink,
        worker,
        command_name,
        request_id,
        job_id,
        outcome,
    )
}

pub(crate) fn dispatch_next_queued_job(
    state: &Arc<super::AppStateInner>,
    event_sink: &dyn super::RuntimeEventSink,
) {
    dispatch::dispatch_next_queued_job(state, event_sink);
}

pub(crate) fn run_smoke_ping(
    state: &Arc<super::AppStateInner>,
    event_sink: &dyn super::RuntimeEventSink,
) -> HostResult<SmokePingResult> {
    execution::run_smoke_ping(state, event_sink)
}

pub(crate) fn run_smoke_ping_job(
    state: &Arc<super::AppStateInner>,
    event_sink: &dyn super::RuntimeEventSink,
    request_payload: Value,
) -> HostResult<CompletedWorkerJob> {
    execution::run_smoke_ping_job(state, event_sink, request_payload)
}
