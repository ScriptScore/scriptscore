// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::errors::{HostError, HostResult};
use crate::models::{
    AppSettings, ExamWorkspaceState, QuestionRecord, StudentWorkflowAlignmentPage,
    StudentWorkflowAnswer, StudentWorkflowState, StudentWorkflowSubmission, WorkerJobResult,
    WorkerStatus, WorkspaceWarning,
};
use crate::project_store;
use crate::state::results_lms_report;
use crate::worker::CompletedWorkerJob;

use super::results::{
    apply_detect_page_ocr_results, apply_feedback_and_markup, apply_feedback_rows,
    apply_final_grading_rows, apply_highlight_rows, apply_preliminary_confidence_to_answers,
    build_answers_for_manual_pii_block, build_answers_from_pii_results, build_canonicalize_targets,
    build_crop_targets, build_detect_review, build_detect_targets, build_parse_targets,
    build_preliminary_answer_score_requests,
    build_preliminary_answer_score_requests_for_stale_question, crop_targets_from_detect_review,
    feedback_request_json, final_rows_from_preliminary, intake_pages_as_cli,
    merge_parse_results_into_answers, parse_alignment_pages, parse_canonicalized_pages,
    student_workflow_pages_as_cli, template_pages_as_cli,
};
use super::shared::{
    fail_submission, find_submission_mut, llm_config_json, mark_submission_failed,
    persisted_llm_request_payload, required_array, success_data,
};
use super::{
    run_reserved_job, start_runtime_job, AlignmentOutcome, GradingStageContext, ParseOutcome,
    RuntimeEventSink, RuntimeJobRequest, SubmissionRuntimeContext, WorkflowExecutionContext,
};

enum DetectStageOutcome {
    CropTargets(Vec<Value>),
    NeedsReview,
}

const SCANS_DETECT_COMMAND: &str = "scans.detect";

pub(super) fn continue_after_alignment_review(
    state: &Arc<super::AppStateInner>,
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    context: SubmissionRuntimeContext<'_>,
    student_ref: &str,
    workflow_state: &mut StudentWorkflowState,
) -> HostResult<()> {
    let workspace = context.workspace;
    let intake = context.intake;
    let exec = WorkflowExecutionContext {
        state,
        project_path,
        event_sink,
    };
    prepare_existing_submission_stage(
        project_path,
        workflow_state,
        event_sink,
        student_ref,
        "canonicalize",
    )?;
    if let Err(err) = run_canonicalize_stage(exec, workspace, intake, student_ref, workflow_state) {
        return mark_ref_for_direct_stage_error(
            project_path,
            workflow_state,
            event_sink,
            student_ref,
            &err,
        );
    }
    continue_after_canonicalize(
        state,
        project_path,
        event_sink,
        context,
        student_ref,
        workflow_state,
    )
}

pub(super) fn continue_after_canonicalize(
    state: &Arc<super::AppStateInner>,
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    context: SubmissionRuntimeContext<'_>,
    student_ref: &str,
    workflow_state: &mut StudentWorkflowState,
) -> HostResult<()> {
    let workspace = context.workspace;
    let settings = context.settings;
    let exec = WorkflowExecutionContext {
        state,
        project_path,
        event_sink,
    };
    let crop_targets = match run_detect_stage(exec, workspace, student_ref, workflow_state) {
        Ok(DetectStageOutcome::CropTargets(crop_targets)) => crop_targets,
        Ok(DetectStageOutcome::NeedsReview) => return Ok(()),
        Err(err) => {
            return mark_ref_for_direct_stage_error(
                project_path,
                workflow_state,
                event_sink,
                student_ref,
                &err,
            )
        }
    };
    if crop_targets.is_empty() {
        return mark_failed_and_emit_ready(
            project_path,
            workflow_state,
            event_sink,
            student_ref,
            "No question regions were detected for crop.",
        );
    }
    let crop_rows = match run_crop_stage(exec, student_ref, workflow_state, crop_targets) {
        Ok(crop_rows) => crop_rows,
        Err(err) => {
            return mark_ref_for_direct_stage_error(
                project_path,
                workflow_state,
                event_sink,
                student_ref,
                &err,
            )
        }
    };
    let pii_outcome = match run_pii_stage(
        exec,
        student_ref,
        workflow_state,
        workspace,
        &crop_rows,
        settings,
        context.pii_trigger_words,
    ) {
        Ok(outcome) => outcome,
        Err(err) => {
            return mark_ref_for_direct_stage_error(
                project_path,
                workflow_state,
                event_sink,
                student_ref,
                &err,
            )
        }
    };
    if matches!(
        pii_outcome,
        PiiStageOutcome::ManualOnly | PiiStageOutcome::Stopped
    ) {
        return Ok(());
    }
    match run_parse_stage(
        exec,
        settings,
        workspace,
        student_ref,
        workflow_state,
        &crop_rows,
    ) {
        Ok(ParseOutcome::Failed) => save_workflow_state_and_emit(
            project_path,
            workflow_state,
            event_sink,
            Some(student_ref),
            WorkerStatus::Ready,
        ),
        Ok(ParseOutcome::NeedsReview) => {
            set_submission_stage(workflow_state, student_ref, "parse_review")?;
            save_workflow_state_and_emit(
                project_path,
                workflow_state,
                event_sink,
                Some(student_ref),
                WorkerStatus::Ready,
            )
        }
        Ok(ParseOutcome::Continue) => {
            save_workflow_state_and_emit(
                project_path,
                workflow_state,
                event_sink,
                Some(student_ref),
                WorkerStatus::Busy,
            )?;
            run_grading_pipeline(
                state,
                project_path,
                settings,
                event_sink,
                workspace,
                student_ref,
                workflow_state,
            )
        }
        Err(err) => mark_ref_for_direct_stage_error(
            project_path,
            workflow_state,
            event_sink,
            student_ref,
            &err,
        ),
    }
}

pub(super) fn continue_after_detect_review(
    state: &Arc<super::AppStateInner>,
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    context: SubmissionRuntimeContext<'_>,
    student_ref: &str,
    workflow_state: &mut StudentWorkflowState,
) -> HostResult<()> {
    let workspace = context.workspace;
    let settings = context.settings;
    let exec = WorkflowExecutionContext {
        state,
        project_path,
        event_sink,
    };
    let crop_targets = {
        let submission = find_submission_mut(workflow_state, student_ref)?;
        let review = submission.detect_review.as_ref().ok_or_else(|| {
            HostError::Validation(format!(
                "Student '{student_ref}' is not pending detect review."
            ))
        })?;
        crop_targets_from_detect_review(review, student_ref)?
    };
    if crop_targets.is_empty() {
        return mark_failed_and_emit_ready(
            project_path,
            workflow_state,
            event_sink,
            student_ref,
            "No question regions were resolved for crop.",
        );
    }
    let crop_rows = match run_crop_stage(exec, student_ref, workflow_state, crop_targets) {
        Ok(crop_rows) => crop_rows,
        Err(err) => {
            return mark_ref_for_direct_stage_error(
                project_path,
                workflow_state,
                event_sink,
                student_ref,
                &err,
            )
        }
    };
    let pii_outcome = match run_pii_stage(
        exec,
        student_ref,
        workflow_state,
        workspace,
        &crop_rows,
        settings,
        context.pii_trigger_words,
    ) {
        Ok(outcome) => outcome,
        Err(err) => {
            return mark_ref_for_direct_stage_error(
                project_path,
                workflow_state,
                event_sink,
                student_ref,
                &err,
            )
        }
    };
    if matches!(
        pii_outcome,
        PiiStageOutcome::ManualOnly | PiiStageOutcome::Stopped
    ) {
        return Ok(());
    }
    match run_parse_stage(
        exec,
        settings,
        workspace,
        student_ref,
        workflow_state,
        &crop_rows,
    ) {
        Ok(ParseOutcome::Failed) => save_workflow_state_and_emit(
            project_path,
            workflow_state,
            event_sink,
            Some(student_ref),
            WorkerStatus::Ready,
        ),
        Ok(ParseOutcome::NeedsReview) => {
            set_submission_stage(workflow_state, student_ref, "parse_review")?;
            save_workflow_state_and_emit(
                project_path,
                workflow_state,
                event_sink,
                Some(student_ref),
                WorkerStatus::Ready,
            )
        }
        Ok(ParseOutcome::Continue) => {
            save_workflow_state_and_emit(
                project_path,
                workflow_state,
                event_sink,
                Some(student_ref),
                WorkerStatus::Busy,
            )?;
            run_grading_pipeline(
                state,
                project_path,
                settings,
                event_sink,
                workspace,
                student_ref,
                workflow_state,
            )
        }
        Err(err) => mark_ref_for_direct_stage_error(
            project_path,
            workflow_state,
            event_sink,
            student_ref,
            &err,
        ),
    }
}

pub(super) struct BatchWorkflowInputs<'a> {
    pub(super) workspace: &'a ExamWorkspaceState,
    pub(super) settings: &'a AppSettings,
    pub(super) intake_by_ref: &'a HashMap<String, crate::models::StudentIntakeSummary>,
    pub(super) pii_trigger_words_by_student_ref: &'a HashMap<String, Vec<String>>,
    pub(super) eligible_refs: Vec<String>,
}

pub(super) fn process_submissions_batched(
    state: &Arc<super::AppStateInner>,
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workflow_state: &mut StudentWorkflowState,
    inputs: BatchWorkflowInputs<'_>,
) -> HostResult<()> {
    let plan = classify_batch_resume_points(workflow_state, inputs.eligible_refs)?;
    let exec = WorkflowExecutionContext {
        state,
        project_path,
        event_sink,
    };
    let mut after_alignment = plan.after_alignment;
    after_alignment.extend(run_alignment_batch(
        exec,
        project_path,
        event_sink,
        inputs.workspace,
        inputs.intake_by_ref,
        workflow_state,
        &plan.from_start,
    )?);
    let mut after_canonicalize = plan.after_canonicalize;
    after_canonicalize.extend(run_canonicalize_batch(
        exec,
        project_path,
        event_sink,
        inputs.workspace,
        inputs.intake_by_ref,
        workflow_state,
        &after_alignment,
    )?);
    let mut after_parse = plan.after_parse;
    after_parse.extend(run_parse_batch(
        exec,
        project_path,
        event_sink,
        inputs.settings,
        inputs.workspace,
        workflow_state,
        &plan.after_pii,
    )?);
    after_parse.extend(run_scan_prep_batch(
        exec,
        inputs.workspace,
        inputs.settings,
        inputs.pii_trigger_words_by_student_ref,
        workflow_state,
        &after_canonicalize,
    )?);
    run_grading_batch(
        exec,
        project_path,
        event_sink,
        inputs.workspace,
        inputs.settings,
        workflow_state,
        &after_parse,
    )
}

struct BatchResumePlan {
    from_start: Vec<String>,
    after_alignment: Vec<String>,
    after_canonicalize: Vec<String>,
    after_pii: Vec<String>,
    after_parse: Vec<String>,
}

fn classify_batch_resume_points(
    workflow_state: &mut StudentWorkflowState,
    eligible_refs: Vec<String>,
) -> HostResult<BatchResumePlan> {
    let mut plan = BatchResumePlan {
        from_start: Vec::new(),
        after_alignment: Vec::new(),
        after_canonicalize: Vec::new(),
        after_pii: Vec::new(),
        after_parse: Vec::new(),
    };
    for student_ref in eligible_refs {
        let submission = find_submission_mut(workflow_state, &student_ref)?;
        match failed_resume_point(submission) {
            FailedResumePoint::FromStart => plan.from_start.push(student_ref),
            FailedResumePoint::AfterAlignment => plan.after_alignment.push(student_ref),
            FailedResumePoint::AfterCanonicalize => plan.after_canonicalize.push(student_ref),
            FailedResumePoint::AfterPii => plan.after_pii.push(student_ref),
            FailedResumePoint::AfterParse => plan.after_parse.push(student_ref),
        }
    }
    Ok(plan)
}

fn run_alignment_batch(
    exec: WorkflowExecutionContext<'_>,
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workspace: &ExamWorkspaceState,
    intake_by_ref: &HashMap<String, crate::models::StudentIntakeSummary>,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
) -> HostResult<Vec<String>> {
    if student_refs.is_empty() {
        return Ok(Vec::new());
    }
    for student_ref in student_refs {
        let intake = intake_by_ref.get(student_ref).ok_or_else(|| {
            HostError::Validation(format!("Student intake '{student_ref}' was missing."))
        })?;
        prepare_submission_for_stage(
            project_path,
            workflow_state,
            event_sink,
            student_ref,
            intake,
            "alignment",
            WorkerStatus::Busy,
        )?;
    }
    let student_pages = student_refs
        .iter()
        .map(|student_ref| {
            let intake = intake_by_ref.get(student_ref).ok_or_else(|| {
                HostError::Validation(format!("Student intake '{student_ref}' was missing."))
            })?;
            intake_pages_as_cli(intake)
        })
        .collect::<HostResult<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let Some(completed) = run_batch_cli_job_or_stop(
        exec,
        workflow_state,
        student_refs,
        "scans.align-auto",
        json!({
            "marker_mode": "prefer_aruco",
            "mode": "fast",
            "template_pages": template_pages_as_cli(workspace),
            "student_pages": student_pages,
            "providers": { "alignment_engine": "core_template_match" },
        }),
        json!({ "student_refs": student_refs }),
    )?
    else {
        return Ok(Vec::new());
    };
    apply_alignment_batch_results(
        project_path,
        event_sink,
        workspace,
        workflow_state,
        student_refs,
        &completed,
    )
}

fn apply_alignment_batch_results(
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workspace: &ExamWorkspaceState,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
    completed: &CompletedWorkerJob,
) -> HostResult<Vec<String>> {
    let mut continue_refs = Vec::new();
    for student_ref in student_refs {
        let filtered =
            filtered_completed_by_student(completed, &["alignment_results"], student_ref)?;
        match apply_alignment_results(workspace, workflow_state, student_ref, filtered)? {
            AlignmentOutcome::Failed => {}
            AlignmentOutcome::NeedsReview => {
                set_submission_stage(workflow_state, student_ref, "alignment_review")?;
            }
            AlignmentOutcome::Continue => continue_refs.push(student_ref.clone()),
        }
    }
    save_workflow_state_for_refs_and_emit(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        WorkerStatus::Busy,
    )?;
    Ok(continue_refs)
}

fn run_canonicalize_batch(
    exec: WorkflowExecutionContext<'_>,
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workspace: &ExamWorkspaceState,
    intake_by_ref: &HashMap<String, crate::models::StudentIntakeSummary>,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
) -> HostResult<Vec<String>> {
    if student_refs.is_empty() {
        return Ok(Vec::new());
    }
    set_refs_stage(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        "canonicalize",
    )?;
    let plan =
        build_canonicalize_batch_plan(workspace, intake_by_ref, workflow_state, student_refs)?;
    if !plan.failed_refs.is_empty() {
        let worker_status = if plan.runnable_refs.is_empty() {
            WorkerStatus::Ready
        } else {
            WorkerStatus::Busy
        };
        save_workflow_state_for_refs_and_emit(
            project_path,
            workflow_state,
            event_sink,
            &plan.failed_refs,
            worker_status,
        )?;
    }
    if plan.runnable_refs.is_empty() {
        return Ok(Vec::new());
    }
    let Some(completed) = run_batch_cli_job_or_stop(
        exec,
        workflow_state,
        &plan.runnable_refs,
        "scans.canonicalize",
        json!({ "canonicalize_targets": plan.targets }),
        json!({ "student_refs": plan.runnable_refs }),
    )?
    else {
        return Ok(Vec::new());
    };
    apply_canonicalize_batch_results(
        project_path,
        event_sink,
        workflow_state,
        &plan.runnable_refs,
        &completed,
    )
}

struct CanonicalizeBatchPlan {
    runnable_refs: Vec<String>,
    failed_refs: Vec<String>,
    targets: Vec<Value>,
}

fn build_canonicalize_batch_plan(
    workspace: &ExamWorkspaceState,
    intake_by_ref: &HashMap<String, crate::models::StudentIntakeSummary>,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
) -> HostResult<CanonicalizeBatchPlan> {
    let mut runnable_refs = Vec::new();
    let mut failed_refs = Vec::new();
    let mut targets = Vec::new();
    for student_ref in student_refs {
        let intake = match intake_by_ref.get(student_ref) {
            Some(intake) => intake,
            None => {
                mark_submission_failed(
                    workflow_state,
                    student_ref,
                    &format!("Student intake '{student_ref}' was missing."),
                )?;
                failed_refs.push(student_ref.clone());
                continue;
            }
        };
        let target_result = {
            let submission = find_submission_mut(workflow_state, student_ref)?;
            build_canonicalize_targets(workspace, intake, submission)
        };
        match target_result {
            Ok(student_targets) => {
                targets.extend(student_targets);
                runnable_refs.push(student_ref.clone());
            }
            Err(err) => {
                mark_submission_failed(workflow_state, student_ref, &err.to_string())?;
                failed_refs.push(student_ref.clone());
            }
        }
    }
    Ok(CanonicalizeBatchPlan {
        runnable_refs,
        failed_refs,
        targets,
    })
}

#[cfg(test)]
fn build_canonicalize_batch_targets(
    workspace: &ExamWorkspaceState,
    intake_by_ref: &HashMap<String, crate::models::StudentIntakeSummary>,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
) -> HostResult<Vec<Value>> {
    let mut targets = Vec::new();
    for student_ref in student_refs {
        let intake = intake_by_ref.get(student_ref).ok_or_else(|| {
            HostError::Validation(format!("Student intake '{student_ref}' was missing."))
        })?;
        let submission = find_submission_mut(workflow_state, student_ref)?;
        targets.extend(build_canonicalize_targets(workspace, intake, submission)?);
    }
    Ok(targets)
}

fn apply_canonicalize_batch_results(
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
    completed: &CompletedWorkerJob,
) -> HostResult<Vec<String>> {
    let mut continue_refs = Vec::new();
    for student_ref in student_refs {
        let filtered =
            filtered_completed_by_student(completed, &["canonicalize_results"], student_ref)?;
        let canonicalize_data = success_data(&filtered.result.envelope)?;
        let pages = parse_canonicalized_pages(canonicalize_data)?;
        let submission = find_submission_mut(workflow_state, student_ref)?;
        submission.latest_job_id = Some(completed.job_id.clone());
        submission.page_artifacts = pages;
        if submission.page_artifacts.len() != submission.alignment_pages.len() {
            fail_submission(
                submission,
                "Canonicalize failed for one or more student pages.",
            );
            continue;
        }
        submission.stage = "detect".into();
        continue_refs.push(student_ref.clone());
    }
    save_workflow_state_for_refs_and_emit(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        WorkerStatus::Busy,
    )?;
    Ok(continue_refs)
}

fn run_scan_prep_batch(
    exec: WorkflowExecutionContext<'_>,
    workspace: &ExamWorkspaceState,
    settings: &AppSettings,
    pii_trigger_words_by_student_ref: &HashMap<String, Vec<String>>,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
) -> HostResult<Vec<String>> {
    let crop_targets_by_ref = run_detect_batch(
        exec,
        exec.project_path,
        exec.event_sink,
        workspace,
        workflow_state,
        student_refs,
    )?;
    let crop_rows_by_ref = run_crop_batch(
        exec,
        exec.project_path,
        exec.event_sink,
        workflow_state,
        &crop_targets_by_ref,
    )?;
    let pii_refs = run_pii_batch(
        exec,
        workspace,
        settings,
        pii_trigger_words_by_student_ref,
        workflow_state,
        crop_rows_by_ref,
    )?;
    run_parse_batch(
        exec,
        exec.project_path,
        exec.event_sink,
        settings,
        workspace,
        workflow_state,
        &pii_refs,
    )
}

fn run_detect_batch(
    exec: WorkflowExecutionContext<'_>,
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workspace: &ExamWorkspaceState,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
) -> HostResult<HashMap<String, Vec<Value>>> {
    if student_refs.is_empty() {
        return Ok(HashMap::new());
    }
    let DetectBatchRefs {
        mut crop_targets_by_ref,
        detect_refs,
    } = split_detect_refs_by_reusable_crop_targets(workflow_state, student_refs)?;
    if detect_refs.is_empty() {
        return Ok(crop_targets_by_ref);
    }
    set_refs_stage(
        project_path,
        workflow_state,
        event_sink,
        &detect_refs,
        "detect",
    )?;
    let mut targets = Vec::new();
    for student_ref in &detect_refs {
        let submission = find_submission_mut(workflow_state, student_ref)?;
        targets.extend(build_detect_targets(workspace, submission)?);
    }
    let Some(completed) = run_batch_cli_job_or_stop(
        exec,
        workflow_state,
        &detect_refs,
        SCANS_DETECT_COMMAND,
        json!({ "detect_targets": targets }),
        json!({ "student_refs": detect_refs }),
    )?
    else {
        return Ok(crop_targets_by_ref);
    };
    if let Some(err) = detect_batch_command_error(&completed) {
        mark_refs_stopped_for_error(project_path, workflow_state, event_sink, &detect_refs, &err)?;
        return Ok(crop_targets_by_ref);
    }
    crop_targets_by_ref.extend(apply_detect_batch_results(
        workspace,
        project_path,
        event_sink,
        workflow_state,
        &detect_refs,
        &completed,
    )?);
    Ok(crop_targets_by_ref)
}

struct DetectBatchRefs {
    crop_targets_by_ref: HashMap<String, Vec<Value>>,
    detect_refs: Vec<String>,
}

fn split_detect_refs_by_reusable_crop_targets(
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
) -> HostResult<DetectBatchRefs> {
    let mut crop_targets_by_ref = HashMap::new();
    let mut detect_refs = Vec::new();
    for student_ref in student_refs {
        let submission = find_submission_mut(workflow_state, student_ref)?;
        if submission.stage == "crop" {
            if let Some(targets) = crop_targets_from_resolved_detect_review(submission)? {
                crop_targets_by_ref.insert(student_ref.clone(), targets);
                continue;
            }
        }
        detect_refs.push(student_ref.clone());
    }
    Ok(DetectBatchRefs {
        crop_targets_by_ref,
        detect_refs,
    })
}

fn apply_detect_batch_results(
    workspace: &ExamWorkspaceState,
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
    completed: &CompletedWorkerJob,
) -> HostResult<HashMap<String, Vec<Value>>> {
    let mut crop_targets_by_ref = HashMap::new();
    for student_ref in student_refs {
        if let Some(crop_targets) = apply_detect_result_for_student(
            workspace,
            project_path,
            event_sink,
            workflow_state,
            student_ref,
            completed,
        )? {
            crop_targets_by_ref.insert(student_ref.clone(), crop_targets);
        }
    }
    save_workflow_state_for_refs_and_emit(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        if crop_targets_by_ref.is_empty() {
            WorkerStatus::Ready
        } else {
            WorkerStatus::Busy
        },
    )?;
    Ok(crop_targets_by_ref)
}

fn apply_detect_result_for_student(
    workspace: &ExamWorkspaceState,
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workflow_state: &mut StudentWorkflowState,
    student_ref: &str,
    completed: &CompletedWorkerJob,
) -> HostResult<Option<Vec<Value>>> {
    let detect_data = match detect_data_for_student(completed, student_ref) {
        Ok(data) => data,
        Err(err) => {
            mark_ref_stopped_for_error(
                project_path,
                workflow_state,
                event_sink,
                student_ref,
                &err,
            )?;
            return Ok(None);
        }
    };
    match detect_crop_targets_for_student(
        workspace,
        workflow_state,
        student_ref,
        completed,
        detect_data,
    ) {
        Ok(targets) => Ok(targets),
        Err(err) => {
            mark_ref_stopped_for_error(
                project_path,
                workflow_state,
                event_sink,
                student_ref,
                &err,
            )?;
            Ok(None)
        }
    }
}

fn detect_crop_targets_for_student(
    workspace: &ExamWorkspaceState,
    workflow_state: &mut StudentWorkflowState,
    student_ref: &str,
    completed: &CompletedWorkerJob,
    detect_data: serde_json::Map<String, Value>,
) -> HostResult<Option<Vec<Value>>> {
    let submission = find_submission_mut(workflow_state, student_ref)?;
    submission.latest_job_id = Some(completed.job_id.clone());
    apply_detect_page_ocr_results(submission, &detect_data)?;
    submission.detect_review = build_detect_review(workspace, submission, &detect_data)?;
    if submission.detect_review.is_some() {
        submission.stage = "detect_review".into();
        return Ok(None);
    }
    let crop_targets = build_crop_targets(&detect_data)?;
    if crop_targets.is_empty() {
        fail_submission(submission, "No question regions were detected for crop.");
        return Ok(None);
    }
    submission.detect_review = None;
    submission.stage = "crop".into();
    Ok(Some(crop_targets))
}

fn run_crop_batch(
    exec: WorkflowExecutionContext<'_>,
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workflow_state: &mut StudentWorkflowState,
    crop_targets_by_ref: &HashMap<String, Vec<Value>>,
) -> HostResult<HashMap<String, Vec<Value>>> {
    let student_refs = sorted_keys(crop_targets_by_ref);
    if student_refs.is_empty() {
        return Ok(HashMap::new());
    }
    set_refs_stage(
        project_path,
        workflow_state,
        event_sink,
        &student_refs,
        "crop",
    )?;
    let mut out = HashMap::new();
    for student_ref in &student_refs {
        let page_artifacts = {
            let submission = find_submission_mut(workflow_state, student_ref)?;
            submission.page_artifacts.clone()
        };
        let targets = crop_targets_for_cli(
            crop_targets_by_ref
                .get(student_ref)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        );
        let Some(completed) = run_batch_cli_job_or_stop(
            exec,
            workflow_state,
            std::slice::from_ref(student_ref),
            "scans.crop",
            json!({
                "pages": student_workflow_pages_as_cli(&page_artifacts, student_ref),
                "question_crop_targets": targets,
            }),
            json!({ "student_ref": student_ref }),
        )?
        else {
            continue;
        };
        let rows = match crop_result_rows_for_student(&completed) {
            Ok(rows) => rows,
            Err(err) => {
                mark_refs_stopped_for_error(
                    project_path,
                    workflow_state,
                    event_sink,
                    std::slice::from_ref(student_ref),
                    &err,
                )?;
                continue;
            }
        };
        let submission = find_submission_mut(workflow_state, student_ref)?;
        submission.latest_job_id = Some(completed.job_id);
        submission.detect_review = None;
        submission.stage = "pii".into();
        out.insert(student_ref.clone(), rows);
    }
    save_workflow_state_for_refs_and_emit(
        project_path,
        workflow_state,
        event_sink,
        &student_refs,
        WorkerStatus::Busy,
    )?;
    Ok(out)
}

fn crop_targets_for_cli(targets: &[Value]) -> Vec<Value> {
    targets
        .iter()
        .map(|target| {
            let mut target = target.clone();
            if let Some(object) = target.as_object_mut() {
                object.remove("student_ref");
            }
            target
        })
        .collect()
}

fn crop_targets_from_resolved_detect_review(
    submission: &StudentWorkflowSubmission,
) -> HostResult<Option<Vec<Value>>> {
    let Some(review) = submission.detect_review.as_ref() else {
        return Ok(None);
    };
    if review
        .pending_rows
        .iter()
        .any(|row| row.resolved_region.is_none())
    {
        return Ok(None);
    }
    crop_targets_from_detect_review(review, &submission.student_ref).map(Some)
}

fn run_pii_batch(
    exec: WorkflowExecutionContext<'_>,
    workspace: &ExamWorkspaceState,
    settings: &AppSettings,
    pii_trigger_words_by_student_ref: &HashMap<String, Vec<String>>,
    workflow_state: &mut StudentWorkflowState,
    crop_rows_by_ref: HashMap<String, Vec<Value>>,
) -> HostResult<Vec<String>> {
    if crop_rows_by_ref.is_empty() {
        return Ok(Vec::new());
    }
    let student_refs = sorted_keys(&crop_rows_by_ref);
    set_refs_stage(
        exec.project_path,
        workflow_state,
        exec.event_sink,
        &student_refs,
        "pii",
    )?;
    let pii_students = prepare_pii_batch_inputs(
        exec.project_path,
        exec.event_sink,
        workspace,
        workflow_state,
        pii_trigger_words_by_student_ref,
        &crop_rows_by_ref,
    )?;
    if pii_students.is_empty() {
        save_workflow_state_for_refs_and_emit(
            exec.project_path,
            workflow_state,
            exec.event_sink,
            &student_refs,
            WorkerStatus::Ready,
        )?;
        return Ok(Vec::new());
    }
    let pii_student_refs = pii_students
        .iter()
        .filter_map(|row| {
            row.get("student_ref")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    let pii_model_dir = match resolve_pii_model_dir(exec.state, settings) {
        Ok(path) => path,
        Err(err) => {
            mark_refs_stopped_for_error(
                exec.project_path,
                workflow_state,
                exec.event_sink,
                &pii_student_refs,
                &err,
            )?;
            return Ok(Vec::new());
        }
    };
    let Some(completed) = run_batch_cli_job_or_stop(
        exec,
        workflow_state,
        &pii_student_refs,
        "scans.pii",
        json!({
            "students": pii_students,
            "pii_runtime_config": {
                "paddle_model_dir": pii_model_dir.to_worker_payload_path(),
                "max_workers": 2,
            },
        }),
        sanitized_pii_batch_request_payload(&crop_rows_by_ref),
    )?
    else {
        return Ok(Vec::new());
    };
    apply_pii_batch_results(
        exec.project_path,
        exec.event_sink,
        workspace,
        workflow_state,
        &crop_rows_by_ref,
        &completed,
    )
}

fn prepare_pii_batch_inputs(
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workspace: &ExamWorkspaceState,
    workflow_state: &mut StudentWorkflowState,
    pii_trigger_words_by_student_ref: &HashMap<String, Vec<String>>,
    crop_rows_by_ref: &HashMap<String, Vec<Value>>,
) -> HostResult<Vec<Value>> {
    let mut students = Vec::new();
    for student_ref in sorted_keys(crop_rows_by_ref) {
        let crop_rows = crop_rows_by_ref
            .get(&student_ref)
            .expect("key came from map");
        let clean_crop_rows = clean_crop_row_count(crop_rows);
        let trigger_words = pii_trigger_words_by_student_ref.get(&student_ref);
        if clean_crop_rows == 0 || trigger_words.map(Vec::is_empty).unwrap_or(true) {
            set_manual_pii_block(workspace, workflow_state, &student_ref, crop_rows)?;
            continue;
        }
        students.push(pii_student_request(
            &student_ref,
            trigger_words.expect("checked above"),
            crop_rows,
        ));
    }
    save_workflow_state_for_refs_and_emit(
        project_path,
        workflow_state,
        event_sink,
        &sorted_keys(crop_rows_by_ref),
        WorkerStatus::Busy,
    )?;
    Ok(students)
}

fn set_manual_pii_block(
    workspace: &ExamWorkspaceState,
    workflow_state: &mut StudentWorkflowState,
    student_ref: &str,
    crop_rows: &[Value],
) -> HostResult<()> {
    let submission = find_submission_mut(workflow_state, student_ref)?;
    submission.answers = build_answers_for_manual_pii_block(
        workspace,
        crop_rows,
        pii_identity_unavailable_warning(),
    )?;
    submission.stage = "manual_grading".into();
    submission.failure_message = None;
    submission.latest_job_id = None;
    Ok(())
}

fn clean_crop_row_count(crop_rows: &[Value]) -> usize {
    crop_rows
        .iter()
        .filter(|row| matches!(row.get("status").and_then(Value::as_str), Some("ok")))
        .count()
}

fn pii_targets_from_crop_rows(crop_rows: &[Value]) -> Vec<Value> {
    crop_rows
        .iter()
        .filter(|row| matches!(row.get("status").and_then(Value::as_str), Some("ok")))
        .map(|row| {
            json!({
                "question_id": row.get("question_id").cloned().unwrap_or_default(),
                "question_crop_path": row.get("question_crop_path").cloned().unwrap_or_default(),
            })
        })
        .collect()
}

fn pii_student_request(student_ref: &str, trigger_words: &[String], crop_rows: &[Value]) -> Value {
    json!({
        "student_ref": student_ref,
        "pii_trigger_words": trigger_words,
        "pii_targets": pii_targets_from_crop_rows(crop_rows),
    })
}

fn apply_pii_batch_results(
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workspace: &ExamWorkspaceState,
    workflow_state: &mut StudentWorkflowState,
    crop_rows_by_ref: &HashMap<String, Vec<Value>>,
    completed: &CompletedWorkerJob,
) -> HostResult<Vec<String>> {
    let mut parse_refs = Vec::new();
    for student_ref in sorted_keys(crop_rows_by_ref) {
        if let Some(parse_ref) = apply_pii_result_for_student(
            project_path,
            event_sink,
            workspace,
            workflow_state,
            crop_rows_by_ref,
            completed,
            &student_ref,
        )? {
            parse_refs.push(parse_ref);
        }
    }
    save_workflow_state_for_refs_and_emit(
        project_path,
        workflow_state,
        event_sink,
        &sorted_keys(crop_rows_by_ref),
        WorkerStatus::Busy,
    )?;
    Ok(parse_refs)
}

fn apply_pii_result_for_student(
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workspace: &ExamWorkspaceState,
    workflow_state: &mut StudentWorkflowState,
    crop_rows_by_ref: &HashMap<String, Vec<Value>>,
    completed: &CompletedWorkerJob,
    student_ref: &str,
) -> HostResult<Option<String>> {
    if find_submission_mut(workflow_state, student_ref)?.stage != "pii" {
        return Ok(None);
    }
    match pii_parse_ref_for_student(
        workspace,
        workflow_state,
        crop_rows_by_ref,
        completed,
        student_ref,
    ) {
        Ok(parse_ref) => Ok(parse_ref),
        Err(err) => {
            mark_ref_stopped_for_error(
                project_path,
                workflow_state,
                event_sink,
                student_ref,
                &err,
            )?;
            Ok(None)
        }
    }
}

fn pii_parse_ref_for_student(
    workspace: &ExamWorkspaceState,
    workflow_state: &mut StudentWorkflowState,
    crop_rows_by_ref: &HashMap<String, Vec<Value>>,
    completed: &CompletedWorkerJob,
    student_ref: &str,
) -> HostResult<Option<String>> {
    let pii_data = pii_data_for_student(completed, student_ref)?;
    if required_array(&pii_data, "pii_results").is_ok_and(|rows| rows.is_empty()) {
        return Ok(None);
    }
    let crop_rows = crop_rows_by_ref
        .get(student_ref)
        .expect("key came from map");
    let answers = build_answers_from_pii_results(workspace, crop_rows, &pii_data)?;
    let submission = find_submission_mut(workflow_state, student_ref)?;
    submission.latest_job_id = Some(completed.job_id.clone());
    submission.answers = answers;
    if submission
        .answers
        .iter()
        .all(|answer| answer.manual_grading_required)
    {
        submission.stage = "manual_grading".into();
        submission.failure_message = None;
        Ok(None)
    } else {
        submission.stage = "parse".into();
        Ok(Some(student_ref.to_string()))
    }
}

fn run_parse_batch(
    exec: WorkflowExecutionContext<'_>,
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    settings: &AppSettings,
    workspace: &ExamWorkspaceState,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
) -> HostResult<Vec<String>> {
    if student_refs.is_empty() {
        return Ok(Vec::new());
    }
    set_refs_stage(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        "parse",
    )?;
    let mut parse_targets = Vec::new();
    for student_ref in student_refs {
        let submission = find_submission_mut(workflow_state, student_ref)?;
        let crop_rows = crop_rows_from_answers(submission);
        parse_targets.extend(build_parse_targets(workspace, submission, &crop_rows)?);
    }
    if parse_targets.is_empty() {
        mark_refs_failed_with_message(
            project_path,
            workflow_state,
            event_sink,
            student_refs,
            "PII screening produced no parseable answer rows.",
        )?;
        return Ok(Vec::new());
    }
    let Some(completed) = run_batch_cli_job_or_stop(
        exec,
        workflow_state,
        student_refs,
        "scans.parse",
        json!({
            "parse_targets": parse_targets,
            "providers": { "llm_provider": settings.llm_provider },
            "llm_config": llm_config_json(settings),
        }),
        persisted_llm_request_payload(settings, json!({ "student_refs": student_refs })),
    )?
    else {
        return Ok(Vec::new());
    };
    apply_parse_batch_results(
        project_path,
        event_sink,
        workflow_state,
        student_refs,
        &completed,
    )
}

fn crop_rows_from_answers(submission: &StudentWorkflowSubmission) -> Vec<Value> {
    submission
        .answers
        .iter()
        .filter_map(|answer| {
            answer.crop_image_path.as_ref().map(|path| {
                json!({
                    "student_ref": submission.student_ref,
                    "question_id": answer.question_id,
                    "status": "ok",
                    "question_crop_path": path,
                })
            })
        })
        .collect()
}

fn apply_parse_batch_results(
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
    completed: &CompletedWorkerJob,
) -> HostResult<Vec<String>> {
    let mut grading_refs = Vec::new();
    for student_ref in student_refs {
        if apply_parse_result_for_student(
            project_path,
            event_sink,
            workflow_state,
            student_ref,
            completed,
        )? {
            grading_refs.push(student_ref.clone());
        }
    }
    save_workflow_state_for_refs_and_emit(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        WorkerStatus::Busy,
    )?;
    Ok(grading_refs)
}

fn apply_parse_result_for_student(
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workflow_state: &mut StudentWorkflowState,
    student_ref: &str,
    completed: &CompletedWorkerJob,
) -> HostResult<bool> {
    match parse_grading_ready_for_student(workflow_state, student_ref, completed) {
        Ok(grading_ready) => Ok(grading_ready),
        Err(err) => {
            mark_ref_stopped_for_error(
                project_path,
                workflow_state,
                event_sink,
                student_ref,
                &err,
            )?;
            Ok(false)
        }
    }
}

fn parse_grading_ready_for_student(
    workflow_state: &mut StudentWorkflowState,
    student_ref: &str,
    completed: &CompletedWorkerJob,
) -> HostResult<bool> {
    let parse_data = parse_data_for_student(completed, student_ref)?;
    let submission = find_submission_mut(workflow_state, student_ref)?;
    submission.latest_job_id = Some(completed.job_id.clone());
    merge_parse_results_into_answers(submission, &parse_data)?;
    if submission
        .answers
        .iter()
        .any(|answer| matches!(answer.parse_status.as_str(), "error" | "cancelled"))
    {
        fail_submission(submission, "Parsing failed for one or more answers.");
        Ok(false)
    } else if submission
        .answers
        .iter()
        .any(|answer| answer.review_required)
    {
        submission.stage = "parse_review".into();
        Ok(false)
    } else {
        submission.stage = "grading".into();
        Ok(true)
    }
}

fn run_grading_batch(
    exec: WorkflowExecutionContext<'_>,
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workspace: &ExamWorkspaceState,
    settings: &AppSettings,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
) -> HostResult<()> {
    if student_refs.is_empty() {
        return emit_ready_for_empty_batch_continuation(project_path, workflow_state, event_sink);
    }
    set_refs_stage(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        "grading",
    )?;
    let question_by_id = workspace
        .questions
        .iter()
        .map(|question| (question.question_id.clone(), question))
        .collect::<HashMap<_, _>>();
    let score_requests_by_ref = build_batch_answer_score_requests(
        workspace,
        workflow_state,
        &question_by_id,
        student_refs,
    )?;
    let answer_score_requests = score_requests_by_ref
        .values()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    let ready_refs = score_requests_by_ref
        .iter()
        .filter(|(_, rows)| !rows.is_empty())
        .map(|(student_ref, _)| student_ref.clone())
        .collect::<Vec<_>>();
    settle_empty_grading_refs(
        project_path,
        event_sink,
        workflow_state,
        student_refs,
        &score_requests_by_ref,
    )?;
    if answer_score_requests.is_empty() {
        return emit_ready_for_empty_batch_continuation(project_path, workflow_state, event_sink);
    }
    let grading_payload =
        persisted_llm_request_payload(settings, json!({ "student_refs": ready_refs }));
    let preliminary_rows = run_grading_value_stage(
        exec,
        workflow_state,
        &ready_refs,
        "grading.score-preliminary",
        preliminary_grading_request_payload(&answer_score_requests, settings),
        grading_payload.clone(),
        "preliminary_scores",
    )?;
    if preliminary_rows.is_empty() {
        return Ok(());
    }
    let final_rows = persist_batch_preliminary_rows(
        project_path,
        event_sink,
        workspace,
        workflow_state,
        &question_by_id,
        &ready_refs,
        &preliminary_rows,
    )?;
    let feedback_rows = run_grading_value_stage(
        exec,
        workflow_state,
        &ready_refs,
        "grading.draft-feedback",
        json!({
            "feedback_requests": feedback_request_rows(&final_rows),
            "providers": { "llm_provider": settings.llm_provider },
            "llm_config": llm_config_json(settings),
        }),
        grading_payload.clone(),
        "feedback_drafts",
    )?;
    if feedback_rows.is_empty() {
        return Ok(());
    }
    persist_batch_feedback_rows(
        project_path,
        event_sink,
        workflow_state,
        &ready_refs,
        &feedback_rows,
    )?;
    let highlight_rows = run_grading_value_stage(
        exec,
        workflow_state,
        &ready_refs,
        "grading.markup",
        json!({
            "markup_requests": feedback_request_rows(&final_rows),
            "providers": { "llm_provider": settings.llm_provider },
            "llm_config": llm_config_json(settings),
        }),
        grading_payload,
        "highlight_results",
    )?;
    if highlight_rows.is_empty() {
        return Ok(());
    }
    finish_batch_grading(
        project_path,
        event_sink,
        workflow_state,
        &ready_refs,
        final_rows,
        &feedback_rows,
        &highlight_rows,
    )
}

pub(super) fn regrade_stale_question_answers(
    state: &Arc<super::AppStateInner>,
    project_path: &Path,
    settings: &AppSettings,
    event_sink: &dyn RuntimeEventSink,
    workspace: &ExamWorkspaceState,
    workflow_state: &mut StudentWorkflowState,
    question_id: &str,
) -> HostResult<()> {
    if !question_can_regrade(workspace, question_id)? {
        return Ok(());
    }
    let question_by_id = workspace
        .questions
        .iter()
        .map(|question| (question.question_id.clone(), question))
        .collect::<HashMap<_, _>>();
    let Some(targets) =
        prepare_question_regrade_targets(workspace, workflow_state, &question_by_id, question_id)?
    else {
        return Ok(());
    };
    set_refs_stage(
        project_path,
        workflow_state,
        event_sink,
        &targets.ready_refs,
        "grading",
    )?;

    let exec = WorkflowExecutionContext {
        state,
        project_path,
        event_sink,
    };
    let grading_payload = persisted_llm_request_payload(
        settings,
        json!({ "student_refs": targets.ready_refs, "question_id": question_id, "regrade": true }),
    );
    let Some(rows) = run_question_regrade_stages(
        exec,
        workspace,
        settings,
        workflow_state,
        &question_by_id,
        &targets,
        grading_payload,
    )?
    else {
        return Ok(());
    };
    finish_question_regrade(
        project_path,
        event_sink,
        workflow_state,
        &targets.ready_refs,
        rows.final_rows,
        &rows.feedback_rows,
        &rows.highlight_rows,
    )
}

struct QuestionRegradeTargets {
    ready_refs: Vec<String>,
    answer_score_requests: Vec<Value>,
}

struct QuestionRegradeRows {
    final_rows: Vec<Value>,
    feedback_rows: Vec<Value>,
    highlight_rows: Vec<Value>,
}

fn question_can_regrade(workspace: &ExamWorkspaceState, question_id: &str) -> HostResult<bool> {
    let question = workspace
        .questions
        .iter()
        .find(|question| question.question_id == question_id)
        .ok_or_else(|| HostError::Validation(format!("Question '{question_id}' was not found.")))?;
    if !question.rubric.is_approved() {
        return Err(HostError::Validation(
            "Rubric must be approved before regrading stale answers.".into(),
        ));
    }
    Ok(!workspace
        .moderation_state
        .question_reviews
        .iter()
        .any(|review| review.question_id == question_id))
}

fn prepare_question_regrade_targets(
    workspace: &ExamWorkspaceState,
    workflow_state: &mut StudentWorkflowState,
    question_by_id: &HashMap<String, &QuestionRecord>,
    question_id: &str,
) -> HostResult<Option<QuestionRegradeTargets>> {
    let score_requests_by_ref = build_regrade_answer_score_requests(
        workspace,
        workflow_state,
        question_by_id,
        question_id,
    )?;
    let answer_score_requests = score_requests_by_ref
        .values()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    if answer_score_requests.is_empty() {
        return Ok(None);
    }
    let ready_refs = score_requests_by_ref
        .iter()
        .filter(|(_, rows)| !rows.is_empty())
        .map(|(student_ref, _)| student_ref.clone())
        .collect::<Vec<_>>();
    Ok(Some(QuestionRegradeTargets {
        ready_refs,
        answer_score_requests,
    }))
}

fn run_question_regrade_stages(
    exec: WorkflowExecutionContext<'_>,
    workspace: &ExamWorkspaceState,
    settings: &AppSettings,
    workflow_state: &mut StudentWorkflowState,
    question_by_id: &HashMap<String, &QuestionRecord>,
    targets: &QuestionRegradeTargets,
    grading_payload: Value,
) -> HostResult<Option<QuestionRegradeRows>> {
    let preliminary_rows = run_grading_value_stage(
        exec,
        workflow_state,
        &targets.ready_refs,
        "grading.score-preliminary",
        preliminary_grading_request_payload(&targets.answer_score_requests, settings),
        grading_payload.clone(),
        "preliminary_scores",
    )?;
    if preliminary_rows.is_empty() {
        return Ok(None);
    }
    let final_rows = persist_batch_preliminary_rows(
        exec.project_path,
        exec.event_sink,
        workspace,
        workflow_state,
        question_by_id,
        &targets.ready_refs,
        &preliminary_rows,
    )?;
    let feedback_rows = run_grading_value_stage(
        exec,
        workflow_state,
        &targets.ready_refs,
        "grading.draft-feedback",
        json!({
            "feedback_requests": feedback_request_rows(&final_rows),
            "providers": { "llm_provider": settings.llm_provider },
            "llm_config": llm_config_json(settings),
        }),
        grading_payload.clone(),
        "feedback_drafts",
    )?;
    if feedback_rows.is_empty() {
        return Ok(None);
    }
    persist_batch_feedback_rows(
        exec.project_path,
        exec.event_sink,
        workflow_state,
        &targets.ready_refs,
        &feedback_rows,
    )?;
    let highlight_rows = run_grading_value_stage(
        exec,
        workflow_state,
        &targets.ready_refs,
        "grading.markup",
        json!({
            "markup_requests": feedback_request_rows(&final_rows),
            "providers": { "llm_provider": settings.llm_provider },
            "llm_config": llm_config_json(settings),
        }),
        grading_payload,
        "highlight_results",
    )?;
    if highlight_rows.is_empty() {
        return Ok(None);
    }
    Ok(Some(QuestionRegradeRows {
        final_rows,
        feedback_rows,
        highlight_rows,
    }))
}

fn finish_question_regrade(
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
    final_rows: Vec<Value>,
    feedback_rows: &[Value],
    highlight_rows: &[Value],
) -> HostResult<()> {
    for student_ref in student_refs {
        let student_final_rows = rows_for_student(&final_rows, student_ref);
        let student_feedback_rows = rows_for_student(feedback_rows, student_ref);
        let student_highlight_rows = rows_for_student(highlight_rows, student_ref);
        let submission = find_submission_mut(workflow_state, student_ref)?;
        apply_final_grading_rows(submission, &student_final_rows)?;
        apply_feedback_rows(submission, &student_feedback_rows)?;
        apply_highlight_rows(submission, &student_highlight_rows)?;
        submission.stage = if submission
            .answers
            .iter()
            .any(|answer| answer.manual_grading_required)
        {
            "manual_grading".into()
        } else {
            "graded".into()
        };
        submission.failure_message = None;
    }
    save_workflow_state_for_refs_and_emit(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        WorkerStatus::Ready,
    )
}

fn emit_ready_for_empty_batch_continuation(
    project_path: &Path,
    workflow_state: &mut StudentWorkflowState,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<()> {
    let _ = project_path;
    update_workflow_status(workflow_state);
    emit_workflow_state_updated(event_sink, workflow_state, None, &[], WorkerStatus::Ready);
    Ok(())
}

fn build_regrade_answer_score_requests(
    workspace: &ExamWorkspaceState,
    workflow_state: &mut StudentWorkflowState,
    question_by_id: &HashMap<String, &QuestionRecord>,
    question_id: &str,
) -> HostResult<HashMap<String, Vec<Value>>> {
    let mut out = HashMap::new();
    for submission in workflow_state
        .submissions
        .iter()
        .filter(|submission| submission.stage == "graded")
    {
        out.insert(
            submission.student_ref.clone(),
            build_preliminary_answer_score_requests_for_stale_question(
                workspace,
                submission,
                question_by_id,
                question_id,
            )?,
        );
    }
    Ok(out)
}

fn build_batch_answer_score_requests(
    workspace: &ExamWorkspaceState,
    workflow_state: &mut StudentWorkflowState,
    question_by_id: &HashMap<String, &QuestionRecord>,
    student_refs: &[String],
) -> HostResult<HashMap<String, Vec<Value>>> {
    let mut out = HashMap::new();
    for student_ref in student_refs {
        let submission = find_submission_mut(workflow_state, student_ref)?;
        out.insert(
            student_ref.clone(),
            build_preliminary_answer_score_requests(workspace, submission, question_by_id)?,
        );
    }
    Ok(out)
}

fn preliminary_grading_max_workers(settings: &AppSettings) -> i64 {
    settings.preliminary_grading_max_workers.clamp(1, 4)
}

fn preliminary_grading_runtime_config(settings: &AppSettings) -> Value {
    json!({ "max_workers": preliminary_grading_max_workers(settings) })
}

fn preliminary_grading_request_payload(
    answer_score_requests: &[Value],
    settings: &AppSettings,
) -> Value {
    json!({
        "answer_score_requests": answer_score_requests,
        "grading_runtime_config": preliminary_grading_runtime_config(settings),
        "providers": { "llm_provider": settings.llm_provider },
        "llm_config": llm_config_json(settings),
    })
}

fn settle_empty_grading_refs(
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
    score_requests_by_ref: &HashMap<String, Vec<Value>>,
) -> HostResult<()> {
    for student_ref in student_refs {
        if score_requests_by_ref
            .get(student_ref)
            .is_some_and(|rows| !rows.is_empty())
        {
            continue;
        }
        let submission = find_submission_mut(workflow_state, student_ref)?;
        if submission
            .answers
            .iter()
            .any(|answer| answer.manual_grading_required)
        {
            submission.stage = "manual_grading".into();
            submission.failure_message = None;
        } else {
            fail_submission(
                submission,
                "No verified answers were available for grading.",
            );
        }
    }
    save_workflow_state_for_refs_and_emit(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        WorkerStatus::Busy,
    )
}

fn run_grading_value_stage(
    exec: WorkflowExecutionContext<'_>,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
    command_name: &str,
    request_payload: Value,
    persisted_payload: Value,
    rows_key: &str,
) -> HostResult<Vec<Value>> {
    let Some(completed) = run_batch_cli_job_or_stop(
        exec,
        workflow_state,
        student_refs,
        command_name,
        request_payload,
        persisted_payload,
    )?
    else {
        return Ok(Vec::new());
    };
    for student_ref in student_refs {
        record_submission_job_id(workflow_state, student_ref, &completed.job_id)?;
    }
    grading_rows_or_stop(
        exec.project_path,
        exec.event_sink,
        workflow_state,
        student_refs,
        &completed,
        rows_key,
    )
}

fn persist_batch_preliminary_rows(
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workspace: &ExamWorkspaceState,
    workflow_state: &mut StudentWorkflowState,
    question_by_id: &HashMap<String, &QuestionRecord>,
    student_refs: &[String],
    preliminary_rows: &[Value],
) -> HostResult<Vec<Value>> {
    let mut final_rows = Vec::new();
    for student_ref in student_refs {
        let rows = rows_for_student(preliminary_rows, student_ref);
        {
            let submission = find_submission_mut(workflow_state, student_ref)?;
            apply_preliminary_confidence_to_answers(submission, &rows)?;
        }
        let student_final_rows = {
            let submission = find_submission_mut(workflow_state, student_ref)?;
            final_rows_from_preliminary(workspace, submission, question_by_id, &rows)?
        };
        {
            let submission = find_submission_mut(workflow_state, student_ref)?;
            apply_final_grading_rows(submission, &student_final_rows)?;
        }
        final_rows.extend(student_final_rows);
    }
    save_workflow_state_for_refs_and_emit(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        WorkerStatus::Busy,
    )?;
    Ok(final_rows)
}

fn persist_batch_feedback_rows(
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
    feedback_rows: &[Value],
) -> HostResult<()> {
    for student_ref in student_refs {
        let rows = rows_for_student(feedback_rows, student_ref);
        let submission = find_submission_mut(workflow_state, student_ref)?;
        apply_feedback_rows(submission, &rows)?;
    }
    save_workflow_state_for_refs_and_emit(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        WorkerStatus::Busy,
    )
}

fn finish_batch_grading(
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
    final_rows: Vec<Value>,
    feedback_rows: &[Value],
    highlight_rows: &[Value],
) -> HostResult<()> {
    for student_ref in student_refs {
        let student_final_rows = rows_for_student(&final_rows, student_ref);
        let student_feedback_rows = rows_for_student(feedback_rows, student_ref);
        let student_highlight_rows = rows_for_student(highlight_rows, student_ref);
        let submission = find_submission_mut(workflow_state, student_ref)?;
        apply_feedback_and_markup(
            submission,
            student_final_rows,
            &student_feedback_rows,
            &student_highlight_rows,
        )?;
        submission.stage = if submission
            .answers
            .iter()
            .any(|answer| answer.manual_grading_required)
        {
            "manual_grading".into()
        } else {
            "graded".into()
        };
        submission.failure_message = None;
    }
    save_workflow_state_for_refs_and_emit(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        WorkerStatus::Ready,
    )
}

fn rows_for_student(rows: &[Value], student_ref: &str) -> Vec<Value> {
    rows.iter()
        .filter(|row| row.get("student_ref").and_then(Value::as_str) == Some(student_ref))
        .cloned()
        .collect()
}

fn set_refs_stage(
    project_path: &Path,
    workflow_state: &mut StudentWorkflowState,
    event_sink: &dyn RuntimeEventSink,
    student_refs: &[String],
    stage: &str,
) -> HostResult<()> {
    for student_ref in student_refs {
        let submission = find_submission_mut(workflow_state, student_ref)?;
        submission.stage = stage.into();
        submission.failure_message = None;
    }
    save_workflow_state_for_refs_and_emit(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        WorkerStatus::Busy,
    )
}

fn completed_was_cancelled(completed: &CompletedWorkerJob) -> bool {
    completed.result.terminal_type == "job_cancelled"
}

fn run_batch_cli_job_or_stop(
    exec: WorkflowExecutionContext<'_>,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
    command_name: &str,
    worker_request_payload: Value,
    persisted_request_payload: Value,
) -> HostResult<Option<CompletedWorkerJob>> {
    match run_cli_job(
        exec.state,
        exec.event_sink,
        exec.project_path,
        command_name,
        worker_request_payload,
        persisted_request_payload,
    ) {
        Ok(completed) if completed_was_cancelled(&completed) => {
            mark_refs_stopped_and_emit(
                exec.project_path,
                workflow_state,
                exec.event_sink,
                student_refs,
            )?;
            Ok(None)
        }
        Ok(completed) => Ok(Some(completed)),
        Err(err) => {
            mark_refs_stopped_for_error(
                exec.project_path,
                workflow_state,
                exec.event_sink,
                student_refs,
                &err,
            )?;
            Ok(None)
        }
    }
}

fn stopped_message_for_error(err: &HostError) -> String {
    format!("Workflow stopped before this stage completed: {err}")
}

fn detect_batch_command_error(completed: &CompletedWorkerJob) -> Option<HostError> {
    if completed
        .result
        .envelope
        .get("data")
        .and_then(Value::as_object)
        .is_some()
    {
        return None;
    }
    Some(
        success_data(&completed.result.envelope)
            .err()
            .unwrap_or_else(|| {
                HostError::Protocol("Detect command completed without result data.".into())
            }),
    )
}

fn mark_refs_stopped_and_emit(
    project_path: &Path,
    workflow_state: &mut StudentWorkflowState,
    event_sink: &dyn RuntimeEventSink,
    student_refs: &[String],
) -> HostResult<()> {
    mark_refs_stopped_with_message_and_emit(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        "Workflow stopped before this stage completed.",
    )
}

fn mark_refs_stopped_for_error(
    project_path: &Path,
    workflow_state: &mut StudentWorkflowState,
    event_sink: &dyn RuntimeEventSink,
    student_refs: &[String],
    err: &HostError,
) -> HostResult<()> {
    mark_refs_stopped_with_message_and_emit(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        &stopped_message_for_error(err),
    )
}

fn mark_refs_stopped_with_message_and_emit(
    project_path: &Path,
    workflow_state: &mut StudentWorkflowState,
    event_sink: &dyn RuntimeEventSink,
    student_refs: &[String],
    message: &str,
) -> HostResult<()> {
    for student_ref in student_refs {
        let submission = find_submission_mut(workflow_state, student_ref)?;
        submission.stage = "stopped".into();
        submission.failure_message = Some(message.into());
    }
    save_workflow_state_for_refs_and_emit(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        WorkerStatus::Ready,
    )
}

fn crop_result_rows_for_student(completed: &CompletedWorkerJob) -> HostResult<Vec<Value>> {
    required_rows_from_completed(completed, "crop_results")
}

fn required_rows_from_completed(
    completed: &CompletedWorkerJob,
    rows_key: &str,
) -> HostResult<Vec<Value>> {
    let data = success_data(&completed.result.envelope)?;
    required_array(data, rows_key).map(|rows| rows.to_vec())
}

fn result_data_for_student(
    completed: &CompletedWorkerJob,
    row_keys: &[&str],
    required_key: &str,
    student_ref: &str,
    require_student_rows: bool,
) -> HostResult<serde_json::Map<String, Value>> {
    let filtered = filtered_completed_by_student(completed, row_keys, student_ref)?;
    let data = success_data(&filtered.result.envelope)?;
    let rows = required_array(data, required_key)?;
    if require_student_rows && rows.is_empty() {
        return Err(HostError::Protocol(format!(
            "Command result was missing {required_key} rows for student '{student_ref}'."
        )));
    }
    Ok(data.clone())
}

fn detect_data_for_student(
    completed: &CompletedWorkerJob,
    student_ref: &str,
) -> HostResult<serde_json::Map<String, Value>> {
    result_data_for_student(
        completed,
        &["detect_results", "page_ocr_results"],
        "detect_results",
        student_ref,
        false,
    )
}

fn pii_data_for_student(
    completed: &CompletedWorkerJob,
    student_ref: &str,
) -> HostResult<serde_json::Map<String, Value>> {
    result_data_for_student(
        completed,
        &["pii_results"],
        "pii_results",
        student_ref,
        true,
    )
}

fn parse_data_for_student(
    completed: &CompletedWorkerJob,
    student_ref: &str,
) -> HostResult<serde_json::Map<String, Value>> {
    result_data_for_student(
        completed,
        &["parse_results"],
        "parse_results",
        student_ref,
        true,
    )
}

fn grading_rows_or_stop(
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
    completed: &CompletedWorkerJob,
    rows_key: &str,
) -> HostResult<Vec<Value>> {
    let data = match required_rows_from_completed(completed, rows_key) {
        Ok(rows) => rows,
        Err(err) => {
            mark_refs_stopped_for_error(
                project_path,
                workflow_state,
                event_sink,
                student_refs,
                &err,
            )?;
            return Ok(Vec::new());
        }
    };
    Ok(data)
}

fn mark_refs_failed_with_message(
    project_path: &Path,
    workflow_state: &mut StudentWorkflowState,
    event_sink: &dyn RuntimeEventSink,
    student_refs: &[String],
    message: &str,
) -> HostResult<()> {
    for student_ref in student_refs {
        mark_submission_failed(workflow_state, student_ref, message)?;
    }
    save_workflow_state_for_refs_and_emit(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        WorkerStatus::Ready,
    )
}

fn filtered_completed_by_student(
    completed: &CompletedWorkerJob,
    row_keys: &[&str],
    student_ref: &str,
) -> HostResult<CompletedWorkerJob> {
    let mut envelope = completed.result.envelope.clone();
    let data = envelope
        .get_mut("data")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| HostError::Protocol("Worker success envelope was missing data.".into()))?;
    for key in row_keys {
        if let Some(Value::Array(rows)) = data.get_mut(*key) {
            rows.retain(|row| row.get("student_ref").and_then(Value::as_str) == Some(student_ref));
        }
    }
    Ok(CompletedWorkerJob {
        job_id: completed.job_id.clone(),
        result: WorkerJobResult {
            terminal_type: completed.result.terminal_type.clone(),
            terminal_payload: completed.result.terminal_payload.clone(),
            envelope,
            events: completed.result.events.clone(),
        },
    })
}

fn sanitized_pii_batch_request_payload(crop_rows_by_ref: &HashMap<String, Vec<Value>>) -> Value {
    let students = sorted_keys(crop_rows_by_ref)
        .into_iter()
        .map(|student_ref| {
            let question_ids = crop_rows_by_ref
                .get(&student_ref)
                .into_iter()
                .flatten()
                .filter(|row| matches!(row.get("status").and_then(Value::as_str), Some("ok")))
                .filter_map(|row| row.get("question_id").and_then(Value::as_str))
                .collect::<Vec<_>>();
            json!({
                "student_ref": student_ref,
                "question_ids": question_ids,
                "pii_target_count": question_ids.len(),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "students": students,
        "pii_runtime": "local_paddle",
    })
}

fn sorted_keys<T>(map: &HashMap<String, T>) -> Vec<String> {
    let mut keys = map.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    keys
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FailedResumePoint {
    FromStart,
    AfterAlignment,
    AfterCanonicalize,
    AfterPii,
    AfterParse,
}

fn failed_resume_point(submission: &StudentWorkflowSubmission) -> FailedResumePoint {
    if has_resumable_parse_outputs(submission) {
        FailedResumePoint::AfterParse
    } else if has_reparseable_answer_crops(submission) {
        FailedResumePoint::AfterPii
    } else if has_complete_canonicalize_outputs(submission) {
        FailedResumePoint::AfterCanonicalize
    } else if !submission.alignment_pages.is_empty() {
        FailedResumePoint::AfterAlignment
    } else {
        FailedResumePoint::FromStart
    }
}

fn has_complete_canonicalize_outputs(submission: &StudentWorkflowSubmission) -> bool {
    !submission.page_artifacts.is_empty()
        && (submission.alignment_pages.is_empty()
            || submission.page_artifacts.len() == submission.alignment_pages.len())
}

fn has_resumable_parse_outputs(submission: &StudentWorkflowSubmission) -> bool {
    !submission.answers.is_empty()
        && submission.answers.iter().all(|answer| {
            answer.manual_grading_required
                || (answer.verified
                    && !answer.review_required
                    && !answer.stale
                    && !matches!(answer.parse_status.as_str(), "error" | "cancelled"))
        })
}

fn has_reparseable_answer_crops(submission: &StudentWorkflowSubmission) -> bool {
    !submission.answers.is_empty()
        && submission
            .answers
            .iter()
            .any(|answer| answer.stale && !answer.manual_grading_required)
        && submission.answers.iter().all(|answer| {
            answer.manual_grading_required
                || matches!(answer.crop_image_path.as_deref(), Some(path) if !path.is_empty())
        })
}

pub(super) fn run_grading_for_submission(
    state: &Arc<super::AppStateInner>,
    project_path: &Path,
    settings: &AppSettings,
    event_sink: &dyn RuntimeEventSink,
    workspace: &ExamWorkspaceState,
    student_ref: &str,
) -> HostResult<()> {
    let mut workflow_state = project_store::load_student_workflow_state(project_path)?;
    run_grading_pipeline(
        state,
        project_path,
        settings,
        event_sink,
        workspace,
        student_ref,
        &mut workflow_state,
    )?;
    Ok(())
}

fn run_grading_pipeline(
    state: &Arc<super::AppStateInner>,
    project_path: &Path,
    settings: &AppSettings,
    event_sink: &dyn RuntimeEventSink,
    workspace: &ExamWorkspaceState,
    student_ref: &str,
    workflow_state: &mut StudentWorkflowState,
) -> HostResult<()> {
    prepare_existing_submission_stage(
        project_path,
        workflow_state,
        event_sink,
        student_ref,
        "grading",
    )?;
    let question_by_id = workspace
        .questions
        .iter()
        .map(|question| (question.question_id.clone(), question))
        .collect::<HashMap<_, _>>();
    let answer_score_requests = {
        let submission = find_submission_mut(workflow_state, student_ref)?;
        build_preliminary_answer_score_requests(workspace, submission, &question_by_id)?
    };
    if answer_score_requests.is_empty() {
        let submission = find_submission_mut(workflow_state, student_ref)?;
        if submission
            .answers
            .iter()
            .any(|answer| answer.manual_grading_required)
        {
            submission.stage = "manual_grading".into();
            submission.failure_message = None;
            return save_workflow_state_and_emit(
                project_path,
                workflow_state,
                event_sink,
                Some(student_ref),
                WorkerStatus::Ready,
            );
        }
        return mark_failed_and_emit_ready(
            project_path,
            workflow_state,
            event_sink,
            student_ref,
            "No verified answers were available for grading.",
        );
    }

    let grading_persisted_payload =
        persisted_llm_request_payload(settings, json!({ "student_ref": student_ref }));
    let exec = WorkflowExecutionContext {
        state,
        project_path,
        event_sink,
    };
    let grading_stage = GradingStageContext {
        settings,
        student_ref,
        grading_persisted_payload: &grading_persisted_payload,
    };
    let Some(stage_outputs) = run_grading_model_stages(
        exec,
        grading_stage,
        workspace,
        workflow_state,
        &question_by_id,
        &answer_score_requests,
    )?
    else {
        return Ok(());
    };
    finish_grading_submission(
        project_path,
        event_sink,
        student_ref,
        workflow_state,
        stage_outputs.final_rows,
        &stage_outputs.feedback_rows,
        &stage_outputs.highlight_rows,
    )
}

struct GradingStageOutputs {
    final_rows: Vec<Value>,
    feedback_rows: Vec<Value>,
    highlight_rows: Vec<Value>,
}

fn run_grading_model_stages(
    exec: WorkflowExecutionContext<'_>,
    grading_stage: GradingStageContext<'_>,
    workspace: &ExamWorkspaceState,
    workflow_state: &mut StudentWorkflowState,
    question_by_id: &HashMap<String, &QuestionRecord>,
    answer_score_requests: &[Value],
) -> HostResult<Option<GradingStageOutputs>> {
    let Some(preliminary_rows) = run_stage_or_mark_stopped(
        exec.project_path,
        workflow_state,
        exec.event_sink,
        grading_stage.student_ref,
        |state| run_preliminary_grading_stage(exec, grading_stage, state, answer_score_requests),
    )?
    else {
        return Ok(None);
    };
    let final_rows = persist_preliminary_grading_rows(
        exec,
        grading_stage,
        workspace,
        workflow_state,
        question_by_id,
        &preliminary_rows,
    )?;
    let Some(feedback_rows) = run_stage_or_mark_stopped(
        exec.project_path,
        workflow_state,
        exec.event_sink,
        grading_stage.student_ref,
        |state| run_feedback_stage(exec, grading_stage, state, &final_rows),
    )?
    else {
        return Ok(None);
    };
    persist_feedback_rows(
        exec,
        grading_stage.student_ref,
        workflow_state,
        &feedback_rows,
    )?;
    let Some(highlight_rows) = run_stage_or_mark_stopped(
        exec.project_path,
        workflow_state,
        exec.event_sink,
        grading_stage.student_ref,
        |state| run_markup_stage(exec, grading_stage, state, &final_rows),
    )?
    else {
        return Ok(None);
    };
    Ok(Some(GradingStageOutputs {
        final_rows,
        feedback_rows,
        highlight_rows,
    }))
}

fn persist_preliminary_grading_rows(
    exec: WorkflowExecutionContext<'_>,
    grading_stage: GradingStageContext<'_>,
    workspace: &ExamWorkspaceState,
    workflow_state: &mut StudentWorkflowState,
    question_by_id: &HashMap<String, &QuestionRecord>,
    preliminary_rows: &[Value],
) -> HostResult<Vec<Value>> {
    {
        let submission = find_submission_mut(workflow_state, grading_stage.student_ref)?;
        apply_preliminary_confidence_to_answers(submission, preliminary_rows)?;
    }
    let final_rows = {
        let submission = find_submission_mut(workflow_state, grading_stage.student_ref)?;
        final_rows_from_preliminary(workspace, submission, question_by_id, preliminary_rows)?
    };
    {
        let submission = find_submission_mut(workflow_state, grading_stage.student_ref)?;
        apply_final_grading_rows(submission, &final_rows)?;
    }
    save_workflow_state_and_emit(
        exec.project_path,
        workflow_state,
        exec.event_sink,
        Some(grading_stage.student_ref),
        WorkerStatus::Busy,
    )?;
    Ok(final_rows)
}

fn persist_feedback_rows(
    exec: WorkflowExecutionContext<'_>,
    student_ref: &str,
    workflow_state: &mut StudentWorkflowState,
    feedback_rows: &[Value],
) -> HostResult<()> {
    {
        let submission = find_submission_mut(workflow_state, student_ref)?;
        apply_feedback_rows(submission, feedback_rows)?;
    }
    save_workflow_state_and_emit(
        exec.project_path,
        workflow_state,
        exec.event_sink,
        Some(student_ref),
        WorkerStatus::Busy,
    )
}

fn run_stage_or_mark_stopped<F>(
    project_path: &Path,
    workflow_state: &mut StudentWorkflowState,
    event_sink: &dyn RuntimeEventSink,
    student_ref: &str,
    run_stage: F,
) -> HostResult<Option<Vec<Value>>>
where
    F: FnOnce(&mut StudentWorkflowState) -> HostResult<Vec<Value>>,
{
    match run_stage(workflow_state) {
        Ok(rows) => Ok(Some(rows)),
        Err(err) => {
            mark_ref_stopped_for_error(
                project_path,
                workflow_state,
                event_sink,
                student_ref,
                &err,
            )?;
            Ok(None)
        }
    }
}

fn prepare_submission_for_stage(
    project_path: &Path,
    workflow_state: &mut StudentWorkflowState,
    event_sink: &dyn RuntimeEventSink,
    student_ref: &str,
    intake: &crate::models::StudentIntakeSummary,
    stage: &str,
    worker_status: WorkerStatus,
) -> HostResult<()> {
    let submission = find_submission_mut(workflow_state, student_ref)?;
    reset_submission_for_restart(submission, intake);
    submission.stage = stage.into();
    save_workflow_state_and_emit(
        project_path,
        workflow_state,
        event_sink,
        Some(student_ref),
        worker_status,
    )
}

fn prepare_existing_submission_stage(
    project_path: &Path,
    workflow_state: &mut StudentWorkflowState,
    event_sink: &dyn RuntimeEventSink,
    student_ref: &str,
    stage: &str,
) -> HostResult<()> {
    let submission = find_submission_mut(workflow_state, student_ref)?;
    submission.stage = stage.into();
    submission.failure_message = None;
    save_workflow_state_and_emit(
        project_path,
        workflow_state,
        event_sink,
        Some(student_ref),
        WorkerStatus::Busy,
    )
}

fn set_submission_stage(
    workflow_state: &mut StudentWorkflowState,
    student_ref: &str,
    stage: &str,
) -> HostResult<()> {
    let submission = find_submission_mut(workflow_state, student_ref)?;
    submission.stage = stage.into();
    Ok(())
}

fn mark_failed_and_emit_ready(
    project_path: &Path,
    workflow_state: &mut StudentWorkflowState,
    event_sink: &dyn RuntimeEventSink,
    student_ref: &str,
    message: &str,
) -> HostResult<()> {
    mark_submission_failed(workflow_state, student_ref, message)?;
    save_workflow_state_and_emit(
        project_path,
        workflow_state,
        event_sink,
        Some(student_ref),
        WorkerStatus::Ready,
    )
}

fn direct_stage_student_data_error(err: &HostError) -> Option<&str> {
    match err {
        HostError::Protocol(message)
            if matches!(
                message.as_str(),
                "Canonicalize failed for one or more student pages."
                    | "PII screening produced no parseable answer rows."
            ) =>
        {
            Some(message.as_str())
        }
        _ => None,
    }
}

fn mark_ref_for_direct_stage_error(
    project_path: &Path,
    workflow_state: &mut StudentWorkflowState,
    event_sink: &dyn RuntimeEventSink,
    student_ref: &str,
    err: &HostError,
) -> HostResult<()> {
    if let Some(message) = direct_stage_student_data_error(err) {
        mark_failed_and_emit_ready(
            project_path,
            workflow_state,
            event_sink,
            student_ref,
            message,
        )
    } else {
        mark_ref_stopped_for_error(project_path, workflow_state, event_sink, student_ref, err)
    }
}

fn mark_ref_stopped_for_error(
    project_path: &Path,
    workflow_state: &mut StudentWorkflowState,
    event_sink: &dyn RuntimeEventSink,
    student_ref: &str,
    err: &HostError,
) -> HostResult<()> {
    mark_refs_stopped_for_error(
        project_path,
        workflow_state,
        event_sink,
        &[student_ref.to_string()],
        err,
    )
}

fn apply_alignment_results(
    workspace: &ExamWorkspaceState,
    workflow_state: &mut StudentWorkflowState,
    student_ref: &str,
    completed: CompletedWorkerJob,
) -> HostResult<super::AlignmentOutcome> {
    let submission = find_submission_mut(workflow_state, student_ref)?;
    submission.latest_job_id = Some(completed.job_id.clone());
    let alignment_pages =
        apply_alignment_review_metadata(parse_alignment_pages(&completed)?, workspace);
    let has_actionable_low_confidence = alignment_pages
        .iter()
        .any(|page| page.low_confidence && !page.review_exempt);
    submission.alignment_pages = alignment_pages;
    if submission.alignment_pages.is_empty() {
        fail_submission(
            submission,
            "Alignment failed for one or more pages. Review the trace and retry.",
        );
        return Ok(super::AlignmentOutcome::Failed);
    }
    if has_actionable_low_confidence {
        return Ok(super::AlignmentOutcome::NeedsReview);
    }
    Ok(super::AlignmentOutcome::Continue)
}

fn apply_alignment_review_metadata(
    alignment_pages: Vec<StudentWorkflowAlignmentPage>,
    workspace: &ExamWorkspaceState,
) -> Vec<StudentWorkflowAlignmentPage> {
    let question_counts =
        workspace
            .questions
            .iter()
            .fold(HashMap::<i64, i64>::new(), |mut counts, question| {
                *counts.entry(question.page_number).or_insert(0) += 1;
                counts
            });
    alignment_pages
        .into_iter()
        .map(|mut page| {
            page.question_count = question_counts
                .get(&page.page_number)
                .copied()
                .unwrap_or_default();
            if page.question_count == 0 {
                page.review_exempt = true;
                page.review_exempt_reason = Some("no_questions".into());
            } else {
                page.review_exempt = false;
                page.review_exempt_reason = None;
            }
            page
        })
        .collect()
}

fn run_canonicalize_stage(
    exec: WorkflowExecutionContext<'_>,
    workspace: &ExamWorkspaceState,
    intake: &crate::models::StudentIntakeSummary,
    student_ref: &str,
    workflow_state: &mut StudentWorkflowState,
) -> HostResult<()> {
    let canonicalize_targets = {
        let submission = find_submission_mut(workflow_state, student_ref)?;
        build_canonicalize_targets(workspace, intake, submission)?
    };
    let expected_pages = canonicalize_targets.len();
    let completed = run_cli_job(
        exec.state,
        exec.event_sink,
        exec.project_path,
        "scans.canonicalize",
        json!({ "canonicalize_targets": canonicalize_targets }),
        json!({ "student_ref": student_ref }),
    )?;
    let submission = find_submission_mut(workflow_state, student_ref)?;
    submission.latest_job_id = Some(completed.job_id.clone());
    let canonicalize_data = success_data(&completed.result.envelope)?;
    submission.page_artifacts = parse_canonicalized_pages(canonicalize_data)?;
    if submission.page_artifacts.len() != expected_pages {
        return Err(crate::errors::HostError::Protocol(
            "Canonicalize failed for one or more student pages.".into(),
        ));
    }
    submission.stage = "detect".into();
    save_workflow_state_and_emit(
        exec.project_path,
        workflow_state,
        exec.event_sink,
        Some(student_ref),
        WorkerStatus::Busy,
    )
}

fn run_detect_stage(
    exec: WorkflowExecutionContext<'_>,
    workspace: &ExamWorkspaceState,
    student_ref: &str,
    workflow_state: &mut StudentWorkflowState,
) -> HostResult<DetectStageOutcome> {
    let detect_targets = {
        let submission = find_submission_mut(workflow_state, student_ref)?;
        build_detect_targets(workspace, submission)?
    };
    let completed = run_cli_job(
        exec.state,
        exec.event_sink,
        exec.project_path,
        SCANS_DETECT_COMMAND,
        json!({ "detect_targets": detect_targets }),
        json!({ "student_ref": student_ref }),
    )?;
    let detect_data = success_data(&completed.result.envelope)?;
    {
        let submission = find_submission_mut(workflow_state, student_ref)?;
        submission.latest_job_id = Some(completed.job_id.clone());
        apply_detect_page_ocr_results(submission, detect_data)?;
        submission.detect_review = build_detect_review(workspace, submission, detect_data)?;
        if submission.detect_review.is_some() {
            submission.stage = "detect_review".into();
            save_workflow_state_and_emit(
                exec.project_path,
                workflow_state,
                exec.event_sink,
                Some(student_ref),
                WorkerStatus::Ready,
            )?;
            return Ok(DetectStageOutcome::NeedsReview);
        }
    }
    Ok(DetectStageOutcome::CropTargets(build_crop_targets(
        detect_data,
    )?))
}

fn run_crop_stage(
    exec: WorkflowExecutionContext<'_>,
    student_ref: &str,
    workflow_state: &mut StudentWorkflowState,
    crop_targets: Vec<Value>,
) -> HostResult<Vec<Value>> {
    set_submission_stage(workflow_state, student_ref, "crop")?;
    save_workflow_state_and_emit(
        exec.project_path,
        workflow_state,
        exec.event_sink,
        Some(student_ref),
        WorkerStatus::Busy,
    )?;
    let page_artifacts = {
        let submission = find_submission_mut(workflow_state, student_ref)?;
        submission.page_artifacts.clone()
    };
    let completed = run_cli_job(
        exec.state,
        exec.event_sink,
        exec.project_path,
        "scans.crop",
        json!({
            "pages": student_workflow_pages_as_cli(&page_artifacts, student_ref),
            "question_crop_targets": crop_targets_for_cli(&crop_targets),
        }),
        json!({ "student_ref": student_ref }),
    )?;
    let submission = find_submission_mut(workflow_state, student_ref)?;
    submission.latest_job_id = Some(completed.job_id.clone());
    submission.detect_review = None;
    let crop_data = success_data(&completed.result.envelope)?;
    required_array(crop_data, "crop_results").map(|rows| rows.to_vec())
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PiiStageOutcome {
    Continue,
    ManualOnly,
    Stopped,
}

fn set_manual_pii_block_and_emit(
    exec: WorkflowExecutionContext<'_>,
    workflow_state: &mut StudentWorkflowState,
    workspace: &ExamWorkspaceState,
    student_ref: &str,
    crop_rows: &[Value],
) -> HostResult<()> {
    set_manual_pii_block(workspace, workflow_state, student_ref, crop_rows)?;
    save_workflow_state_and_emit(
        exec.project_path,
        workflow_state,
        exec.event_sink,
        Some(student_ref),
        WorkerStatus::Ready,
    )
}

fn run_pii_stage(
    exec: WorkflowExecutionContext<'_>,
    student_ref: &str,
    workflow_state: &mut StudentWorkflowState,
    workspace: &ExamWorkspaceState,
    crop_rows: &[Value],
    settings: &AppSettings,
    pii_trigger_words: Option<&[String]>,
) -> HostResult<PiiStageOutcome> {
    set_submission_stage(workflow_state, student_ref, "pii")?;
    save_workflow_state_and_emit(
        exec.project_path,
        workflow_state,
        exec.event_sink,
        Some(student_ref),
        WorkerStatus::Busy,
    )?;
    if should_manual_block_pii_stage(crop_rows, pii_trigger_words) {
        set_manual_pii_block_and_emit(exec, workflow_state, workspace, student_ref, crop_rows)?;
        return Ok(PiiStageOutcome::ManualOnly);
    }
    let trigger_words = pii_trigger_words.expect("manual block handled empty PII trigger words");
    let pii_model_dir = match resolve_pii_model_dir(exec.state, settings) {
        Ok(path) => path,
        Err(err) => {
            mark_ref_stopped_for_error(
                exec.project_path,
                workflow_state,
                exec.event_sink,
                student_ref,
                &err,
            )?;
            return Ok(PiiStageOutcome::Stopped);
        }
    };
    let Some(completed) = run_pii_cli_job_or_stop(
        exec,
        workflow_state,
        student_ref,
        crop_rows,
        trigger_words,
        &pii_model_dir,
    )?
    else {
        return Ok(PiiStageOutcome::Stopped);
    };
    let answers = match direct_pii_answers_for_completed(workspace, crop_rows, &completed) {
        Ok(answers) => answers,
        Err(err) => {
            mark_ref_stopped_for_error(
                exec.project_path,
                workflow_state,
                exec.event_sink,
                student_ref,
                &err,
            )?;
            return Ok(PiiStageOutcome::Stopped);
        }
    };
    let submission = find_submission_mut(workflow_state, student_ref)?;
    submission.latest_job_id = Some(completed.job_id.clone());
    submission.answers = answers;
    if submission
        .answers
        .iter()
        .all(|answer| answer.manual_grading_required)
    {
        submission.stage = "manual_grading".into();
        submission.failure_message = None;
        save_workflow_state_and_emit(
            exec.project_path,
            workflow_state,
            exec.event_sink,
            Some(student_ref),
            WorkerStatus::Ready,
        )?;
        return Ok(PiiStageOutcome::ManualOnly);
    }
    Ok(PiiStageOutcome::Continue)
}

fn should_manual_block_pii_stage(
    crop_rows: &[Value],
    pii_trigger_words: Option<&[String]>,
) -> bool {
    clean_crop_row_count(crop_rows) == 0
        || pii_trigger_words.is_none_or(|trigger_words| trigger_words.is_empty())
}

fn run_pii_cli_job_or_stop(
    exec: WorkflowExecutionContext<'_>,
    workflow_state: &mut StudentWorkflowState,
    student_ref: &str,
    crop_rows: &[Value],
    trigger_words: &[String],
    pii_model_dir: &Path,
) -> HostResult<Option<CompletedWorkerJob>> {
    match run_cli_job(
        exec.state,
        exec.event_sink,
        exec.project_path,
        "scans.pii",
        json!({
            "students": [pii_student_request(student_ref, trigger_words, crop_rows)],
            "pii_runtime_config": {
                "paddle_model_dir": pii_model_dir.to_worker_payload_path(),
                "max_workers": 2,
            },
        }),
        sanitized_pii_request_payload(student_ref, crop_rows),
    ) {
        Ok(completed) if completed_was_cancelled(&completed) => {
            mark_refs_stopped_and_emit(
                exec.project_path,
                workflow_state,
                exec.event_sink,
                &[student_ref.to_string()],
            )?;
            Ok(None)
        }
        Ok(completed) => Ok(Some(completed)),
        Err(err) => {
            mark_ref_stopped_for_error(
                exec.project_path,
                workflow_state,
                exec.event_sink,
                student_ref,
                &err,
            )?;
            Ok(None)
        }
    }
}

fn direct_pii_answers_for_completed(
    workspace: &ExamWorkspaceState,
    crop_rows: &[Value],
    completed: &CompletedWorkerJob,
) -> HostResult<Vec<StudentWorkflowAnswer>> {
    let pii_data = success_data(&completed.result.envelope)?;
    build_answers_from_pii_results(workspace, crop_rows, pii_data)
}

fn run_parse_stage(
    exec: WorkflowExecutionContext<'_>,
    settings: &AppSettings,
    workspace: &ExamWorkspaceState,
    student_ref: &str,
    workflow_state: &mut StudentWorkflowState,
    crop_rows: &[Value],
) -> HostResult<super::ParseOutcome> {
    set_submission_stage(workflow_state, student_ref, "parse")?;
    save_workflow_state_and_emit(
        exec.project_path,
        workflow_state,
        exec.event_sink,
        Some(student_ref),
        WorkerStatus::Busy,
    )?;
    let parse_targets = {
        let submission = find_submission_mut(workflow_state, student_ref)?;
        build_parse_targets(workspace, submission, crop_rows)?
    };
    if parse_targets.is_empty() {
        return Err(HostError::Protocol(
            "PII screening produced no parseable answer rows.".into(),
        ));
    }
    let completed = run_cli_job(
        exec.state,
        exec.event_sink,
        exec.project_path,
        "scans.parse",
        json!({
            "parse_targets": parse_targets,
            "providers": { "llm_provider": settings.llm_provider },
            "llm_config": llm_config_json(settings),
        }),
        persisted_llm_request_payload(settings, json!({ "student_ref": student_ref })),
    )?;
    let submission = find_submission_mut(workflow_state, student_ref)?;
    submission.latest_job_id = Some(completed.job_id.clone());
    let parse_data = success_data(&completed.result.envelope)?;
    merge_parse_results_into_answers(submission, parse_data)?;
    if submission
        .answers
        .iter()
        .any(|answer| matches!(answer.parse_status.as_str(), "error" | "cancelled"))
    {
        fail_submission(submission, "Parsing failed for one or more answers.");
        return Ok(super::ParseOutcome::Failed);
    }
    if submission
        .answers
        .iter()
        .any(|answer| answer.review_required)
    {
        return Ok(super::ParseOutcome::NeedsReview);
    }
    Ok(super::ParseOutcome::Continue)
}

fn run_preliminary_grading_stage(
    exec: WorkflowExecutionContext<'_>,
    grading: super::GradingStageContext<'_>,
    workflow_state: &mut StudentWorkflowState,
    answer_score_requests: &[Value],
) -> HostResult<Vec<Value>> {
    let completed = run_cli_job(
        exec.state,
        exec.event_sink,
        exec.project_path,
        "grading.score-preliminary",
        preliminary_grading_request_payload(answer_score_requests, grading.settings),
        grading.grading_persisted_payload.clone(),
    )?;
    record_submission_job_id(workflow_state, grading.student_ref, &completed.job_id)?;
    let preliminary_data = success_data(&completed.result.envelope)?;
    required_array(preliminary_data, "preliminary_scores").map(|rows| rows.to_vec())
}

fn run_feedback_stage(
    exec: WorkflowExecutionContext<'_>,
    grading: super::GradingStageContext<'_>,
    workflow_state: &mut StudentWorkflowState,
    final_rows: &[Value],
) -> HostResult<Vec<Value>> {
    let completed = run_cli_job(
        exec.state,
        exec.event_sink,
        exec.project_path,
        "grading.draft-feedback",
        json!({
            "feedback_requests": feedback_request_rows(final_rows),
            "providers": { "llm_provider": grading.settings.llm_provider },
            "llm_config": llm_config_json(grading.settings),
        }),
        grading.grading_persisted_payload.clone(),
    )?;
    record_submission_job_id(workflow_state, grading.student_ref, &completed.job_id)?;
    let feedback_data = success_data(&completed.result.envelope)?;
    required_array(feedback_data, "feedback_drafts").map(|rows| rows.to_vec())
}

fn run_markup_stage(
    exec: WorkflowExecutionContext<'_>,
    grading: super::GradingStageContext<'_>,
    workflow_state: &mut StudentWorkflowState,
    final_rows: &[Value],
) -> HostResult<Vec<Value>> {
    let completed = run_cli_job(
        exec.state,
        exec.event_sink,
        exec.project_path,
        "grading.markup",
        json!({
            "markup_requests": feedback_request_rows(final_rows),
            "providers": { "llm_provider": grading.settings.llm_provider },
            "llm_config": llm_config_json(grading.settings),
        }),
        grading.grading_persisted_payload.clone(),
    )?;
    record_submission_job_id(workflow_state, grading.student_ref, &completed.job_id)?;
    let markup_data = success_data(&completed.result.envelope)?;
    required_array(markup_data, "highlight_results").map(|rows| rows.to_vec())
}

fn finish_grading_submission(
    project_path: &Path,
    event_sink: &dyn RuntimeEventSink,
    student_ref: &str,
    workflow_state: &mut StudentWorkflowState,
    final_rows: Vec<Value>,
    feedback_rows: &[Value],
    highlight_rows: &[Value],
) -> HostResult<()> {
    let submission = find_submission_mut(workflow_state, student_ref)?;
    apply_feedback_and_markup(submission, final_rows, feedback_rows, highlight_rows)?;
    submission.stage = if submission
        .answers
        .iter()
        .any(|answer| answer.manual_grading_required)
    {
        "manual_grading".into()
    } else {
        "graded".into()
    };
    submission.failure_message = None;
    save_workflow_state_and_emit(
        project_path,
        workflow_state,
        event_sink,
        Some(student_ref),
        WorkerStatus::Ready,
    )
}

fn record_submission_job_id(
    workflow_state: &mut StudentWorkflowState,
    student_ref: &str,
    job_id: &str,
) -> HostResult<()> {
    let submission = find_submission_mut(workflow_state, student_ref)?;
    submission.latest_job_id = Some(job_id.to_string());
    Ok(())
}

fn feedback_request_rows(final_rows: &[Value]) -> Vec<Value> {
    final_rows.iter().map(feedback_request_json).collect()
}

fn pii_identity_unavailable_warning() -> WorkspaceWarning {
    WorkspaceWarning {
        code: Some("pii_identity_unavailable".into()),
        message:
            "Student identity context was unavailable for PII screening, so this answer requires manual grading."
                .into(),
        scope: Some("answer".into()),
    }
}

fn sanitized_pii_request_payload(student_ref: &str, crop_rows: &[Value]) -> Value {
    let question_ids = crop_rows
        .iter()
        .filter(|row| matches!(row.get("status").and_then(Value::as_str), Some("ok")))
        .filter_map(|row| row.get("question_id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    let pii_target_count = question_ids.len();
    json!({
        "student_ref": student_ref,
        "question_ids": question_ids,
        "pii_target_count": pii_target_count,
        "pii_runtime": "local_paddle",
    })
}

fn resolve_pii_model_dir(
    state: &Arc<super::AppStateInner>,
    settings: &AppSettings,
) -> HostResult<std::path::PathBuf> {
    resolve_pii_model_dir_candidates([
        std::env::var_os("SCRIPTSCORE_PII_PADDLE_MODEL_DIR").map(std::path::PathBuf::from),
        configured_pii_model_dir(settings),
        bundled_pii_model_dir(state),
        Some(dev_checkout_pii_model_dir()),
    ])
}

fn resolve_pii_model_dir_candidates(
    candidates: [Option<std::path::PathBuf>; 4],
) -> HostResult<std::path::PathBuf> {
    for path in candidates.into_iter().flatten() {
        if is_valid_paddle_model_dir(&path) {
            return Ok(path);
        }
    }
    Err(HostError::Validation(
        "Paddle model directory was not found. Set SCRIPTSCORE_PII_PADDLE_MODEL_DIR, configure the desktop PII model directory, or install bundled/dev paddle models.".into(),
    ))
}

trait WorkerPayloadPath {
    fn to_worker_payload_path(&self) -> String;
}

impl WorkerPayloadPath for Path {
    fn to_worker_payload_path(&self) -> String {
        normalize_worker_payload_path(self)
            .to_string_lossy()
            .into_owned()
    }
}

#[cfg(windows)]
fn normalize_worker_payload_path(path: &Path) -> std::path::PathBuf {
    let path_text = path.to_string_lossy();
    if let Some(rest) = path_text.strip_prefix(r"\\?\UNC\") {
        std::path::PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = path_text.strip_prefix(r"\\?\") {
        std::path::PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

#[cfg(not(windows))]
fn normalize_worker_payload_path(path: &Path) -> std::path::PathBuf {
    path.to_path_buf()
}

fn is_valid_paddle_model_dir(path: &Path) -> bool {
    ["det", "rec"].iter().all(|name| {
        let model_dir = path.join(name);
        model_dir.is_dir()
            && (model_dir.join("inference.json").is_file()
                || model_dir.join("inference.pdmodel").is_file())
            && (!model_dir.join("inference.pdmodel").is_file()
                || model_dir.join("inference.pdiparams").is_file())
    })
}

fn configured_pii_model_dir(settings: &AppSettings) -> Option<std::path::PathBuf> {
    settings
        .pii_paddle_model_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
}

fn bundled_pii_model_dir(state: &Arc<super::AppStateInner>) -> Option<std::path::PathBuf> {
    state
        .bundled_resource_dir()
        .map(|resource_dir| resource_dir.join("models/paddle"))
}

fn dev_checkout_pii_model_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../cli/models/paddle")
        .canonicalize()
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cli/models/paddle"))
}

fn run_cli_job(
    state: &Arc<super::AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    project_path: &Path,
    command_name: &str,
    mut worker_request_payload: Value,
    persisted_request_payload: Value,
) -> HostResult<CompletedWorkerJob> {
    let reserved = start_runtime_job(state, event_sink, command_name)?;
    let output_artifacts_dir =
        project_store::command_output_dir(project_path, command_name, &reserved.job_id);
    fs::create_dir_all(&output_artifacts_dir)?;
    if let Some(payload) = worker_request_payload.as_object_mut() {
        payload.insert(
            "output_artifacts_dir".into(),
            Value::String(output_artifacts_dir.to_string_lossy().into_owned()),
        );
    }
    run_reserved_job(
        state,
        event_sink,
        reserved,
        RuntimeJobRequest {
            command_name,
            worker_request_payload,
            persisted_request_payload,
            output_artifacts_dir: Some(&output_artifacts_dir),
            project_path: Some(project_path),
            stdin_bytes: None,
        },
    )
}

fn reset_submission_for_restart(
    submission: &mut StudentWorkflowSubmission,
    intake: &crate::models::StudentIntakeSummary,
) {
    submission.canonical_pdf_path = intake.canonical_pdf_path.clone();
    submission.page_count = intake.page_count;
    submission.stage = "intake_ready".into();
    submission.failure_message = None;
    submission.warnings.clear();
    submission.page_artifacts.clear();
    submission.alignment_pages.clear();
    submission.detect_review = None;
    submission.answers.clear();
}

pub(super) fn ensure_submission_row(
    workflow_state: &mut StudentWorkflowState,
    intake: &crate::models::StudentIntakeSummary,
) {
    if workflow_state
        .submissions
        .iter()
        .any(|submission| submission.student_ref == intake.student_ref)
    {
        return;
    }
    workflow_state.submissions.push(StudentWorkflowSubmission {
        student_ref: intake.student_ref.clone(),
        canonical_pdf_path: intake.canonical_pdf_path.clone(),
        page_count: intake.page_count,
        stage: "intake_ready".into(),
        latest_job_id: None,
        failure_message: None,
        warnings: intake.warnings.clone(),
        page_artifacts: Vec::new(),
        alignment_pages: Vec::new(),
        detect_review: None,
        answers: Vec::new(),
    });
}

pub(super) fn intake_map(
    workspace: &ExamWorkspaceState,
) -> HostResult<HashMap<String, crate::models::StudentIntakeSummary>> {
    Ok(workspace
        .student_intake
        .items
        .iter()
        .filter(|item| {
            !item.canonical_pdf_path.trim().is_empty() && !item.exam_page_paths.is_empty()
        })
        .map(|item| (item.student_ref.clone(), item.clone()))
        .collect())
}

pub(super) fn save_workflow_state(
    project_path: &Path,
    workflow_state: &mut StudentWorkflowState,
) -> HostResult<()> {
    update_workflow_status(workflow_state);
    *workflow_state = project_store::save_student_workflow_state(project_path, workflow_state)?;
    // Report JPEG assets depend only on persisted crop PNGs, so warm them in the background
    // after workflow-state saves rather than on interactive Results/LMS preview.
    results_lms_report::spawn_report_asset_prewarm(
        project_path.to_path_buf(),
        workflow_state.clone(),
    );
    Ok(())
}

fn save_workflow_state_for_refs(
    project_path: &Path,
    workflow_state: &mut StudentWorkflowState,
    student_refs: &[String],
) -> HostResult<()> {
    update_workflow_status(workflow_state);
    let submissions = student_refs
        .iter()
        .map(|student_ref| {
            workflow_state
                .submissions
                .iter()
                .find(|submission| submission.student_ref == *student_ref)
                .cloned()
                .ok_or_else(|| {
                    HostError::Validation(format!(
                        "Student workflow state was missing '{student_ref}'."
                    ))
                })
        })
        .collect::<HostResult<Vec<_>>>()?;
    *workflow_state = project_store::save_student_workflow_submissions(project_path, &submissions)?;
    results_lms_report::spawn_report_asset_prewarm(
        project_path.to_path_buf(),
        workflow_state.clone(),
    );
    Ok(())
}

fn update_workflow_status(workflow_state: &mut StudentWorkflowState) {
    workflow_state.status = if workflow_state.submissions.is_empty() {
        "not_started".into()
    } else if workflow_state.submissions.iter().any(|submission| {
        matches!(
            submission.stage.as_str(),
            "alignment_review" | "detect_review" | "parse_review" | "manual_grading" | "failed"
        )
    }) {
        "attention".into()
    } else if workflow_state
        .submissions
        .iter()
        .all(|submission| submission.stage == "graded")
    {
        "graded".into()
    } else if workflow_state
        .submissions
        .iter()
        .any(|submission| !matches!(submission.stage.as_str(), "intake_ready" | "stopped"))
    {
        "running".into()
    } else {
        "ready".into()
    };
}

pub(super) fn save_workflow_state_and_emit(
    project_path: &Path,
    workflow_state: &mut StudentWorkflowState,
    event_sink: &dyn RuntimeEventSink,
    student_ref: Option<&str>,
    worker_status: WorkerStatus,
) -> HostResult<()> {
    if let Some(student_ref) = student_ref {
        save_workflow_state_for_refs(project_path, workflow_state, &[student_ref.to_string()])?;
    } else {
        save_workflow_state(project_path, workflow_state)?;
    }
    let stage = student_ref.and_then(|target| {
        workflow_state
            .submissions
            .iter()
            .find(|submission| submission.student_ref == target)
            .map(|submission| submission.stage.clone())
    });
    event_sink.emit_runtime_event(crate::models::RuntimeJobEvent {
        event_type: "workflow_state_updated".into(),
        command_name: "student.workflow".into(),
        worker_status,
        request_id: None,
        job_id: None,
        payload: json!({
            "workflowStateUpdated": true,
            "studentRef": student_ref,
            "stage": stage,
            "workflowStatus": workflow_state.status,
        }),
    });
    Ok(())
}

fn save_workflow_state_for_refs_and_emit(
    project_path: &Path,
    workflow_state: &mut StudentWorkflowState,
    event_sink: &dyn RuntimeEventSink,
    student_refs: &[String],
    worker_status: WorkerStatus,
) -> HostResult<()> {
    save_workflow_state_for_refs(project_path, workflow_state, student_refs)?;
    emit_workflow_state_updated(
        event_sink,
        workflow_state,
        None,
        student_refs,
        worker_status,
    );
    Ok(())
}

fn emit_workflow_state_updated(
    event_sink: &dyn RuntimeEventSink,
    workflow_state: &StudentWorkflowState,
    student_ref: Option<&str>,
    student_refs: &[String],
    worker_status: WorkerStatus,
) {
    event_sink.emit_runtime_event(crate::models::RuntimeJobEvent {
        event_type: "workflow_state_updated".into(),
        command_name: "student.workflow".into(),
        worker_status,
        request_id: None,
        job_id: None,
        payload: json!({
            "workflowStateUpdated": true,
            "studentRef": student_ref,
            "studentRefs": student_refs,
            "workflowStatus": workflow_state.status,
        }),
    });
}

#[cfg(test)]
#[path = "student_workflow_pipeline_tests.rs"]
mod tests;
