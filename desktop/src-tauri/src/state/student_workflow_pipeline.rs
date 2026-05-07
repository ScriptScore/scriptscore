// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::errors::{HostError, HostResult};
use crate::models::{
    AppSettings, ExamWorkspaceState, QuestionRecord, StudentWorkflowAlignmentPage,
    StudentWorkflowState, StudentWorkflowSubmission, WorkerJobResult, WorkerStatus,
    WorkspaceWarning,
};
use crate::project_store;
use crate::state::results_lms_report;
use crate::worker::CompletedWorkerJob;

use super::results::{
    apply_detect_page_ocr_results, apply_feedback_and_markup, apply_feedback_rows,
    apply_final_grading_rows, apply_preliminary_confidence_to_answers,
    build_answers_for_manual_pii_block, build_answers_from_pii_results, build_canonicalize_targets,
    build_crop_targets, build_detect_review, build_detect_targets, build_parse_targets,
    build_preliminary_answer_score_requests, crop_targets_from_detect_review,
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
        return mark_failed_and_emit_ready(
            project_path,
            workflow_state,
            event_sink,
            student_ref,
            &err.to_string(),
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
            return mark_failed_and_emit_ready(
                project_path,
                workflow_state,
                event_sink,
                student_ref,
                &err.to_string(),
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
            return mark_failed_and_emit_ready(
                project_path,
                workflow_state,
                event_sink,
                student_ref,
                &err.to_string(),
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
            return mark_failed_and_emit_ready(
                project_path,
                workflow_state,
                event_sink,
                student_ref,
                &err.to_string(),
            )
        }
    };
    if pii_outcome == PiiStageOutcome::ManualOnly {
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
        Err(err) => mark_failed_and_emit_ready(
            project_path,
            workflow_state,
            event_sink,
            student_ref,
            &err.to_string(),
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
            return mark_failed_and_emit_ready(
                project_path,
                workflow_state,
                event_sink,
                student_ref,
                &err.to_string(),
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
            return mark_failed_and_emit_ready(
                project_path,
                workflow_state,
                event_sink,
                student_ref,
                &err.to_string(),
            )
        }
    };
    if pii_outcome == PiiStageOutcome::ManualOnly {
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
        Err(err) => mark_failed_and_emit_ready(
            project_path,
            workflow_state,
            event_sink,
            student_ref,
            &err.to_string(),
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
        after_parse: Vec::new(),
    };
    for student_ref in eligible_refs {
        let submission = find_submission_mut(workflow_state, &student_ref)?;
        match failed_resume_point(submission) {
            FailedResumePoint::FromStart => plan.from_start.push(student_ref),
            FailedResumePoint::AfterAlignment => plan.after_alignment.push(student_ref),
            FailedResumePoint::AfterCanonicalize => plan.after_canonicalize.push(student_ref),
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
    let completed = match run_cli_job(
        exec.state,
        exec.event_sink,
        exec.project_path,
        "scans.align-auto",
        json!({
            "marker_mode": "prefer_aruco",
            "mode": "fast",
            "template_pages": template_pages_as_cli(workspace),
            "student_pages": student_pages,
            "providers": { "alignment_engine": "core_template_match" },
        }),
        json!({ "student_refs": student_refs }),
    ) {
        Ok(completed) if completed_was_cancelled(&completed) => {
            mark_refs_stopped_and_emit(project_path, workflow_state, event_sink, student_refs)?;
            return Ok(Vec::new());
        }
        Ok(completed) => completed,
        Err(err) => {
            mark_refs_failed_and_emit(
                project_path,
                workflow_state,
                event_sink,
                student_refs,
                &err,
            )?;
            return Ok(Vec::new());
        }
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
    let targets =
        build_canonicalize_batch_targets(workspace, intake_by_ref, workflow_state, student_refs)?;
    let completed = match run_cli_job(
        exec.state,
        exec.event_sink,
        exec.project_path,
        "scans.canonicalize",
        json!({ "canonicalize_targets": targets }),
        json!({ "student_refs": student_refs }),
    ) {
        Ok(completed) if completed_was_cancelled(&completed) => {
            mark_refs_stopped_and_emit(project_path, workflow_state, event_sink, student_refs)?;
            return Ok(Vec::new());
        }
        Ok(completed) => completed,
        Err(err) => {
            mark_refs_failed_and_emit(
                project_path,
                workflow_state,
                event_sink,
                student_refs,
                &err,
            )?;
            return Ok(Vec::new());
        }
    };
    apply_canonicalize_batch_results(
        project_path,
        event_sink,
        workflow_state,
        student_refs,
        &completed,
    )
}

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
    let completed = match run_cli_job(
        exec.state,
        exec.event_sink,
        exec.project_path,
        "scans.detect",
        json!({ "detect_targets": targets }),
        json!({ "student_refs": detect_refs }),
    ) {
        Ok(completed) if completed_was_cancelled(&completed) => {
            mark_refs_stopped_and_emit(project_path, workflow_state, event_sink, &detect_refs)?;
            return Ok(crop_targets_by_ref);
        }
        Ok(completed) => completed,
        Err(err) => {
            mark_refs_failed_and_emit(
                project_path,
                workflow_state,
                event_sink,
                &detect_refs,
                &err,
            )?;
            return Ok(crop_targets_by_ref);
        }
    };
    if let Some(err) = detect_batch_command_error(&completed) {
        mark_refs_failed_and_emit(project_path, workflow_state, event_sink, &detect_refs, &err)?;
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
        let filtered = filtered_completed_by_student(
            completed,
            &["detect_results", "page_ocr_results"],
            student_ref,
        )?;
        let detect_data = success_data(&filtered.result.envelope)?;
        let submission = find_submission_mut(workflow_state, student_ref)?;
        submission.latest_job_id = Some(completed.job_id.clone());
        apply_detect_page_ocr_results(submission, detect_data)?;
        submission.detect_review = build_detect_review(workspace, submission, detect_data)?;
        if submission.detect_review.is_some() {
            submission.stage = "detect_review".into();
            continue;
        }
        let crop_targets = build_crop_targets(detect_data)?;
        if crop_targets.is_empty() {
            fail_submission(submission, "No question regions were detected for crop.");
            continue;
        }
        submission.detect_review = None;
        submission.stage = "crop".into();
        crop_targets_by_ref.insert(student_ref.clone(), crop_targets);
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
        let completed = match run_cli_job(
            exec.state,
            exec.event_sink,
            exec.project_path,
            "scans.crop",
            json!({
                "pages": student_workflow_pages_as_cli(&page_artifacts, student_ref),
                "question_crop_targets": targets,
            }),
            json!({ "student_ref": student_ref }),
        ) {
            Ok(completed) if completed_was_cancelled(&completed) => {
                mark_refs_stopped_and_emit(
                    project_path,
                    workflow_state,
                    event_sink,
                    std::slice::from_ref(student_ref),
                )?;
                continue;
            }
            Ok(completed) => completed,
            Err(err) => {
                mark_refs_failed_and_emit(
                    project_path,
                    workflow_state,
                    event_sink,
                    std::slice::from_ref(student_ref),
                    &err,
                )?;
                continue;
            }
        };
        let crop_data = success_data(&completed.result.envelope)?;
        let rows = required_array(crop_data, "crop_results")?.to_vec();
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
    let completed = match run_cli_job(
        exec.state,
        exec.event_sink,
        exec.project_path,
        "scans.pii",
        json!({
            "students": pii_students,
            "pii_runtime_config": {
                "paddle_model_dir": resolve_pii_model_dir(exec.state, settings)?
                    .to_string_lossy()
                    .into_owned(),
                "max_workers": 2,
            },
        }),
        sanitized_pii_batch_request_payload(&crop_rows_by_ref),
    ) {
        Ok(completed) if completed_was_cancelled(&completed) => {
            mark_refs_stopped_and_emit(
                exec.project_path,
                workflow_state,
                exec.event_sink,
                &pii_student_refs,
            )?;
            return Ok(Vec::new());
        }
        Ok(completed) => completed,
        Err(err) => {
            mark_refs_failed_and_emit(
                exec.project_path,
                workflow_state,
                exec.event_sink,
                &pii_student_refs,
                &err,
            )?;
            return Ok(Vec::new());
        }
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
        let clean_crop_rows = crop_rows
            .iter()
            .filter(|row| matches!(row.get("status").and_then(Value::as_str), Some("ok")))
            .count();
        let trigger_words = pii_trigger_words_by_student_ref.get(&student_ref);
        if clean_crop_rows == 0 || trigger_words.map(Vec::is_empty).unwrap_or(true) {
            set_manual_pii_block(workspace, workflow_state, &student_ref, crop_rows)?;
            continue;
        }
        students.push(json!({
            "student_ref": student_ref,
            "pii_trigger_words": trigger_words.expect("checked above"),
            "pii_targets": crop_rows.iter()
                .filter(|row| matches!(row.get("status").and_then(Value::as_str), Some("ok")))
                .map(|row| {
                    json!({
                        "question_id": row.get("question_id").cloned().unwrap_or_default(),
                        "question_crop_path": row.get("question_crop_path").cloned().unwrap_or_default(),
                    })
                })
                .collect::<Vec<_>>(),
        }));
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
        let filtered = filtered_completed_by_student(completed, &["pii_results"], &student_ref)?;
        let pii_data = success_data(&filtered.result.envelope)?;
        if required_array(pii_data, "pii_results")?.is_empty() {
            continue;
        }
        let crop_rows = crop_rows_by_ref
            .get(&student_ref)
            .expect("key came from map");
        let submission = find_submission_mut(workflow_state, &student_ref)?;
        submission.latest_job_id = Some(completed.job_id.clone());
        submission.answers = build_answers_from_pii_results(workspace, crop_rows, pii_data)?;
        if submission
            .answers
            .iter()
            .all(|answer| answer.manual_grading_required)
        {
            submission.stage = "manual_grading".into();
            submission.failure_message = None;
        } else {
            submission.stage = "parse".into();
            parse_refs.push(student_ref);
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
    let completed = match run_cli_job(
        exec.state,
        exec.event_sink,
        exec.project_path,
        "scans.parse",
        json!({
            "parse_targets": parse_targets,
            "providers": { "llm_provider": settings.llm_provider },
            "llm_config": llm_config_json(settings),
        }),
        persisted_llm_request_payload(settings, json!({ "student_refs": student_refs })),
    ) {
        Ok(completed) if completed_was_cancelled(&completed) => {
            mark_refs_stopped_and_emit(project_path, workflow_state, event_sink, student_refs)?;
            return Ok(Vec::new());
        }
        Ok(completed) => completed,
        Err(err) => {
            mark_refs_failed_and_emit(
                project_path,
                workflow_state,
                event_sink,
                student_refs,
                &err,
            )?;
            return Ok(Vec::new());
        }
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
        let filtered = filtered_completed_by_student(completed, &["parse_results"], student_ref)?;
        let parse_data = success_data(&filtered.result.envelope)?;
        let submission = find_submission_mut(workflow_state, student_ref)?;
        submission.latest_job_id = Some(completed.job_id.clone());
        merge_parse_results_into_answers(submission, parse_data)?;
        if submission
            .answers
            .iter()
            .any(|answer| matches!(answer.parse_status.as_str(), "error" | "cancelled"))
        {
            fail_submission(submission, "Parsing failed for one or more answers.");
        } else if submission
            .answers
            .iter()
            .any(|answer| answer.review_required)
        {
            submission.stage = "parse_review".into();
        } else {
            submission.stage = "grading".into();
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
    let completed = match run_cli_job(
        exec.state,
        exec.event_sink,
        exec.project_path,
        command_name,
        request_payload,
        persisted_payload,
    ) {
        Ok(completed) if completed_was_cancelled(&completed) => {
            mark_refs_stopped_and_emit(
                exec.project_path,
                workflow_state,
                exec.event_sink,
                student_refs,
            )?;
            return Ok(Vec::new());
        }
        Ok(completed) => completed,
        Err(err) => {
            mark_refs_failed_and_emit(
                exec.project_path,
                workflow_state,
                exec.event_sink,
                student_refs,
                &err,
            )?;
            return Ok(Vec::new());
        }
    };
    for student_ref in student_refs {
        record_submission_job_id(workflow_state, student_ref, &completed.job_id)?;
    }
    let data = success_data(&completed.result.envelope)?;
    required_array(data, rows_key).map(|rows| rows.to_vec())
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

fn mark_refs_failed_and_emit(
    project_path: &Path,
    workflow_state: &mut StudentWorkflowState,
    event_sink: &dyn RuntimeEventSink,
    student_refs: &[String],
    err: &HostError,
) -> HostResult<()> {
    mark_refs_failed_with_message(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        &err.to_string(),
    )
}

fn completed_was_cancelled(completed: &CompletedWorkerJob) -> bool {
    completed.result.terminal_type == "job_cancelled"
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
    for student_ref in student_refs {
        let submission = find_submission_mut(workflow_state, student_ref)?;
        submission.stage = "stopped".into();
        submission.failure_message = Some("Workflow stopped before this stage completed.".into());
    }
    save_workflow_state_for_refs_and_emit(
        project_path,
        workflow_state,
        event_sink,
        student_refs,
        WorkerStatus::Ready,
    )
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
    AfterParse,
}

fn failed_resume_point(submission: &StudentWorkflowSubmission) -> FailedResumePoint {
    if has_resumable_parse_outputs(submission) {
        FailedResumePoint::AfterParse
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
    let Some(preliminary_rows) = run_stage_or_mark_failed(
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
    let Some(feedback_rows) = run_stage_or_mark_failed(
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
    let Some(highlight_rows) = run_stage_or_mark_failed(
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

fn run_stage_or_mark_failed<F>(
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
            mark_failed_and_emit_ready(
                project_path,
                workflow_state,
                event_sink,
                student_ref,
                &err.to_string(),
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
        "scans.detect",
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
    let clean_crop_rows = crop_rows
        .iter()
        .filter(|row| matches!(row.get("status").and_then(Value::as_str), Some("ok")))
        .count();
    if clean_crop_rows == 0 {
        let submission = find_submission_mut(workflow_state, student_ref)?;
        submission.answers = build_answers_for_manual_pii_block(
            workspace,
            crop_rows,
            pii_identity_unavailable_warning(),
        )?;
        submission.stage = "manual_grading".into();
        submission.failure_message = None;
        submission.latest_job_id = None;
        save_workflow_state_and_emit(
            exec.project_path,
            workflow_state,
            exec.event_sink,
            Some(student_ref),
            WorkerStatus::Ready,
        )?;
        return Ok(PiiStageOutcome::ManualOnly);
    }
    let Some(trigger_words) = pii_trigger_words.filter(|items| !items.is_empty()) else {
        let submission = find_submission_mut(workflow_state, student_ref)?;
        submission.answers = build_answers_for_manual_pii_block(
            workspace,
            crop_rows,
            pii_identity_unavailable_warning(),
        )?;
        submission.stage = "manual_grading".into();
        submission.failure_message = None;
        submission.latest_job_id = None;
        save_workflow_state_and_emit(
            exec.project_path,
            workflow_state,
            exec.event_sink,
            Some(student_ref),
            WorkerStatus::Ready,
        )?;
        return Ok(PiiStageOutcome::ManualOnly);
    };
    let completed = run_cli_job(
        exec.state,
        exec.event_sink,
        exec.project_path,
        "scans.pii",
        json!({
            "students": [{
                "student_ref": student_ref,
                "pii_trigger_words": trigger_words,
                "pii_targets": crop_rows.iter()
                    .filter(|row| matches!(row.get("status").and_then(Value::as_str), Some("ok")))
                    .map(|row| {
                        json!({
                            "question_id": row.get("question_id").cloned().unwrap_or_default(),
                            "question_crop_path": row.get("question_crop_path").cloned().unwrap_or_default(),
                        })
                    })
                    .collect::<Vec<_>>(),
            }],
            "pii_runtime_config": {
                "paddle_model_dir": resolve_pii_model_dir(exec.state, settings)?
                    .to_string_lossy()
                    .into_owned(),
                "max_workers": 2,
            },
        }),
        sanitized_pii_request_payload(student_ref, crop_rows),
    )?;
    let submission = find_submission_mut(workflow_state, student_ref)?;
    submission.latest_job_id = Some(completed.job_id.clone());
    let pii_data = success_data(&completed.result.envelope)?;
    submission.answers = build_answers_from_pii_results(workspace, crop_rows, pii_data)?;
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
        if path.is_dir() {
            return Ok(path);
        }
    }
    Err(HostError::Validation(
        "Paddle model directory was not found. Set SCRIPTSCORE_PII_PADDLE_MODEL_DIR, configure the desktop PII model directory, or install bundled/dev paddle models.".into(),
    ))
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
mod tests {
    use super::{
        apply_alignment_results, apply_canonicalize_batch_results, apply_detect_batch_results,
        apply_parse_batch_results, apply_pii_batch_results, build_batch_answer_score_requests,
        build_canonicalize_batch_targets, build_crop_targets, build_detect_review,
        classify_batch_resume_points, completed_was_cancelled, crop_rows_from_answers,
        crop_targets_for_cli, detect_batch_command_error, emit_ready_for_empty_batch_continuation,
        ensure_submission_row, failed_resume_point, feedback_request_rows,
        filtered_completed_by_student, finish_batch_grading, intake_map,
        mark_refs_failed_with_message, mark_refs_stopped_and_emit, persist_batch_feedback_rows,
        persist_batch_preliminary_rows, pii_identity_unavailable_warning,
        preliminary_grading_request_payload, preliminary_grading_runtime_config,
        prepare_pii_batch_inputs, record_submission_job_id, reset_submission_for_restart,
        resolve_pii_model_dir_candidates, rows_for_student, sanitized_pii_batch_request_payload,
        sanitized_pii_request_payload, save_workflow_state, save_workflow_state_and_emit,
        settle_empty_grading_refs, sorted_keys, split_detect_refs_by_reusable_crop_targets,
        FailedResumePoint,
    };
    use crate::models::{
        AppSettings, ExamWorkspaceState, InstructorProfile, ProjectConfig, ProjectSummary,
        QuestionRecord, RuntimeJobEvent, StudentIntakeState, StudentIntakeSummary,
        StudentWorkflowAlignmentPage, StudentWorkflowAnswer, StudentWorkflowDetectRegion,
        StudentWorkflowDetectReview, StudentWorkflowDetectReviewRow, StudentWorkflowPage,
        StudentWorkflowState, StudentWorkflowSubmission, StudentWorkflowTransform,
        TemplatePageArtifactSummary, TemplateQuestionRegion, WorkerJobResult, WorkerStatus,
        WorkspaceWarning,
    };
    use crate::project_store;
    use crate::test_support::{lock_env_vars, EnvVarGuard};
    use crate::worker::CompletedWorkerJob;
    use image::{ImageBuffer, Rgba};
    use serde_json::json;
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[derive(Clone, Default)]
    struct RecordingEventSink {
        events: Arc<Mutex<Vec<RuntimeJobEvent>>>,
    }

    impl RecordingEventSink {
        fn snapshot(&self) -> Vec<RuntimeJobEvent> {
            self.events.lock().expect("event sink lock").clone()
        }
    }

    impl crate::state::RuntimeEventSink for RecordingEventSink {
        fn emit_runtime_event(&self, event: RuntimeJobEvent) {
            self.events.lock().expect("event sink lock").push(event);
        }
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

    fn create_project(test_root: &Path) -> PathBuf {
        let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", test_root);
        let created = project_store::create_project(
            "Workflow Pipeline Test",
            None,
            None,
            None,
            &InstructorProfile::default(),
        )
        .expect("project should be created");
        PathBuf::from(created.project_path)
    }

    #[test]
    fn preliminary_grading_runtime_config_clamps_worker_count() {
        let mut settings = AppSettings {
            preliminary_grading_max_workers: 0,
            ..Default::default()
        };
        assert_eq!(
            preliminary_grading_runtime_config(&settings)["max_workers"],
            json!(1)
        );

        settings.preliminary_grading_max_workers = 3;
        assert_eq!(
            preliminary_grading_runtime_config(&settings)["max_workers"],
            json!(3)
        );

        settings.preliminary_grading_max_workers = 5;
        assert_eq!(
            preliminary_grading_runtime_config(&settings)["max_workers"],
            json!(4)
        );
    }

    #[test]
    fn preliminary_grading_request_payload_includes_runtime_config() {
        let settings = AppSettings {
            llm_provider: "ollama_native".into(),
            llm_base_url: "http://127.0.0.1:11434".into(),
            llm_model: "qwen3.5:9b".into(),
            preliminary_grading_max_workers: 2,
            ..AppSettings::default()
        };
        let answer_rows = vec![json!({
            "student_ref": "student_1",
            "question_id": "q1",
        })];

        let payload = preliminary_grading_request_payload(&answer_rows, &settings);

        assert_eq!(payload["answer_score_requests"], json!(answer_rows));
        assert_eq!(payload["grading_runtime_config"]["max_workers"], json!(2));
        assert_eq!(payload["providers"]["llm_provider"], json!("ollama_native"));
        assert_eq!(payload["llm_config"]["model"], json!("qwen3.5:9b"));
    }

    fn intake_summary(student_ref: &str) -> StudentIntakeSummary {
        StudentIntakeSummary {
            student_ref: student_ref.into(),
            local_display_name: None,
            canonical_pdf_path: format!("/tmp/{student_ref}.pdf"),
            ingest_status: "ready".into(),
            page_count: 2,
            exam_page_paths: vec!["/tmp/1.png".into(), "/tmp/2.png".into()],
            warnings: Vec::new(),
            binding_token_hex: None,
        }
    }

    fn workflow_submission(student_ref: &str) -> StudentWorkflowSubmission {
        StudentWorkflowSubmission {
            student_ref: student_ref.into(),
            canonical_pdf_path: format!("/tmp/{student_ref}.pdf"),
            page_count: 2,
            stage: "intake_ready".into(),
            latest_job_id: None,
            failure_message: None,
            warnings: Vec::new(),
            page_artifacts: Vec::new(),
            alignment_pages: Vec::new(),
            detect_review: None,
            answers: Vec::new(),
        }
    }

    fn alignment_page(page_number: i64) -> StudentWorkflowAlignmentPage {
        StudentWorkflowAlignmentPage {
            page_number,
            confidence: Some(0.94),
            low_confidence: false,
            review_exempt: false,
            review_exempt_reason: None,
            question_count: 1,
            transform: StudentWorkflowTransform {
                rotation: 1.5,
                scale: 0.98,
                translate_x: 2.0,
                translate_y: -3.0,
            },
            warnings: Vec::new(),
        }
    }

    fn workflow_page(student_ref: &str, page_number: i64) -> StudentWorkflowPage {
        StudentWorkflowPage {
            page_number,
            image_path: format!("/tmp/{student_ref}-page-{page_number}.png"),
            source_pdf_path: Some(format!("/tmp/{student_ref}.pdf")),
            ocr_metadata_path: None,
        }
    }

    fn minimal_workspace_with_question() -> ExamWorkspaceState {
        ExamWorkspaceState {
            project: ProjectSummary {
                project_id: "proj_1".into(),
                display_name: "Workflow".into(),
                subject: None,
                course_code: None,
                lms_course_id: None,
                project_path: "/tmp/project".into(),
                created_at: "0".into(),
                updated_at: "0".into(),
            },
            status: "ready".into(),
            status_label: "Ready".into(),
            failure_message: None,
            template_preview_artifacts: Vec::new(),
            aruco_status: Default::default(),
            questions: vec![QuestionRecord {
                question_id: "q1".into(),
                question_number: 1,
                page_number: 1,
                max_points: None,
                text: "Question 1".into(),
                baseline_pdf_text: "Question 1".into(),
                region: Some(TemplateQuestionRegion {
                    x: 5,
                    y: 10,
                    width: 90,
                    height: 120,
                }),
                source_artifact_id: None,
                image_path: None,
                analysis: Default::default(),
                rubric: Default::default(),
            }],
            redaction_regions: Vec::new(),
            warnings: Vec::new(),
            can_approve: false,
            can_approve_rubric: false,
            project_config: ProjectConfig::default(),
            student_roster: Vec::new(),
            student_intake: StudentIntakeState::not_started(),
            student_workflow: StudentWorkflowState::not_started(),
            moderation_state: Default::default(),
            results_lms_state: Default::default(),
            results_lms_rows: Vec::new(),
            results_lms_metrics: None,
            results_lms_review_summary: None,
            workflow_stage: "ready".into(),
            workflow_label: "Ready".into(),
        }
    }

    fn minimal_workspace_with_template_pages() -> ExamWorkspaceState {
        let mut workspace = minimal_workspace_with_question();
        workspace.template_preview_artifacts = vec![
            TemplatePageArtifactSummary {
                artifact_id: "template-page-1".into(),
                page_number: 1,
                image_path: "/tmp/template-1.png".into(),
                label: "Page 1".into(),
            },
            TemplatePageArtifactSummary {
                artifact_id: "template-page-2".into(),
                page_number: 2,
                image_path: "/tmp/template-2.png".into(),
                label: "Page 2".into(),
            },
        ];
        workspace
    }

    fn write_sample_png(path: &Path, width: u32, height: u32) {
        let image =
            ImageBuffer::from_fn(width, height, |_x, _y| Rgba([32_u8, 64_u8, 96_u8, 255_u8]));
        image.save(path).expect("sample png should save");
    }

    #[test]
    fn failed_resume_point_prefers_canonicalized_pages_then_alignment_then_restart() {
        let mut submission = workflow_submission("student_1");
        assert_eq!(
            failed_resume_point(&submission),
            FailedResumePoint::FromStart
        );

        submission
            .alignment_pages
            .push(StudentWorkflowAlignmentPage {
                page_number: 1,
                confidence: Some(0.9),
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
            });
        assert_eq!(
            failed_resume_point(&submission),
            FailedResumePoint::AfterAlignment
        );

        let mut partial_canonicalize = submission.clone();
        partial_canonicalize
            .alignment_pages
            .push(StudentWorkflowAlignmentPage {
                page_number: 2,
                confidence: Some(0.9),
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
            });
        partial_canonicalize
            .page_artifacts
            .push(StudentWorkflowPage {
                page_number: 1,
                image_path: "/tmp/partial-canonical.png".into(),
                source_pdf_path: Some("/tmp/student.pdf".into()),
                ocr_metadata_path: None,
            });
        assert_eq!(
            failed_resume_point(&partial_canonicalize),
            FailedResumePoint::AfterAlignment
        );

        submission.page_artifacts.push(StudentWorkflowPage {
            page_number: 1,
            image_path: "/tmp/canonical.png".into(),
            source_pdf_path: Some("/tmp/student.pdf".into()),
            ocr_metadata_path: Some("/tmp/ocr.json".into()),
        });
        assert_eq!(
            failed_resume_point(&submission),
            FailedResumePoint::AfterCanonicalize
        );

        submission.answers.push(StudentWorkflowAnswer {
            question_id: "q1".into(),
            question_number: 1,
            crop_image_path: Some("/tmp/q1.png".into()),
            pii_prescreen: None,
            manual_grading_required: false,
            manual_grading_reason: None,
            moderation_eligible: false,
            parse_status: "ok".into(),
            parse_confidence: Some("high".into()),
            parse_confidence_source: Some("combined".into()),
            raw_parsed_text: Some("answer".into()),
            verified_text: Some("answer".into()),
            review_required: false,
            verified: true,
            stale: false,
            grading_status: "draft_ready".into(),
            grading_confidence: None,
            grading_confidence_reason: None,
            question_max_points: Some(5),
            total_points_awarded: Some(5),
            feedback_text: None,
            criterion_results: Vec::new(),
            highlights: Vec::new(),
            warnings: Vec::new(),
        });
        assert_eq!(
            failed_resume_point(&submission),
            FailedResumePoint::AfterParse
        );
    }

    #[test]
    fn classify_batch_resume_points_groups_students_by_restart_stage() {
        let mut from_start = workflow_submission("student_1");
        let mut after_alignment = workflow_submission("student_2");
        after_alignment.alignment_pages = vec![alignment_page(1)];
        let mut after_canonicalize = workflow_submission("student_3");
        after_canonicalize.alignment_pages = vec![alignment_page(1)];
        after_canonicalize.page_artifacts = vec![workflow_page("student_3", 1)];
        let mut after_parse = workflow_submission("student_4");
        after_parse.answers = vec![answer_seed("q1")];
        after_parse.answers[0].verified = true;
        after_parse.answers[0].parse_status = "ok".into();
        from_start.stage = "failed".into();
        after_alignment.stage = "failed".into();
        after_canonicalize.stage = "failed".into();
        after_parse.stage = "failed".into();
        let mut state = StudentWorkflowState {
            status: "attention".into(),
            latest_job_id: None,
            submissions: vec![from_start, after_alignment, after_canonicalize, after_parse],
        };

        let plan = classify_batch_resume_points(
            &mut state,
            vec![
                "student_1".into(),
                "student_2".into(),
                "student_3".into(),
                "student_4".into(),
            ],
        )
        .expect("resume points should classify");

        assert_eq!(plan.from_start, vec!["student_1"]);
        assert_eq!(plan.after_alignment, vec!["student_2"]);
        assert_eq!(plan.after_canonicalize, vec!["student_3"]);
        assert_eq!(plan.after_parse, vec!["student_4"]);
        let error = match classify_batch_resume_points(&mut state, vec!["missing".into()]) {
            Ok(_) => panic!("missing submission should fail validation"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("missing"));
    }

    #[test]
    fn canonicalize_batch_targets_use_intake_pages_template_pages_and_transforms() {
        let workspace = minimal_workspace_with_template_pages();
        let mut intake_by_ref = HashMap::new();
        intake_by_ref.insert("student_1".to_string(), intake_summary("student_1"));
        let mut submission = workflow_submission("student_1");
        submission.alignment_pages = vec![alignment_page(1), alignment_page(2)];
        let mut state = StudentWorkflowState {
            status: "running".into(),
            latest_job_id: None,
            submissions: vec![submission],
        };

        let targets = build_canonicalize_batch_targets(
            &workspace,
            &intake_by_ref,
            &mut state,
            &["student_1".to_string()],
        )
        .expect("canonicalize targets should build");

        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0]["page"]["student_ref"], "student_1");
        assert_eq!(targets[0]["page"]["source_pdf_path"], "/tmp/student_1.pdf");
        assert_eq!(
            targets[0]["template_page"]["image_path"],
            "/tmp/template-1.png"
        );
        assert_eq!(
            targets[1]["template_page"]["image_path"],
            "/tmp/template-2.png"
        );
        assert_eq!(targets[0]["transform"]["rotation"], json!(1.5));

        let error = build_canonicalize_batch_targets(
            &workspace,
            &HashMap::new(),
            &mut state,
            &["student_1".to_string()],
        )
        .expect_err("missing intake should fail");
        assert!(error
            .to_string()
            .contains("Student intake 'student_1' was missing"));
    }

    #[test]
    fn apply_canonicalize_batch_results_continues_complete_and_fails_partial_outputs() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-canonicalize-batch-results");
        std::fs::create_dir_all(&test_root).expect("test root should exist");
        let project_path = create_project(&test_root);
        let sink = RecordingEventSink::default();
        let mut complete = workflow_submission("student_1");
        complete.alignment_pages = vec![alignment_page(1), alignment_page(2)];
        let mut partial = workflow_submission("student_2");
        partial.alignment_pages = vec![alignment_page(1), alignment_page(2)];
        let mut state = StudentWorkflowState {
            status: "running".into(),
            latest_job_id: None,
            submissions: vec![complete, partial],
        };
        let refs = vec!["student_1".to_string(), "student_2".to_string()];
        let completed = CompletedWorkerJob {
            job_id: "canonicalize-batch".into(),
            result: WorkerJobResult {
                terminal_type: "job_finished".into(),
                terminal_payload: json!({}),
                envelope: json!({
                    "data": {
                        "canonicalize_results": [
                            {"student_ref": "student_1", "status": "ok", "output_page": {"page_number": 1, "image_path": "/tmp/student_1-1.png", "source_pdf_path": "/tmp/student_1.pdf"}},
                            {"student_ref": "student_1", "status": "ok", "output_page": {"page_number": 2, "image_path": "/tmp/student_1-2.png", "source_pdf_path": "/tmp/student_1.pdf"}},
                            {"student_ref": "student_2", "status": "ok", "output_page": {"page_number": 1, "image_path": "/tmp/student_2-1.png", "source_pdf_path": "/tmp/student_2.pdf"}},
                            {"student_ref": "student_2", "status": "error"}
                        ]
                    }
                }),
                events: Vec::new(),
            },
        };

        let continue_refs =
            apply_canonicalize_batch_results(&project_path, &sink, &mut state, &refs, &completed)
                .expect("canonicalize batch should apply");

        assert_eq!(continue_refs, vec!["student_1".to_string()]);
        assert_eq!(state.submissions[0].stage, "detect");
        assert_eq!(state.submissions[0].page_artifacts.len(), 2);
        assert_eq!(
            state.submissions[0].latest_job_id.as_deref(),
            Some("canonicalize-batch")
        );
        assert_eq!(state.submissions[1].stage, "failed");
        assert_eq!(
            state.submissions[1].failure_message.as_deref(),
            Some("Canonicalize failed for one or more student pages.")
        );
        let events = sink.snapshot();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload["studentRefs"], json!(refs));

        let _ = std::fs::remove_dir_all(&test_root);
    }

    #[test]
    fn ensure_submission_row_adds_only_missing_students() {
        let mut state = StudentWorkflowState {
            status: "ready".into(),
            latest_job_id: None,
            submissions: vec![workflow_submission("student_1")],
        };

        ensure_submission_row(&mut state, &intake_summary("student_1"));
        ensure_submission_row(&mut state, &intake_summary("student_2"));

        assert_eq!(state.submissions.len(), 2);
        assert_eq!(state.submissions[1].student_ref, "student_2");
    }

    #[test]
    fn intake_map_requires_canonical_and_page_paths() {
        let workspace = ExamWorkspaceState {
            project: ProjectSummary {
                project_id: "proj_1".into(),
                display_name: "Workflow".into(),
                subject: None,
                course_code: None,
                lms_course_id: None,
                project_path: "/tmp/project".into(),
                created_at: "0".into(),
                updated_at: "0".into(),
            },
            status: "ready".into(),
            status_label: "Ready".into(),
            failure_message: None,
            template_preview_artifacts: Vec::new(),
            aruco_status: Default::default(),
            questions: Vec::new(),
            redaction_regions: Vec::new(),
            warnings: Vec::new(),
            can_approve: false,
            can_approve_rubric: false,
            project_config: ProjectConfig::default(),
            student_roster: Vec::new(),
            student_intake: StudentIntakeState {
                status: "ready".into(),
                latest_job_id: None,
                unresolved_count: 0,
                items: vec![
                    intake_summary("student_1"),
                    StudentIntakeSummary {
                        student_ref: "student_2".into(),
                        local_display_name: None,
                        canonical_pdf_path: "".into(),
                        ingest_status: "ready".into(),
                        page_count: 0,
                        exam_page_paths: vec!["/tmp/only.png".into()],
                        warnings: Vec::new(),
                        binding_token_hex: None,
                    },
                ],
            },
            student_workflow: StudentWorkflowState::not_started(),
            moderation_state: Default::default(),
            results_lms_state: Default::default(),
            results_lms_rows: Vec::new(),
            results_lms_metrics: None,
            results_lms_review_summary: None,
            workflow_stage: "ready".into(),
            workflow_label: "Ready".into(),
        };

        let mapped = intake_map(&workspace).expect("intake map should build");
        assert_eq!(mapped.len(), 1);
        assert!(mapped.contains_key("student_1"));
    }

    #[test]
    fn apply_alignment_results_classifies_failed_and_low_confidence_rows() {
        let workspace = minimal_workspace_with_question();
        let mut workflow_state = StudentWorkflowState {
            status: "ready".into(),
            latest_job_id: None,
            submissions: vec![workflow_submission("student_1")],
        };

        let failed = apply_alignment_results(
            &workspace,
            &mut workflow_state,
            "student_1",
            CompletedWorkerJob {
                job_id: "job-fail".into(),
                result: WorkerJobResult {
                    terminal_type: "job_finished".into(),
                    terminal_payload: serde_json::json!({}),
                    envelope: serde_json::json!({
                        "data": {
                            "alignment_results": [
                                { "page_number": 1, "status": "failed", "confidence": 0.28 }
                            ]
                        }
                    }),
                    events: Vec::new(),
                },
            },
        )
        .expect("failed alignment should parse");
        assert!(matches!(
            failed,
            crate::state::student_workflow::AlignmentOutcome::NeedsReview
        ));
        assert_eq!(
            workflow_state.submissions[0].failure_message.as_deref(),
            None
        );
        assert_eq!(workflow_state.submissions[0].alignment_pages.len(), 1);
        let failed_page = &workflow_state.submissions[0].alignment_pages[0];
        assert!(failed_page.low_confidence);
        assert_eq!(failed_page.confidence, Some(0.28));
        assert_eq!(failed_page.transform.rotation, 0.0);
        assert_eq!(failed_page.transform.scale, 1.0);
        assert_eq!(failed_page.transform.translate_x, 0.0);
        assert_eq!(failed_page.transform.translate_y, 0.0);

        workflow_state.submissions[0] = workflow_submission("student_1");
        let review = apply_alignment_results(
            &workspace,
            &mut workflow_state,
            "student_1",
            CompletedWorkerJob {
                job_id: "job-review".into(),
                result: WorkerJobResult {
                    terminal_type: "job_finished".into(),
                    terminal_payload: serde_json::json!({}),
                    envelope: serde_json::json!({
                        "data": {
                            "alignment_results": [
                                { "page_number": 1, "status": "low_confidence", "confidence": 0.34, "transform": {"rotation": 0.0, "scale": 1.0, "translate_x": 0.0, "translate_y": 0.0} }
                            ]
                        }
                    }),
                    events: Vec::new(),
                },
            },
        )
        .expect("review alignment should parse");
        assert!(matches!(
            review,
            crate::state::student_workflow::AlignmentOutcome::NeedsReview
        ));
        assert_eq!(workflow_state.submissions[0].alignment_pages.len(), 1);
        assert!(workflow_state.submissions[0].alignment_pages[0].low_confidence);
        assert_eq!(
            workflow_state.submissions[0].alignment_pages[0].question_count,
            1
        );
    }

    #[test]
    fn apply_alignment_results_exempts_low_confidence_pages_without_questions() {
        let mut workspace = minimal_workspace_with_question();
        workspace.questions.clear();
        let mut workflow_state = StudentWorkflowState {
            status: "ready".into(),
            latest_job_id: None,
            submissions: vec![workflow_submission("student_1")],
        };

        let outcome = apply_alignment_results(
            &workspace,
            &mut workflow_state,
            "student_1",
            CompletedWorkerJob {
                job_id: "job-exempt".into(),
                result: WorkerJobResult {
                    terminal_type: "job_finished".into(),
                    terminal_payload: serde_json::json!({}),
                    envelope: serde_json::json!({
                        "data": {
                            "alignment_results": [
                                { "page_number": 1, "status": "low_confidence", "confidence": 0.34, "transform": {"rotation": 0.0, "scale": 1.0, "translate_x": 0.0, "translate_y": 0.0} }
                            ]
                        }
                    }),
                    events: Vec::new(),
                },
            },
        )
        .expect("exempt alignment should parse");

        assert!(matches!(
            outcome,
            crate::state::student_workflow::AlignmentOutcome::Continue
        ));
        let page = &workflow_state.submissions[0].alignment_pages[0];
        assert!(page.low_confidence);
        assert!(page.review_exempt);
        assert_eq!(page.review_exempt_reason.as_deref(), Some("no_questions"));
        assert_eq!(page.question_count, 0);
    }

    #[test]
    fn apply_alignment_results_keeps_review_for_mixed_question_bearing_pages() {
        let mut workspace = minimal_workspace_with_question();
        workspace.questions[0].page_number = 2;
        let mut workflow_state = StudentWorkflowState {
            status: "ready".into(),
            latest_job_id: None,
            submissions: vec![workflow_submission("student_1")],
        };

        let outcome = apply_alignment_results(
            &workspace,
            &mut workflow_state,
            "student_1",
            CompletedWorkerJob {
                job_id: "job-mixed".into(),
                result: WorkerJobResult {
                    terminal_type: "job_finished".into(),
                    terminal_payload: serde_json::json!({}),
                    envelope: serde_json::json!({
                        "data": {
                            "alignment_results": [
                                { "page_number": 1, "status": "low_confidence", "confidence": 0.34, "transform": {"rotation": 0.0, "scale": 1.0, "translate_x": 0.0, "translate_y": 0.0} },
                                { "page_number": 2, "status": "low_confidence", "confidence": 0.31, "transform": {"rotation": 0.1, "scale": 1.0, "translate_x": 2.0, "translate_y": 0.0} }
                            ]
                        }
                    }),
                    events: Vec::new(),
                },
            },
        )
        .expect("mixed alignment should parse");

        assert!(matches!(
            outcome,
            crate::state::student_workflow::AlignmentOutcome::NeedsReview
        ));
        assert!(workflow_state.submissions[0].alignment_pages[0].review_exempt);
        assert!(!workflow_state.submissions[0].alignment_pages[1].review_exempt);
        assert_eq!(
            workflow_state.submissions[0].alignment_pages[1].question_count,
            1
        );
    }

    #[test]
    fn filtered_completed_by_student_keeps_only_scoped_rows() {
        let completed = CompletedWorkerJob {
            job_id: "job-batch".into(),
            result: WorkerJobResult {
                terminal_type: "job_finished".into(),
                terminal_payload: serde_json::json!({}),
                envelope: serde_json::json!({
                    "data": {
                        "detect_results": [
                            { "student_ref": "student_1", "page_number": 1, "question_id": "q1", "status": "ok", "region": {"x": 0, "y": 0, "width": 10, "height": 10, "units": "rendered_page_pixels"}, "region_source": "ocr_refined" },
                            { "student_ref": "student_2", "page_number": 1, "question_id": "q1", "status": "ok", "region": {"x": 1, "y": 1, "width": 10, "height": 10, "units": "rendered_page_pixels"}, "region_source": "ocr_refined" }
                        ],
                        "page_ocr_results": [
                            { "student_ref": "student_1", "page_number": 1, "ocr_metadata_path": "/tmp/one.json" },
                            { "student_ref": "student_2", "page_number": 1, "ocr_metadata_path": "/tmp/two.json" }
                        ]
                    }
                }),
                events: Vec::new(),
            },
        };

        let filtered = filtered_completed_by_student(
            &completed,
            &["detect_results", "page_ocr_results"],
            "student_2",
        )
        .expect("filtered envelope should build");
        let data = filtered.result.envelope["data"]
            .as_object()
            .expect("data should be object");

        assert_eq!(data["detect_results"].as_array().unwrap().len(), 1);
        assert_eq!(
            data["detect_results"][0]["student_ref"].as_str(),
            Some("student_2")
        );
        assert_eq!(data["page_ocr_results"].as_array().unwrap().len(), 1);
        assert_eq!(
            data["page_ocr_results"][0]["ocr_metadata_path"].as_str(),
            Some("/tmp/two.json")
        );
    }

    #[test]
    fn build_crop_targets_preserves_detect_student_scope() {
        let mut data = serde_json::Map::new();
        data.insert(
            "detect_results".into(),
            serde_json::json!([
                {
                    "student_ref": "student_1",
                    "page_number": 1,
                    "question_id": "q1",
                    "status": "ok",
                    "region": {"x": 0, "y": 0, "width": 10, "height": 10, "units": "rendered_page_pixels"},
                    "region_source": "ocr_refined"
                }
            ]),
        );

        let targets = build_crop_targets(&data).expect("crop targets should build");

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0]["student_ref"].as_str(), Some("student_1"));
        assert_eq!(targets[0]["question_id"].as_str(), Some("q1"));
    }

    #[test]
    fn build_crop_targets_excludes_template_fallback_rows() {
        let mut data = serde_json::Map::new();
        data.insert(
            "detect_results".into(),
            serde_json::json!([
                {
                    "student_ref": "student_1",
                    "page_number": 1,
                    "question_id": "q1",
                    "status": "warning",
                    "region": {"x": 0, "y": 0, "width": 10, "height": 10, "units": "rendered_page_pixels"},
                    "region_source": "template_fallback"
                }
            ]),
        );

        let targets = build_crop_targets(&data).expect("crop targets should build");

        assert!(targets.is_empty());
    }

    #[test]
    fn build_detect_review_captures_template_fallback_for_manual_resolution() {
        let workspace = minimal_workspace_with_question();
        let mut submission = workflow_submission("student_1");
        submission.page_artifacts = vec![StudentWorkflowPage {
            page_number: 1,
            image_path: "/tmp/page-1.png".into(),
            source_pdf_path: None,
            ocr_metadata_path: None,
        }];
        let mut data = serde_json::Map::new();
        data.insert(
            "detect_results".into(),
            serde_json::json!([
                {
                    "student_ref": "student_1",
                    "page_number": 1,
                    "question_id": "q1",
                    "status": "warning",
                    "region": {"x": 5, "y": 10, "width": 90, "height": 120, "units": "rendered_page_pixels"},
                    "region_source": "template_fallback",
                    "warnings": [{"code": "detect_template_fallback", "message": "Needs review."}]
                }
            ]),
        );

        let review = build_detect_review(&workspace, &submission, &data)
            .expect("detect review should build")
            .expect("fallback row should require review");

        assert_eq!(review.pending_rows.len(), 1);
        assert_eq!(
            review.pending_rows[0].source_page_image_path,
            "/tmp/page-1.png"
        );
        assert_eq!(review.pending_rows[0].template_region.y, 10);
        assert!(review.trusted_crop_targets.is_empty());
    }

    #[test]
    fn split_detect_refs_reuses_resolved_detect_review_crop_targets() {
        let mut ready_for_crop = workflow_submission("student_1");
        ready_for_crop.stage = "crop".into();
        ready_for_crop.detect_review = Some(StudentWorkflowDetectReview {
            pending_rows: vec![StudentWorkflowDetectReviewRow {
                question_id: "q1".into(),
                page_number: 1,
                source_page_image_path: "/tmp/page.png".into(),
                template_region: StudentWorkflowDetectRegion {
                    x: 5,
                    y: 10,
                    width: 90,
                    height: 120,
                    units: "rendered_page_pixels".into(),
                },
                warnings: Vec::new(),
                resolved_region: Some(StudentWorkflowDetectRegion {
                    x: 8,
                    y: 12,
                    width: 88,
                    height: 118,
                    units: "rendered_page_pixels".into(),
                }),
            }],
            trusted_crop_targets: vec![json!({
                "student_ref": "student_1",
                "page_number": 1,
                "question_id": "trusted_q",
                "region": {"x": 1, "y": 2, "width": 3, "height": 4, "units": "rendered_page_pixels"}
            })],
        });
        let mut unresolved = workflow_submission("student_2");
        unresolved.stage = "crop".into();
        unresolved.detect_review = Some(StudentWorkflowDetectReview {
            pending_rows: vec![StudentWorkflowDetectReviewRow {
                question_id: "q1".into(),
                page_number: 1,
                source_page_image_path: "/tmp/page.png".into(),
                template_region: StudentWorkflowDetectRegion {
                    x: 5,
                    y: 10,
                    width: 90,
                    height: 120,
                    units: "rendered_page_pixels".into(),
                },
                warnings: Vec::new(),
                resolved_region: None,
            }],
            trusted_crop_targets: Vec::new(),
        });
        let mut workflow_state = StudentWorkflowState {
            status: "running".into(),
            latest_job_id: None,
            submissions: vec![ready_for_crop, unresolved],
        };

        let refs = vec!["student_1".to_string(), "student_2".to_string()];
        let split = split_detect_refs_by_reusable_crop_targets(&mut workflow_state, &refs)
            .expect("detect refs should split");

        assert_eq!(split.detect_refs, vec!["student_2".to_string()]);
        let reused = split
            .crop_targets_by_ref
            .get("student_1")
            .expect("resolved review should produce crop targets");
        assert_eq!(reused.len(), 2);
        assert_eq!(reused[0]["question_id"], "trusted_q");
        assert_eq!(reused[1]["question_id"], "q1");
        assert_eq!(reused[1]["region"]["x"], 8);
    }

    #[test]
    fn apply_detect_batch_results_branches_review_crop_and_failure_rows() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-detect-batch-branches");
        std::fs::create_dir_all(&test_root).expect("test root should exist");
        let project_path = create_project(&test_root);
        let workspace = minimal_workspace_with_question();
        let sink = RecordingEventSink::default();
        let mut needs_review = workflow_submission("student_1");
        needs_review.page_artifacts = vec![StudentWorkflowPage {
            page_number: 1,
            image_path: "/tmp/student_1-page.png".into(),
            source_pdf_path: None,
            ocr_metadata_path: None,
        }];
        let mut ready_for_crop = workflow_submission("student_2");
        ready_for_crop.page_artifacts = vec![StudentWorkflowPage {
            page_number: 1,
            image_path: "/tmp/student_2-page.png".into(),
            source_pdf_path: None,
            ocr_metadata_path: None,
        }];
        let mut no_regions = workflow_submission("student_3");
        no_regions.page_artifacts = vec![StudentWorkflowPage {
            page_number: 1,
            image_path: "/tmp/student_3-page.png".into(),
            source_pdf_path: None,
            ocr_metadata_path: None,
        }];
        let mut workflow_state = StudentWorkflowState {
            status: "running".into(),
            latest_job_id: None,
            submissions: vec![needs_review, ready_for_crop, no_regions],
        };
        let completed = CompletedWorkerJob {
            job_id: "detect-batch".into(),
            result: WorkerJobResult {
                terminal_type: "job_finished".into(),
                terminal_payload: json!({}),
                envelope: json!({
                    "data": {
                        "detect_results": [
                            {
                                "student_ref": "student_1",
                                "page_number": 1,
                                "question_id": "q1",
                                "status": "warning",
                                "region": {"x": 5, "y": 10, "width": 90, "height": 120, "units": "rendered_page_pixels"},
                                "region_source": "template_fallback"
                            },
                            {
                                "student_ref": "student_2",
                                "page_number": 1,
                                "question_id": "q1",
                                "status": "ok",
                                "region": {"x": 6, "y": 11, "width": 89, "height": 119, "units": "rendered_page_pixels"},
                                "region_source": "ocr_refined"
                            }
                        ],
                        "page_ocr_results": [
                            {"student_ref": "student_2", "page_number": 1, "ocr_metadata_path": "/tmp/student_2-ocr.json"}
                        ]
                    }
                }),
                events: Vec::new(),
            },
        };
        let refs = vec![
            "student_1".to_string(),
            "student_2".to_string(),
            "student_3".to_string(),
        ];

        let crop_targets = apply_detect_batch_results(
            &workspace,
            &project_path,
            &sink,
            &mut workflow_state,
            &refs,
            &completed,
        )
        .expect("detect batch should apply");

        assert_eq!(workflow_state.submissions[0].stage, "detect_review");
        assert!(workflow_state.submissions[0].detect_review.is_some());
        assert_eq!(workflow_state.submissions[1].stage, "crop");
        assert_eq!(
            workflow_state.submissions[1].page_artifacts[0]
                .ocr_metadata_path
                .as_deref(),
            Some("/tmp/student_2-ocr.json")
        );
        assert_eq!(workflow_state.submissions[2].stage, "failed");
        assert_eq!(
            workflow_state.submissions[2].failure_message.as_deref(),
            Some("No question regions were detected for crop.")
        );
        assert_eq!(crop_targets["student_2"][0]["question_id"], "q1");
        let events = sink.snapshot();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload["studentRefs"], json!(refs));
        assert_eq!(events[0].payload["workflowStatus"], "attention");

        let _ = std::fs::remove_dir_all(&test_root);
    }

    #[test]
    fn detect_batch_command_error_reports_missing_paddleocr_models() {
        let completed = CompletedWorkerJob {
            job_id: "detect-batch".into(),
            result: WorkerJobResult {
                terminal_type: "command_error".into(),
                terminal_payload: json!({}),
                envelope: json!({
                    "error": {
                        "category": "external_dependency",
                        "code": "ocr_dependency_unavailable",
                        "details": {
                            "error": "paddleocr backend unavailable: missing PaddleOCR det model directory: C:\\scriptscore\\scriptscore\\cli\\models\\paddle\\det",
                            "error_type": "RuntimeError",
                            "model_dir": "C:\\scriptscore\\scriptscore\\cli\\models\\paddle"
                        },
                        "message": "PaddleOCR dependencies or models are not available.",
                        "retryable": false,
                        "write_state": "no_write"
                    }
                }),
                events: Vec::new(),
            },
        };

        let err = detect_batch_command_error(&completed)
            .expect("detect command error should be recognized");
        assert_eq!(
            err.to_string(),
            "Command failed: PaddleOCR dependencies or models are not available."
        );
    }

    #[test]
    fn apply_parse_batch_results_branches_failed_review_and_grading_rows() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-parse-batch-branches");
        std::fs::create_dir_all(&test_root).expect("test root should exist");
        let project_path = create_project(&test_root);
        let sink = RecordingEventSink::default();
        let mut failed = workflow_submission("student_1");
        failed.answers = vec![answer_seed("q1")];
        let mut needs_review = workflow_submission("student_2");
        needs_review.answers = vec![answer_seed("q1")];
        let mut grading = workflow_submission("student_3");
        grading.answers = vec![answer_seed("q1")];
        let mut workflow_state = StudentWorkflowState {
            status: "running".into(),
            latest_job_id: None,
            submissions: vec![failed, needs_review, grading],
        };
        let completed = CompletedWorkerJob {
            job_id: "parse-batch".into(),
            result: WorkerJobResult {
                terminal_type: "job_finished".into(),
                terminal_payload: json!({}),
                envelope: json!({
                    "data": {
                        "parse_results": [
                            {"student_ref": "student_1", "question_id": "q1", "status": "error", "parsed_text": ""},
                            {"student_ref": "student_2", "question_id": "q1", "status": "ok", "confidence": "low", "confidence_source": "combined", "parsed_text": "uncertain"},
                            {"student_ref": "student_3", "question_id": "q1", "status": "ok", "confidence": "high", "confidence_source": "combined", "parsed_text": "clean answer"}
                        ]
                    }
                }),
                events: Vec::new(),
            },
        };
        let refs = vec![
            "student_1".to_string(),
            "student_2".to_string(),
            "student_3".to_string(),
        ];

        let grading_refs =
            apply_parse_batch_results(&project_path, &sink, &mut workflow_state, &refs, &completed)
                .expect("parse batch should apply");

        assert_eq!(grading_refs, vec!["student_3".to_string()]);
        assert_eq!(workflow_state.submissions[0].stage, "failed");
        assert_eq!(
            workflow_state.submissions[0].failure_message.as_deref(),
            Some("Parsing failed for one or more answers.")
        );
        assert_eq!(workflow_state.submissions[1].stage, "parse_review");
        assert!(workflow_state.submissions[1].answers[0].review_required);
        assert_eq!(workflow_state.submissions[2].stage, "grading");
        assert!(workflow_state.submissions[2].answers[0].verified);
        assert_eq!(sink.snapshot()[0].payload["workflowStatus"], "attention");

        let _ = std::fs::remove_dir_all(&test_root);
    }

    #[test]
    fn stopped_and_failed_ref_updates_emit_ready_worker_status() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-workflow-terminal-ref-events");
        std::fs::create_dir_all(&test_root).expect("test root should exist");
        let project_path = create_project(&test_root);
        let sink = RecordingEventSink::default();
        let refs = vec!["student_1".to_string()];
        let mut state = StudentWorkflowState {
            status: "running".into(),
            latest_job_id: Some("job-1".into()),
            submissions: vec![workflow_submission("student_1")],
        };

        mark_refs_stopped_and_emit(&project_path, &mut state, &sink, &refs)
            .expect("stop update should emit");
        mark_refs_failed_with_message(&project_path, &mut state, &sink, &refs, "Detect failed")
            .expect("failure update should emit");

        let events = sink.snapshot();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0].worker_status, WorkerStatus::Ready));
        assert_eq!(events[0].payload["workflowStatus"], "ready");
        assert_eq!(state.submissions[0].stage, "failed");
        assert_eq!(
            state.submissions[0].failure_message.as_deref(),
            Some("Detect failed")
        );
        assert!(matches!(events[1].worker_status, WorkerStatus::Ready));
        assert_eq!(events[1].payload["workflowStatus"], "attention");

        let _ = std::fs::remove_dir_all(&test_root);
    }

    #[test]
    fn crop_targets_for_cli_removes_student_scope_for_legacy_contracts() {
        let targets = serde_json::json!([
            {
                "student_ref": "student_1",
                "page_number": 1,
                "question_id": "q1",
                "region": {"x": 0, "y": 0, "width": 10, "height": 10, "units": "rendered_page_pixels"}
            }
        ]);
        let stripped = crop_targets_for_cli(targets.as_array().unwrap());

        assert_eq!(stripped.len(), 1);
        assert!(stripped[0].get("student_ref").is_none());
        assert_eq!(stripped[0]["question_id"].as_str(), Some("q1"));
    }

    #[test]
    fn split_detect_refs_reuses_resolved_review_crop_targets_only() {
        let mut reusable = workflow_submission("student_1");
        reusable.stage = "crop".into();
        reusable.detect_review = Some(StudentWorkflowDetectReview {
            trusted_crop_targets: vec![json!({
                "student_ref": "student_1",
                "question_id": "q_trusted",
                "page_number": 1,
                "region": {"x": 1, "y": 2, "width": 3, "height": 4, "units": "rendered_page_pixels"}
            })],
            pending_rows: vec![StudentWorkflowDetectReviewRow {
                question_id: "q_resolved".into(),
                page_number: 1,
                source_page_image_path: "/tmp/page.png".into(),
                template_region: StudentWorkflowDetectRegion {
                    x: 5,
                    y: 6,
                    width: 7,
                    height: 8,
                    units: "rendered_page_pixels".into(),
                },
                warnings: Vec::new(),
                resolved_region: Some(StudentWorkflowDetectRegion {
                    x: 9,
                    y: 10,
                    width: 11,
                    height: 12,
                    units: "rendered_page_pixels".into(),
                }),
            }],
        });
        let mut unresolved = workflow_submission("student_2");
        unresolved.stage = "crop".into();
        unresolved.detect_review = Some(StudentWorkflowDetectReview {
            trusted_crop_targets: Vec::new(),
            pending_rows: vec![StudentWorkflowDetectReviewRow {
                question_id: "q_pending".into(),
                page_number: 1,
                source_page_image_path: "/tmp/page.png".into(),
                template_region: StudentWorkflowDetectRegion {
                    x: 1,
                    y: 1,
                    width: 2,
                    height: 2,
                    units: "rendered_page_pixels".into(),
                },
                warnings: Vec::new(),
                resolved_region: None,
            }],
        });
        let mut fresh_detect = workflow_submission("student_3");
        fresh_detect.stage = "detect".into();
        let mut state = StudentWorkflowState {
            status: "running".into(),
            latest_job_id: None,
            submissions: vec![reusable, unresolved, fresh_detect],
        };

        let split = split_detect_refs_by_reusable_crop_targets(
            &mut state,
            &[
                "student_1".to_string(),
                "student_2".to_string(),
                "student_3".to_string(),
            ],
        )
        .expect("detect refs should split");

        assert_eq!(split.detect_refs, vec!["student_2", "student_3"]);
        let reused = split
            .crop_targets_by_ref
            .get("student_1")
            .expect("resolved review should be reused");
        assert_eq!(reused.len(), 2);
        assert_eq!(reused[0]["question_id"], "q_trusted");
        assert_eq!(reused[1]["question_id"], "q_resolved");
        assert_eq!(reused[1]["region"]["x"], json!(9));
    }

    #[test]
    fn sanitized_pii_batch_payload_keeps_only_student_refs_question_ids_and_counts() {
        let mut crop_rows = HashMap::new();
        crop_rows.insert(
            "student_2".to_string(),
            vec![
                json!({"status": "ok", "question_id": "q2", "question_crop_path": "/secret/q2.png"}),
                json!({"status": "error", "question_id": "q3", "question_crop_path": "/secret/q3.png"}),
            ],
        );
        crop_rows.insert(
            "student_1".to_string(),
            vec![json!({"status": "ok", "question_id": "q1", "question_crop_path": "/secret/q1.png"})],
        );

        let payload = sanitized_pii_batch_request_payload(&crop_rows);

        assert_eq!(payload["pii_runtime"], "local_paddle");
        assert_eq!(payload["students"][0]["student_ref"], "student_1");
        assert_eq!(payload["students"][0]["question_ids"], json!(["q1"]));
        assert_eq!(payload["students"][0]["pii_target_count"], json!(1));
        assert_eq!(payload["students"][1]["student_ref"], "student_2");
        assert_eq!(payload["students"][1]["question_ids"], json!(["q2"]));
        assert!(!payload.to_string().contains("/secret/"));
    }

    #[test]
    fn small_pipeline_helpers_reset_filter_record_and_sanitize_payloads() {
        let warning = WorkspaceWarning {
            code: Some("intake_warning".into()),
            message: "Check page order".into(),
            scope: Some("student".into()),
        };
        let intake = StudentIntakeSummary {
            student_ref: "student_1".into(),
            local_display_name: Some("Ada Local".into()),
            canonical_pdf_path: "/new/student.pdf".into(),
            ingest_status: "ready".into(),
            page_count: 3,
            exam_page_paths: vec!["/new/page-1.png".into()],
            warnings: vec![warning.clone()],
            binding_token_hex: None,
        };
        let mut submission = workflow_submission("student_1");
        submission.canonical_pdf_path = "/old/student.pdf".into();
        submission.page_count = 1;
        submission.stage = "failed".into();
        submission.latest_job_id = Some("old-job".into());
        submission.failure_message = Some("old failure".into());
        submission.warnings = vec![warning];
        submission.page_artifacts = vec![workflow_page("student_1", 1)];
        submission.alignment_pages = vec![alignment_page(1)];
        submission.detect_review = Some(StudentWorkflowDetectReview {
            pending_rows: Vec::new(),
            trusted_crop_targets: Vec::new(),
        });
        submission.answers = vec![answer_seed("q1")];

        reset_submission_for_restart(&mut submission, &intake);

        assert_eq!(submission.canonical_pdf_path, "/new/student.pdf");
        assert_eq!(submission.page_count, 3);
        assert_eq!(submission.stage, "intake_ready");
        assert_eq!(submission.latest_job_id.as_deref(), Some("old-job"));
        assert!(submission.failure_message.is_none());
        assert!(submission.warnings.is_empty());
        assert!(submission.page_artifacts.is_empty());
        assert!(submission.alignment_pages.is_empty());
        assert!(submission.detect_review.is_none());
        assert!(submission.answers.is_empty());

        let rows = vec![
            json!({"student_ref": "student_2", "question_id": "q2"}),
            json!({"student_ref": "student_1", "question_id": "q1"}),
            json!({"student_ref": "student_1", "question_id": "q3"}),
            json!({"question_id": "missing_student"}),
        ];
        let filtered = rows_for_student(&rows, "student_1");
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0]["question_id"], "q1");

        let mut workflow_state = StudentWorkflowState {
            status: "ready".into(),
            latest_job_id: None,
            submissions: vec![workflow_submission("student_1")],
        };
        record_submission_job_id(&mut workflow_state, "student_1", "job-123")
            .expect("job id should record");
        assert_eq!(
            workflow_state.submissions[0].latest_job_id.as_deref(),
            Some("job-123")
        );
        assert!(record_submission_job_id(&mut workflow_state, "missing", "job-456").is_err());

        let mut keyed_rows = HashMap::new();
        keyed_rows.insert("b".to_string(), vec![json!({})]);
        keyed_rows.insert("a".to_string(), vec![json!({})]);
        assert_eq!(sorted_keys(&keyed_rows), vec!["a", "b"]);

        let warning = pii_identity_unavailable_warning();
        assert_eq!(warning.code.as_deref(), Some("pii_identity_unavailable"));
        assert_eq!(warning.scope.as_deref(), Some("answer"));

        let single_payload = sanitized_pii_request_payload(
            "student_1",
            &[
                json!({"status": "ok", "question_id": "q1", "question_crop_path": "/secret/q1.png"}),
                json!({"status": "error", "question_id": "q2", "question_crop_path": "/secret/q2.png"}),
            ],
        );
        assert_eq!(single_payload["student_ref"], "student_1");
        assert_eq!(single_payload["question_ids"], json!(["q1"]));
        assert_eq!(single_payload["pii_target_count"], json!(1));
        assert!(!single_payload.to_string().contains("/secret/"));

        let feedback_rows = feedback_request_rows(&[
            json!({"student_ref": "student_1", "question_id": "q1", "score": 4, "feedback": "keep"}),
        ]);
        assert_eq!(feedback_rows.len(), 1);
        assert_eq!(feedback_rows[0]["student_ref"], "student_1");
    }

    #[test]
    fn settle_empty_grading_refs_routes_manual_blocks_and_no_answer_failures() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-empty-grading-refs");
        std::fs::create_dir_all(&test_root).expect("test root should exist");
        let project_path = create_project(&test_root);
        let sink = RecordingEventSink::default();
        let mut manual = workflow_submission("student_1");
        manual.stage = "grading".into();
        manual.answers = vec![answer_seed("q1")];
        manual.answers[0].manual_grading_required = true;
        let mut missing_verified = workflow_submission("student_2");
        missing_verified.stage = "grading".into();
        missing_verified.answers = vec![answer_seed("q1")];
        let mut state = StudentWorkflowState {
            status: "running".into(),
            latest_job_id: None,
            submissions: vec![manual, missing_verified],
        };
        let refs = vec!["student_1".to_string(), "student_2".to_string()];
        let score_requests = refs
            .iter()
            .map(|student_ref| (student_ref.clone(), Vec::new()))
            .collect::<HashMap<_, _>>();

        settle_empty_grading_refs(&project_path, &sink, &mut state, &refs, &score_requests)
            .expect("empty grading refs should settle");

        assert_eq!(state.submissions[0].stage, "manual_grading");
        assert_eq!(state.submissions[0].failure_message, None);
        assert_eq!(state.submissions[1].stage, "failed");
        assert_eq!(
            state.submissions[1].failure_message.as_deref(),
            Some("No verified answers were available for grading.")
        );
        assert_eq!(sink.snapshot()[0].payload["workflowStatus"], "attention");

        let _ = std::fs::remove_dir_all(&test_root);
    }

    #[test]
    fn completed_was_cancelled_matches_only_cancel_terminal_type() {
        let cancelled = CompletedWorkerJob {
            job_id: "job-cancelled".into(),
            result: WorkerJobResult {
                terminal_type: "job_cancelled".into(),
                terminal_payload: json!({}),
                envelope: json!({"data": {}}),
                events: Vec::new(),
            },
        };
        let finished = CompletedWorkerJob {
            job_id: "job-finished".into(),
            result: WorkerJobResult {
                terminal_type: "job_finished".into(),
                terminal_payload: json!({}),
                envelope: json!({"data": {}}),
                events: Vec::new(),
            },
        };

        assert!(completed_was_cancelled(&cancelled));
        assert!(!completed_was_cancelled(&finished));
    }

    #[test]
    fn pii_batch_preparation_blocks_missing_identity_and_applies_clean_results() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-pii-batch-prep");
        std::fs::create_dir_all(&test_root).expect("test root should exist");
        let project_path = create_project(&test_root);
        let sink = RecordingEventSink::default();
        let workspace = minimal_workspace_with_question();
        let mut state = StudentWorkflowState {
            status: "running".into(),
            latest_job_id: None,
            submissions: vec![
                workflow_submission("student_1"),
                workflow_submission("student_2"),
                workflow_submission("student_3"),
            ],
        };
        for submission in &mut state.submissions {
            submission.stage = "pii".into();
        }
        let crop_rows_by_ref = HashMap::from([
            (
                "student_1".to_string(),
                vec![json!({
                    "student_ref": "student_1",
                    "question_id": "q1",
                    "status": "ok",
                    "question_crop_path": "/tmp/student_1-q1.png"
                })],
            ),
            (
                "student_2".to_string(),
                vec![json!({
                    "student_ref": "student_2",
                    "question_id": "q1",
                    "status": "error"
                })],
            ),
            (
                "student_3".to_string(),
                vec![json!({
                    "student_ref": "student_3",
                    "question_id": "q1",
                    "status": "ok",
                    "question_crop_path": "/tmp/student_3-q1.png"
                })],
            ),
        ]);
        let trigger_words_by_ref =
            HashMap::from([("student_3".to_string(), vec!["Ada".to_string()])]);

        let pii_inputs = prepare_pii_batch_inputs(
            &project_path,
            &sink,
            &workspace,
            &mut state,
            &trigger_words_by_ref,
            &crop_rows_by_ref,
        )
        .expect("pii batch inputs should prepare");

        assert_eq!(pii_inputs.len(), 1);
        assert_eq!(pii_inputs[0]["student_ref"], "student_3");
        assert_eq!(pii_inputs[0]["pii_targets"][0]["question_id"], "q1");
        assert_eq!(state.submissions[0].stage, "manual_grading");
        assert_eq!(state.submissions[1].stage, "manual_grading");
        assert_eq!(
            state.submissions[0].answers[0]
                .manual_grading_reason
                .as_deref(),
            Some("pii_ambiguous")
        );

        let completed = CompletedWorkerJob {
            job_id: "pii-batch".into(),
            result: WorkerJobResult {
                terminal_type: "job_finished".into(),
                terminal_payload: json!({}),
                envelope: json!({
                    "data": {
                        "pii_results": [
                            {
                                "student_ref": "student_3",
                                "question_id": "q1",
                                "status": "ok",
                                "contains_handwriting": "no",
                                "contains_pii": false,
                                "pii_types_detected": []
                            }
                        ]
                    }
                }),
                events: Vec::new(),
            },
        };

        let parse_refs = apply_pii_batch_results(
            &project_path,
            &sink,
            &workspace,
            &mut state,
            &crop_rows_by_ref,
            &completed,
        )
        .expect("pii results should apply");

        assert_eq!(parse_refs, vec!["student_3"]);
        assert_eq!(state.submissions[2].stage, "parse");
        assert_eq!(
            state.submissions[2].latest_job_id.as_deref(),
            Some("pii-batch")
        );
        assert!(!state.submissions[2].answers[0].manual_grading_required);
        assert_eq!(sink.snapshot().len(), 2);

        let _ = std::fs::remove_dir_all(&test_root);
    }

    #[test]
    fn batch_grading_persistence_applies_scores_feedback_and_highlights() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-batch-grading-persist");
        std::fs::create_dir_all(&test_root).expect("test root should exist");
        let project_path = create_project(&test_root);
        let sink = RecordingEventSink::default();
        let workspace = minimal_workspace_with_question();
        let question_by_id = workspace
            .questions
            .iter()
            .map(|question| (question.question_id.clone(), question))
            .collect::<HashMap<_, _>>();
        let mut submission = workflow_submission("student_1");
        submission.stage = "grading".into();
        submission.answers = vec![answer_seed("q1")];
        submission.answers[0].verified = true;
        submission.answers[0].verified_text = Some("A clear verified answer".into());
        let mut state = StudentWorkflowState {
            status: "running".into(),
            latest_job_id: None,
            submissions: vec![submission],
        };
        let refs = vec!["student_1".to_string()];

        let score_requests =
            build_batch_answer_score_requests(&workspace, &mut state, &question_by_id, &refs)
                .expect("score requests should build");
        assert_eq!(score_requests["student_1"].len(), 1);
        assert_eq!(
            crop_rows_from_answers(&state.submissions[0])[0]["question_crop_path"],
            "/tmp/q1.png"
        );

        let preliminary_rows = vec![json!({
            "student_ref": "student_1",
            "question_id": "q1",
            "criterion_index": 0,
            "criterion_label": "Correctness",
            "points_awarded": 4,
            "rationale": "Correct answer with minor omission.",
            "confidence": "medium",
            "confidence_reason": "Enough evidence"
        })];
        let final_rows = persist_batch_preliminary_rows(
            &project_path,
            &sink,
            &workspace,
            &mut state,
            &question_by_id,
            &refs,
            &preliminary_rows,
        )
        .expect("preliminary rows should persist");
        assert_eq!(final_rows[0]["total_points_awarded"], 4);
        assert_eq!(
            state.submissions[0].answers[0].total_points_awarded,
            Some(4)
        );

        let feedback_rows = vec![json!({
            "student_ref": "student_1",
            "question_id": "q1",
            "feedback_text": "Solid answer."
        })];
        persist_batch_feedback_rows(&project_path, &sink, &mut state, &refs, &feedback_rows)
            .expect("feedback rows should persist");
        assert_eq!(
            state.submissions[0].answers[0].feedback_text.as_deref(),
            Some("Solid answer.")
        );

        let highlight_rows = vec![json!({
            "student_ref": "student_1",
            "question_id": "q1",
            "highlights": [
                {
                    "kind": "strength",
                    "start_char": 0,
                    "end_char": 5,
                    "text": "Solid"
                }
            ]
        })];
        finish_batch_grading(
            &project_path,
            &sink,
            &mut state,
            &refs,
            final_rows,
            &feedback_rows,
            &highlight_rows,
        )
        .expect("batch grading should finish");

        assert_eq!(state.submissions[0].stage, "graded");
        assert_eq!(
            state.submissions[0].answers[0].highlights[0].kind,
            "strength"
        );
        assert_eq!(state.status, "graded");
        assert_eq!(sink.snapshot().len(), 3);

        let _ = std::fs::remove_dir_all(&test_root);
    }

    fn answer_seed(question_id: &str) -> StudentWorkflowAnswer {
        StudentWorkflowAnswer {
            question_id: question_id.into(),
            question_number: 1,
            crop_image_path: Some(format!("/tmp/{question_id}.png")),
            pii_prescreen: None,
            manual_grading_required: false,
            manual_grading_reason: None,
            moderation_eligible: true,
            parse_status: "pending".into(),
            parse_confidence: None,
            parse_confidence_source: None,
            raw_parsed_text: None,
            verified_text: None,
            review_required: false,
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
        }
    }

    #[test]
    fn save_workflow_state_derives_ready_attention_running_and_graded() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-workflow-state");
        std::fs::create_dir_all(&test_root).expect("test root should exist");
        let project_path = create_project(&test_root);

        let mut state = StudentWorkflowState {
            status: "not_started".into(),
            latest_job_id: None,
            submissions: vec![workflow_submission("student_1")],
        };
        save_workflow_state(&project_path, &mut state).expect("ready state should save");
        assert_eq!(state.status, "ready");

        state.submissions[0].stage = "stopped".into();
        save_workflow_state(&project_path, &mut state).expect("stopped state should save");
        assert_eq!(state.status, "ready");

        state.submissions[0].stage = "alignment_review".into();
        save_workflow_state(&project_path, &mut state).expect("attention state should save");
        assert_eq!(state.status, "attention");

        state.submissions[0].stage = "grading".into();
        save_workflow_state(&project_path, &mut state).expect("running state should save");
        assert_eq!(state.status, "running");

        state.submissions[0].stage = "graded".into();
        save_workflow_state(&project_path, &mut state).expect("graded state should save");
        assert_eq!(state.status, "graded");

        std::fs::remove_dir_all(&test_root).expect("test project should clean up");
    }

    #[test]
    fn save_workflow_state_prewarms_report_assets_when_crop_pngs_exist() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-workflow-prewarm");
        std::fs::create_dir_all(&test_root).expect("test root should exist");
        let project_path = create_project(&test_root);
        let crop_path = test_root.join("question_1.png");
        write_sample_png(&crop_path, 640, 420);

        let mut state = StudentWorkflowState {
            status: "not_started".into(),
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
                    crop_image_path: Some(crop_path.to_string_lossy().into_owned()),
                    pii_prescreen: None,
                    manual_grading_required: false,
                    manual_grading_reason: None,
                    moderation_eligible: true,
                    parse_status: "ok".into(),
                    parse_confidence: Some("high".into()),
                    parse_confidence_source: Some("combined".into()),
                    raw_parsed_text: Some("answer".into()),
                    verified_text: Some("answer".into()),
                    review_required: false,
                    verified: true,
                    stale: false,
                    grading_status: "draft_ready".into(),
                    grading_confidence: Some("high".into()),
                    grading_confidence_reason: None,
                    question_max_points: Some(5),
                    total_points_awarded: Some(5),
                    feedback_text: Some("Correct.".into()),
                    criterion_results: Vec::new(),
                    highlights: Vec::new(),
                    warnings: Vec::new(),
                }],
            }],
        };

        save_workflow_state(&project_path, &mut state).expect("workflow state should save");

        let report_asset_path = project_path
            .join("artifacts")
            .join("results_lms/report_images")
            .join("student_1")
            .join("question_1.jpg");
        for _ in 0..50 {
            if report_asset_path.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(report_asset_path.exists());

        std::fs::remove_dir_all(&test_root).expect("test project should clean up");
    }

    #[test]
    fn save_workflow_state_and_emit_reports_stage_and_status() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-workflow-emit");
        std::fs::create_dir_all(&test_root).expect("test root should exist");
        let project_path = create_project(&test_root);
        let sink = RecordingEventSink::default();

        let mut state = StudentWorkflowState {
            status: "not_started".into(),
            latest_job_id: None,
            submissions: vec![workflow_submission("student_1")],
        };
        state.submissions[0].stage = "parse_review".into();

        save_workflow_state_and_emit(
            &project_path,
            &mut state,
            &sink,
            Some("student_1"),
            WorkerStatus::Ready,
        )
        .expect("workflow state should save and emit");

        let events = sink.snapshot();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "workflow_state_updated");
        assert_eq!(events[0].payload["studentRef"], "student_1");
        assert_eq!(events[0].payload["stage"], "parse_review");
        assert_eq!(events[0].payload["workflowStatus"], "attention");

        std::fs::remove_dir_all(&test_root).expect("test project should clean up");
    }

    #[test]
    fn empty_batch_continuation_marks_worker_ready() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-empty-batch-ready");
        std::fs::create_dir_all(&test_root).expect("test root should exist");
        let project_path = create_project(&test_root);
        let sink = RecordingEventSink::default();

        let mut state = StudentWorkflowState {
            status: "running".into(),
            latest_job_id: Some("job_parse".into()),
            submissions: vec![workflow_submission("student_1")],
        };
        state.submissions[0].stage = "parse_review".into();

        emit_ready_for_empty_batch_continuation(&project_path, &mut state, &sink)
            .expect("empty continuation should save and emit ready");

        let events = sink.snapshot();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "workflow_state_updated");
        assert!(matches!(&events[0].worker_status, WorkerStatus::Ready));
        assert_eq!(events[0].payload["studentRef"], serde_json::Value::Null);
        assert_eq!(events[0].payload["workflowStatus"], "attention");

        std::fs::remove_dir_all(&test_root).expect("test project should clean up");
    }

    #[test]
    fn resolve_pii_model_dir_candidates_prefers_configured_order() {
        let root = temp_root("scriptscore-pii-model-order");
        let env_dir = root.join("env");
        let settings_dir = root.join("settings");
        let bundled_dir = root.join("bundled");
        let dev_dir = root.join("dev");
        fs::create_dir_all(&env_dir).expect("env dir should exist");
        fs::create_dir_all(&settings_dir).expect("settings dir should exist");
        fs::create_dir_all(&bundled_dir).expect("bundled dir should exist");
        fs::create_dir_all(&dev_dir).expect("dev dir should exist");

        let resolved = resolve_pii_model_dir_candidates([
            Some(env_dir.clone()),
            Some(settings_dir.clone()),
            Some(bundled_dir.clone()),
            Some(dev_dir.clone()),
        ])
        .expect("env override should win");
        assert_eq!(resolved, env_dir);

        let resolved = resolve_pii_model_dir_candidates([
            Some(root.join("missing-env")),
            Some(settings_dir.clone()),
            Some(bundled_dir.clone()),
            Some(dev_dir.clone()),
        ])
        .expect("settings override should win when env is absent");
        assert_eq!(resolved, settings_dir);

        let resolved = resolve_pii_model_dir_candidates([
            Some(root.join("missing-env")),
            Some(root.join("missing-settings")),
            Some(bundled_dir.clone()),
            Some(dev_dir.clone()),
        ])
        .expect("bundled resource should win when env/settings are absent");
        assert_eq!(resolved, bundled_dir);

        let resolved = resolve_pii_model_dir_candidates([
            Some(root.join("missing-env")),
            Some(root.join("missing-settings")),
            Some(root.join("missing-bundled")),
            Some(dev_dir.clone()),
        ])
        .expect("dev checkout should remain the final fallback");
        assert_eq!(resolved, dev_dir);
    }

    #[test]
    fn resolve_pii_model_dir_candidates_errors_when_missing_everywhere() {
        let root = temp_root("scriptscore-pii-model-missing");
        let error = resolve_pii_model_dir_candidates([
            Some(root.join("missing-env")),
            Some(root.join("missing-settings")),
            Some(root.join("missing-bundled")),
            Some(root.join("missing-dev")),
        ])
        .expect_err("missing model dirs should fail");

        assert!(error
            .to_string()
            .contains("Paddle model directory was not found"));
    }
}
