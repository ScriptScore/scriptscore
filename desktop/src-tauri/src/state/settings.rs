// SPDX-License-Identifier: AGPL-3.0-only
use std::sync::Arc;

use serde_json::Value;

use super::runtime::{run_reserved_job, start_runtime_job, RuntimeJobRequest};
use super::{AppStateInner, RuntimeEventSink};
use crate::errors::{HostError, HostResult};
use crate::models::{LlmModelValidation, VisionCapableModel};

pub(crate) fn list_llm_models(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    provider_name: String,
    base_url: String,
    api_key: Option<String>,
) -> HostResult<Vec<VisionCapableModel>> {
    let project_path = state.lock().current_project_path_optional();
    let request_payload = serde_json::json!({
        "providers": {
            "llm_provider": provider_name
        },
        "llm_discovery_config": {
            "base_url": base_url,
            "api_key": api_key
        },
        "required_capabilities": ["vision"]
    });
    let reserved = start_runtime_job(state, event_sink, "runtime.list-llm-models")?;
    let completed = run_reserved_job(
        state,
        event_sink,
        reserved,
        RuntimeJobRequest {
            command_name: "runtime.list-llm-models",
            worker_request_payload: request_payload.clone(),
            persisted_request_payload: request_payload,
            output_artifacts_dir: None,
            project_path: project_path.as_deref(),
            stdin_bytes: None,
        },
    )?;

    let raw_models = completed
        .result
        .envelope
        .get("data")
        .and_then(|value| value.get("models"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            HostError::Protocol("runtime.list-llm-models response was missing data.models.".into())
        })?;

    raw_models
        .iter()
        .cloned()
        .map(serde_json::from_value::<VisionCapableModel>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| {
            HostError::Protocol(format!(
                "runtime.list-llm-models returned an invalid model payload: {err}"
            ))
        })
}

pub(crate) fn validate_llm_model(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    provider_name: String,
    base_url: String,
    model: String,
    api_key: Option<String>,
) -> HostResult<LlmModelValidation> {
    let project_path = state.lock().current_project_path_optional();
    let request_payload = serde_json::json!({
        "providers": {
            "llm_provider": provider_name
        },
        "llm_discovery_config": {
            "base_url": base_url,
            "api_key": api_key
        },
        "model": model,
        "required_capabilities": ["vision"]
    });
    let reserved = start_runtime_job(state, event_sink, "runtime.validate-llm-model")?;
    let completed = run_reserved_job(
        state,
        event_sink,
        reserved,
        RuntimeJobRequest {
            command_name: "runtime.validate-llm-model",
            worker_request_payload: request_payload.clone(),
            persisted_request_payload: request_payload,
            output_artifacts_dir: None,
            project_path: project_path.as_deref(),
            stdin_bytes: None,
        },
    )?;

    let raw_data = completed
        .result
        .envelope
        .get("data")
        .cloned()
        .ok_or_else(|| {
            HostError::Protocol(
                "runtime.validate-llm-model response was missing envelope data.".into(),
            )
        })?;

    serde_json::from_value::<LlmModelValidation>(raw_data).map_err(|err| {
        HostError::Protocol(format!(
            "runtime.validate-llm-model returned an invalid validation payload: {err}"
        ))
    })
}
