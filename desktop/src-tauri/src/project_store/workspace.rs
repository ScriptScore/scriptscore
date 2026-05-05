// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::Connection;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::errors::HostResult;
use crate::models::{
    ExamWorkspaceState, LmsUploadMode, LmsUploadStudentStatus, ModerationState, ProjectConfig,
    QuestionRecord, ResultFinalizationRecord, ResultQuestionRow, ResultStudentRow,
    ResultsExamMetrics, ResultsLmsReviewSummary, ResultsLmsState, ResultsQuestionMetric,
    RubricCriterion, StudentRosterRow, StudentWorkflowAnswer, StudentWorkflowCriterionResult,
    StudentWorkflowState, StudentWorkflowSubmission, TemplatePageArtifactSummary,
    TemplateRedactionRegion, TemplateSetupPayload, WorkspaceWarning,
};
use crate::workflow_status::{
    derive_post_intake_workflow_stage, derive_project_workflow_stage,
    derive_results_workflow_stage, TemplateSetupStatus,
};

use super::project_config::load_project_config;
use super::projects::load_project_summary;
use super::schema::{initialize_schema, parse_json, project_db_path};
use super::template_setup::load_template_setup_state;
use super::{
    load_moderation_state, load_results_lms_state, load_student_intake_state, load_student_roster,
    load_student_workflow_state,
    student_roster::attach_binding_tokens,
    workflow_state::{load_question_analysis_states, load_rubric_states},
};

pub fn load_exam_workspace_state(project_path: &Path) -> HostResult<ExamWorkspaceState> {
    let summary = load_project_summary(project_path)?;
    let connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let project_config = load_project_config(&connection)?;
    let (status, payload) = load_template_setup_state(&connection, &summary.project_id)?;
    let template_preview_artifacts = load_template_page_artifacts(&connection, project_path)?;
    let questions = load_workspace_questions(&connection, project_path)?;
    let redaction_regions = load_redaction_regions(&connection)?;
    let skip_redaction = did_skip_redaction(&payload, &project_config);
    let warnings = workspace_warnings(
        payload.warnings,
        skip_redaction,
        redaction_regions.is_empty(),
    );
    let mut workflow_stage = derive_project_workflow_stage(
        &status,
        project_config.redaction_required,
        !redaction_regions.is_empty(),
        skip_redaction,
        &questions,
    );
    let student_roster = load_student_roster(project_path)?;
    let mut student_intake = load_student_intake_state(project_path)?;
    attach_binding_tokens(project_path, &mut student_intake)?;
    let mut student_workflow = load_student_workflow_state(project_path)?;
    attach_workflow_criterion_labels(&questions, &mut student_workflow);
    let moderation_state = load_moderation_state(project_path)?;
    let results_lms_state = load_results_lms_state(project_path)?;
    let results_lms_rows = derive_results_lms_rows(
        &questions,
        &student_roster,
        &student_workflow,
        &moderation_state,
        &results_lms_state,
    );
    let results_lms_metrics =
        derive_results_lms_metrics(&questions, &results_lms_rows, &moderation_state);
    let results_lms_review_summary =
        derive_results_lms_review_summary(&student_workflow, &moderation_state);
    if matches!(
        workflow_stage,
        crate::workflow_status::ProjectWorkflowStage::StudentIntakeReady
    ) {
        if let Some(post_intake) = derive_post_intake_workflow_stage(&student_workflow) {
            workflow_stage = post_intake;
        }
    }
    if matches!(
        workflow_stage,
        crate::workflow_status::ProjectWorkflowStage::StudentGradingComplete
    ) {
        if let Some(results_stage) = derive_results_workflow_stage(
            &results_lms_rows,
            results_lms_state.selected_target.is_some(),
        ) {
            workflow_stage = results_stage;
        }
    }
    Ok(ExamWorkspaceState {
        project: summary,
        status: status.as_str().to_string(),
        status_label: workspace_status_label(&status, skip_redaction, redaction_regions.is_empty()),
        failure_message: payload.failure_message,
        template_preview_artifacts,
        aruco_status: payload.aruco_status,
        questions: questions.clone(),
        redaction_regions: redaction_regions.clone(),
        warnings,
        can_approve: can_approve_template_setup(
            &status,
            &questions,
            &redaction_regions,
            skip_redaction,
        ),
        can_approve_rubric: can_approve_rubric_editing(&status, &questions),
        project_config,
        student_roster,
        student_intake,
        student_workflow,
        moderation_state,
        results_lms_state,
        results_lms_rows,
        results_lms_metrics: Some(results_lms_metrics),
        results_lms_review_summary: Some(results_lms_review_summary),
        workflow_stage: workflow_stage.as_str().to_string(),
        workflow_label: workflow_stage.label().to_string(),
    })
}

fn load_workspace_questions(
    connection: &Connection,
    project_path: &Path,
) -> HostResult<Vec<QuestionRecord>> {
    let mut questions = load_questions(connection, project_path)?;
    attach_question_workflow_state(connection, &mut questions)?;
    Ok(questions)
}

fn attach_question_workflow_state(
    connection: &Connection,
    questions: &mut [QuestionRecord],
) -> HostResult<()> {
    let question_ids = questions
        .iter()
        .map(|question| question.question_id.clone())
        .collect::<Vec<String>>();
    let analysis_states = load_question_analysis_states(connection, &question_ids)?;
    let rubric_states = load_rubric_states(connection, &question_ids)?;
    for question in questions {
        if let Some(analysis) = analysis_states.get(&question.question_id) {
            question.analysis = analysis.clone();
        }
        if let Some(rubric) = rubric_states.get(&question.question_id) {
            question.rubric = rubric.clone();
        }
    }
    Ok(())
}

fn attach_workflow_criterion_labels(
    questions: &[QuestionRecord],
    workflow: &mut StudentWorkflowState,
) {
    let criteria_by_question = questions
        .iter()
        .map(|question| {
            (
                question.question_id.as_str(),
                question.rubric.criteria.as_slice(),
            )
        })
        .collect::<HashMap<_, _>>();
    for submission in &mut workflow.submissions {
        for answer in &mut submission.answers {
            if let Some(criteria) = criteria_by_question.get(answer.question_id.as_str()) {
                attach_answer_criterion_labels(criteria, &mut answer.criterion_results);
                seed_manual_answer_criterion_results(criteria, answer);
            }
        }
    }
}

fn derive_results_lms_rows(
    questions: &[QuestionRecord],
    _student_roster: &[StudentRosterRow],
    student_workflow: &StudentWorkflowState,
    moderation_state: &ModerationState,
    results_lms_state: &ResultsLmsState,
) -> Vec<ResultStudentRow> {
    let score_overrides = moderation_state
        .score_overrides
        .iter()
        .map(|entry| {
            (
                (entry.student_ref.as_str(), entry.question_id.as_str()),
                entry.moderated_total_points,
            )
        })
        .collect::<HashMap<_, _>>();
    let feedback_overrides = moderation_state
        .feedback_overrides
        .iter()
        .map(|entry| {
            (
                (entry.student_ref.as_str(), entry.question_id.as_str()),
                entry.feedback_text.as_str(),
            )
        })
        .collect::<HashMap<_, _>>();
    let finalizations_by_student = results_lms_state
        .finalization_records
        .iter()
        .map(|record| (record.student_ref.as_str(), record))
        .collect::<HashMap<_, _>>();

    let mut rows = student_workflow
        .submissions
        .iter()
        .filter(|submission| submission.stage == "graded")
        .map(|submission| {
            build_results_lms_row(
                submission,
                questions,
                &score_overrides,
                &feedback_overrides,
                &finalizations_by_student,
                results_lms_state,
            )
        })
        .collect::<Vec<_>>();

    rows.sort_by(|left, right| left.student_ref.cmp(&right.student_ref));
    rows
}

fn derive_results_lms_metrics(
    questions: &[QuestionRecord],
    rows: &[ResultStudentRow],
    moderation_state: &ModerationState,
) -> ResultsExamMetrics {
    let mut scored_totals = rows
        .iter()
        .filter(|row| row.aggregate_complete)
        .map(|row| row.aggregate_total)
        .collect::<Vec<_>>();
    scored_totals.sort_unstable();
    let scored_student_count = scored_totals.len() as i64;
    let average_score = (!scored_totals.is_empty())
        .then(|| scored_totals.iter().sum::<i64>() as f64 / scored_totals.len() as f64);
    let median_score = match scored_totals.as_slice() {
        [] => None,
        values if values.len() % 2 == 1 => Some(values[values.len() / 2] as f64),
        values => {
            let upper = values.len() / 2;
            Some((values[upper - 1] as f64 + values[upper] as f64) / 2.0)
        }
    };
    let reviewed_question_ids = moderation_state
        .question_reviews
        .iter()
        .map(|review| review.question_id.as_str())
        .collect::<HashSet<_>>();
    let question_metrics = questions
        .iter()
        .map(|question| {
            let points = rows
                .iter()
                .filter_map(|row| {
                    row.question_rows
                        .iter()
                        .find(|entry| entry.question_id == question.question_id)
                        .and_then(|entry| entry.effective_total_points)
                })
                .collect::<Vec<_>>();
            let sample_size = points.len() as i64;
            let average_points = (!points.is_empty())
                .then(|| points.iter().sum::<i64>() as f64 / points.len() as f64);
            let average_percent = match (average_points, question.max_points) {
                (Some(value), Some(max_points)) if max_points > 0 => {
                    Some((value / max_points as f64) * 100.0)
                }
                _ => None,
            };

            ResultsQuestionMetric {
                question_id: question.question_id.clone(),
                question_number: question.question_number,
                max_points: question.max_points,
                reviewed: reviewed_question_ids.contains(question.question_id.as_str()),
                sample_size,
                average_points,
                average_percent,
                difficulty_percent: average_percent.map(|percent| 100.0 - percent),
            }
        })
        .collect::<Vec<_>>();

    ResultsExamMetrics {
        scored_student_count,
        average_score,
        median_score,
        min_score: scored_totals.first().copied(),
        max_score: scored_totals.last().copied(),
        question_metrics,
    }
}

fn derive_results_lms_review_summary(
    student_workflow: &StudentWorkflowState,
    moderation_state: &ModerationState,
) -> ResultsLmsReviewSummary {
    let reviewable_question_ids = student_workflow
        .submissions
        .iter()
        .flat_map(|submission| submission.answers.iter())
        .filter(|answer| answer.moderation_eligible)
        .map(|answer| answer.question_id.clone())
        .collect::<HashSet<_>>();
    let reviewed_question_ids = moderation_state
        .question_reviews
        .iter()
        .map(|review| review.question_id.as_str())
        .collect::<HashSet<_>>();
    let unreviewed_question_count = reviewable_question_ids
        .iter()
        .filter(|question_id| !reviewed_question_ids.contains(question_id.as_str()))
        .count() as i64;

    ResultsLmsReviewSummary {
        total_reviewable_questions: reviewable_question_ids.len() as i64,
        unreviewed_question_count,
        has_unreviewed_questions: unreviewed_question_count > 0,
    }
}

fn build_results_lms_row(
    submission: &StudentWorkflowSubmission,
    questions: &[QuestionRecord],
    score_overrides: &HashMap<(&str, &str), i64>,
    feedback_overrides: &HashMap<(&str, &str), &str>,
    finalizations_by_student: &HashMap<&str, &ResultFinalizationRecord>,
    results_lms_state: &ResultsLmsState,
) -> ResultStudentRow {
    let answers_by_question = submission
        .answers
        .iter()
        .map(|answer| (answer.question_id.as_str(), answer))
        .collect::<HashMap<_, _>>();
    let (question_rows, blocked_reasons, aggregate_total, aggregate_complete) = questions
        .iter()
        .map(|question| {
            build_result_question_row(
                submission,
                question,
                answers_by_question
                    .get(question.question_id.as_str())
                    .copied(),
                score_overrides,
                feedback_overrides,
            )
        })
        .fold(
            (Vec::new(), Vec::new(), 0_i64, !questions.is_empty()),
            |mut acc, outcome| {
                if let Some(message) = outcome.blocked_reason.clone() {
                    acc.1.push(message);
                    acc.3 = false;
                } else if let Some(points) = outcome.row.effective_total_points {
                    acc.2 += points;
                }
                acc.0.push(outcome.row);
                acc
            },
        );
    let ready_to_finalize = aggregate_complete && !question_rows.is_empty();
    let result_fingerprint =
        ready_to_finalize.then(|| result_fingerprint_for_row(&question_rows, aggregate_total));
    let finalization = finalizations_by_student
        .get(submission.student_ref.as_str())
        .copied();
    let finalized = is_current_finalization(finalization, result_fingerprint.as_deref());
    let stale_finalization = finalization.is_some() && !finalized;
    let (uploaded, upload_failed, latest_upload_error, last_upload_attempt_id) =
        derive_upload_status(
            results_lms_state,
            &submission.student_ref,
            result_fingerprint.as_deref(),
        );

    ResultStudentRow {
        student_ref: submission.student_ref.clone(),
        aggregate_total,
        aggregate_complete,
        ready_to_finalize,
        blocked_reasons,
        question_rows,
        result_fingerprint,
        finalized,
        stale_finalization,
        finalized_at: finalization.map(|record| record.finalized_at.clone()),
        uploaded,
        upload_failed,
        latest_upload_error,
        last_upload_attempt_id,
    }
}

struct ResultQuestionOutcome {
    row: ResultQuestionRow,
    blocked_reason: Option<String>,
}

fn build_result_question_row(
    submission: &StudentWorkflowSubmission,
    question: &QuestionRecord,
    answer: Option<&StudentWorkflowAnswer>,
    score_overrides: &HashMap<(&str, &str), i64>,
    feedback_overrides: &HashMap<(&str, &str), &str>,
) -> ResultQuestionOutcome {
    let Some(answer) = answer else {
        let message = format!(
            "Question {} is missing from this graded submission.",
            question.question_number
        );
        return ResultQuestionOutcome {
            row: ResultQuestionRow {
                question_id: question.question_id.clone(),
                question_number: question.question_number,
                max_points: question.max_points,
                effective_total_points: None,
                effective_feedback_text: String::new(),
                uses_moderated_total: false,
                uses_moderated_feedback: false,
                blocked_reason: Some(message.clone()),
            },
            blocked_reason: Some(message),
        };
    };

    let override_key = (
        submission.student_ref.as_str(),
        question.question_id.as_str(),
    );
    let moderated_total = score_overrides.get(&override_key).copied();
    let effective_total_points = moderated_total.or(answer.total_points_awarded);
    let blocked_reason = effective_total_points.is_none().then(|| {
        format!(
            "Question {} has no effective score yet.",
            question.question_number
        )
    });

    ResultQuestionOutcome {
        row: ResultQuestionRow {
            question_id: question.question_id.clone(),
            question_number: question.question_number,
            max_points: question.max_points,
            effective_total_points,
            effective_feedback_text: feedback_overrides
                .get(&override_key)
                .copied()
                .unwrap_or_else(|| answer.feedback_text.as_deref().unwrap_or(""))
                .to_string(),
            uses_moderated_total: moderated_total.is_some(),
            uses_moderated_feedback: feedback_overrides.contains_key(&override_key),
            blocked_reason: blocked_reason.clone(),
        },
        blocked_reason,
    }
}

fn is_current_finalization(
    finalization: Option<&ResultFinalizationRecord>,
    result_fingerprint: Option<&str>,
) -> bool {
    matches!(
        (finalization, result_fingerprint),
        (Some(record), Some(fingerprint)) if record.result_fingerprint == fingerprint
    )
}

fn derive_upload_status(
    results_lms_state: &ResultsLmsState,
    student_ref: &str,
    result_fingerprint: Option<&str>,
) -> (bool, bool, Option<String>, Option<String>) {
    let Some((target, result_fingerprint)) =
        upload_status_context(results_lms_state, result_fingerprint)
    else {
        return (false, false, None, None);
    };

    let mut summary = UploadStatusSummary::default();

    for attempt in results_lms_state.upload_attempts.iter().rev() {
        if !matches_upload_target(attempt, target) {
            continue;
        }
        let Some(result) = matching_upload_result(attempt, student_ref, result_fingerprint) else {
            continue;
        };
        summary.record(attempt, result);
    }

    summary.into_status()
}

fn upload_status_context<'a>(
    results_lms_state: &'a ResultsLmsState,
    result_fingerprint: Option<&'a str>,
) -> Option<(&'a crate::models::ResultsLmsTarget, &'a str)> {
    Some((
        results_lms_state.selected_target.as_ref()?,
        result_fingerprint?,
    ))
}

fn matches_upload_target(
    attempt: &crate::models::LmsUploadAttemptResult,
    target: &crate::models::ResultsLmsTarget,
) -> bool {
    attempt.provider == target.provider
        && attempt.course_id == target.course_id
        && attempt.assignment_id == target.assignment_id
}

fn matching_upload_result<'a>(
    attempt: &'a crate::models::LmsUploadAttemptResult,
    student_ref: &str,
    result_fingerprint: &str,
) -> Option<&'a crate::models::LmsUploadStudentResult> {
    attempt.student_results.iter().find(|result| {
        result.student_ref == student_ref && result.result_fingerprint == result_fingerprint
    })
}

#[derive(Default)]
struct UploadStatusSummary {
    uploaded_attempt_id: Option<String>,
    failed_attempt_id: Option<String>,
    failed_error: Option<String>,
}

impl UploadStatusSummary {
    fn record(
        &mut self,
        attempt: &crate::models::LmsUploadAttemptResult,
        result: &crate::models::LmsUploadStudentResult,
    ) {
        if self.uploaded_attempt_id.is_none()
            && attempt.mode == LmsUploadMode::Live
            && result.status == LmsUploadStudentStatus::Uploaded
        {
            self.uploaded_attempt_id = Some(attempt.attempt_id.clone());
        }
        if self.failed_attempt_id.is_none() && result.status == LmsUploadStudentStatus::Failed {
            self.failed_attempt_id = Some(attempt.attempt_id.clone());
            self.failed_error = result.sanitized_error.clone();
        }
    }

    fn into_status(self) -> (bool, bool, Option<String>, Option<String>) {
        if let Some(attempt_id) = self.uploaded_attempt_id {
            return (true, false, None, Some(attempt_id));
        }
        if let Some(attempt_id) = self.failed_attempt_id {
            return (false, true, self.failed_error, Some(attempt_id));
        }
        (false, false, None, None)
    }
}

fn result_fingerprint_for_row(question_rows: &[ResultQuestionRow], aggregate_total: i64) -> String {
    let payload = serde_json::json!({
        "questions": question_rows.iter().map(|row| serde_json::json!({
            "question_id": row.question_id,
            "question_number": row.question_number,
            "effective_total_points": row.effective_total_points,
            "effective_feedback_text": row.effective_feedback_text,
        })).collect::<Vec<_>>(),
        "aggregate_total": aggregate_total,
    });
    let mut hasher = Sha256::new();
    hasher.update(payload.to_string().as_bytes());
    hex::encode(hasher.finalize())
}

fn attach_answer_criterion_labels(
    criteria: &[RubricCriterion],
    results: &mut [StudentWorkflowCriterionResult],
) {
    for result in results {
        if let Some(criterion) = criterion_for_result(criteria, result.criterion_index) {
            if !criterion.label.trim().is_empty() {
                result.label = criterion.label.clone();
            }
            result.points = criterion.points;
        }
    }
}

fn seed_manual_answer_criterion_results(
    criteria: &[RubricCriterion],
    answer: &mut StudentWorkflowAnswer,
) {
    if !answer.manual_grading_required {
        return;
    }

    for (criterion_index, criterion) in criteria.iter().enumerate() {
        let Ok(criterion_index) = i64::try_from(criterion_index) else {
            continue;
        };
        if answer
            .criterion_results
            .iter()
            .any(|result| result.criterion_index == criterion_index)
        {
            continue;
        }
        answer
            .criterion_results
            .push(StudentWorkflowCriterionResult {
                criterion_index,
                label: criterion.label.clone(),
                points: criterion.points,
                points_awarded: 0,
                rationale: String::new(),
            });
    }
    answer
        .criterion_results
        .sort_by_key(|result| result.criterion_index);
}

fn criterion_for_result(
    criteria: &[RubricCriterion],
    criterion_index: i64,
) -> Option<&RubricCriterion> {
    usize::try_from(criterion_index)
        .ok()
        .and_then(|index| criteria.get(index))
}

fn did_skip_redaction(payload: &TemplateSetupPayload, project_config: &ProjectConfig) -> bool {
    payload.redaction_skip_acknowledged_at.is_some() || !project_config.redaction_required
}

fn workspace_warnings(
    mut warnings: Vec<WorkspaceWarning>,
    skip_redaction: bool,
    has_no_redactions: bool,
) -> Vec<WorkspaceWarning> {
    if skip_redaction && has_no_redactions {
        warnings.insert(0, skipped_redaction_warning());
    }
    warnings
}

fn can_approve_template_setup(
    status: &TemplateSetupStatus,
    questions: &[QuestionRecord],
    redaction_regions: &[TemplateRedactionRegion],
    skip_redaction: bool,
) -> bool {
    status.can_approve(
        !questions.is_empty(),
        !redaction_regions.is_empty(),
        skip_redaction,
    )
}

fn can_approve_rubric_editing(status: &TemplateSetupStatus, questions: &[QuestionRecord]) -> bool {
    matches!(
        status,
        TemplateSetupStatus::Draft | TemplateSetupStatus::Approved
    ) && !questions.is_empty()
}

pub(crate) fn load_template_page_artifacts(
    connection: &Connection,
    project_path: &Path,
) -> HostResult<Vec<TemplatePageArtifactSummary>> {
    let mut statement = connection.prepare(
        "SELECT artifact_id, relative_path, metadata_json
         FROM artifact
         WHERE role = 'rendered_template_page'",
    )?;
    let rows = statement.query_map([], |row| {
        let metadata_json: Option<String> = row.get(2)?;
        let metadata = parse_json(metadata_json.as_deref())?;
        Ok(TemplatePageArtifactSummary {
            artifact_id: row.get(0)?,
            image_path: project_path
                .join(row.get::<_, String>(1)?)
                .to_string_lossy()
                .into_owned(),
            page_number: metadata
                .get("page_number")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            label: metadata
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("Page")
                .to_string(),
        })
    })?;
    let mut artifacts = Vec::new();
    for row in rows {
        artifacts.push(row?);
    }
    artifacts.sort_by_key(|artifact| artifact.page_number);
    Ok(artifacts)
}

pub(crate) fn load_questions(
    connection: &Connection,
    project_path: &Path,
) -> HostResult<Vec<QuestionRecord>> {
    let mut statement = connection.prepare(
        "SELECT q.question_id, q.question_number, q.page_number, q.max_points, q.prompt_text,
                q.baseline_pdf_text, q.region_x, q.region_y, q.region_width, q.region_height,
                q.source_artifact_id, a.relative_path
         FROM question q
         LEFT JOIN artifact a ON a.artifact_id = q.source_artifact_id
         ORDER BY q.question_number ASC, q.question_id ASC",
    )?;
    let rows = statement.query_map([], |row| question_record_from_row(row, project_path))?;
    let mut questions = Vec::new();
    for row in rows {
        questions.push(row?);
    }
    Ok(questions)
}

fn question_record_from_row(
    row: &rusqlite::Row<'_>,
    project_path: &Path,
) -> rusqlite::Result<QuestionRecord> {
    Ok(QuestionRecord {
        question_id: row.get(0)?,
        question_number: row.get(1)?,
        page_number: row.get(2)?,
        max_points: row.get(3)?,
        text: row.get(4)?,
        baseline_pdf_text: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
        region: question_region_from_row(row)?,
        source_artifact_id: row.get(10)?,
        image_path: question_image_path_from_row(row, project_path)?,
        analysis: Default::default(),
        rubric: Default::default(),
    })
}

fn question_region_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<Option<crate::models::TemplateQuestionRegion>> {
    let region_x: Option<i64> = row.get(6)?;
    let region_y: Option<i64> = row.get(7)?;
    let region_width: Option<i64> = row.get(8)?;
    let region_height: Option<i64> = row.get(9)?;
    Ok(match (region_x, region_y, region_width, region_height) {
        (Some(x), Some(y), Some(width), Some(height)) => {
            Some(crate::models::TemplateQuestionRegion {
                x,
                y,
                width,
                height,
            })
        }
        _ => None,
    })
}

fn question_image_path_from_row(
    row: &rusqlite::Row<'_>,
    project_path: &Path,
) -> rusqlite::Result<Option<String>> {
    let relative_path: Option<String> = row.get(11)?;
    Ok(relative_path
        .as_ref()
        .map(|path| project_path.join(path).to_string_lossy().into_owned()))
}

pub(crate) fn load_redaction_regions(
    connection: &Connection,
) -> HostResult<Vec<TemplateRedactionRegion>> {
    let mut statement = connection.prepare(
        "SELECT region_id, page_number, x, y, width, height, label, sort_order
         FROM template_redaction_region
         ORDER BY page_number ASC, sort_order ASC, region_id ASC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(TemplateRedactionRegion {
            region_id: row.get(0)?,
            page_number: row.get(1)?,
            x: row.get(2)?,
            y: row.get(3)?,
            width: row.get(4)?,
            height: row.get(5)?,
            label: row.get(6)?,
            sort_order: row.get(7)?,
        })
    })?;
    let mut regions = Vec::new();
    for row in rows {
        regions.push(row?);
    }
    Ok(regions)
}

pub(crate) fn workspace_status_label(
    status: &TemplateSetupStatus,
    skip_redaction: bool,
    has_no_redactions: bool,
) -> String {
    status.workspace_status_label(skip_redaction, has_no_redactions)
}

fn skipped_redaction_warning() -> WorkspaceWarning {
    WorkspaceWarning {
        code: Some("redaction_skipped".into()),
        message: "Redaction was skipped. Student-identifying regions will not be masked unless you add them before approval.".into(),
        scope: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        LmsUploadAttemptResult, LmsUploadMode, LmsUploadStudentResult, LmsUploadStudentStatus,
        ModerationFeedbackOverride, ModerationScoreOverride, QuestionRecord,
        ResultFinalizationRecord, ResultsLmsState, ResultsLmsTarget, RubricCriterion, RubricState,
        StudentWorkflowAnswer, StudentWorkflowCriterionResult, StudentWorkflowState,
        StudentWorkflowSubmission,
    };

    fn question_with_rubric() -> QuestionRecord {
        QuestionRecord {
            question_id: "question_1".into(),
            question_number: 1,
            page_number: 1,
            max_points: Some(5),
            text: "Explain.".into(),
            baseline_pdf_text: "Explain.".into(),
            region: None,
            source_artifact_id: None,
            image_path: None,
            analysis: Default::default(),
            rubric: RubricState {
                criteria: vec![
                    RubricCriterion {
                        criterion_id: "criterion_1".into(),
                        label: "Historical context".into(),
                        points: 2,
                        partial_credit_guidance: "Award for context.".into(),
                        source: "manual".into(),
                    },
                    RubricCriterion {
                        criterion_id: "criterion_2".into(),
                        label: "Specific evidence".into(),
                        points: 3,
                        partial_credit_guidance: "Award for evidence.".into(),
                        source: "manual".into(),
                    },
                ],
                ..RubricState::default()
            },
        }
    }

    fn workflow_with_unlabeled_results() -> StudentWorkflowState {
        StudentWorkflowState {
            status: "graded".into(),
            latest_job_id: None,
            submissions: vec![StudentWorkflowSubmission {
                student_ref: "student_1".into(),
                canonical_pdf_path: "/tmp/student_1.pdf".into(),
                page_count: 1,
                stage: "graded".into(),
                latest_job_id: None,
                failure_message: None,
                warnings: Vec::new(),
                page_artifacts: Vec::new(),
                alignment_pages: Vec::new(),
                detect_review: None,
                answers: vec![StudentWorkflowAnswer {
                    question_id: "question_1".into(),
                    question_number: 1,
                    crop_image_path: None,
                    pii_prescreen: None,
                    manual_grading_required: false,
                    manual_grading_reason: None,
                    moderation_eligible: false,
                    parse_status: "ok".into(),
                    parse_confidence: Some("high".into()),
                    parse_confidence_source: Some("combined".into()),
                    raw_parsed_text: Some("Answer".into()),
                    verified_text: Some("Answer".into()),
                    review_required: false,
                    verified: true,
                    stale: false,
                    grading_status: "draft_ready".into(),
                    grading_confidence: Some("high".into()),
                    grading_confidence_reason: None,
                    question_max_points: Some(5),
                    total_points_awarded: Some(5),
                    feedback_text: None,
                    criterion_results: vec![
                        StudentWorkflowCriterionResult {
                            criterion_index: 0,
                            label: String::new(),
                            points: 0,
                            points_awarded: 2,
                            rationale: "Context is correct.".into(),
                        },
                        StudentWorkflowCriterionResult {
                            criterion_index: 1,
                            label: "Criterion 2".into(),
                            points: 0,
                            points_awarded: 3,
                            rationale: "Evidence is correct.".into(),
                        },
                    ],
                    highlights: Vec::new(),
                    warnings: Vec::new(),
                }],
            }],
        }
    }

    #[test]
    fn workflow_criterion_labels_are_hydrated_from_current_rubric() {
        let question = question_with_rubric();
        let mut workflow = workflow_with_unlabeled_results();

        attach_workflow_criterion_labels(&[question], &mut workflow);

        let results = &workflow.submissions[0].answers[0].criterion_results;
        assert_eq!(results[0].label, "Historical context");
        assert_eq!(results[0].points, 2);
        assert_eq!(results[1].label, "Specific evidence");
        assert_eq!(results[1].points, 3);
    }

    #[test]
    fn manual_answers_seed_missing_criterion_rows_from_current_rubric() {
        let question = question_with_rubric();
        let mut workflow = StudentWorkflowState {
            status: "attention".into(),
            latest_job_id: None,
            submissions: vec![StudentWorkflowSubmission {
                student_ref: "student_1".into(),
                canonical_pdf_path: "/tmp/student_1.pdf".into(),
                page_count: 1,
                stage: "manual_grading".into(),
                latest_job_id: None,
                failure_message: None,
                warnings: Vec::new(),
                page_artifacts: Vec::new(),
                alignment_pages: Vec::new(),
                detect_review: None,
                answers: vec![StudentWorkflowAnswer {
                    question_id: "question_1".into(),
                    question_number: 1,
                    crop_image_path: Some("/tmp/q1.png".into()),
                    pii_prescreen: None,
                    manual_grading_required: true,
                    manual_grading_reason: Some("pii_detected".into()),
                    moderation_eligible: true,
                    parse_status: "blocked".into(),
                    parse_confidence: None,
                    parse_confidence_source: None,
                    raw_parsed_text: None,
                    verified_text: None,
                    review_required: false,
                    verified: false,
                    stale: false,
                    grading_status: "manual_required".into(),
                    grading_confidence: None,
                    grading_confidence_reason: None,
                    question_max_points: Some(5),
                    total_points_awarded: None,
                    feedback_text: None,
                    criterion_results: Vec::new(),
                    highlights: Vec::new(),
                    warnings: Vec::new(),
                }],
            }],
        };

        attach_workflow_criterion_labels(&[question], &mut workflow);

        let results = &workflow.submissions[0].answers[0].criterion_results;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].label, "Historical context");
        assert_eq!(results[0].points_awarded, 0);
        assert_eq!(results[1].label, "Specific evidence");
        assert_eq!(results[1].points_awarded, 0);
    }

    fn result_question(question_id: &str, number: i64, max_points: i64) -> QuestionRecord {
        QuestionRecord {
            question_id: question_id.into(),
            question_number: number,
            page_number: number,
            max_points: Some(max_points),
            text: format!("Question {number}"),
            baseline_pdf_text: format!("Question {number}"),
            region: None,
            source_artifact_id: None,
            image_path: None,
            analysis: Default::default(),
            rubric: Default::default(),
        }
    }

    fn graded_submission_with_answer(
        student_ref: &str,
        question_id: &str,
        total_points_awarded: Option<i64>,
        feedback_text: Option<&str>,
    ) -> StudentWorkflowSubmission {
        StudentWorkflowSubmission {
            student_ref: student_ref.into(),
            canonical_pdf_path: format!("/tmp/{student_ref}.pdf"),
            page_count: 1,
            stage: "graded".into(),
            latest_job_id: None,
            failure_message: None,
            warnings: Vec::new(),
            page_artifacts: Vec::new(),
            alignment_pages: Vec::new(),
            detect_review: None,
            answers: vec![StudentWorkflowAnswer {
                question_id: question_id.into(),
                question_number: 1,
                crop_image_path: None,
                pii_prescreen: None,
                manual_grading_required: false,
                manual_grading_reason: None,
                moderation_eligible: true,
                parse_status: "ok".into(),
                parse_confidence: Some("high".into()),
                parse_confidence_source: Some("combined".into()),
                raw_parsed_text: Some("Answer".into()),
                verified_text: Some("Answer".into()),
                review_required: false,
                verified: true,
                stale: false,
                grading_status: "draft_ready".into(),
                grading_confidence: Some("high".into()),
                grading_confidence_reason: None,
                question_max_points: Some(5),
                total_points_awarded,
                feedback_text: feedback_text.map(str::to_string),
                criterion_results: Vec::new(),
                highlights: Vec::new(),
                warnings: Vec::new(),
            }],
        }
    }

    #[test]
    fn results_rows_prefer_moderated_totals_and_feedback() {
        let rows = derive_results_lms_rows(
            &[result_question("question_1", 1, 5)],
            &[],
            &StudentWorkflowState {
                status: "graded".into(),
                latest_job_id: None,
                submissions: vec![graded_submission_with_answer(
                    "student_1",
                    "question_1",
                    Some(3),
                    Some("Automated"),
                )],
            },
            &ModerationState {
                score_overrides: vec![ModerationScoreOverride {
                    student_ref: "student_1".into(),
                    question_id: "question_1".into(),
                    moderated_total_points: 4,
                }],
                feedback_overrides: vec![ModerationFeedbackOverride {
                    student_ref: "student_1".into(),
                    question_id: "question_1".into(),
                    feedback_text: "Moderated".into(),
                }],
                question_reviews: Vec::new(),
            },
            &ResultsLmsState::default(),
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].aggregate_total, 4);
        assert!(rows[0].ready_to_finalize);
        assert_eq!(
            rows[0].question_rows[0].effective_feedback_text,
            "Moderated"
        );
        assert!(rows[0].question_rows[0].uses_moderated_total);
        assert!(rows[0].question_rows[0].uses_moderated_feedback);
    }

    #[test]
    fn results_rows_block_when_effective_score_is_missing() {
        let rows = derive_results_lms_rows(
            &[result_question("question_1", 1, 5)],
            &[],
            &StudentWorkflowState {
                status: "graded".into(),
                latest_job_id: None,
                submissions: vec![graded_submission_with_answer(
                    "student_1",
                    "question_1",
                    None,
                    Some("Automated"),
                )],
            },
            &ModerationState::default(),
            &ResultsLmsState::default(),
        );

        assert_eq!(rows.len(), 1);
        assert!(!rows[0].ready_to_finalize);
        assert_eq!(rows[0].blocked_reasons.len(), 1);
        assert!(rows[0].result_fingerprint.is_none());
    }

    #[test]
    fn results_rows_mark_stale_finalization_when_fingerprint_changes() {
        let initial_rows = derive_results_lms_rows(
            &[result_question("question_1", 1, 5)],
            &[],
            &StudentWorkflowState {
                status: "graded".into(),
                latest_job_id: None,
                submissions: vec![graded_submission_with_answer(
                    "student_1",
                    "question_1",
                    Some(3),
                    Some("Automated"),
                )],
            },
            &ModerationState::default(),
            &ResultsLmsState::default(),
        );
        let fingerprint = initial_rows[0]
            .result_fingerprint
            .clone()
            .expect("ready rows should have a fingerprint");

        let rows = derive_results_lms_rows(
            &[result_question("question_1", 1, 5)],
            &[],
            &StudentWorkflowState {
                status: "graded".into(),
                latest_job_id: None,
                submissions: vec![graded_submission_with_answer(
                    "student_1",
                    "question_1",
                    Some(4),
                    Some("Updated"),
                )],
            },
            &ModerationState::default(),
            &ResultsLmsState {
                finalization_records: vec![ResultFinalizationRecord {
                    student_ref: "student_1".into(),
                    result_fingerprint: fingerprint,
                    finalized_at: "1".into(),
                }],
                ..ResultsLmsState::default()
            },
        );

        assert!(!rows[0].finalized);
        assert!(rows[0].stale_finalization);
    }

    #[test]
    fn results_rows_derive_latest_upload_status_from_current_target() {
        let fingerprint = derive_results_lms_rows(
            &[result_question("question_1", 1, 5)],
            &[],
            &StudentWorkflowState {
                status: "graded".into(),
                latest_job_id: None,
                submissions: vec![graded_submission_with_answer(
                    "student_1",
                    "question_1",
                    Some(3),
                    Some("Automated"),
                )],
            },
            &ModerationState::default(),
            &ResultsLmsState::default(),
        )[0]
        .result_fingerprint
        .clone()
        .expect("fingerprint");

        let rows = derive_results_lms_rows(
            &[result_question("question_1", 1, 5)],
            &[],
            &StudentWorkflowState {
                status: "graded".into(),
                latest_job_id: None,
                submissions: vec![graded_submission_with_answer(
                    "student_1",
                    "question_1",
                    Some(3),
                    Some("Automated"),
                )],
            },
            &ModerationState::default(),
            &ResultsLmsState {
                selected_target: Some(ResultsLmsTarget {
                    provider: "canvas".into(),
                    course_id: "course-1".into(),
                    assignment_id: "assignment-1".into(),
                }),
                finalization_records: vec![ResultFinalizationRecord {
                    student_ref: "student_1".into(),
                    result_fingerprint: fingerprint.clone(),
                    finalized_at: "1".into(),
                }],
                upload_attempts: vec![
                    LmsUploadAttemptResult {
                        attempt_id: "attempt_old".into(),
                        mode: LmsUploadMode::Live,
                        provider: "canvas".into(),
                        course_id: "course-1".into(),
                        assignment_id: "assignment-1".into(),
                        started_at: "1".into(),
                        finished_at: "2".into(),
                        attempted_count: 1,
                        success_count: 0,
                        failure_count: 1,
                        student_results: vec![LmsUploadStudentResult {
                            student_ref: "student_1".into(),
                            result_fingerprint: fingerprint.clone(),
                            status: LmsUploadStudentStatus::Failed,
                            sanitized_error: Some("Failed".into()),
                        }],
                    },
                    LmsUploadAttemptResult {
                        attempt_id: "attempt_new".into(),
                        mode: LmsUploadMode::Live,
                        provider: "canvas".into(),
                        course_id: "course-1".into(),
                        assignment_id: "assignment-1".into(),
                        started_at: "3".into(),
                        finished_at: "4".into(),
                        attempted_count: 1,
                        success_count: 1,
                        failure_count: 0,
                        student_results: vec![LmsUploadStudentResult {
                            student_ref: "student_1".into(),
                            result_fingerprint: fingerprint,
                            status: LmsUploadStudentStatus::Uploaded,
                            sanitized_error: None,
                        }],
                    },
                ],
                ..ResultsLmsState::default()
            },
        );

        assert!(rows[0].uploaded);
        assert!(!rows[0].upload_failed);
        assert_eq!(
            rows[0].last_upload_attempt_id.as_deref(),
            Some("attempt_new")
        );
    }

    #[test]
    fn results_rows_preserve_prior_live_upload_when_later_retry_fails() {
        let fingerprint = derive_results_lms_rows(
            &[result_question("question_1", 1, 5)],
            &[],
            &StudentWorkflowState {
                status: "graded".into(),
                latest_job_id: None,
                submissions: vec![graded_submission_with_answer(
                    "student_1",
                    "question_1",
                    Some(3),
                    Some("Automated"),
                )],
            },
            &ModerationState::default(),
            &ResultsLmsState::default(),
        )[0]
        .result_fingerprint
        .clone()
        .expect("fingerprint");

        let rows = derive_results_lms_rows(
            &[result_question("question_1", 1, 5)],
            &[],
            &StudentWorkflowState {
                status: "graded".into(),
                latest_job_id: None,
                submissions: vec![graded_submission_with_answer(
                    "student_1",
                    "question_1",
                    Some(3),
                    Some("Automated"),
                )],
            },
            &ModerationState::default(),
            &ResultsLmsState {
                selected_target: Some(ResultsLmsTarget {
                    provider: "canvas".into(),
                    course_id: "course-1".into(),
                    assignment_id: "assignment-1".into(),
                }),
                finalization_records: vec![ResultFinalizationRecord {
                    student_ref: "student_1".into(),
                    result_fingerprint: fingerprint.clone(),
                    finalized_at: "1".into(),
                }],
                upload_attempts: vec![
                    LmsUploadAttemptResult {
                        attempt_id: "attempt_success".into(),
                        mode: LmsUploadMode::Live,
                        provider: "canvas".into(),
                        course_id: "course-1".into(),
                        assignment_id: "assignment-1".into(),
                        started_at: "1".into(),
                        finished_at: "2".into(),
                        attempted_count: 1,
                        success_count: 1,
                        failure_count: 0,
                        student_results: vec![LmsUploadStudentResult {
                            student_ref: "student_1".into(),
                            result_fingerprint: fingerprint.clone(),
                            status: LmsUploadStudentStatus::Uploaded,
                            sanitized_error: None,
                        }],
                    },
                    LmsUploadAttemptResult {
                        attempt_id: "attempt_retry_failed".into(),
                        mode: LmsUploadMode::Live,
                        provider: "canvas".into(),
                        course_id: "course-1".into(),
                        assignment_id: "assignment-1".into(),
                        started_at: "3".into(),
                        finished_at: "4".into(),
                        attempted_count: 1,
                        success_count: 0,
                        failure_count: 1,
                        student_results: vec![LmsUploadStudentResult {
                            student_ref: "student_1".into(),
                            result_fingerprint: fingerprint,
                            status: LmsUploadStudentStatus::Failed,
                            sanitized_error: Some("Duplicate rejected".into()),
                        }],
                    },
                ],
                ..ResultsLmsState::default()
            },
        );

        assert!(rows[0].uploaded);
        assert!(!rows[0].upload_failed);
        assert_eq!(
            rows[0].last_upload_attempt_id.as_deref(),
            Some("attempt_success")
        );
    }
}
