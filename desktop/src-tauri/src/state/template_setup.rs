// SPDX-License-Identifier: AGPL-3.0-only
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::errors::{HostError, HostResult};
use crate::models::{
    ArtifactRecord, QuestionRecord, TemplatePageArtifactSummary, TemplateQuestionRegion,
    WorkspaceWarning,
};
use crate::worker::CompletedWorkerJob;

type ExamSetupSuccess = (
    Vec<TemplatePageArtifactSummary>,
    Vec<ArtifactRecord>,
    Vec<QuestionRecord>,
    Vec<WorkspaceWarning>,
);

pub(crate) fn parse_exam_setup_success(
    project_path: &Path,
    completed: &CompletedWorkerJob,
) -> HostResult<ExamSetupSuccess> {
    let data = success_data(&completed.result.envelope)?;
    let warnings = parse_workspace_warnings(&completed.result.envelope)?;
    let (mut page_artifacts, mut artifact_records) = parse_template_pages(
        project_path,
        completed,
        required_array(data, "template_pages")?,
    )?;
    let (mut questions, question_artifacts) =
        parse_question_rows(project_path, completed, required_array(data, "questions")?)?;
    artifact_records.extend(question_artifacts);

    page_artifacts.sort_by_key(|artifact| artifact.page_number);
    questions.sort_by(|left, right| {
        left.question_number
            .cmp(&right.question_number)
            .then_with(|| left.question_id.cmp(&right.question_id))
    });
    Ok((page_artifacts, artifact_records, questions, warnings))
}

pub(crate) fn exam_setup_failure_message(envelope: &Value) -> String {
    envelope
        .get("error")
        .and_then(Value::as_object)
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("Template setup failed.")
        .to_string()
}

pub(crate) fn run_token() -> String {
    format!(
        "setup_{:x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_millis()
    )
}

fn required_i64(value: &Value, field_name: &str) -> HostResult<i64> {
    value
        .get(field_name)
        .and_then(Value::as_i64)
        .ok_or_else(|| HostError::Protocol(format!("exam.setup row was missing {field_name}.")))
}

fn required_string(value: &Value, field_name: &str) -> HostResult<String> {
    value
        .get(field_name)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| HostError::Protocol(format!("exam.setup row was missing {field_name}.")))
}

fn success_data(envelope: &Value) -> HostResult<&serde_json::Map<String, Value>> {
    envelope
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| HostError::Protocol("exam.setup success envelope was missing data.".into()))
}

fn required_array<'a>(
    value: &'a serde_json::Map<String, Value>,
    field_name: &str,
) -> HostResult<&'a [Value]> {
    value
        .get(field_name)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| {
            HostError::Protocol(format!(
                "exam.setup success envelope was missing {field_name}."
            ))
        })
}

fn parse_template_pages(
    project_path: &Path,
    completed: &CompletedWorkerJob,
    template_pages: &[Value],
) -> HostResult<(Vec<TemplatePageArtifactSummary>, Vec<ArtifactRecord>)> {
    let mut page_artifacts = Vec::with_capacity(template_pages.len());
    let mut artifact_records = Vec::with_capacity(template_pages.len());
    for page in template_pages {
        let page_summary = parse_template_page(project_path, completed, page)?;
        artifact_records.push(page_summary.1);
        page_artifacts.push(page_summary.0);
    }
    Ok((page_artifacts, artifact_records))
}

fn parse_template_page(
    project_path: &Path,
    completed: &CompletedWorkerJob,
    page: &Value,
) -> HostResult<(TemplatePageArtifactSummary, ArtifactRecord)> {
    let page_number = page
        .get("page_number")
        .and_then(Value::as_i64)
        .ok_or_else(|| HostError::Protocol("Template page was missing page_number.".into()))?;
    let image_path = page
        .get("image_path")
        .and_then(Value::as_str)
        .ok_or_else(|| HostError::Protocol("Template page was missing image_path.".into()))?;
    let absolute_path = PathBuf::from(image_path);
    let artifact_id = format!("template_page_{}_{}", completed.job_id, page_number);
    let label = format!("Page {}", page_number);
    let summary = TemplatePageArtifactSummary {
        artifact_id: artifact_id.clone(),
        page_number,
        image_path: absolute_path.to_string_lossy().into_owned(),
        label: label.clone(),
    };
    let record = ArtifactRecord {
        artifact_id,
        job_id: Some(completed.job_id.clone()),
        kind: "image".into(),
        role: "rendered_template_page".into(),
        relative_path: relative_project_path(project_path, &absolute_path)?,
        mime_type: Some("image/png".into()),
        byte_size: Some(fs::metadata(&absolute_path)?.len() as i64),
        metadata_json: Some(
            serde_json::json!({"page_number": page_number, "label": label}).to_string(),
        ),
    };
    Ok((summary, record))
}

fn parse_question_rows(
    project_path: &Path,
    completed: &CompletedWorkerJob,
    question_rows: &[Value],
) -> HostResult<(Vec<QuestionRecord>, Vec<ArtifactRecord>)> {
    let mut questions = Vec::with_capacity(question_rows.len());
    let mut artifact_records = Vec::with_capacity(question_rows.len());
    for question in question_rows {
        let parsed = parse_question_row(project_path, completed, question)?;
        questions.push(parsed.0);
        artifact_records.push(parsed.1);
    }
    Ok((questions, artifact_records))
}

fn parse_question_row(
    project_path: &Path,
    completed: &CompletedWorkerJob,
    question: &Value,
) -> HostResult<(QuestionRecord, ArtifactRecord)> {
    let question_id = required_string(question, "question_id")?;
    let question_number = required_i64(question, "question_number")?;
    let page_number = required_i64(question, "page_number")?;
    let max_points = required_i64(question, "max_points")?;
    let baseline_pdf_text = required_string(question, "baseline_pdf_text")?;
    let template_question_png_path = required_string(question, "template_question_png_path")?;
    let region = match question.get("region") {
        Some(value) => parse_question_region(value)?,
        None => None,
    };
    let artifact_id = format!("template_question_{}_{}", completed.job_id, question_id);
    let absolute_path = PathBuf::from(&template_question_png_path);
    let question_record = QuestionRecord {
        question_id: question_id.clone(),
        question_number,
        page_number,
        max_points: Some(max_points),
        text: baseline_pdf_text.clone(),
        baseline_pdf_text,
        region: region.clone(),
        source_artifact_id: Some(artifact_id.clone()),
        image_path: Some(template_question_png_path),
        analysis: Default::default(),
        rubric: Default::default(),
    };
    let artifact_record = ArtifactRecord {
        artifact_id,
        job_id: Some(completed.job_id.clone()),
        kind: "image".into(),
        role: "template_question".into(),
        relative_path: relative_project_path(project_path, &absolute_path)?,
        mime_type: Some("image/png".into()),
        byte_size: Some(fs::metadata(&absolute_path)?.len() as i64),
        metadata_json: Some(
            serde_json::json!({
                "question_id": question_id,
                "question_number": question_number,
                "page_number": page_number,
                "region": region,
            })
            .to_string(),
        ),
    };
    Ok((question_record, artifact_record))
}

fn parse_question_region(value: &Value) -> HostResult<Option<TemplateQuestionRegion>> {
    if value.is_null() {
        return Ok(None);
    }
    let object = value.as_object().ok_or_else(|| {
        HostError::Protocol("exam.setup question region must be an object.".into())
    })?;
    Ok(Some(TemplateQuestionRegion {
        x: object.get("x").and_then(Value::as_i64).ok_or_else(|| {
            HostError::Protocol("exam.setup question region was missing x.".into())
        })?,
        y: object.get("y").and_then(Value::as_i64).ok_or_else(|| {
            HostError::Protocol("exam.setup question region was missing y.".into())
        })?,
        width: object.get("width").and_then(Value::as_i64).ok_or_else(|| {
            HostError::Protocol("exam.setup question region was missing width.".into())
        })?,
        height: object
            .get("height")
            .and_then(Value::as_i64)
            .ok_or_else(|| {
                HostError::Protocol("exam.setup question region was missing height.".into())
            })?,
    }))
}

fn relative_project_path(project_path: &Path, absolute_path: &Path) -> HostResult<String> {
    crate::path_utils::relative_existing_path(project_path, absolute_path)
        .map(|path| path.to_string_lossy().into_owned())
        .ok_or_else(|| {
            HostError::Project(format!(
                "Artifact '{}' is outside the project directory '{}'.",
                absolute_path.display(),
                project_path.display()
            ))
        })
}

fn parse_workspace_warnings(envelope: &Value) -> HostResult<Vec<WorkspaceWarning>> {
    let Some(warnings) = envelope.get("warnings") else {
        return Ok(Vec::new());
    };
    let warnings = warnings
        .as_array()
        .ok_or_else(|| HostError::Protocol("exam.setup warnings must be an array.".into()))?;
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
                .ok_or_else(|| {
                    HostError::Protocol("exam.setup warning rows must include a message.".into())
                })?
                .to_string();
            let scope = warning.get("scope").and_then(parse_warning_scope);
            Ok(WorkspaceWarning {
                code,
                message,
                scope,
            })
        })
        .collect()
}

fn parse_warning_scope(value: &Value) -> Option<String> {
    if value.is_null() {
        return None;
    }
    if let Some(scope) = value.as_str() {
        return Some(scope.to_string());
    }
    serde_json::to_string(value).ok()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use crate::models::WorkerJobResult;
    use crate::worker::CompletedWorkerJob;

    use super::parse_exam_setup_success;

    fn temp_root(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_millis()
        ))
    }

    #[test]
    fn parse_exam_setup_success_preserves_workspace_warnings() {
        let project_path = temp_root("scriptscore-template-setup-warning");
        let output_dir = project_path.join("artifacts");
        fs::create_dir_all(&output_dir).expect("output dir should exist");
        let page_png = output_dir.join("page-1.png");
        let question_png = output_dir.join("question-1.png");
        fs::write(&page_png, b"page").expect("page png should write");
        fs::write(&question_png, b"question").expect("question png should write");

        let completed = CompletedWorkerJob {
            job_id: "job_exam_setup_1".into(),
            result: WorkerJobResult {
                terminal_type: "job_finished".into(),
                terminal_payload: json!({
                    "exit_code": 0,
                    "envelope": {
                        "ok": true
                    }
                }),
                envelope: json!({
                    "ok": true,
                    "command": "exam.setup",
                    "warnings": [
                        {
                            "code": "question_detection_partial",
                            "message": "One question label needed manual review.",
                            "scope": { "page_number": 1 }
                        },
                        {
                            "code": "template_layout_notice",
                            "message": "Header text was not matched to a question.",
                            "scope": null
                        }
                    ],
                    "data": {
                        "template_pages": [
                            {
                                "page_number": 1,
                                "image_path": page_png.to_string_lossy().to_string()
                            }
                        ],
                        "questions": [
                            {
                                "question_id": "question_1",
                                "question_number": 1,
                                "page_number": 1,
                                "max_points": 5,
                                "baseline_pdf_text": "Explain westward expansion.",
                                "template_question_png_path": question_png.to_string_lossy().to_string()
                            }
                        ]
                    }
                }),
                events: Vec::new(),
            },
        };

        let (_pages, _artifacts, _questions, warnings) =
            parse_exam_setup_success(&project_path, &completed)
                .expect("exam.setup parsing should succeed");

        assert_eq!(warnings.len(), 2);
        assert_eq!(
            warnings[0].code.as_deref(),
            Some("question_detection_partial")
        );
        assert_eq!(warnings[0].scope.as_deref(), Some(r#"{"page_number":1}"#));
        assert_eq!(warnings[1].code.as_deref(), Some("template_layout_notice"));
        assert_eq!(
            warnings[1].message,
            "Header text was not matched to a question."
        );
        assert_eq!(warnings[1].scope, None);
    }
}
