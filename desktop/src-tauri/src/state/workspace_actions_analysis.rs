// SPDX-License-Identifier: AGPL-3.0-only
use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::errors::{HostError, HostResult};
use crate::models::{
    AppSettings, ExamWorkspaceState, QuestionAnalysisState, QuestionRecord, WorkspaceWarning,
};
use crate::project_store;
use crate::worker::CompletedWorkerJob;

use super::shared::{
    llm_config_json, llm_config_trace_json, parse_warnings, required_string, success_data,
};
use super::{
    run_reserved_job, start_runtime_job, AppStateInner, RuntimeEventSink, RuntimeJobRequest,
};

pub(crate) fn analyze_question_targets_for_workspace(workspace: &ExamWorkspaceState) -> Vec<Value> {
    workspace
        .questions
        .iter()
        .filter_map(|question| {
            question.image_path.as_ref().map(|image_path| {
                json!({
                    "question_id": question.question_id,
                    "template_question_png_path": image_path,
                    "baseline_pdf_text": question.baseline_pdf_text,
                })
            })
        })
        .collect()
}

pub(crate) fn persist_exam_analyze_outputs(
    project_path: &Path,
    completed: &CompletedWorkerJob,
) -> HostResult<()> {
    persist_analyze_results(project_path, completed)?;
    persist_batch_analyze_trace_ref(project_path, &completed.job_id)?;
    Ok(())
}

pub(crate) fn reanalyze_question(
    state: &Arc<AppStateInner>,
    project_path: &Path,
    question_id: &str,
    settings: &AppSettings,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<ExamWorkspaceState> {
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    let question = reanalysis_question(&workspace, question_id)?;

    let reserved = start_runtime_job(state, event_sink, "exam.analyze")?;
    let output_artifacts_dir =
        project_store::command_output_dir(project_path, "exam.analyze", &reserved.job_id);
    std::fs::create_dir_all(&output_artifacts_dir)?;

    let question_targets = question_targets_for_reanalysis(question);
    let worker_request_payload = analyze_worker_request_payload(settings, &question_targets);
    let persisted_request_payload = analyze_persisted_request_payload(settings, &question_targets);

    run_reserved_job(
        state,
        event_sink,
        reserved,
        RuntimeJobRequest {
            command_name: "exam.analyze",
            worker_request_payload,
            persisted_request_payload,
            output_artifacts_dir: Some(&output_artifacts_dir),
            project_path: Some(project_path),
            stdin_bytes: None,
        },
    )?;

    project_store::load_exam_workspace_state(project_path)
}

pub(crate) fn analyze_worker_request_payload(
    settings: &AppSettings,
    question_targets: &[Value],
) -> Value {
    json!({
        "question_targets": question_targets,
        "providers": { "llm_provider": settings.llm_provider },
        "llm_config": llm_config_json(settings),
    })
}

fn reanalysis_question<'a>(
    workspace: &'a ExamWorkspaceState,
    question_id: &str,
) -> HostResult<&'a QuestionRecord> {
    let question = workspace
        .questions
        .iter()
        .find(|q| q.question_id == question_id)
        .ok_or_else(|| HostError::Validation(format!("Question '{question_id}' not found")))?;
    if question.rubric.is_approved() {
        return Err(HostError::Validation(
            "Re-analyzing a question with an approved rubric is not available. Save a grading-impacting edit to rescind rubric approval first.".into(),
        ));
    }
    Ok(question)
}

fn question_targets_for_reanalysis(question: &QuestionRecord) -> Vec<Value> {
    question
        .image_path
        .as_ref()
        .map(|image_path| {
            json!({
                "question_id": question.question_id,
                "template_question_png_path": image_path,
                "baseline_pdf_text": question.baseline_pdf_text,
            })
        })
        .into_iter()
        .collect()
}

pub(crate) fn analyze_persisted_request_payload(
    settings: &AppSettings,
    question_targets: &[Value],
) -> Value {
    json!({
        "question_target_ids": question_targets
            .iter()
            .filter_map(|target| target.get("question_id").and_then(Value::as_str))
            .collect::<Vec<&str>>(),
        "question_targets": question_targets,
        "providers": { "llm_provider": settings.llm_provider },
        "llm_config": llm_config_trace_json(settings),
    })
}

fn persist_analyze_results(project_path: &Path, completed: &CompletedWorkerJob) -> HostResult<()> {
    let previous = project_store::load_exam_workspace_state(project_path)?;
    let mut changed_question_ids = Vec::new();
    let data = success_data(&completed.result.envelope)?;
    let results = data
        .get("question_results")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            HostError::Protocol(
                "exam.analyze success envelope was missing question_results.".into(),
            )
        })?;
    for result in results {
        let question_id = required_string(result, "question_id")?;
        changed_question_ids.push(question_id.clone());
        let state = QuestionAnalysisState {
            status: result
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("error")
                .to_string(),
            question_text_clean: result
                .get("question_text_clean")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            question_context: result
                .get("question_context")
                .and_then(Value::as_str)
                .map(String::from),
            warnings: parse_warnings(result.get("warnings"))?,
            latest_job_id: Some(completed.job_id.clone()),
        };
        project_store::persist_question_analysis_state(project_path, &question_id, &state)?;
    }
    reconcile_approved_rubrics_after_analysis(project_path, &previous, &changed_question_ids)?;
    Ok(())
}

fn reconcile_approved_rubrics_after_analysis(
    project_path: &Path,
    previous: &ExamWorkspaceState,
    changed_question_ids: &[String],
) -> HostResult<()> {
    if changed_question_ids.is_empty() {
        return Ok(());
    }
    let current = project_store::load_exam_workspace_state(project_path)?;
    for question_id in changed_question_ids {
        reconcile_approved_rubric_after_question_analysis(
            project_path,
            previous,
            &current,
            question_id,
        )?;
    }
    Ok(())
}

fn reconcile_approved_rubric_after_question_analysis(
    project_path: &Path,
    previous: &ExamWorkspaceState,
    current: &ExamWorkspaceState,
    question_id: &String,
) -> HostResult<()> {
    let Some(before) = question_by_id(previous, question_id) else {
        return Ok(());
    };
    if !before.rubric.is_approved() {
        return Ok(());
    }
    let Some(after) = question_by_id(current, question_id) else {
        return Ok(());
    };
    if !question_approval_basis_changed(before, after) {
        return Ok(());
    }
    let mut rubric = after.rubric.clone();
    rubric.mark_draft();
    project_store::save_rubric_state(project_path, question_id, &rubric)?;
    project_store::mark_student_answers_stale_for_questions(
        project_path,
        std::slice::from_ref(question_id),
    )?;
    Ok(())
}

fn question_by_id<'a>(
    workspace: &'a ExamWorkspaceState,
    question_id: &str,
) -> Option<&'a QuestionRecord> {
    workspace
        .questions
        .iter()
        .find(|question| question.question_id == question_id)
}

fn question_approval_basis_changed(previous: &QuestionRecord, current: &QuestionRecord) -> bool {
    previous.text != current.text
        || previous.max_points != current.max_points
        || question_context(previous) != question_context(current)
}

fn question_context(question: &QuestionRecord) -> &str {
    question
        .analysis
        .question_context
        .as_deref()
        .unwrap_or_default()
}

fn persist_batch_analyze_trace_ref(project_path: &Path, job_id: &str) -> HostResult<()> {
    let mut connection =
        rusqlite::Connection::open(project_store::schema::project_db_path(project_path))?;
    let project_config = project_store::load_project_config(&connection)?;
    let transaction = connection.transaction()?;
    project_store::update_batch_analyze_trace_ref(
        &transaction,
        &project_config.project_id,
        job_id,
    )?;
    transaction.commit()?;
    Ok(())
}

fn batch_analyze_failure_message(envelope: &Value) -> String {
    envelope
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| "Question analysis failed.".to_string())
}

/// When a batch `exam.analyze` job fails or is cancelled, persist the trace ref and mark each target
/// question as `error` with `latest_job_id` so the Review UI and job trace load the failed run.
pub(crate) fn persist_exam_analyze_batch_failure(
    project_path: &Path,
    completed: &CompletedWorkerJob,
    persisted_request_payload: &Value,
) -> HostResult<()> {
    persist_batch_analyze_trace_ref(project_path, &completed.job_id)?;
    let message = batch_analyze_failure_message(&completed.result.envelope);
    let ids: Vec<String> = persisted_request_payload
        .get("question_target_ids")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(std::string::ToString::to_string))
                .collect()
        })
        .unwrap_or_default();

    for qid in ids {
        let state = QuestionAnalysisState {
            status: "error".into(),
            question_text_clean: None,
            question_context: None,
            warnings: vec![WorkspaceWarning {
                code: Some("analyze_failed".into()),
                message: message.clone(),
                scope: Some("exam.analyze".into()),
            }],
            latest_job_id: Some(completed.job_id.clone()),
        };
        project_store::persist_question_analysis_state(project_path, &qid, &state)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use rusqlite::{params, Connection};
    use serde_json::json;

    use super::*;
    use crate::models::{
        RubricApprovalBasis, RubricCriterion, RubricState, StudentWorkflowAnswer,
        StudentWorkflowState, StudentWorkflowSubmission, WorkerJobResult,
    };

    #[test]
    fn reanalysis_that_changes_approved_question_basis_stales_existing_grading() {
        let project_path = std::env::temp_dir().join(format!(
            "scriptscore-reanalysis-stale-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_millis()
        ));
        std::fs::create_dir_all(&project_path).expect("project dir should exist");
        let connection = Connection::open(project_store::schema::project_db_path(&project_path))
            .expect("project db should open");
        project_store::schema::initialize_schema(&connection).expect("schema should initialize");
        connection
            .execute(
                "INSERT INTO project (
                    project_id, display_name, redaction_required, instructor_profile_json, trace_refs_json
                ) VALUES ('project_1', 'Project', 0, '{}', '{}')",
                [],
            )
            .expect("project should insert");
        connection
            .execute(
                "INSERT INTO question (
                    question_id, question_number, page_number, max_points, prompt_text, baseline_pdf_text
                ) VALUES ('question_1', 1, 1, 5, 'Original prompt', 'Original prompt')",
                [],
            )
            .expect("question should insert");

        let criterion = RubricCriterion {
            criterion_id: "criterion_1".into(),
            label: "Correctness".into(),
            points: 5,
            partial_credit_guidance: "Award up to 5 points.".into(),
            source: "manual".into(),
        };
        project_store::save_rubric_state(
            &project_path,
            "question_1",
            &RubricState {
                status: "approved".into(),
                criteria: vec![criterion.clone()],
                warnings: vec![],
                approved_at: Some("1".into()),
                latest_job_id: None,
                approval_basis: Some(RubricApprovalBasis {
                    question_text: "Original prompt".into(),
                    question_context: "".into(),
                    max_points: Some(5),
                    criteria: vec![criterion],
                }),
            },
        )
        .expect("rubric should save");
        project_store::save_student_workflow_state(
            &project_path,
            &StudentWorkflowState {
                status: "graded".into(),
                latest_job_id: Some("grade_job".into()),
                submissions: vec![StudentWorkflowSubmission {
                    student_ref: "student_1".into(),
                    canonical_pdf_path: "/tmp/student.pdf".into(),
                    page_count: 1,
                    stage: "graded".into(),
                    latest_job_id: Some("grade_job".into()),
                    failure_message: None,
                    warnings: vec![],
                    page_artifacts: vec![],
                    alignment_pages: vec![],
                    detect_review: None,
                    answers: vec![StudentWorkflowAnswer {
                        question_id: "question_1".into(),
                        question_number: 1,
                        crop_image_path: None,
                        pii_prescreen: None,
                        manual_grading_required: false,
                        manual_grading_reason: None,
                        moderation_eligible: true,
                        parse_status: "verified".into(),
                        parse_confidence: None,
                        parse_confidence_source: None,
                        raw_parsed_text: Some("answer".into()),
                        verified_text: Some("answer".into()),
                        review_required: false,
                        verified: true,
                        stale: false,
                        grading_status: "graded".into(),
                        grading_confidence: None,
                        grading_confidence_reason: None,
                        question_max_points: Some(5),
                        total_points_awarded: Some(4),
                        feedback_text: Some("Good".into()),
                        criterion_results: vec![],
                        highlights: vec![],
                        warnings: vec![],
                    }],
                }],
            },
        )
        .expect("student workflow should save");

        let completed = CompletedWorkerJob {
            job_id: "analyze_job".into(),
            result: WorkerJobResult {
                terminal_type: "success".into(),
                terminal_payload: json!({}),
                envelope: json!({
                    "ok": true,
                    "data": {
                        "question_results": [{
                            "question_id": "question_1",
                            "status": "ok",
                            "question_text_clean": "Changed prompt",
                            "question_context": "",
                            "warnings": []
                        }]
                    }
                }),
                events: vec![],
            },
        };
        persist_analyze_results(&project_path, &completed).expect("analysis should persist");

        let rubric = project_store::load_rubric_state(&project_path, "question_1")
            .expect("rubric should load");
        assert_eq!(rubric.status, "draft");
        assert_eq!(rubric.approved_at, None);
        let workflow = project_store::load_student_workflow_state(&project_path)
            .expect("workflow should load");
        assert!(workflow.submissions[0].answers[0].stale);

        connection
            .execute(
                "DELETE FROM workflow_state WHERE scope_type = ?1 AND scope_id = ?2 AND state_key = ?3",
                params!["question", "question_1", "question_rubric"],
            )
            .expect("cleanup query should run");
        std::fs::remove_dir_all(&project_path).expect("project dir should clean up");
    }
}
