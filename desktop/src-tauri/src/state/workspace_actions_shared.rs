// SPDX-License-Identifier: AGPL-3.0-only
use serde_json::{json, Value};

use crate::errors::{HostError, HostResult};
use crate::models::{AppSettings, InstructorProfile, WorkspaceWarning};

pub(super) fn success_data(envelope: &Value) -> HostResult<&serde_json::Map<String, Value>> {
    envelope
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| HostError::Protocol("worker success envelope was missing data.".into()))
}

pub(super) fn parse_warnings(value: Option<&Value>) -> HostResult<Vec<WorkspaceWarning>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let warnings = value
        .as_array()
        .ok_or_else(|| HostError::Protocol("warning collection must be an array.".into()))?;
    warnings
        .iter()
        .map(|warning| {
            Ok(WorkspaceWarning {
                code: warning
                    .get("code")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                message: warning
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Unknown warning")
                    .to_string(),
                scope: warning.get("scope").and_then(|scope| {
                    if scope.is_null() {
                        None
                    } else {
                        Some(scope.to_string())
                    }
                }),
            })
        })
        .collect()
}

pub(super) fn required_string(value: &Value, field_name: &str) -> HostResult<String> {
    value
        .get(field_name)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| HostError::Protocol(format!("worker result row was missing {field_name}.")))
}

pub(super) fn current_timestamp() -> String {
    format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::from_secs(0))
            .as_secs()
    )
}

pub(super) fn llm_config_json(settings: &AppSettings) -> Value {
    let mut config = json!({
        "base_url": settings.llm_base_url,
        "model": settings.llm_model,
    });
    if let Some(api_key) = settings.llm_api_key.as_deref() {
        config["api_key"] = Value::String(api_key.to_string());
    }
    config
}

pub(super) fn llm_config_trace_json(settings: &AppSettings) -> Value {
    json!({
        "base_url": settings.llm_base_url,
        "model": settings.llm_model,
    })
}

pub(crate) fn cli_instructor_profile_json(profile: &InstructorProfile) -> Value {
    let additional = profile.additional_guidance.trim();
    json!({
        "grading_strictness": profile.grading_strictness,
        "syntax_leniency": profile.syntax_leniency,
        "ocr_tolerance": profile.ocr_tolerance,
        "partial_credit_style": profile.partial_credit_style,
        "feedback_style": profile.feedback_style,
        "additional_guidance": if additional.is_empty() {
            Value::Null
        } else {
            Value::String(additional.to_string())
        },
    })
}
