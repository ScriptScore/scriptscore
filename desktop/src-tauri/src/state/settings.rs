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
            missing_runtime_data_error(
                &completed.result.envelope,
                "runtime.list-llm-models",
                "data.models",
            )
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
            missing_runtime_data_error(
                &completed.result.envelope,
                "runtime.validate-llm-model",
                "envelope data",
            )
        })?;

    serde_json::from_value::<LlmModelValidation>(raw_data).map_err(|err| {
        HostError::Protocol(format!(
            "runtime.validate-llm-model returned an invalid validation payload: {err}"
        ))
    })
}

fn missing_runtime_data_error(envelope: &Value, command_name: &str, field_name: &str) -> HostError {
    if let Some(message) = envelope
        .get("error")
        .and_then(Value::as_object)
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty())
    {
        return HostError::Protocol(format!("{command_name} failed: {message}"));
    }
    HostError::Protocol(format!("{command_name} response was missing {field_name}."))
}
