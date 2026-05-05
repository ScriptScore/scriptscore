// SPDX-License-Identifier: AGPL-3.0-only
use std::path::Path;
use std::sync::Arc;

use serde_json::Value;

use super::runtime::{run_reserved_job, start_runtime_job, RuntimeJobRequest};
use super::{AppStateInner, RuntimeEventSink};
use crate::errors::{HostError, HostResult};
use crate::models::{
    AppSettings, ExamWorkspaceState, JobTraceState, ProjectConfig, RubricUpdateInput,
    StudentIntakeInput, StudentIntakePageOrderUpdateInput,
};
use crate::project_store;
use crate::worker::CompletedWorkerJob;

#[path = "workspace_actions_analysis.rs"]
mod analysis;
#[path = "workspace_actions_intake.rs"]
mod intake;
#[path = "workspace_actions_rubrics.rs"]
mod rubrics;
#[path = "workspace_actions_shared.rs"]
mod shared;

pub(crate) use analysis::{
    analyze_persisted_request_payload, analyze_question_targets_for_workspace,
    analyze_worker_request_payload, persist_exam_analyze_batch_failure,
};
pub(crate) use intake::build_intake_default_pdf_rects_request;
pub(crate) use shared::cli_instructor_profile_json;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntakePreviewPage {
    pub page_number: i64,
    /// Total pages in the source PDF (same on every `scans.pdf-render-page` response).
    pub page_count: i64,
    pub page_width_pt: f64,
    pub page_height_pt: f64,
    pub png_width_px: i64,
    pub png_height_px: i64,
    /// Base64-encoded PNG bytes (no data URL prefix).
    pub png_base64: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfPointRect {
    pub page_number: i64,
    pub x_pt: f64,
    pub y_pt: f64,
    pub width_pt: f64,
    pub height_pt: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfTextClipInput {
    pub pdf_path: String,
    pub page_number: i64,
    pub x_pt: f64,
    pub y_pt: f64,
    pub width_pt: f64,
    pub height_pt: f64,
}

/// Persist DB-side outputs after the worker succeeds, before `finish_runtime_job` runs.
pub(crate) fn persist_host_outputs_after_worker_success(
    command_name: &str,
    project_path: &Path,
    completed: &CompletedWorkerJob,
    persisted_request_payload: &Value,
) -> HostResult<()> {
    match command_name {
        "exam.analyze" => analysis::persist_exam_analyze_outputs(project_path, completed),
        "exam.generate-rubric" => {
            let question_id = persisted_request_payload
                .get("question_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    HostError::Protocol(
                        "exam.generate-rubric persisted payload was missing question_id.".into(),
                    )
                })?;
            let replace_existing = persisted_request_payload
                .get("replace_existing")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let min_pts = persisted_request_payload
                .get("minimum_credit_row_points")
                .and_then(|v| v.as_i64());
            rubrics::persist_generated_rubric(
                project_path,
                question_id,
                completed,
                replace_existing,
                min_pts,
            )
        }
        _ => Ok(()),
    }
}

pub(crate) fn reanalyze_question(
    state: &Arc<AppStateInner>,
    project_path: &Path,
    question_id: &str,
    settings: &AppSettings,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<ExamWorkspaceState> {
    analysis::reanalyze_question(state, project_path, question_id, settings, event_sink)
}

/// Payloads and paths for a `exam.generate-rubric` run (direct or queued).
pub(crate) use rubrics::GenerateRubricPrepared;

pub(crate) fn prepare_generate_rubric_for_job(
    project_path: &Path,
    question_id: &str,
    replace_existing: bool,
    settings: &AppSettings,
    job_id: &str,
) -> HostResult<GenerateRubricPrepared> {
    rubrics::prepare_generate_rubric_for_job(
        project_path,
        question_id,
        replace_existing,
        settings,
        job_id,
    )
}

pub(crate) fn generate_question_rubric(
    state: &Arc<AppStateInner>,
    project_path: &Path,
    question_id: &str,
    replace_existing: bool,
    settings: &AppSettings,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<ExamWorkspaceState> {
    rubrics::generate_question_rubric(
        state,
        project_path,
        question_id,
        replace_existing,
        settings,
        event_sink,
    )
}

pub(crate) fn save_rubric_update(
    project_path: &Path,
    input: RubricUpdateInput,
) -> HostResult<ExamWorkspaceState> {
    rubrics::save_rubric_update(project_path, input)
}

pub(crate) fn save_project_config(
    project_path: &Path,
    config: ProjectConfig,
) -> HostResult<ExamWorkspaceState> {
    rubrics::save_project_config(project_path, config)
}

pub(crate) fn load_trace(
    project_path: &Path,
    job_id: Option<String>,
    command_name: Option<String>,
) -> HostResult<Option<JobTraceState>> {
    if let Some(job_id) = job_id {
        return project_store::load_job_trace(project_path, &job_id).map(Some);
    }
    if let Some(command_name) = command_name {
        return project_store::load_latest_job_trace_for_command(project_path, &command_name);
    }
    Ok(None)
}

pub(crate) fn run_student_intake(
    state: &Arc<AppStateInner>,
    project_path: &Path,
    inputs: Vec<StudentIntakeInput>,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<ExamWorkspaceState> {
    intake::run_student_intake(state, project_path, inputs, event_sink)
}

pub(crate) fn save_student_intake_page_order(
    project_path: &Path,
    input: StudentIntakePageOrderUpdateInput,
) -> HostResult<ExamWorkspaceState> {
    intake::save_student_intake_page_order(project_path, input)
}

#[cfg(test)]
fn exam_page_paths_from_ingest_pdf_result(result: &Value) -> Vec<String> {
    intake::exam_page_paths_from_ingest_pdf_result(result)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::models::AppSettings;

    use super::{analyze_persisted_request_payload, exam_page_paths_from_ingest_pdf_result};

    #[test]
    fn persisted_analysis_request_keeps_context_without_api_key() {
        let payload = analyze_persisted_request_payload(
            &AppSettings {
                llm_provider: "ollama_native".into(),
                llm_base_url: "http://127.0.0.1:11434".into(),
                llm_model: "qwen2.5vl:7b".into(),
                llm_api_key: Some("secret-key".into()),
                ..AppSettings::default()
            },
            &[json!({
                "question_id": "q1",
                "template_question_png_path": "/tmp/question.png",
                "baseline_pdf_text": "What is force?",
            })],
        );

        assert_eq!(payload["question_target_ids"], json!(["q1"]));
        assert_eq!(payload["providers"]["llm_provider"], "ollama_native");
        assert_eq!(payload["llm_config"]["base_url"], "http://127.0.0.1:11434");
        assert_eq!(payload["llm_config"]["model"], "qwen2.5vl:7b");
        assert!(payload["llm_config"].get("api_key").is_none());
        assert_eq!(payload["question_targets"][0]["question_id"], "q1");
    }

    #[test]
    fn exam_page_paths_from_ingest_result_preserves_ingest_output_order() {
        let result = json!({
            "student_ref": "student_1",
            "pages": [
                {"page_number": 2, "image_path": "/tmp/b.png"},
                {"page_number": 1, "image_path": "/tmp/a.png"},
            ],
        });
        let paths = exam_page_paths_from_ingest_pdf_result(&result);
        assert_eq!(
            paths,
            vec!["/tmp/b.png".to_string(), "/tmp/a.png".to_string()]
        );
    }

    #[test]
    fn exam_page_paths_from_ingest_result_skips_blank_paths() {
        let result = json!({
            "pages": [
                {"page_number": 1, "image_path": "  "},
                {"page_number": 2, "image_path": "/ok.png"},
            ],
        });
        let paths = exam_page_paths_from_ingest_pdf_result(&result);
        assert_eq!(paths, vec!["/ok.png".to_string()]);
    }
}
