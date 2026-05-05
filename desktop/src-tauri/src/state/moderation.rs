// SPDX-License-Identifier: AGPL-3.0-only
use std::path::Path;

use crate::errors::{HostError, HostResult};
use crate::models::{
    ExamWorkspaceState, ModerationFeedbackOverride, ModerationQuestionReview,
    ModerationScoreOverride, ModerationState, SaveCriterionScoreInput, SaveModeratedFeedbackInput,
    SaveModeratedScoreInput, SetModerationQuestionReviewedInput, StudentWorkflowCriterionResult,
};
use crate::project_store;

fn validate_points_against_rubric(
    workspace: &ExamWorkspaceState,
    input: &SaveCriterionScoreInput,
) -> HostResult<()> {
    if input.points_awarded < 0 {
        return Err(HostError::Validation(
            "Points awarded cannot be negative.".into(),
        ));
    }

    let question = workspace
        .questions
        .iter()
        .find(|q| q.question_id == input.question_id)
        .ok_or_else(|| {
            HostError::Validation(format!("Question '{}' was not found.", input.question_id))
        })?;

    let rubric = &question.rubric;
    let criterion = rubric
        .criteria
        .get(usize::try_from(input.criterion_index).unwrap_or(usize::MAX))
        .ok_or_else(|| {
            HostError::Validation(format!(
                "Criterion index {} is out of range for question '{}' ({} criteria).",
                input.criterion_index,
                input.question_id,
                rubric.criteria.len()
            ))
        })?;

    if input.points_awarded > criterion.points {
        return Err(HostError::Validation(format!(
            "Points awarded ({}) cannot exceed criterion max points ({}).",
            input.points_awarded, criterion.points
        )));
    }

    Ok(())
}

fn apply_criterion_score_to_workflow(
    workspace: &ExamWorkspaceState,
    workflow: &mut crate::models::StudentWorkflowState,
    input: &SaveCriterionScoreInput,
) -> HostResult<()> {
    let question = workspace
        .questions
        .iter()
        .find(|q| q.question_id == input.question_id)
        .ok_or_else(|| {
            HostError::Validation(format!("Question '{}' was not found.", input.question_id))
        })?;
    let submission = workflow
        .submissions
        .iter_mut()
        .find(|s| s.student_ref == input.student_ref)
        .ok_or_else(|| {
            HostError::Validation(format!(
                "Student '{}' was not found in the workflow.",
                input.student_ref
            ))
        })?;

    let answer = submission
        .answers
        .iter_mut()
        .find(|a| a.question_id == input.question_id)
        .ok_or_else(|| {
            HostError::Validation(format!(
                "Question '{}' was not found for student '{}'.",
                input.question_id, input.student_ref
            ))
        })?;

    for (criterion_index, criterion) in question.rubric.criteria.iter().enumerate() {
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

    let criterion_result_count = answer.criterion_results.len();
    let target = answer
        .criterion_results
        .iter_mut()
        .find(|result| result.criterion_index == input.criterion_index)
        .ok_or_else(|| {
            HostError::Validation(format!(
                "Criterion index {} was not found in this answer ({} criterion results).",
                input.criterion_index, criterion_result_count
            ))
        })?;

    target.points_awarded = input.points_awarded;

    answer.total_points_awarded = Some(
        answer
            .criterion_results
            .iter()
            .map(|c: &StudentWorkflowCriterionResult| c.points_awarded)
            .sum(),
    );

    Ok(())
}

pub(crate) fn save_criterion_score(
    project_path: &Path,
    input: SaveCriterionScoreInput,
) -> HostResult<ExamWorkspaceState> {
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    validate_points_against_rubric(&workspace, &input)?;

    let mut workflow = project_store::load_student_workflow_state(project_path)?;
    apply_criterion_score_to_workflow(&workspace, &mut workflow, &input)?;

    let submission = workflow
        .submissions
        .iter()
        .find(|submission| submission.student_ref == input.student_ref)
        .cloned()
        .ok_or_else(|| {
            HostError::Validation(format!(
                "Student workflow state was missing '{}'.",
                input.student_ref
            ))
        })?;
    project_store::save_student_workflow_submissions(project_path, &[submission])?;
    project_store::load_exam_workspace_state(project_path)
}

fn validate_moderated_score(
    workspace: &ExamWorkspaceState,
    input: &SaveModeratedScoreInput,
) -> HostResult<i64> {
    if input.moderated_total_points < 0 {
        return Err(HostError::Validation(
            "Moderated points cannot be negative.".into(),
        ));
    }

    let question = workspace
        .questions
        .iter()
        .find(|q| q.question_id == input.question_id)
        .ok_or_else(|| {
            HostError::Validation(format!("Question '{}' was not found.", input.question_id))
        })?;
    let question_max_points = question.max_points.or_else(|| {
        workspace
            .student_workflow
            .submissions
            .iter()
            .flat_map(|submission| submission.answers.iter())
            .find(|answer| answer.question_id == input.question_id)
            .and_then(|answer| answer.question_max_points)
    });
    let max_points = question_max_points.ok_or_else(|| {
        HostError::Validation(format!(
            "Question '{}' has no max points for moderation.",
            input.question_id
        ))
    })?;
    if input.moderated_total_points > max_points {
        return Err(HostError::Validation(format!(
            "Moderated points ({}) cannot exceed question max points ({}).",
            input.moderated_total_points, max_points
        )));
    }

    let answer = workspace
        .student_workflow
        .submissions
        .iter()
        .find(|submission| submission.student_ref == input.student_ref)
        .and_then(|submission| {
            submission
                .answers
                .iter()
                .find(|answer| answer.question_id == input.question_id)
        })
        .ok_or_else(|| {
            HostError::Validation(format!(
                "Question '{}' was not found for student '{}'.",
                input.question_id, input.student_ref
            ))
        })?;
    if !answer.moderation_eligible {
        return Err(HostError::Validation(format!(
            "Question '{}' for student '{}' is not moderation-eligible.",
            input.question_id, input.student_ref
        )));
    }
    Ok(answer.total_points_awarded.unwrap_or(0))
}

fn upsert_score_override(
    moderation: &mut ModerationState,
    input: &SaveModeratedScoreInput,
    original_total_points: i64,
) {
    moderation
        .question_reviews
        .retain(|review| review.question_id != input.question_id);

    if input.moderated_total_points == original_total_points {
        moderation.score_overrides.retain(|entry| {
            !(entry.student_ref == input.student_ref && entry.question_id == input.question_id)
        });
        return;
    }

    if let Some(existing) = moderation.score_overrides.iter_mut().find(|entry| {
        entry.student_ref == input.student_ref && entry.question_id == input.question_id
    }) {
        existing.moderated_total_points = input.moderated_total_points;
    } else {
        moderation.score_overrides.push(ModerationScoreOverride {
            student_ref: input.student_ref.clone(),
            question_id: input.question_id.clone(),
            moderated_total_points: input.moderated_total_points,
        });
    }
}

fn validate_moderated_feedback<'a>(
    workspace: &'a ExamWorkspaceState,
    input: &SaveModeratedFeedbackInput,
) -> HostResult<&'a str> {
    let question_exists = workspace
        .questions
        .iter()
        .any(|question| question.question_id == input.question_id);
    if !question_exists {
        return Err(HostError::Validation(format!(
            "Question '{}' was not found.",
            input.question_id
        )));
    }

    let answer = workspace
        .student_workflow
        .submissions
        .iter()
        .find(|submission| submission.student_ref == input.student_ref)
        .and_then(|submission| {
            submission
                .answers
                .iter()
                .find(|answer| answer.question_id == input.question_id)
        })
        .ok_or_else(|| {
            HostError::Validation(format!(
                "Question '{}' was not found for student '{}'.",
                input.question_id, input.student_ref
            ))
        })?;
    if !answer.moderation_eligible {
        return Err(HostError::Validation(format!(
            "Question '{}' for student '{}' is not moderation-eligible.",
            input.question_id, input.student_ref
        )));
    }

    Ok(answer.feedback_text.as_deref().unwrap_or(""))
}

fn upsert_feedback_override(
    moderation: &mut ModerationState,
    input: &SaveModeratedFeedbackInput,
    original_feedback_text: &str,
) {
    let normalized_feedback = input.feedback_text.trim().to_string();
    let normalized_original = original_feedback_text.trim();
    if normalized_feedback == normalized_original {
        moderation.feedback_overrides.retain(|entry| {
            !(entry.student_ref == input.student_ref && entry.question_id == input.question_id)
        });
        return;
    }

    if let Some(existing) = moderation.feedback_overrides.iter_mut().find(|entry| {
        entry.student_ref == input.student_ref && entry.question_id == input.question_id
    }) {
        existing.feedback_text = normalized_feedback;
    } else {
        moderation
            .feedback_overrides
            .push(ModerationFeedbackOverride {
                student_ref: input.student_ref.clone(),
                question_id: input.question_id.clone(),
                feedback_text: normalized_feedback,
            });
    }
}

pub(crate) fn save_moderated_score(
    project_path: &Path,
    input: SaveModeratedScoreInput,
) -> HostResult<ExamWorkspaceState> {
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    let original_total_points = validate_moderated_score(&workspace, &input)?;

    let mut moderation = project_store::load_moderation_state(project_path)?;
    upsert_score_override(&mut moderation, &input, original_total_points);

    project_store::save_moderation_state(project_path, &moderation)?;
    project_store::load_exam_workspace_state(project_path)
}

pub(crate) fn save_moderated_feedback(
    project_path: &Path,
    input: SaveModeratedFeedbackInput,
) -> HostResult<ExamWorkspaceState> {
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    let original_feedback_text = validate_moderated_feedback(&workspace, &input)?;

    let mut moderation = project_store::load_moderation_state(project_path)?;
    upsert_feedback_override(&mut moderation, &input, original_feedback_text);

    project_store::save_moderation_state(project_path, &moderation)?;
    project_store::load_exam_workspace_state(project_path)
}

fn validate_question_has_moderation_rows(
    workspace: &ExamWorkspaceState,
    question_id: &str,
) -> HostResult<()> {
    let question_exists = workspace
        .questions
        .iter()
        .any(|q| q.question_id == question_id);
    if !question_exists {
        return Err(HostError::Validation(format!(
            "Question '{}' was not found.",
            question_id
        )));
    }

    let has_rows = workspace
        .student_workflow
        .submissions
        .iter()
        .flat_map(|submission| submission.answers.iter())
        .any(|answer| answer.question_id == question_id && answer.moderation_eligible);
    if !has_rows {
        return Err(HostError::Validation(format!(
            "Question '{}' has no moderation-eligible answers.",
            question_id
        )));
    }

    Ok(())
}

pub(crate) fn set_question_reviewed(
    project_path: &Path,
    input: SetModerationQuestionReviewedInput,
) -> HostResult<ExamWorkspaceState> {
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    validate_question_has_moderation_rows(&workspace, &input.question_id)?;

    let mut moderation = project_store::load_moderation_state(project_path)?;
    moderation
        .question_reviews
        .retain(|review| review.question_id != input.question_id);
    if input.reviewed {
        moderation.question_reviews.push(ModerationQuestionReview {
            question_id: input.question_id,
            reviewed_at: crate::project_store::schema::current_timestamp(),
        });
    }

    project_store::save_moderation_state(project_path, &moderation)?;
    project_store::load_exam_workspace_state(project_path)
}

#[cfg(test)]
mod tests {
    use super::{
        save_criterion_score, save_moderated_feedback, save_moderated_score, set_question_reviewed,
    };
    use crate::models::{
        InstructorProfile, ModerationState, QuestionAnalysisState, QuestionRecord, RubricCriterion,
        RubricState, SaveCriterionScoreInput, SaveModeratedFeedbackInput, SaveModeratedScoreInput,
        SetModerationQuestionReviewedInput, StudentWorkflowAnswer, StudentWorkflowState,
        StudentWorkflowSubmission, TemplateSetupPayload,
    };
    use crate::project_store;
    use crate::test_support::{lock_env_vars, EnvVarGuard};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn temp_root(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_millis()
        ))
    }

    fn create_project(test_root: &Path) -> PathBuf {
        let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", test_root);
        let created = project_store::create_project(
            "Moderation Test",
            Some("History".into()),
            None,
            None,
            &InstructorProfile::default(),
        )
        .expect("project should be created");
        PathBuf::from(created.project_path)
    }

    fn seed_question(project_path: &Path) {
        project_store::persist_template_setup_success(
            project_path,
            &TemplateSetupPayload::default(),
            &[],
            &[],
            &[QuestionRecord {
                question_id: "question_1".into(),
                question_number: 1,
                page_number: 1,
                max_points: Some(5),
                text: "Explain the event.".into(),
                baseline_pdf_text: "Explain the event.".into(),
                region: None,
                source_artifact_id: None,
                image_path: None,
                analysis: QuestionAnalysisState::default(),
                rubric: RubricState::default(),
            }],
        )
        .expect("template setup should persist");
        project_store::save_rubric_state(
            project_path,
            "question_1",
            &RubricState {
                status: "draft".into(),
                criteria: vec![
                    RubricCriterion {
                        criterion_id: "criterion_0".into(),
                        label: "Main point".into(),
                        points: 2,
                        partial_credit_guidance: "Award up to 2.".into(),
                        source: "manual".into(),
                    },
                    RubricCriterion {
                        criterion_id: "criterion_1".into(),
                        label: "Supporting detail".into(),
                        points: 3,
                        partial_credit_guidance: "Award up to 3.".into(),
                        source: "manual".into(),
                    },
                ],
                warnings: Vec::new(),
                approved_at: None,
                latest_job_id: None,
                approval_basis: None,
            },
        )
        .expect("rubric should persist");
    }

    fn seed_workflow(project_path: &Path) {
        project_store::save_student_workflow_state(
            project_path,
            &StudentWorkflowState {
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
                        total_points_awarded: Some(4),
                        feedback_text: Some("Solid answer.".into()),
                        criterion_results: vec![
                            crate::models::StudentWorkflowCriterionResult {
                                criterion_index: 1,
                                label: "Supporting detail".into(),
                                points: 3,
                                points_awarded: 3,
                                rationale: "Has the detail.".into(),
                            },
                            crate::models::StudentWorkflowCriterionResult {
                                criterion_index: 0,
                                label: "Main point".into(),
                                points: 2,
                                points_awarded: 1,
                                rationale: "Missed part of the main point.".into(),
                            },
                        ],
                        highlights: Vec::new(),
                        warnings: Vec::new(),
                    }],
                }],
            },
        )
        .expect("workflow should persist");
    }

    #[test]
    fn save_criterion_score_updates_matching_criterion_index_not_vector_position() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-moderation-save-score");
        let project_path = create_project(&test_root);
        seed_question(&project_path);
        seed_workflow(&project_path);

        let workspace = save_criterion_score(
            &project_path,
            SaveCriterionScoreInput {
                student_ref: "student_1".into(),
                question_id: "question_1".into(),
                criterion_index: 0,
                points_awarded: 2,
            },
        )
        .expect("criterion score should save");

        let answer = &workspace.student_workflow.submissions[0].answers[0];
        assert_eq!(answer.total_points_awarded, Some(5));
        assert_eq!(answer.feedback_text.as_deref(), Some("Solid answer."));
        let criterion_zero = answer
            .criterion_results
            .iter()
            .find(|result| result.criterion_index == 0)
            .expect("criterion 0 should still exist");
        let criterion_one = answer
            .criterion_results
            .iter()
            .find(|result| result.criterion_index == 1)
            .expect("criterion 1 should still exist");
        assert_eq!(criterion_zero.points_awarded, 2);
        assert_eq!(criterion_one.points_awarded, 3);
    }

    #[test]
    fn save_criterion_score_seeds_manual_answer_criteria_from_rubric() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-moderation-save-score-manual");
        let project_path = create_project(&test_root);
        seed_question(&project_path);
        project_store::save_student_workflow_state(
            &project_path,
            &StudentWorkflowState {
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
            },
        )
        .expect("manual workflow should persist");

        let workspace = save_criterion_score(
            &project_path,
            SaveCriterionScoreInput {
                student_ref: "student_1".into(),
                question_id: "question_1".into(),
                criterion_index: 1,
                points_awarded: 2,
            },
        )
        .expect("criterion score should save for manual answer");

        let answer = &workspace.student_workflow.submissions[0].answers[0];
        assert_eq!(answer.criterion_results.len(), 2);
        assert_eq!(answer.criterion_results[0].criterion_index, 0);
        assert_eq!(answer.criterion_results[0].points_awarded, 0);
        assert_eq!(answer.criterion_results[1].criterion_index, 1);
        assert_eq!(answer.criterion_results[1].points_awarded, 2);
        assert_eq!(answer.total_points_awarded, Some(2));
    }

    #[test]
    fn save_criterion_score_rejects_missing_criterion_result_by_identifier() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-moderation-save-score-missing");
        let project_path = create_project(&test_root);
        seed_question(&project_path);
        seed_workflow(&project_path);

        let error = save_criterion_score(
            &project_path,
            SaveCriterionScoreInput {
                student_ref: "student_1".into(),
                question_id: "question_1".into(),
                criterion_index: 5,
                points_awarded: 1,
            },
        )
        .expect_err("missing criterion result should fail");

        assert!(
            error
                .to_string()
                .contains("Criterion index 5 is out of range for question 'question_1'"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn save_criterion_score_rejects_points_above_criterion_max() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-moderation-save-score-range");
        let project_path = create_project(&test_root);
        seed_question(&project_path);
        seed_workflow(&project_path);

        let error = save_criterion_score(
            &project_path,
            SaveCriterionScoreInput {
                student_ref: "student_1".into(),
                question_id: "question_1".into(),
                criterion_index: 0,
                points_awarded: 3,
            },
        )
        .expect_err("score above criterion max should fail");

        assert!(
            error
                .to_string()
                .contains("Points awarded (3) cannot exceed criterion max points (2)."),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn save_moderated_score_persists_override_without_touching_original_criteria() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-moderation-save-aggregate");
        let project_path = create_project(&test_root);
        seed_question(&project_path);
        seed_workflow(&project_path);

        let workspace = save_moderated_score(
            &project_path,
            SaveModeratedScoreInput {
                student_ref: "student_1".into(),
                question_id: "question_1".into(),
                moderated_total_points: 2,
            },
        )
        .expect("moderated score should save");

        let answer = &workspace.student_workflow.submissions[0].answers[0];
        assert_eq!(answer.total_points_awarded, Some(4));
        assert_eq!(answer.criterion_results[0].points_awarded, 3);
        assert_eq!(answer.criterion_results[1].points_awarded, 1);
        assert_eq!(workspace.moderation_state.score_overrides.len(), 1);
        assert!(workspace.moderation_state.feedback_overrides.is_empty());
        assert_eq!(
            workspace.moderation_state.score_overrides[0].moderated_total_points,
            2
        );
    }

    #[test]
    fn save_moderated_score_clears_review_and_redundant_override() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-moderation-clear-review");
        let project_path = create_project(&test_root);
        seed_question(&project_path);
        seed_workflow(&project_path);
        project_store::save_moderation_state(
            &project_path,
            &ModerationState {
                score_overrides: Vec::new(),
                feedback_overrides: Vec::new(),
                question_reviews: vec![crate::models::ModerationQuestionReview {
                    question_id: "question_1".into(),
                    reviewed_at: "123".into(),
                }],
            },
        )
        .expect("moderation state should seed");

        let workspace = save_moderated_score(
            &project_path,
            SaveModeratedScoreInput {
                student_ref: "student_1".into(),
                question_id: "question_1".into(),
                moderated_total_points: 4,
            },
        )
        .expect("matching original score should clear override");

        assert!(workspace.moderation_state.score_overrides.is_empty());
        assert!(workspace.moderation_state.feedback_overrides.is_empty());
        assert!(workspace.moderation_state.question_reviews.is_empty());
    }

    #[test]
    fn set_question_reviewed_marks_and_clears_review_state() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-moderation-review-toggle");
        let project_path = create_project(&test_root);
        seed_question(&project_path);
        seed_workflow(&project_path);

        let reviewed = set_question_reviewed(
            &project_path,
            SetModerationQuestionReviewedInput {
                question_id: "question_1".into(),
                reviewed: true,
            },
        )
        .expect("review should save");
        assert_eq!(reviewed.moderation_state.question_reviews.len(), 1);

        let cleared = set_question_reviewed(
            &project_path,
            SetModerationQuestionReviewedInput {
                question_id: "question_1".into(),
                reviewed: false,
            },
        )
        .expect("review should clear");
        assert!(cleared.moderation_state.question_reviews.is_empty());
    }

    #[test]
    fn save_moderated_feedback_persists_override_without_touching_original_feedback() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-moderation-save-feedback");
        let project_path = create_project(&test_root);
        seed_question(&project_path);
        seed_workflow(&project_path);

        let workspace = save_moderated_feedback(
            &project_path,
            SaveModeratedFeedbackInput {
                student_ref: "student_1".into(),
                question_id: "question_1".into(),
                feedback_text: "Edited by moderator".into(),
            },
        )
        .expect("moderated feedback should save");

        let answer = &workspace.student_workflow.submissions[0].answers[0];
        assert_eq!(answer.feedback_text.as_deref(), Some("Solid answer."));
        assert!(workspace.moderation_state.score_overrides.is_empty());
        assert_eq!(workspace.moderation_state.feedback_overrides.len(), 1);
        assert_eq!(
            workspace.moderation_state.feedback_overrides[0].feedback_text,
            "Edited by moderator"
        );
    }

    #[test]
    fn save_moderated_feedback_preserves_question_review_state() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-moderation-feedback-review-state");
        let project_path = create_project(&test_root);
        seed_question(&project_path);
        seed_workflow(&project_path);

        let reviewed = set_question_reviewed(
            &project_path,
            SetModerationQuestionReviewedInput {
                question_id: "question_1".into(),
                reviewed: true,
            },
        )
        .expect("review should save");
        assert_eq!(reviewed.moderation_state.question_reviews.len(), 1);

        let workspace = save_moderated_feedback(
            &project_path,
            SaveModeratedFeedbackInput {
                student_ref: "student_1".into(),
                question_id: "question_1".into(),
                feedback_text: "Edited by moderator".into(),
            },
        )
        .expect("moderated feedback should save");

        assert_eq!(workspace.moderation_state.question_reviews.len(), 1);
        assert_eq!(
            workspace.moderation_state.question_reviews[0].question_id,
            "question_1"
        );
    }
}
