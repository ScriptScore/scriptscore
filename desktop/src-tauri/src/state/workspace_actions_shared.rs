// SPDX-License-Identifier: AGPL-3.0-only
use serde_json::{json, Map, Value};

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
    let mut profile_json = Map::new();
    if profile.enabled_tags.grading_strictness {
        profile_json.insert(
            "grading_strictness".into(),
            Value::String(profile.grading_strictness.clone()),
        );
    }
    if profile.enabled_tags.syntax_leniency {
        profile_json.insert(
            "syntax_leniency".into(),
            Value::String(profile.syntax_leniency.clone()),
        );
    }
    if profile.enabled_tags.ocr_tolerance {
        profile_json.insert(
            "ocr_tolerance".into(),
            Value::String(profile.ocr_tolerance.clone()),
        );
    }
    if profile.enabled_tags.partial_credit_style {
        profile_json.insert(
            "partial_credit_style".into(),
            Value::String(profile.partial_credit_style.clone()),
        );
    }
    if profile.enabled_tags.feedback_style {
        profile_json.insert(
            "feedback_style".into(),
            Value::String(profile.feedback_style.clone()),
        );
    }
    profile_json.insert(
        "additional_guidance".into(),
        if additional.is_empty() {
            Value::Null
        } else {
            Value::String(additional.to_string())
        },
    );
    Value::Object(profile_json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::InstructorProfileEnabledTags;

    #[test]
    fn cli_instructor_profile_json_omits_disabled_dimensions() {
        let profile = InstructorProfile {
            syntax_leniency: "high".into(),
            ocr_tolerance: "high".into(),
            partial_credit_style: "generous".into(),
            additional_guidance: "Use terse notes.".into(),
            ..Default::default()
        };

        let rendered = cli_instructor_profile_json(&profile);

        assert_eq!(rendered["grading_strictness"], "balanced");
        assert_eq!(rendered["feedback_style"], "brief");
        assert_eq!(rendered["additional_guidance"], "Use terse notes.");
        assert!(rendered.get("syntax_leniency").is_none());
        assert!(rendered.get("ocr_tolerance").is_none());
        assert!(rendered.get("partial_credit_style").is_none());
    }

    #[test]
    fn cli_instructor_profile_json_includes_enabled_dimensions() {
        let profile = InstructorProfile {
            enabled_tags: InstructorProfileEnabledTags {
                syntax_leniency: true,
                ocr_tolerance: true,
                partial_credit_style: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let rendered = cli_instructor_profile_json(&profile);

        assert_eq!(rendered["syntax_leniency"], "medium");
        assert_eq!(rendered["ocr_tolerance"], "medium");
        assert_eq!(rendered["partial_credit_style"], "balanced");
    }
}
