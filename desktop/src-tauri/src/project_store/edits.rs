// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::{params, Connection};

use crate::errors::{HostError, HostResult};
use crate::models::{
    ExamWorkspaceState, QuestionEdit, QuestionRecord, RubricApprovalBasis, RubricCriterion,
    TemplateRedactionRegionInput, REDACTION_LABEL_NAME_IDENTIFICATION,
    REDACTION_LABEL_PRIVACY_PROTECTION,
};
use crate::workflow_status::TemplateSetupStatus;

use super::projects::{project_id, touch_project};
use super::schema::{current_timestamp, initialize_schema, project_db_path, unique_suffix};
use super::template_setup::mark_template_setup_draft;
use super::workspace::load_exam_workspace_state;

pub fn save_question_edits(
    project_path: &Path,
    edits: &[QuestionEdit],
) -> HostResult<ExamWorkspaceState> {
    validate_question_edits(edits)?;
    let previous = load_exam_workspace_state(project_path)?;
    validate_accepted_moderation_question_edits(&previous, edits)?;
    let mut connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let project_id = project_id(&connection)?;
    let transaction = connection.transaction()?;
    for edit in edits {
        let updated = transaction.execute(
            "UPDATE question
             SET question_number = ?2,
                 page_number = ?3,
                 max_points = ?4,
                 prompt_text = ?5,
                 updated_at = CURRENT_TIMESTAMP
             WHERE question_id = ?1",
            params![
                edit.question_id,
                edit.question_number,
                edit.page_number,
                edit.max_points,
                edit.text.trim(),
            ],
        )?;
        if updated == 0 {
            return Err(HostError::Project(format!(
                "Question '{}' was not found in the project database.",
                edit.question_id
            )));
        }
    }
    mark_template_setup_draft(&transaction, &project_id)?;
    touch_project(&transaction)?;
    transaction.commit()?;
    for edit in edits {
        if let Some(ref ctx) = edit.question_context {
            super::workflow_state::update_question_analysis_assets(
                project_path,
                &edit.question_id,
                ctx.clone(),
            )?;
        }
    }
    reconcile_question_edit_rubrics(project_path, &previous, edits)?;
    load_exam_workspace_state(project_path)
}

fn validate_accepted_moderation_question_edits(
    previous: &ExamWorkspaceState,
    edits: &[QuestionEdit],
) -> HostResult<()> {
    let reviewed_question_ids = previous
        .moderation_state
        .question_reviews
        .iter()
        .map(|review| review.question_id.as_str())
        .collect::<HashSet<_>>();
    for edit in edits {
        if let Some(question) =
            accepted_moderation_question_changed(previous, &reviewed_question_ids, edit)
        {
            return Err(HostError::Validation(format!(
                "Question {} has been accepted in moderation. Undo moderation acceptance before changing rubric-impacting question details.",
                question.question_number
            )));
        }
    }
    Ok(())
}

fn accepted_moderation_question_changed<'a>(
    previous: &'a ExamWorkspaceState,
    reviewed_question_ids: &HashSet<&str>,
    edit: &QuestionEdit,
) -> Option<&'a QuestionRecord> {
    let question = previous
        .questions
        .iter()
        .find(|question| question.question_id == edit.question_id)?;
    (question.rubric.is_approved()
        && reviewed_question_ids.contains(question.question_id.as_str())
        && question_edit_changes_approval_basis(question, edit))
    .then_some(question)
}

fn question_edit_changes_approval_basis(question: &QuestionRecord, edit: &QuestionEdit) -> bool {
    question.text != edit.text.trim()
        || question.max_points != edit.max_points
        || question_context(question)
            != edit
                .question_context
                .clone()
                .unwrap_or_else(|| question_context(question))
}

fn reconcile_question_edit_rubrics(
    project_path: &Path,
    previous: &ExamWorkspaceState,
    edits: &[QuestionEdit],
) -> HostResult<()> {
    let previous_by_id: HashMap<&str, &QuestionRecord> = previous
        .questions
        .iter()
        .map(|question| (question.question_id.as_str(), question))
        .collect();
    let current = load_exam_workspace_state(project_path)?;
    let current_by_id: HashMap<&str, &QuestionRecord> = current
        .questions
        .iter()
        .map(|question| (question.question_id.as_str(), question))
        .collect();
    for edit in edits {
        reconcile_single_question_edit_rubric(project_path, &previous_by_id, &current_by_id, edit)?;
    }
    Ok(())
}

fn reconcile_single_question_edit_rubric(
    project_path: &Path,
    previous_by_id: &HashMap<&str, &QuestionRecord>,
    current_by_id: &HashMap<&str, &QuestionRecord>,
    edit: &QuestionEdit,
) -> HostResult<()> {
    let Some(previous_question) = previous_by_id.get(edit.question_id.as_str()).copied() else {
        return Ok(());
    };
    if !previous_question.rubric.is_approved() {
        return Ok(());
    }
    let Some(current_question) = current_by_id.get(edit.question_id.as_str()).copied() else {
        return Ok(());
    };
    if !question_approval_basis_changed(previous_question, current_question) {
        return Ok(());
    }
    apply_question_edit_rubric_impact(project_path, previous_question, current_question, edit)
}

fn apply_question_edit_rubric_impact(
    project_path: &Path,
    previous_question: &QuestionRecord,
    current_question: &QuestionRecord,
    edit: &QuestionEdit,
) -> HostResult<()> {
    let mut rubric = current_question.rubric.clone();
    if question_edit_impact(previous_question, current_question, edit) == "minor" {
        rubric.approval_basis = Some(rubric_approval_basis(current_question, &rubric.criteria));
        super::workflow_state::save_rubric_state(project_path, &edit.question_id, &rubric)?;
        return Ok(());
    }
    rubric.mark_draft();
    super::workflow_state::save_rubric_state(project_path, &edit.question_id, &rubric)?;
    super::workflow_state::mark_student_answers_stale_for_questions(
        project_path,
        std::slice::from_ref(&edit.question_id),
    )?;
    Ok(())
}

fn question_approval_basis_changed(previous: &QuestionRecord, current: &QuestionRecord) -> bool {
    previous.text != current.text
        || previous.max_points != current.max_points
        || question_context(previous) != question_context(current)
}

fn question_edit_impact<'a>(
    previous: &QuestionRecord,
    current: &QuestionRecord,
    edit: &'a QuestionEdit,
) -> &'a str {
    if previous.max_points != current.max_points {
        return "grading";
    }
    match edit.rubric_edit_impact.as_deref() {
        Some("minor") => "minor",
        _ => "grading",
    }
}

fn question_context(question: &QuestionRecord) -> String {
    question
        .analysis
        .question_context
        .clone()
        .unwrap_or_default()
}

fn rubric_approval_basis(
    question: &QuestionRecord,
    criteria: &[RubricCriterion],
) -> RubricApprovalBasis {
    RubricApprovalBasis {
        question_text: question.text.clone(),
        question_context: question_context(question),
        max_points: question.max_points,
        criteria: criteria.to_vec(),
    }
}

pub fn save_redaction_regions(
    project_path: &Path,
    regions: &[TemplateRedactionRegionInput],
) -> HostResult<ExamWorkspaceState> {
    validate_redaction_regions(regions)?;
    let mut connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let project_id = project_id(&connection)?;
    let transaction = connection.transaction()?;
    if !regions.is_empty() {
        let (_status, mut payload) =
            super::template_setup::load_template_setup_state(&transaction, &project_id)?;
        payload.redaction_skip_acknowledged_at = None;
        super::template_setup::upsert_template_setup_state(
            &transaction,
            &project_id,
            TemplateSetupStatus::Draft,
            &payload,
        )?;
    }
    transaction.execute("DELETE FROM template_redaction_region", [])?;
    for (index, region) in regions.iter().enumerate() {
        let region_id = region
            .region_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("region_{}", unique_suffix()));
        transaction.execute(
            "INSERT INTO template_redaction_region (
                region_id,
                page_number,
                x,
                y,
                width,
                height,
                label,
                sort_order,
                created_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
            params![
                region_id,
                region.page_number,
                region.x,
                region.y,
                region.width,
                region.height,
                if index == 0 {
                    REDACTION_LABEL_NAME_IDENTIFICATION
                } else {
                    REDACTION_LABEL_PRIVACY_PROTECTION
                },
                index as i64,
            ],
        )?;
    }
    mark_template_setup_draft(&transaction, &project_id)?;
    touch_project(&transaction)?;
    transaction.commit()?;
    load_exam_workspace_state(project_path)
}

pub fn approve_template_setup(project_path: &Path) -> HostResult<ExamWorkspaceState> {
    let current = load_exam_workspace_state(project_path)?;
    if current.questions.is_empty() {
        return Err(HostError::Validation(
            "Template setup cannot be approved before questions are available.".into(),
        ));
    }
    if current.redaction_regions.is_empty() {
        let connection = Connection::open(project_db_path(project_path))?;
        initialize_schema(&connection)?;
        let project_id = project_id(&connection)?;
        let (status, payload) =
            super::template_setup::load_template_setup_state(&connection, &project_id)?;
        if !status.is_draft() || payload.redaction_skip_acknowledged_at.is_none() {
            return Err(HostError::Validation(
                "Template setup cannot be approved before at least one redaction region exists or redaction has been explicitly skipped.".into(),
            ));
        }
    }
    let mut connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let project_id = project_id(&connection)?;
    let transaction = connection.transaction()?;
    let (_status, mut payload) =
        super::template_setup::load_template_setup_state(&transaction, &project_id)?;
    payload.approved_at = Some(current_timestamp());
    payload.failure_message = None;
    super::template_setup::upsert_template_setup_state(
        &transaction,
        &project_id,
        TemplateSetupStatus::Approved,
        &payload,
    )?;
    touch_project(&transaction)?;
    transaction.commit()?;
    load_exam_workspace_state(project_path)
}

pub fn skip_template_redaction(project_path: &Path) -> HostResult<ExamWorkspaceState> {
    let current = load_exam_workspace_state(project_path)?;
    if current.questions.is_empty() {
        return Err(HostError::Validation(
            "Template setup cannot skip redaction before questions are available.".into(),
        ));
    }

    let mut connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let project_id = project_id(&connection)?;
    let transaction = connection.transaction()?;
    let (_status, mut payload) =
        super::template_setup::load_template_setup_state(&transaction, &project_id)?;
    payload.approved_at = None;
    payload.failure_message = None;
    payload.redaction_skip_acknowledged_at = Some(current_timestamp());
    super::template_setup::upsert_template_setup_state(
        &transaction,
        &project_id,
        TemplateSetupStatus::Draft,
        &payload,
    )?;
    touch_project(&transaction)?;
    transaction.commit()?;
    load_exam_workspace_state(project_path)
}

fn validate_question_edits(edits: &[QuestionEdit]) -> HostResult<()> {
    if edits.is_empty() {
        return Err(HostError::Validation(
            "At least one question edit row is required.".into(),
        ));
    }
    edits.iter().try_for_each(validate_question_edit)?;
    Ok(())
}

fn validate_redaction_regions(regions: &[TemplateRedactionRegionInput]) -> HostResult<()> {
    regions.iter().try_for_each(validate_redaction_region)?;
    Ok(())
}

fn validate_question_edit(edit: &QuestionEdit) -> HostResult<()> {
    let question_id = edit.question_id.trim();
    if question_id.is_empty() {
        return Err(HostError::Validation("question_id is required.".into()));
    }
    require_positive_question_value(question_id, "question number", edit.question_number)?;
    require_positive_question_value(question_id, "page number", edit.page_number)?;
    if edit.text.trim().is_empty() {
        return Err(question_validation_error(
            question_id,
            "text may not be empty.",
        ));
    }
    require_non_negative_max_points(question_id, edit.max_points)
}

fn require_positive_question_value(question_id: &str, field: &str, value: i64) -> HostResult<()> {
    if value < 1 {
        return Err(question_validation_error(
            question_id,
            &format!("must have a positive {field}."),
        ));
    }
    Ok(())
}

fn require_non_negative_max_points(question_id: &str, max_points: Option<i64>) -> HostResult<()> {
    if matches!(max_points, Some(value) if value < 0) {
        return Err(question_validation_error(
            question_id,
            "max_points must be a non-negative integer.",
        ));
    }
    Ok(())
}

fn question_validation_error(question_id: &str, message: &str) -> HostError {
    HostError::Validation(format!("Question '{question_id}' {message}"))
}

fn validate_redaction_region(region: &TemplateRedactionRegionInput) -> HostResult<()> {
    if region.page_number < 1 {
        return Err(HostError::Validation(
            "Redaction regions must use a positive page number.".into(),
        ));
    }
    if region.width < 1 || region.height < 1 {
        return Err(HostError::Validation(
            "Redaction regions must have positive width and height.".into(),
        ));
    }
    if region.x < 0 || region.y < 0 {
        return Err(HostError::Validation(
            "Redaction regions may not use negative coordinates.".into(),
        ));
    }
    Ok(())
}
