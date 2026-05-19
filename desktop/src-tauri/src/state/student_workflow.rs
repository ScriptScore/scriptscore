// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use serde_json::Value;

use super::runtime::{run_reserved_job, start_runtime_job, RuntimeJobRequest};
use super::workspace_actions::cli_instructor_profile_json;
use super::{lms_roster_cache, AppStateInner, RuntimeEventSink};
use crate::binding_token::{canvas_course_context, compute_binding_token_hex, TOKEN_VERSION};
use crate::errors::{HostError, HostResult};
use crate::lms::LmsRosterRow;
use crate::models::{
    AppSettings, ExamWorkspaceState, StudentWorkflowAlignmentPage, StudentWorkflowDetectRegion,
    StudentWorkflowTransform,
};
use crate::project_store;

#[path = "student_workflow_pipeline.rs"]
mod pipeline;
#[path = "student_workflow_results.rs"]
mod results;
#[path = "student_workflow_shared.rs"]
mod shared;

use pipeline::{
    continue_after_alignment_review, continue_after_detect_review, ensure_submission_row,
    intake_map, process_submissions_batched, regrade_stale_question_answers,
    run_grading_for_submission, save_workflow_state, BatchWorkflowInputs,
};
#[cfg(test)]
use results::{
    aggregate_preliminary_confidence, final_rows_from_preliminary, parse_review_required,
};
use shared::find_submission_mut;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentWorkflowAlignmentUpdateInput {
    pub student_ref: String,
    pub pages: Vec<StudentWorkflowAlignmentPage>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentWorkflowParseReviewInput {
    pub student_ref: String,
    pub question_id: String,
    pub corrected_text: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentWorkflowDetectReviewResolutionInput {
    pub question_id: String,
    pub page_number: i64,
    pub region: StudentWorkflowDetectRegion,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentWorkflowDetectReviewInput {
    pub student_ref: String,
    pub resolutions: Vec<StudentWorkflowDetectReviewResolutionInput>,
}

struct SubmissionRuntimeContext<'a> {
    workspace: &'a ExamWorkspaceState,
    intake: &'a crate::models::StudentIntakeSummary,
    settings: &'a AppSettings,
    pii_trigger_words: Option<&'a [String]>,
}

#[derive(Clone, Copy)]
struct WorkflowExecutionContext<'a> {
    state: &'a Arc<AppStateInner>,
    project_path: &'a Path,
    event_sink: &'a dyn RuntimeEventSink,
}

#[derive(Clone, Copy)]
struct GradingStageContext<'a> {
    settings: &'a AppSettings,
    student_ref: &'a str,
    grading_persisted_payload: &'a Value,
}

enum AlignmentOutcome {
    Failed,
    NeedsReview,
    Continue,
}

enum ParseOutcome {
    Failed,
    NeedsReview,
    Continue,
}

pub(crate) fn begin_student_workflow(
    state: &Arc<AppStateInner>,
    project_path: &Path,
    settings: &AppSettings,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<ExamWorkspaceState> {
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    let intake_by_ref = intake_map(&workspace)?;
    let pii_trigger_words = load_pii_trigger_words_by_student_ref(state, &workspace, settings);
    let mut workflow_state = project_store::load_student_workflow_state(project_path)?;
    for intake in intake_by_ref.values() {
        ensure_submission_row(&mut workflow_state, intake);
    }
    save_workflow_state(project_path, &mut workflow_state)?;

    let eligible_refs = eligible_student_refs_for_workflow(&workflow_state);

    process_submissions_batched(
        state,
        project_path,
        event_sink,
        &mut workflow_state,
        BatchWorkflowInputs {
            workspace: &workspace,
            settings,
            intake_by_ref: &intake_by_ref,
            pii_trigger_words_by_student_ref: &pii_trigger_words,
            eligible_refs,
        },
    )?;

    project_store::load_exam_workspace_state(project_path)
}

pub(crate) fn regrade_question_answers(
    state: &Arc<AppStateInner>,
    project_path: &Path,
    question_id: &str,
    settings: &AppSettings,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<ExamWorkspaceState> {
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    let mut workflow_state = project_store::load_student_workflow_state(project_path)?;
    regrade_stale_question_answers(
        state,
        project_path,
        settings,
        event_sink,
        &workspace,
        &mut workflow_state,
        question_id,
    )?;
    project_store::load_exam_workspace_state(project_path)
}

fn eligible_student_refs_for_workflow(
    workflow_state: &crate::models::StudentWorkflowState,
) -> Vec<String> {
    workflow_state
        .submissions
        .iter()
        .filter(|submission| stage_is_workflow_eligible(&submission.stage))
        .map(|submission| submission.student_ref.clone())
        .collect()
}

fn stage_is_workflow_eligible(stage: &str) -> bool {
    matches!(
        stage,
        "" | "intake_ready"
            | "stopped"
            | "failed"
            | "alignment"
            | "canonicalize"
            | "detect"
            | "crop"
            | "pii"
            | "parse"
            | "grading"
    )
}

fn submission_runtime_context<'a>(
    workspace: &'a ExamWorkspaceState,
    intake: &'a crate::models::StudentIntakeSummary,
    settings: &'a AppSettings,
    pii_trigger_words_by_student_ref: &'a HashMap<String, Vec<String>>,
) -> SubmissionRuntimeContext<'a> {
    SubmissionRuntimeContext {
        workspace,
        intake,
        settings,
        pii_trigger_words: pii_trigger_words_by_student_ref
            .get(&intake.student_ref)
            .map(Vec::as_slice),
    }
}

fn load_pii_trigger_words_by_student_ref(
    state: &Arc<AppStateInner>,
    workspace: &ExamWorkspaceState,
    settings: &AppSettings,
) -> HashMap<String, Vec<String>> {
    let local = local_pii_trigger_words_by_student_ref(workspace);
    let Some((course_id, roster)) = lms_roster_cache::cached_rows_if_ready(state, settings) else {
        return local;
    };
    let Ok(secret) = crate::secrets::binding_hmac_secret_bytes(settings) else {
        return local;
    };
    let mut out = local;
    out.extend(pii_trigger_words_by_student_ref_from_roster(
        workspace, &roster, &secret, &course_id,
    ));
    out
}

fn local_pii_trigger_words_by_student_ref(
    workspace: &ExamWorkspaceState,
) -> HashMap<String, Vec<String>> {
    workspace
        .student_intake
        .items
        .iter()
        .filter_map(|item| {
            let name = item.local_display_name.as_deref()?.trim();
            if name.is_empty() {
                return None;
            }
            let mut seen = HashSet::new();
            let mut triggers = Vec::new();
            push_pii_trigger(&mut triggers, &mut seen, name);
            for part in pii_name_parts(name) {
                push_pii_trigger(&mut triggers, &mut seen, &part);
            }
            (!triggers.is_empty()).then(|| (item.student_ref.clone(), triggers))
        })
        .collect()
}

fn pii_trigger_words_by_student_ref_from_roster(
    workspace: &ExamWorkspaceState,
    roster: &[LmsRosterRow],
    secret: &[u8],
    course_id: &str,
) -> HashMap<String, Vec<String>> {
    let course_context = canvas_course_context(course_id);
    let persisted_student_refs = workspace
        .student_roster
        .iter()
        .map(|row| (row.binding_token_hex.as_str(), row.student_ref.as_str()))
        .collect::<HashMap<_, _>>();
    let mut out = HashMap::new();
    for row in roster {
        let Ok(token) =
            compute_binding_token_hex(secret, &course_context, row.user_id.trim(), TOKEN_VERSION)
        else {
            continue;
        };
        let Some(student_ref) = persisted_student_refs.get(token.as_str()) else {
            continue;
        };
        let triggers = pii_trigger_words_for_roster_row(row);
        if !triggers.is_empty() {
            out.insert((*student_ref).to_string(), triggers);
        }
    }
    out
}

fn pii_trigger_words_for_roster_row(row: &LmsRosterRow) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    push_pii_trigger(&mut out, &mut seen, row.display_name.trim());
    for part in pii_name_parts(row.display_name.trim()) {
        push_pii_trigger(&mut out, &mut seen, &part);
    }
    if let Some(email) = row.email.as_deref() {
        push_pii_trigger(&mut out, &mut seen, email.trim());
    }
    if let Some(login_id) = row.login_id.as_deref() {
        push_pii_trigger(&mut out, &mut seen, login_id.trim());
    }
    out
}

fn push_pii_trigger(out: &mut Vec<String>, seen: &mut HashSet<String>, raw: &str) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return;
    }
    let key = trimmed.to_lowercase();
    if seen.insert(key) {
        out.push(trimmed.to_string());
    }
}

fn pii_name_parts(display_name: &str) -> Vec<String> {
    display_name
        .split(|ch: char| !ch.is_alphanumeric() && ch != '\'' && ch != '-')
        .filter_map(|part| {
            let trimmed = part.trim();
            (trimmed.len() >= 2).then(|| trimmed.to_string())
        })
        .collect()
}

fn has_active_runtime_job(state: &Arc<AppStateInner>) -> bool {
    state.lock().scheduler.has_active_jobs()
}

pub(crate) fn confirm_student_alignment(
    state: &Arc<AppStateInner>,
    project_path: &Path,
    input: StudentWorkflowAlignmentUpdateInput,
    settings: &AppSettings,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<ExamWorkspaceState> {
    if has_active_runtime_job(state) {
        return save_student_alignment_review(project_path, input);
    }
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    let intake_by_ref = intake_map(&workspace)?;
    let intake = intake_by_ref.get(&input.student_ref).ok_or_else(|| {
        HostError::Validation(format!(
            "No canonical intake submission exists for '{}'.",
            input.student_ref
        ))
    })?;
    let pii_trigger_words = load_pii_trigger_words_by_student_ref(state, &workspace, settings);
    let mut workflow_state = project_store::load_student_workflow_state(project_path)?;
    let submission = find_submission_mut(&mut workflow_state, &input.student_ref)?;
    submission.alignment_pages = input.pages;
    continue_after_alignment_review(
        state,
        project_path,
        event_sink,
        submission_runtime_context(&workspace, intake, settings, &pii_trigger_words),
        &input.student_ref,
        &mut workflow_state,
    )?;
    project_store::load_exam_workspace_state(project_path)
}

pub(crate) fn save_student_alignment_review(
    project_path: &Path,
    input: StudentWorkflowAlignmentUpdateInput,
) -> HostResult<ExamWorkspaceState> {
    let mut workflow_state = project_store::load_student_workflow_state(project_path)?;
    let submission = find_submission_mut(&mut workflow_state, &input.student_ref)?;
    if submission.stage != "alignment_review" {
        return Err(HostError::Validation(format!(
            "Student '{}' is not pending alignment review.",
            input.student_ref
        )));
    }
    submission.alignment_pages = input.pages;
    submission.stage = "canonicalize".into();
    submission.failure_message = None;
    project_store::save_student_workflow_submissions(
        project_path,
        std::slice::from_ref(submission),
    )?;
    project_store::load_exam_workspace_state(project_path)
}

pub(crate) fn confirm_student_detect_review(
    state: &Arc<AppStateInner>,
    project_path: &Path,
    input: StudentWorkflowDetectReviewInput,
    settings: &AppSettings,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<ExamWorkspaceState> {
    if has_active_runtime_job(state) {
        return save_student_detect_review(project_path, input);
    }
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    let intake_by_ref = intake_map(&workspace)?;
    let intake = intake_by_ref.get(&input.student_ref).ok_or_else(|| {
        HostError::Validation(format!(
            "No canonical intake submission exists for '{}'.",
            input.student_ref
        ))
    })?;
    let pii_trigger_words = load_pii_trigger_words_by_student_ref(state, &workspace, settings);
    let mut workflow_state = project_store::load_student_workflow_state(project_path)?;
    {
        let submission = find_submission_mut(&mut workflow_state, &input.student_ref)?;
        if submission.stage != "detect_review" {
            return Err(HostError::Validation(format!(
                "Student '{}' is not pending detect review.",
                input.student_ref
            )));
        }
        let review = submission.detect_review.as_mut().ok_or_else(|| {
            HostError::Validation(format!(
                "Student '{}' is missing detect review details.",
                input.student_ref
            ))
        })?;
        for resolution in input.resolutions {
            results::validate_detect_region(&resolution.region)?;
            let row = review
                .pending_rows
                .iter_mut()
                .find(|row| {
                    row.question_id == resolution.question_id
                        && row.page_number == resolution.page_number
                })
                .ok_or_else(|| {
                    HostError::Validation(format!(
                        "Question '{}' on page {} is not pending detect review for '{}'.",
                        resolution.question_id, resolution.page_number, input.student_ref
                    ))
                })?;
            row.resolved_region = Some(resolution.region);
        }
        if review
            .pending_rows
            .iter()
            .any(|row| row.resolved_region.is_none())
        {
            return Err(HostError::Validation(
                "All pending detect-review rows must have resolved regions before continuing."
                    .into(),
            ));
        }
        submission.failure_message = None;
    }
    continue_after_detect_review(
        state,
        project_path,
        event_sink,
        submission_runtime_context(&workspace, intake, settings, &pii_trigger_words),
        &input.student_ref,
        &mut workflow_state,
    )?;
    project_store::load_exam_workspace_state(project_path)
}

pub(crate) fn save_student_detect_review(
    project_path: &Path,
    input: StudentWorkflowDetectReviewInput,
) -> HostResult<ExamWorkspaceState> {
    let mut workflow_state = project_store::load_student_workflow_state(project_path)?;
    let submission = find_submission_mut(&mut workflow_state, &input.student_ref)?;
    if submission.stage != "detect_review" {
        return Err(HostError::Validation(format!(
            "Student '{}' is not pending detect review.",
            input.student_ref
        )));
    }
    let review = submission.detect_review.as_mut().ok_or_else(|| {
        HostError::Validation(format!(
            "Student '{}' is missing detect review details.",
            input.student_ref
        ))
    })?;
    for resolution in input.resolutions {
        results::validate_detect_region(&resolution.region)?;
        let row = review
            .pending_rows
            .iter_mut()
            .find(|row| {
                row.question_id == resolution.question_id
                    && row.page_number == resolution.page_number
            })
            .ok_or_else(|| {
                HostError::Validation(format!(
                    "Question '{}' on page {} is not pending detect review for '{}'.",
                    resolution.question_id, resolution.page_number, input.student_ref
                ))
            })?;
        row.resolved_region = Some(resolution.region);
    }
    if review
        .pending_rows
        .iter()
        .any(|row| row.resolved_region.is_none())
    {
        return Err(HostError::Validation(
            "All pending detect-review rows must have resolved regions before saving.".into(),
        ));
    }
    submission.stage = "crop".into();
    submission.failure_message = None;
    project_store::save_student_workflow_submissions(
        project_path,
        std::slice::from_ref(submission),
    )?;
    project_store::load_exam_workspace_state(project_path)
}

pub(crate) fn confirm_student_parse_review(
    state: &Arc<AppStateInner>,
    project_path: &Path,
    input: StudentWorkflowParseReviewInput,
    settings: &AppSettings,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<ExamWorkspaceState> {
    if has_active_runtime_job(state) {
        return save_student_parse_review(project_path, input);
    }
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    let mut workflow_state = project_store::load_student_workflow_state(project_path)?;
    let should_run_grading = {
        let submission = find_submission_mut(&mut workflow_state, &input.student_ref)?;
        let answer = submission
            .answers
            .iter_mut()
            .find(|answer| answer.question_id == input.question_id)
            .ok_or_else(|| {
                HostError::Validation(format!(
                    "Question '{}' is not pending parse review for '{}'.",
                    input.question_id, input.student_ref
                ))
            })?;
        answer.verified_text = Some(input.corrected_text.trim().to_string());
        answer.review_required = false;
        answer.verified = true;
        answer.stale = false;
        answer.parse_status = "ok".into();
        submission.stage = if submission.answers.iter().any(|item| item.review_required) {
            "parse_review".into()
        } else {
            "grading".into()
        };
        submission.failure_message = None;
        submission.stage == "grading"
    };
    save_workflow_state(project_path, &mut workflow_state)?;

    if should_run_grading {
        let submission_ref = input.student_ref.clone();
        drop(workflow_state);
        run_grading_for_submission(
            state,
            project_path,
            settings,
            event_sink,
            &workspace,
            &submission_ref,
        )?;
    }

    project_store::load_exam_workspace_state(project_path)
}

pub(crate) fn save_student_parse_review(
    project_path: &Path,
    input: StudentWorkflowParseReviewInput,
) -> HostResult<ExamWorkspaceState> {
    let mut workflow_state = project_store::load_student_workflow_state(project_path)?;
    let submission = find_submission_mut(&mut workflow_state, &input.student_ref)?;
    let answer = submission
        .answers
        .iter_mut()
        .find(|answer| answer.question_id == input.question_id)
        .ok_or_else(|| {
            HostError::Validation(format!(
                "Question '{}' is not pending parse review for '{}'.",
                input.question_id, input.student_ref
            ))
        })?;
    answer.verified_text = Some(input.corrected_text.trim().to_string());
    answer.review_required = false;
    answer.verified = true;
    answer.stale = false;
    answer.parse_status = "ok".into();
    submission.stage = if submission.answers.iter().any(|item| item.review_required) {
        "parse_review".into()
    } else {
        "grading".into()
    };
    submission.failure_message = None;
    project_store::save_student_workflow_submissions(
        project_path,
        std::slice::from_ref(submission),
    )?;
    project_store::load_exam_workspace_state(project_path)
}

#[cfg(test)]
#[path = "student_workflow_tests.rs"]
mod tests;
