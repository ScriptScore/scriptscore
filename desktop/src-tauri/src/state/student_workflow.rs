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
mod tests {
    use super::results::{
        build_answers_for_manual_pii_block, build_answers_from_pii_results, build_parse_targets,
        feedback_request_json,
    };
    use super::{
        aggregate_preliminary_confidence, confirm_student_alignment, confirm_student_detect_review,
        confirm_student_parse_review, final_rows_from_preliminary,
        local_pii_trigger_words_by_student_ref, parse_review_required,
        pii_trigger_words_for_roster_row, stage_is_workflow_eligible, submission_runtime_context,
        StudentWorkflowAlignmentUpdateInput, StudentWorkflowDetectReviewInput,
        StudentWorkflowDetectReviewResolutionInput, StudentWorkflowParseReviewInput,
    };
    use crate::lms::LmsRosterRow;
    use crate::models::{
        AppSettings, ExamWorkspaceState, ProjectConfig, ProjectSummary, QuestionAnalysisState,
        QuestionRecord, RubricCriterion, RubricState, StudentIntakeState, StudentIntakeSummary,
        StudentWorkflowAlignmentPage, StudentWorkflowAnswer, StudentWorkflowDetectRegion,
        StudentWorkflowDetectReview, StudentWorkflowDetectReviewRow, StudentWorkflowPiiPrescreen,
        StudentWorkflowState, StudentWorkflowSubmission, StudentWorkflowTransform,
        WorkspaceWarning,
    };
    use crate::project_store;
    use crate::project_store::schema::{initialize_schema, project_db_path};
    use serde_json::json;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    struct NoopEventSink;

    impl crate::state::RuntimeEventSink for NoopEventSink {
        fn emit_runtime_event(&self, _event: crate::models::RuntimeJobEvent) {}
    }

    fn temp_root(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_millis()
        ))
    }

    fn bootstrap_project(project_path: &std::path::Path) {
        std::fs::create_dir_all(project_path).expect("project root should exist");
        let connection =
            rusqlite::Connection::open(project_db_path(project_path)).expect("project db opens");
        initialize_schema(&connection).expect("schema should initialize");
        connection
            .execute(
                "INSERT INTO project (
                    project_id,
                    display_name,
                    subject,
                    course_code,
                    lms_course_id,
                    redaction_required,
                    instructor_profile_json,
                    trace_refs_json,
                    created_at,
                    updated_at
                ) VALUES (?1, ?2, NULL, NULL, NULL, ?3, '{}', '{}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
                rusqlite::params!["proj_test", "Workflow Review Test", true],
            )
            .expect("project should insert");
    }

    fn app_state_inner_with_active_job() -> Arc<crate::state::AppStateInner> {
        let state =
            crate::state::AppState::bootstrap_with_args([std::ffi::OsString::from("scriptscore")]);
        let inner = state.clone_inner();
        inner.lock().scheduler.__test_set_active_jobs(true);
        inner
    }

    fn save_single_submission(
        project_path: &std::path::Path,
        submission: StudentWorkflowSubmission,
    ) {
        project_store::save_student_workflow_state(
            project_path,
            &StudentWorkflowState {
                status: "attention".into(),
                latest_job_id: None,
                submissions: vec![submission],
            },
        )
        .expect("workflow state should save");
    }

    fn workflow_submission(student_ref: &str, stage: &str) -> StudentWorkflowSubmission {
        StudentWorkflowSubmission {
            student_ref: student_ref.into(),
            canonical_pdf_path: format!("/tmp/{student_ref}.pdf"),
            page_count: 1,
            stage: stage.into(),
            latest_job_id: None,
            failure_message: None,
            warnings: Vec::new(),
            page_artifacts: Vec::new(),
            alignment_pages: Vec::new(),
            detect_review: None,
            answers: Vec::new(),
        }
    }

    fn alignment_page() -> StudentWorkflowAlignmentPage {
        StudentWorkflowAlignmentPage {
            page_number: 1,
            confidence: Some(0.99),
            low_confidence: false,
            review_exempt: false,
            review_exempt_reason: None,
            question_count: 1,
            transform: StudentWorkflowTransform {
                rotation: 0.0,
                scale: 1.0,
                translate_x: 0.0,
                translate_y: 0.0,
            },
            warnings: Vec::new(),
        }
    }

    fn detect_region() -> StudentWorkflowDetectRegion {
        StudentWorkflowDetectRegion {
            x: 1,
            y: 2,
            width: 30,
            height: 40,
            units: "rendered_page_pixels".into(),
        }
    }

    #[test]
    fn stopped_stage_is_workflow_eligible() {
        assert!(stage_is_workflow_eligible("stopped"));
    }

    #[test]
    fn confirm_alignment_with_active_runtime_job_saves_without_continuing() {
        let _guard = crate::test_support::lock_env_vars();
        let project_path = temp_root("scriptscore-confirm-alignment-active-save");
        bootstrap_project(&project_path);
        let mut submission = workflow_submission("student_1", "alignment_review");
        submission.alignment_pages = vec![alignment_page()];
        save_single_submission(&project_path, submission);

        confirm_student_alignment(
            &app_state_inner_with_active_job(),
            &project_path,
            StudentWorkflowAlignmentUpdateInput {
                student_ref: "student_1".into(),
                pages: vec![alignment_page()],
            },
            &AppSettings::default(),
            &NoopEventSink,
        )
        .expect("active job should fall back to save-only alignment review");

        let loaded =
            project_store::load_student_workflow_state(&project_path).expect("workflow loads");
        assert_eq!(loaded.submissions[0].stage, "canonicalize");
        assert_eq!(loaded.submissions[0].latest_job_id, None);
    }

    #[test]
    fn confirm_detect_review_with_active_runtime_job_saves_without_continuing() {
        let _guard = crate::test_support::lock_env_vars();
        let project_path = temp_root("scriptscore-confirm-detect-active-save");
        bootstrap_project(&project_path);
        let mut submission = workflow_submission("student_1", "detect_review");
        submission.detect_review = Some(StudentWorkflowDetectReview {
            pending_rows: vec![StudentWorkflowDetectReviewRow {
                question_id: "question_1".into(),
                page_number: 1,
                source_page_image_path: "/tmp/student_1_p1.png".into(),
                template_region: detect_region(),
                warnings: Vec::new(),
                resolved_region: None,
            }],
            trusted_crop_targets: Vec::new(),
        });
        save_single_submission(&project_path, submission);

        confirm_student_detect_review(
            &app_state_inner_with_active_job(),
            &project_path,
            StudentWorkflowDetectReviewInput {
                student_ref: "student_1".into(),
                resolutions: vec![StudentWorkflowDetectReviewResolutionInput {
                    question_id: "question_1".into(),
                    page_number: 1,
                    region: detect_region(),
                }],
            },
            &AppSettings::default(),
            &NoopEventSink,
        )
        .expect("active job should fall back to save-only detect review");

        let loaded =
            project_store::load_student_workflow_state(&project_path).expect("workflow loads");
        assert_eq!(loaded.submissions[0].stage, "crop");
        assert_eq!(loaded.submissions[0].latest_job_id, None);
    }

    #[test]
    fn confirm_parse_review_with_active_runtime_job_saves_without_continuing() {
        let _guard = crate::test_support::lock_env_vars();
        let project_path = temp_root("scriptscore-confirm-parse-active-save");
        bootstrap_project(&project_path);
        let mut submission = workflow_submission("student_1", "parse_review");
        submission.answers = vec![StudentWorkflowAnswer {
            question_id: "question_1".into(),
            question_number: 1,
            crop_image_path: Some("/tmp/q1.png".into()),
            pii_prescreen: None,
            manual_grading_required: false,
            manual_grading_reason: None,
            moderation_eligible: true,
            parse_status: "warning".into(),
            parse_confidence: Some("low".into()),
            parse_confidence_source: Some("combined".into()),
            raw_parsed_text: Some("raw answer".into()),
            verified_text: Some("raw answer".into()),
            review_required: true,
            verified: false,
            stale: false,
            grading_status: "not_started".into(),
            grading_confidence: None,
            grading_confidence_reason: None,
            question_max_points: Some(5),
            total_points_awarded: None,
            feedback_text: None,
            criterion_results: Vec::new(),
            highlights: Vec::new(),
            warnings: Vec::new(),
        }];
        save_single_submission(&project_path, submission);

        confirm_student_parse_review(
            &app_state_inner_with_active_job(),
            &project_path,
            StudentWorkflowParseReviewInput {
                student_ref: "student_1".into(),
                question_id: "question_1".into(),
                corrected_text: "corrected answer".into(),
            },
            &AppSettings::default(),
            &NoopEventSink,
        )
        .expect("active job should fall back to save-only parse review");

        let loaded =
            project_store::load_student_workflow_state(&project_path).expect("workflow loads");
        assert_eq!(loaded.submissions[0].stage, "grading");
        assert_eq!(loaded.submissions[0].latest_job_id, None);
        assert_eq!(
            loaded.submissions[0].answers[0].verified_text.as_deref(),
            Some("corrected answer")
        );
    }

    #[test]
    fn parse_review_required_prefers_explicit_low_confidence() {
        assert!(parse_review_required("ok", Some("low"), &[]));
        assert!(!parse_review_required("ok", Some("high"), &[]));
    }

    #[test]
    fn parse_review_required_falls_back_to_legacy_warning_codes() {
        assert!(parse_review_required(
            "ok",
            None,
            &[WorkspaceWarning {
                code: Some("handwriting_verify_low_confidence".into()),
                message: "Review me".into(),
                scope: None,
            }],
        ));
    }

    #[test]
    fn aggregate_preliminary_confidence_uses_lowest_signal() {
        let rows = [
            json!({"confidence": "high"}),
            json!({
                "confidence": "low",
                "confidence_reason": "Criterion response parsing failed after retries and fell back to zero."
            }),
        ];
        let refs = rows.iter().collect::<Vec<_>>();
        let (confidence, reason) = aggregate_preliminary_confidence(&refs);
        assert_eq!(confidence.as_deref(), Some("low"));
        assert_eq!(
            reason.as_deref(),
            Some("Criterion response parsing failed after retries and fell back to zero.")
        );
    }

    #[test]
    fn final_rows_from_preliminary_preserves_preliminary_scores() {
        let question = workflow_question();
        let workspace = workspace_with_question(question.clone());
        let question_by_id = HashMap::from([(question.question_id.clone(), &question)]);
        let submission = submission_with_verified_answer();
        let preliminary_rows = preliminary_score_rows();

        let final_rows = final_rows_from_preliminary(
            &workspace,
            &submission,
            &question_by_id,
            &preliminary_rows,
        )
        .expect("preliminary rows should finalize without consistency review");

        assert_eq!(final_rows.len(), 1);
        assert_eq!(
            final_rows[0]
                .get("total_points_awarded")
                .and_then(|value| value.as_i64()),
            Some(5)
        );
        assert_eq!(
            final_rows[0]
                .get("criterion_results")
                .and_then(|value| value.as_array())
                .map(|rows| rows.len()),
            Some(2)
        );
        assert_eq!(
            final_rows[0]
                .get("criterion_results")
                .and_then(|value| value.as_array())
                .and_then(|rows| rows.first())
                .and_then(|row| row.get("rationale"))
                .and_then(|value| value.as_str()),
            Some("Captured the main event.")
        );
        assert_eq!(
            final_rows[0]
                .get("criterion_results")
                .and_then(|value| value.as_array())
                .and_then(|rows| rows.first())
                .and_then(|row| row.get("label"))
                .and_then(|value| value.as_str()),
            Some("Part A")
        );
        assert_eq!(
            final_rows[0]
                .get("criterion_results")
                .and_then(|value| value.as_array())
                .and_then(|rows| rows.first())
                .and_then(|row| row.get("points"))
                .and_then(|value| value.as_i64()),
            Some(2)
        );
    }

    fn workflow_question() -> QuestionRecord {
        QuestionRecord {
            question_id: "question_1".into(),
            question_number: 1,
            page_number: 1,
            max_points: Some(5),
            text: "What happened?".into(),
            baseline_pdf_text: "What happened?".into(),
            region: None,
            source_artifact_id: None,
            image_path: None,
            analysis: Default::default(),
            rubric: RubricState {
                criteria: vec![
                    RubricCriterion {
                        criterion_id: "criterion_1".into(),
                        label: "Part A".into(),
                        points: 2,
                        partial_credit_guidance: "First part".into(),
                        source: "manual".into(),
                    },
                    RubricCriterion {
                        criterion_id: "criterion_2".into(),
                        label: "Part B".into(),
                        points: 3,
                        partial_credit_guidance: "Second part".into(),
                        source: "manual".into(),
                    },
                ],
                ..RubricState::default()
            },
        }
    }

    fn workspace_with_question(question: QuestionRecord) -> ExamWorkspaceState {
        ExamWorkspaceState {
            project: ProjectSummary {
                project_id: "project_1".into(),
                display_name: "Test Project".into(),
                subject: Some("History".into()),
                course_code: None,
                lms_course_id: None,
                project_path: "/tmp/project".into(),
                created_at: "0".into(),
                updated_at: "0".into(),
            },
            status: String::new(),
            status_label: String::new(),
            failure_message: None,
            template_preview_artifacts: Vec::new(),
            aruco_status: Default::default(),
            project_config: ProjectConfig::default(),
            questions: vec![question],
            redaction_regions: Vec::new(),
            warnings: Vec::new(),
            can_approve: false,
            can_approve_rubric: false,
            student_roster: Vec::new(),
            student_intake: Default::default(),
            student_workflow: Default::default(),
            moderation_state: Default::default(),
            results_lms_state: Default::default(),
            results_lms_rows: Vec::new(),
            results_lms_metrics: None,
            results_lms_review_summary: None,
            workflow_stage: String::new(),
            workflow_label: String::new(),
        }
    }

    fn submission_with_verified_answer() -> StudentWorkflowSubmission {
        StudentWorkflowSubmission {
            student_ref: "student_1".into(),
            canonical_pdf_path: "/tmp/student_1.pdf".into(),
            page_count: 1,
            stage: "grading".into(),
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
                raw_parsed_text: Some("A verified answer".into()),
                verified_text: Some("A verified answer".into()),
                review_required: false,
                verified: true,
                stale: false,
                grading_status: "not_started".into(),
                grading_confidence: None,
                grading_confidence_reason: None,
                question_max_points: Some(5),
                total_points_awarded: None,
                feedback_text: None,
                criterion_results: Vec::new(),
                highlights: Vec::new(),
                warnings: Vec::new(),
            }],
        }
    }

    fn preliminary_score_rows() -> Vec<serde_json::Value> {
        vec![
            json!({
                "question_id": "question_1",
                "criterion_index": 0,
                "points_awarded": 2,
                "rationale": "Captured the main event."
            }),
            json!({
                "question_id": "question_1",
                "criterion_index": 1,
                "points_awarded": 3,
                "rationale": "Included the supporting detail."
            }),
        ]
    }

    #[test]
    fn feedback_request_json_strips_desktop_only_criterion_fields() {
        let request = feedback_request_json(&json!({
            "student_ref": "student_1",
            "question_id": "question_1",
            "subject": "History",
            "total_points_awarded": 1,
            "question_max_points": 2,
            "student_answer": "Answer",
            "question_text_clean": "Question",
            "question_context": "",
            "rubric_criteria": [{
                "criterion_index": 0,
                "label": "Evidence",
                "points": 2,
                "partial_credit_guidance": "Award evidence."
            }],
            "criterion_results": [{
                "criterion_index": 0,
                "label": "Evidence",
                "points": 2,
                "points_awarded": 1,
                "rationale": "Partial evidence."
            }]
        }));

        let criterion = request
            .get("criterion_results")
            .and_then(|value| value.as_array())
            .and_then(|rows| rows.first())
            .and_then(|row| row.as_object())
            .expect("criterion result should remain present");
        assert!(criterion.contains_key("criterion_index"));
        assert!(criterion.contains_key("points_awarded"));
        assert!(criterion.contains_key("rationale"));
        assert!(!criterion.contains_key("label"));
        assert!(!criterion.contains_key("points"));
    }

    #[test]
    fn pii_trigger_words_include_name_parts_and_optional_identity_fields() {
        let triggers = pii_trigger_words_for_roster_row(&LmsRosterRow {
            user_id: "canvas_1".into(),
            display_name: "Harper Rivera".into(),
            sort_key: "rivera, harper".into(),
            email: Some("harriet@example.edu".into()),
            login_id: Some("hrivera".into()),
        });

        assert_eq!(
            triggers,
            vec![
                "Harper Rivera".to_string(),
                "Harper".to_string(),
                "Rivera".to_string(),
                "harriet@example.edu".to_string(),
                "hrivera".to_string(),
            ]
        );
    }

    #[test]
    fn build_parse_targets_copies_clean_pii_prescreen_only() {
        let mut question = workflow_question();
        question.image_path = Some("/tmp/template_q1.png".into());
        question.analysis = QuestionAnalysisState {
            status: "ok".into(),
            question_text_clean: Some("What happened?".into()),
            question_context: Some(String::new()),
            warnings: Vec::new(),
            latest_job_id: None,
        };
        let workspace = workspace_with_question(question);
        let mut submission = submission_with_verified_answer();
        submission.answers[0].crop_image_path = Some("/tmp/q1.png".into());
        submission.answers[0].pii_prescreen = Some(StudentWorkflowPiiPrescreen {
            source_command: "scans.pii".into(),
            status: "ok".into(),
            contains_handwriting: "true".into(),
            contains_pii: false,
            pii_types_detected: Vec::new(),
            warnings: Vec::new(),
        });

        let crop_rows = vec![json!({
            "question_id": "question_1",
            "status": "ok",
            "question_crop_path": "/tmp/q1.png"
        })];

        let targets = build_parse_targets(&workspace, &submission, &crop_rows)
            .expect("parse targets should build");

        assert_eq!(targets.len(), 1);
        assert!(targets[0].get("pii_prescreen").is_some());

        submission.answers[0].pii_prescreen = Some(StudentWorkflowPiiPrescreen {
            source_command: "scans.pii".into(),
            status: "warning".into(),
            contains_handwriting: "unknown".into(),
            contains_pii: false,
            pii_types_detected: Vec::new(),
            warnings: vec![WorkspaceWarning {
                code: Some("pii_handwriting_unknown".into()),
                message: "unknown".into(),
                scope: None,
            }],
        });

        let targets = build_parse_targets(&workspace, &submission, &crop_rows)
            .expect("parse targets should build");

        assert!(targets[0].get("pii_prescreen").is_none());
    }

    #[test]
    fn submission_runtime_context_reuses_student_specific_pii_triggers() {
        let intake = StudentIntakeSummary {
            student_ref: "student_1".into(),
            local_display_name: None,
            canonical_pdf_path: "/tmp/student_1.pdf".into(),
            ingest_status: "ok".into(),
            page_count: 1,
            exam_page_paths: vec!["/tmp/page_1.png".into()],
            warnings: Vec::new(),
            binding_token_hex: None,
        };
        let workspace = workspace_with_question(workflow_question());
        let trigger_map = HashMap::from([(
            "student_1".to_string(),
            vec![
                "Harper Rivera".to_string(),
                "harriet@example.edu".to_string(),
            ],
        )]);
        let settings = AppSettings::default();

        let context = submission_runtime_context(&workspace, &intake, &settings, &trigger_map);
        let trigger_words = context
            .pii_trigger_words
            .map(|items| items.iter().map(String::as_str).collect::<Vec<_>>());

        assert_eq!(
            trigger_words,
            Some(vec!["Harper Rivera", "harriet@example.edu"])
        );
    }

    #[test]
    fn local_pii_trigger_words_use_persisted_local_display_names() {
        let mut workspace = workspace_with_question(workflow_question());
        workspace.student_intake = StudentIntakeState {
            status: "ready".into(),
            latest_job_id: None,
            unresolved_count: 0,
            items: vec![StudentIntakeSummary {
                student_ref: "student_1".into(),
                local_display_name: Some("Ada Local".into()),
                canonical_pdf_path: "/tmp/student_1.pdf".into(),
                ingest_status: "ok".into(),
                page_count: 1,
                exam_page_paths: vec!["/tmp/page_1.png".into()],
                warnings: Vec::new(),
                binding_token_hex: None,
            }],
        };

        let triggers = local_pii_trigger_words_by_student_ref(&workspace);

        assert_eq!(
            triggers.get("student_1"),
            Some(&vec![
                "Ada Local".to_string(),
                "Ada".to_string(),
                "Local".to_string()
            ])
        );
    }

    #[test]
    fn build_answers_from_pii_results_keeps_crop_failures_visible() {
        let workspace = workspace_with_question(workflow_question());
        let crop_rows = vec![json!({
            "question_id": "question_1",
            "status": "error",
            "warnings": [{
                "code": "crop_failed",
                "message": "Question crop generation failed.",
                "scope": "answer"
            }]
        })];

        let pii_data = json!({ "pii_results": [] });
        let answers = build_answers_from_pii_results(
            &workspace,
            &crop_rows,
            pii_data
                .as_object()
                .expect("test pii payload should remain an object"),
        )
        .expect("crop-failed rows should still seed answer state");

        assert_eq!(answers.len(), 1);
        assert!(answers[0].manual_grading_required);
        assert_eq!(
            answers[0].manual_grading_reason.as_deref(),
            Some("crop_failed")
        );
        assert_eq!(answers[0].parse_status, "blocked");
        assert_eq!(answers[0].grading_status, "manual_required");
        assert_eq!(
            answers[0]
                .warnings
                .first()
                .and_then(|warning| warning.code.as_deref()),
            Some("crop_failed")
        );
    }

    #[test]
    fn build_answers_from_pii_results_sorts_by_question_number() {
        let question_1 = workflow_question();
        let mut question_2 = workflow_question();
        question_2.question_id = "question_2".into();
        question_2.question_number = 2;
        let mut question_3 = workflow_question();
        question_3.question_id = "question_3".into();
        question_3.question_number = 3;
        let mut workspace = workspace_with_question(question_1);
        workspace.questions.push(question_2);
        workspace.questions.push(question_3);
        let crop_rows = vec![
            json!({
                "question_id": "question_1",
                "status": "ok",
                "question_crop_path": "/tmp/q1.png"
            }),
            json!({
                "question_id": "question_3",
                "status": "ok",
                "question_crop_path": "/tmp/q3.png"
            }),
            json!({
                "question_id": "question_2",
                "status": "ok",
                "question_crop_path": "/tmp/q2.png"
            }),
        ];
        let pii_data = json!({ "pii_results": [] });

        let answers = build_answers_from_pii_results(
            &workspace,
            &crop_rows,
            pii_data
                .as_object()
                .expect("test pii payload should remain an object"),
        )
        .expect("answers should be seeded");

        assert_eq!(
            answers
                .iter()
                .map(|answer| answer.question_number)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn build_answers_from_pii_results_crop_failed_not_moderation_eligible() {
        let workspace = workspace_with_question(workflow_question());
        let crop_rows = vec![json!({
            "question_id": "question_1",
            "status": "error",
            "warnings": [{
                "code": "crop_failed",
                "message": "Question crop generation failed.",
                "scope": "answer"
            }]
        })];

        let pii_data = json!({ "pii_results": [] });
        let answers = build_answers_from_pii_results(
            &workspace,
            &crop_rows,
            pii_data
                .as_object()
                .expect("test pii payload should remain an object"),
        )
        .expect("crop-failed rows should still seed answer state");

        assert!(!answers[0].moderation_eligible);
    }

    #[test]
    fn build_answers_from_pii_results_clean_pii_is_moderation_eligible() {
        let workspace = workspace_with_question(workflow_question());
        let crop_rows = vec![json!({
            "question_id": "question_1",
            "status": "ok",
            "question_crop_path": "/tmp/q1.png"
        })];

        let pii_data = json!({ "pii_results": [{
            "question_id": "question_1",
            "status": "ok",
            "contains_handwriting": "clean",
            "contains_pii": false,
            "pii_types_detected": [],
            "warnings": []
        }]});
        let answers = build_answers_from_pii_results(
            &workspace,
            &crop_rows,
            pii_data
                .as_object()
                .expect("test pii payload should remain an object"),
        )
        .expect("clean pii rows should seed answer state");

        assert!(answers[0].moderation_eligible);
        assert!(!answers[0].manual_grading_required);
    }

    #[test]
    fn build_answers_from_pii_results_unknown_handwriting_without_pii_continues() {
        let workspace = workspace_with_question(workflow_question());
        let crop_rows = vec![json!({
            "question_id": "question_1",
            "status": "ok",
            "question_crop_path": "/tmp/q1.png"
        })];

        let pii_data = json!({ "pii_results": [{
            "question_id": "question_1",
            "status": "warning",
            "contains_handwriting": "unknown",
            "contains_pii": false,
            "pii_types_detected": [],
            "warnings": [{
                "code": "pii_handwriting_unknown",
                "message": "Handwriting detection was inconclusive for this crop.",
                "scope": {
                    "question_id": "question_1",
                    "student_ref": "student_1"
                }
            }]
        }]});
        let answers = build_answers_from_pii_results(
            &workspace,
            &crop_rows,
            pii_data
                .as_object()
                .expect("test pii payload should remain an object"),
        )
        .expect("non-PII warning rows should still seed answer state");

        assert!(answers[0].moderation_eligible);
        assert!(!answers[0].manual_grading_required);
        assert_eq!(answers[0].manual_grading_reason, None);
        assert_eq!(answers[0].parse_status, "not_started");
        assert_eq!(answers[0].grading_status, "not_started");
        assert_eq!(
            answers[0]
                .warnings
                .first()
                .and_then(|warning| warning.code.as_deref()),
            Some("pii_handwriting_unknown")
        );
    }

    #[test]
    fn build_answers_from_pii_results_pii_blocked_still_moderation_eligible() {
        let workspace = workspace_with_question(workflow_question());
        let crop_rows = vec![json!({
            "question_id": "question_1",
            "status": "ok",
            "question_crop_path": "/tmp/q1.png"
        })];

        let pii_data = json!({ "pii_results": [{
            "question_id": "question_1",
            "status": "ok",
            "contains_handwriting": "ambiguous",
            "contains_pii": true,
            "pii_types_detected": ["name"],
            "warnings": []
        }]});
        let answers = build_answers_from_pii_results(
            &workspace,
            &crop_rows,
            pii_data
                .as_object()
                .expect("test pii payload should remain an object"),
        )
        .expect("pii-blocked rows should seed answer state");

        assert!(answers[0].moderation_eligible);
        assert!(answers[0].manual_grading_required);
        assert_eq!(
            answers[0].manual_grading_reason.as_deref(),
            Some("pii_detected")
        );
    }

    #[test]
    fn build_answers_for_manual_pii_block_is_moderation_eligible() {
        let workspace = workspace_with_question(workflow_question());
        let crop_rows = vec![json!({
            "question_id": "question_1",
            "status": "ok",
            "question_crop_path": "/tmp/q1.png"
        })];

        let answers = build_answers_for_manual_pii_block(
            &workspace,
            &crop_rows,
            WorkspaceWarning {
                code: Some("pii_identity_unavailable".into()),
                message: "PII identity context unavailable.".into(),
                scope: Some("answer".into()),
            },
        )
        .expect("manual pii block should seed answer state");

        assert!(answers[0].moderation_eligible);
        assert!(answers[0].manual_grading_required);
    }
}
