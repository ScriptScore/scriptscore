// SPDX-License-Identifier: AGPL-3.0-only
use serde_json::{json, Value};

use crate::errors::{HostError, HostResult};
use crate::models::{
    AppSettings, StudentWorkflowState, StudentWorkflowSubmission, WorkspaceWarning,
};

pub(super) fn fail_submission(submission: &mut StudentWorkflowSubmission, message: &str) {
    submission.stage = "failed".into();
    submission.failure_message = Some(message.to_string());
}

pub(super) fn mark_submission_failed(
    workflow_state: &mut StudentWorkflowState,
    student_ref: &str,
    message: &str,
) -> HostResult<()> {
    let submission = find_submission_mut(workflow_state, student_ref)?;
    fail_submission(submission, message);
    Ok(())
}

pub(super) fn find_submission_mut<'a>(
    workflow_state: &'a mut StudentWorkflowState,
    student_ref: &str,
) -> HostResult<&'a mut StudentWorkflowSubmission> {
    workflow_state
        .submissions
        .iter_mut()
        .find(|submission| submission.student_ref == student_ref)
        .ok_or_else(|| {
            HostError::Validation(format!(
                "Student workflow state was missing '{}'.",
                student_ref
            ))
        })
}

pub(super) fn success_data(envelope: &Value) -> HostResult<&serde_json::Map<String, Value>> {
    if let Some(data) = envelope.get("data").and_then(Value::as_object) {
        return Ok(data);
    }
    if let Some(error) = envelope.get("error").and_then(Value::as_object) {
        if let Some(message) = command_error_message(error) {
            return Err(HostError::Protocol(format!("Command failed: {message}")));
        }
    }
    Err(HostError::Protocol(
        "Command success envelope was missing data.".into(),
    ))
}

fn command_error_message(error: &serde_json::Map<String, Value>) -> Option<String> {
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty());
    let issues = error
        .get("details")
        .and_then(Value::as_object)
        .and_then(|details| details.get("issues"))
        .and_then(Value::as_array);
    let issue_messages = issues
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.get("message").and_then(Value::as_str))
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let issue_summary = match issue_messages.as_slice() {
        [] => None,
        [only] => Some((*only).to_string()),
        [first, rest @ ..] => Some(format!("{first} (+{} more issue(s))", rest.len())),
    };
    match (message, issue_summary) {
        (Some("Request payload is invalid."), Some(issue)) => Some(issue),
        (Some(message), Some(issue)) if issue != message => Some(format!("{message} {issue}")),
        (Some(message), _) => Some(message.to_string()),
        (None, Some(issue)) => Some(issue),
        (None, None) => None,
    }
}

pub(super) fn required_array<'a>(
    value: &'a serde_json::Map<String, Value>,
    field_name: &str,
) -> HostResult<&'a [Value]> {
    value
        .get(field_name)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| HostError::Protocol(format!("Command result was missing {field_name}.")))
}

pub(super) fn parse_warnings(value: Option<&Value>) -> HostResult<Vec<WorkspaceWarning>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let warnings = value
        .as_array()
        .ok_or_else(|| HostError::Protocol("warnings must be an array.".into()))?;
    warnings
        .iter()
        .map(|warning| {
            let code = warning
                .get("code")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let message = warning
                .get("message")
                .and_then(Value::as_str)
                .ok_or_else(|| HostError::Protocol("warning rows must include a message.".into()))?
                .to_string();
            let scope = warning.get("scope").and_then(|scope| {
                if scope.is_null() {
                    None
                } else if let Some(text) = scope.as_str() {
                    Some(text.to_string())
                } else {
                    serde_json::to_string(scope).ok()
                }
            });
            Ok(WorkspaceWarning {
                code,
                message,
                scope,
            })
        })
        .collect()
}

pub(super) fn required_string(value: &Value, field_name: &str) -> HostResult<String> {
    value
        .get(field_name)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| HostError::Protocol(format!("Result row was missing {field_name}.")))
}

pub(super) fn required_i64(value: &Value, field_name: &str) -> HostResult<i64> {
    value
        .get(field_name)
        .and_then(Value::as_i64)
        .ok_or_else(|| HostError::Protocol(format!("Result row was missing {field_name}.")))
}

pub(super) fn required_string_object(
    value: &serde_json::Map<String, Value>,
    field_name: &str,
) -> HostResult<String> {
    value
        .get(field_name)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| HostError::Protocol(format!("Result row was missing {field_name}.")))
}

pub(super) fn required_i64_object(
    value: &serde_json::Map<String, Value>,
    field_name: &str,
) -> HostResult<i64> {
    value
        .get(field_name)
        .and_then(Value::as_i64)
        .ok_or_else(|| HostError::Protocol(format!("Result row was missing {field_name}.")))
}

pub(super) fn required_f64_object(
    value: &serde_json::Map<String, Value>,
    field_name: &str,
) -> HostResult<f64> {
    value
        .get(field_name)
        .and_then(Value::as_f64)
        .ok_or_else(|| HostError::Protocol(format!("Result row was missing {field_name}.")))
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

pub(super) fn persisted_llm_request_payload(settings: &AppSettings, extra: Value) -> Value {
    let mut payload = extra;
    if let Some(object) = payload.as_object_mut() {
        object.insert(
            "providers".into(),
            json!({ "llm_provider": settings.llm_provider }),
        );
        object.insert(
            "llm_config".into(),
            json!({
                "base_url": settings.llm_base_url,
                "model": settings.llm_model,
            }),
        );
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::success_data;

    #[test]
    fn success_data_reports_error_envelope_message() {
        let err = success_data(&serde_json::json!({
            "error": {
                "message": "Request payload is invalid."
            }
        }))
        .expect_err("error envelopes should not look like missing data");

        assert_eq!(
            err.to_string(),
            "Command failed: Request payload is invalid."
        );
    }

    #[test]
    fn success_data_prefers_validation_issue_message_when_top_level_error_is_generic() {
        let err = success_data(&serde_json::json!({
            "error": {
                "message": "Request payload is invalid.",
                "details": {
                    "issues": [
                        {
                            "code": "value_error",
                            "message": "total_points_awarded=11 must be within [0, question_max_points=10] for question_id='q4', student_ref='student_2'.",
                            "path": ["feedback_requests", 6]
                        }
                    ]
                }
            }
        }))
        .expect_err("error envelopes should surface structured validation detail");

        assert_eq!(
            err.to_string(),
            "Command failed: total_points_awarded=11 must be within [0, question_max_points=10] for question_id='q4', student_ref='student_2'."
        );
    }
}
