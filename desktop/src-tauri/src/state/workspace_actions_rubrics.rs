// SPDX-License-Identifier: AGPL-3.0-only
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};
use uuid::Uuid;

use crate::errors::{HostError, HostResult};
use crate::models::{
    AppSettings, ExamWorkspaceState, ProjectConfig, RubricApprovalBasis, RubricCriterion,
    RubricState, RubricUpdateInput, WorkspaceWarning,
};
use crate::project_store;
use crate::worker::CompletedWorkerJob;

use super::shared::{
    cli_instructor_profile_json, current_timestamp, llm_config_json, parse_warnings,
    required_string, success_data,
};
use super::{
    run_reserved_job, start_runtime_job, AppStateInner, RuntimeEventSink, RuntimeJobRequest,
};

enum RubricSaveDisposition {
    Approve,
    PreserveApproval,
    Draft { stale_answers: bool },
}

pub(crate) struct GenerateRubricPrepared {
    pub(crate) worker_request_payload: Value,
    pub(crate) persisted_request_payload: Value,
    pub(crate) output_artifacts_dir: PathBuf,
}

pub(crate) fn prepare_generate_rubric_for_job(
    project_path: &Path,
    question_id: &str,
    replace_existing: bool,
    settings: &AppSettings,
    job_id: &str,
) -> HostResult<GenerateRubricPrepared> {
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    let project_config = project_store::load_project_config(&rusqlite::Connection::open(
        project_store::schema::project_db_path(project_path),
    )?)?;
    let question = workspace
        .questions
        .into_iter()
        .find(|item| item.question_id == question_id)
        .ok_or_else(|| HostError::Validation(format!("Question '{question_id}' was not found.")))?;
    let analysis = question.analysis.clone();
    let question_text_clean = analysis.question_text_clean.clone().ok_or_else(|| {
        HostError::Validation("Question analysis is required before rubric generation.".into())
    })?;
    let question_context = analysis.question_context.clone().unwrap_or_default();
    let total_points = question.max_points.ok_or_else(|| {
        HostError::Validation("Question max points must be set before rubric generation.".into())
    })?;
    if total_points <= 0 {
        return Err(HostError::Validation(
            "Question max points must be greater than zero before rubric generation.".into(),
        ));
    }
    let profile = &project_config.instructor_profile;
    let (cli_max_points, minimum_credit_row_points) = if profile.include_minimum_credit_criterion {
        let min_pts = resolved_minimum_credit_points(total_points, profile.minimum_credit_percent)?;
        if total_points <= min_pts {
            return Err(HostError::Validation(format!(
                "Question max points ({total_points}) must be greater than the rounded minimum credit ({min_pts})."
            )));
        }
        (total_points - min_pts, Some(min_pts))
    } else {
        (total_points, None)
    };
    let output_artifacts_dir =
        project_store::command_output_dir(project_path, "exam.generate-rubric", job_id);
    Ok(GenerateRubricPrepared {
        worker_request_payload: json!({
            "question_id": question.question_id,
            "max_points": cli_max_points,
            "subject": project_config.subject.clone().unwrap_or_else(|| "General".into()),
            "question_text_clean": question_text_clean,
            "question_context": question_context,
            "instructor_profile": cli_instructor_profile_json(profile),
            "host_prepends_minimum_credit_criterion": profile.include_minimum_credit_criterion,
            "providers": { "llm_provider": settings.llm_provider.clone() },
            "llm_config": llm_config_json(settings),
        }),
        persisted_request_payload: json!({
            "question_id": question.question_id,
            "replace_existing": replace_existing,
            "minimum_credit_row_points": minimum_credit_row_points,
        }),
        output_artifacts_dir,
    })
}

pub(crate) fn generate_question_rubric(
    state: &Arc<AppStateInner>,
    project_path: &Path,
    question_id: &str,
    replace_existing: bool,
    settings: &AppSettings,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<ExamWorkspaceState> {
    let reserved = start_runtime_job(state, event_sink, "exam.generate-rubric")?;
    let prepared = prepare_generate_rubric_for_job(
        project_path,
        question_id,
        replace_existing,
        settings,
        &reserved.job_id,
    )?;
    std::fs::create_dir_all(&prepared.output_artifacts_dir)?;

    let _completed = run_reserved_job(
        state,
        event_sink,
        reserved,
        RuntimeJobRequest {
            command_name: "exam.generate-rubric",
            worker_request_payload: prepared.worker_request_payload,
            persisted_request_payload: prepared.persisted_request_payload,
            output_artifacts_dir: Some(prepared.output_artifacts_dir.as_path()),
            project_path: Some(project_path),
            stdin_bytes: None,
        },
    )?;
    project_store::load_exam_workspace_state(project_path)
}

fn validate_rubric_approve_points_sum(
    max_points: Option<i64>,
    criteria: &[RubricCriterion],
) -> HostResult<()> {
    let total = max_points.ok_or_else(|| {
        HostError::Validation("Set question max points before approving the rubric.".into())
    })?;
    if total <= 0 {
        return Err(HostError::Validation(
            "Question max points must be greater than zero before approving the rubric.".into(),
        ));
    }
    let sum: i64 = criteria.iter().map(|c| c.points).sum();
    if sum != total {
        return Err(HostError::Validation(format!(
            "Rubric criteria must sum to {total} points (currently {sum})."
        )));
    }
    Ok(())
}

pub(crate) fn save_rubric_update(
    project_path: &Path,
    input: RubricUpdateInput,
) -> HostResult<ExamWorkspaceState> {
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    let question = workspace
        .questions
        .iter()
        .find(|q| q.question_id == input.question_id)
        .ok_or_else(|| {
            HostError::Validation(format!("Question '{}' was not found.", input.question_id))
        })?;
    if input.approve {
        validate_rubric_approve_points_sum(question.max_points, &input.criteria)?;
    }
    let current = project_store::load_rubric_state(project_path, &input.question_id)?;
    let currently_approved = current.is_approved();
    let structural_change = rubric_structural_change(&current.criteria, &input.criteria);
    let rubric_edit_impact = input.rubric_edit_impact.as_deref();
    let mut next = RubricState {
        status: current.status.clone(),
        criteria: ensure_criterion_ids(input.criteria),
        warnings: current.warnings,
        approved_at: current.approved_at,
        latest_job_id: current.latest_job_id,
        approval_basis: current.approval_basis,
    };
    match rubric_save_disposition(
        input.approve,
        currently_approved,
        structural_change,
        current.criteria != next.criteria,
        rubric_edit_impact,
    ) {
        RubricSaveDisposition::Approve => {
            next.mark_approved(
                current_timestamp(),
                rubric_approval_basis(question, &next.criteria),
            );
        }
        RubricSaveDisposition::PreserveApproval => {
            next.status = crate::workflow_status::RubricStatus::Approved
                .as_str()
                .to_string();
            next.approval_basis = Some(rubric_approval_basis(question, &next.criteria));
        }
        RubricSaveDisposition::Draft { stale_answers } => {
            if stale_answers {
                project_store::mark_student_answers_stale_for_questions(
                    project_path,
                    std::slice::from_ref(&input.question_id),
                )?;
            }
            next.mark_draft();
        }
    }
    project_store::save_rubric_state(project_path, &input.question_id, &next)?;
    project_store::load_exam_workspace_state(project_path)
}

fn rubric_save_disposition(
    approve: bool,
    currently_approved: bool,
    structural_change: bool,
    criteria_changed: bool,
    rubric_edit_impact: Option<&str>,
) -> RubricSaveDisposition {
    if approve {
        return RubricSaveDisposition::Approve;
    }
    if currently_approved && !structural_change && rubric_edit_impact == Some("minor") {
        return RubricSaveDisposition::PreserveApproval;
    }
    RubricSaveDisposition::Draft {
        stale_answers: currently_approved
            && (criteria_changed || rubric_edit_impact == Some("grading")),
    }
}

pub(crate) fn rubric_approval_basis(
    question: &crate::models::QuestionRecord,
    criteria: &[RubricCriterion],
) -> RubricApprovalBasis {
    RubricApprovalBasis {
        question_text: question.text.clone(),
        question_context: question
            .analysis
            .question_context
            .clone()
            .unwrap_or_default(),
        max_points: question.max_points,
        criteria: criteria.to_vec(),
    }
}

fn rubric_structural_change(left: &[RubricCriterion], right: &[RubricCriterion]) -> bool {
    left.len() != right.len()
        || left.iter().zip(right.iter()).any(|(left, right)| {
            left.criterion_id != right.criterion_id || left.points != right.points
        })
}

pub(crate) fn save_project_config(
    project_path: &Path,
    config: ProjectConfig,
) -> HostResult<ExamWorkspaceState> {
    let mut connection =
        rusqlite::Connection::open(project_store::schema::project_db_path(project_path))?;
    let transaction = connection.transaction()?;
    project_store::save_project_config(&transaction, &config)?;
    transaction.commit()?;
    project_store::load_exam_workspace_state(project_path)
}

pub(crate) fn resolved_minimum_credit_points(total_points: i64, percent: i64) -> HostResult<i64> {
    if !(1..=100).contains(&percent) {
        return Err(HostError::Validation(
            "Minimum credit percent must be between 1 and 100.".into(),
        ));
    }
    if total_points < 2 {
        return Err(HostError::Validation(
            "Minimum credit requires the question to have at least 2 max points.".into(),
        ));
    }
    let raw = ((total_points as f64) * (percent as f64) / 100.0).round() as i64;
    let min_pts = raw.clamp(1, total_points - 1);
    Ok(min_pts)
}

pub(crate) fn persist_generated_rubric(
    project_path: &Path,
    question_id: &str,
    completed: &CompletedWorkerJob,
    replace_existing: bool,
    minimum_credit_row_points: Option<i64>,
) -> HostResult<()> {
    let data = success_data(&completed.result.envelope)?;
    let rubric_draft = data
        .get("rubric_draft")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            HostError::Protocol(
                "exam.generate-rubric success envelope was missing rubric_draft.".into(),
            )
        })?;
    let criteria_json = rubric_draft
        .get("criteria")
        .and_then(Value::as_array)
        .ok_or_else(|| HostError::Protocol("rubric_draft was missing criteria.".into()))?;
    let criteria = rubric_criteria_from_draft_json(question_id, criteria_json)?;
    let mut warnings = parse_warnings(rubric_draft.get("warnings"))?;
    strip_minimum_credit_noise_warnings(&mut warnings, minimum_credit_row_points);
    let project_config = project_store::load_project_config(&rusqlite::Connection::open(
        project_store::schema::project_db_path(project_path),
    )?)?;
    let subject_label = project_config
        .subject
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("General");
    let mut generated = match minimum_credit_row_points {
        Some(points) => vec![minimum_credit_criterion(subject_label, points)],
        None => Vec::new(),
    };
    generated.extend(criteria);
    let _ = project_store::append_generated_rubric(
        project_path,
        question_id,
        &generated,
        warnings,
        &completed.job_id,
        replace_existing,
    )?;
    Ok(())
}

fn rubric_criteria_from_draft_json(
    question_id: &str,
    criteria_json: &[Value],
) -> HostResult<Vec<RubricCriterion>> {
    criteria_json
        .iter()
        .enumerate()
        .map(|(index, criterion)| {
            Ok(RubricCriterion {
                criterion_id: format!("generated_{question_id}_{index}"),
                label: required_string(criterion, "label")?,
                points: criterion
                    .get("points")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| {
                        HostError::Protocol("rubric criterion was missing points.".into())
                    })?,
                partial_credit_guidance: required_string(criterion, "partial_credit_guidance")?,
                source: "generated".into(),
            })
        })
        .collect::<HostResult<Vec<RubricCriterion>>>()
}

fn strip_minimum_credit_noise_warnings(
    warnings: &mut Vec<WorkspaceWarning>,
    minimum_credit_row_points: Option<i64>,
) {
    if minimum_credit_row_points.is_none() {
        return;
    }
    warnings.retain(|w| {
        let msg = w.message.to_lowercase();
        !(msg.contains("minimum credit")
            && (msg.contains("missing") || msg.contains("must include")))
    });
}

fn minimum_credit_criterion(subject: &str, points: i64) -> RubricCriterion {
    let p = points.max(1);
    let points_label = if p == 1 {
        "1 point".to_string()
    } else {
        format!("{p} points")
    };
    RubricCriterion {
        criterion_id: format!("minimum_credit_{}", Uuid::new_v4()),
        label: format!("Attempt credit for any non-blank {subject}-related response"),
        points: p,
        partial_credit_guidance: format!(
            "Award {points_label} for any non-blank answer that is minimally related to {subject}, even if incorrect."
        ),
        source: "minimum_credit".into(),
    }
}

fn ensure_criterion_ids(criteria: Vec<RubricCriterion>) -> Vec<RubricCriterion> {
    criteria
        .into_iter()
        .enumerate()
        .map(|(index, mut criterion)| {
            if criterion.criterion_id.trim().is_empty() {
                criterion.criterion_id = format!("criterion_{index}_{}", Uuid::new_v4());
            }
            criterion
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_criterion_ids, minimum_credit_criterion, resolved_minimum_credit_points,
        rubric_criteria_from_draft_json, save_rubric_update, strip_minimum_credit_noise_warnings,
        validate_rubric_approve_points_sum,
    };
    use crate::models::{
        RubricApprovalBasis, RubricCriterion, RubricState, RubricUpdateInput,
        StudentWorkflowAnswer, StudentWorkflowState, StudentWorkflowSubmission, WorkspaceWarning,
    };
    use crate::project_store;
    use rusqlite::Connection;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn resolved_minimum_credit_points_validates_range_and_rounding() {
        assert_eq!(
            resolved_minimum_credit_points(5, 10).expect("minimum credit should resolve"),
            1
        );
        assert_eq!(
            resolved_minimum_credit_points(9, 40).expect("minimum credit should resolve"),
            4
        );
        assert!(resolved_minimum_credit_points(5, 0).is_err());
        assert!(resolved_minimum_credit_points(1, 50).is_err());
    }

    #[test]
    fn minimum_credit_criterion_uses_subject_and_points_label() {
        let criterion = minimum_credit_criterion("History", 2);
        assert_eq!(
            criterion.label,
            "Attempt credit for any non-blank History-related response"
        );
        assert!(criterion.partial_credit_guidance.contains("2 points"));
        assert_eq!(criterion.source, "minimum_credit");
    }

    #[test]
    fn rubric_criteria_from_draft_json_assigns_generated_ids() {
        let criteria = rubric_criteria_from_draft_json(
            "q1",
            serde_json::json!([
                {
                    "label": "Correct idea",
                    "points": 3,
                    "partial_credit_guidance": "Award when mostly correct."
                }
            ])
            .as_array()
            .expect("criteria array"),
        )
        .expect("criteria should parse");

        assert_eq!(criteria.len(), 1);
        assert_eq!(criteria[0].criterion_id, "generated_q1_0");
        assert_eq!(criteria[0].points, 3);
    }

    #[test]
    fn strip_minimum_credit_noise_warnings_removes_minimum_credit_prompts() {
        let mut warnings = vec![
            WorkspaceWarning {
                code: None,
                message: "Minimum credit criterion missing from rubric.".into(),
                scope: None,
            },
            WorkspaceWarning {
                code: None,
                message: "Keep this unrelated warning.".into(),
                scope: None,
            },
        ];

        strip_minimum_credit_noise_warnings(&mut warnings, Some(1));

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, "Keep this unrelated warning.");
    }

    #[test]
    fn ensure_criterion_ids_fills_only_blank_ids() {
        let criteria = ensure_criterion_ids(vec![
            RubricCriterion {
                criterion_id: "".into(),
                label: "A".into(),
                points: 1,
                partial_credit_guidance: "A".into(),
                source: "manual".into(),
            },
            RubricCriterion {
                criterion_id: "existing".into(),
                label: "B".into(),
                points: 2,
                partial_credit_guidance: "B".into(),
                source: "manual".into(),
            },
        ]);

        assert!(criteria[0].criterion_id.starts_with("criterion_0_"));
        assert_eq!(criteria[1].criterion_id, "existing");
    }

    fn sample_criterion(points: i64) -> RubricCriterion {
        RubricCriterion {
            criterion_id: "c1".into(),
            label: "A".into(),
            points,
            partial_credit_guidance: "g".into(),
            source: "manual".into(),
        }
    }

    #[test]
    fn validate_rubric_approve_points_sum_matches_max() {
        assert!(validate_rubric_approve_points_sum(
            Some(5),
            &[sample_criterion(2), sample_criterion(3)]
        )
        .is_ok());
    }

    #[test]
    fn validate_rubric_approve_points_sum_rejects_mismatch() {
        let err = validate_rubric_approve_points_sum(
            Some(5),
            &[sample_criterion(2), sample_criterion(2)],
        )
        .expect_err("sum should not match");
        assert!(
            err.to_string().contains("must sum to 5"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn validate_rubric_approve_points_sum_requires_max_points() {
        assert!(validate_rubric_approve_points_sum(None, &[sample_criterion(5)]).is_err());
    }

    #[test]
    fn grading_impact_revoke_without_content_changes_marks_existing_grading_stale() {
        let project_path = std::env::temp_dir().join(format!(
            "scriptscore-rubric-revoke-stale-{}",
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

        let criterion = sample_criterion(5);
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
                    criteria: vec![criterion.clone()],
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

        save_rubric_update(
            &project_path,
            RubricUpdateInput {
                question_id: "question_1".into(),
                criteria: vec![criterion],
                approve: false,
                rubric_edit_impact: Some("grading".into()),
            },
        )
        .expect("rubric revoke should save");

        let rubric = project_store::load_rubric_state(&project_path, "question_1")
            .expect("rubric should load");
        assert_eq!(rubric.status, "draft");
        assert_eq!(rubric.approved_at, None);
        let workflow = project_store::load_student_workflow_state(&project_path)
            .expect("workflow should load");
        assert!(workflow.submissions[0].answers[0].stale);

        drop(connection);
        std::fs::remove_dir_all(&project_path).expect("project dir should clean up");
    }
}
