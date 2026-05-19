// SPDX-License-Identifier: AGPL-3.0-only
use super::{
    apply_alignment_results, apply_canonicalize_batch_results, apply_detect_batch_results,
    apply_parse_batch_results, apply_pii_batch_results, build_batch_answer_score_requests,
    build_canonicalize_batch_plan, build_canonicalize_batch_targets, build_crop_targets,
    build_detect_review, build_regrade_answer_score_requests, classify_batch_resume_points,
    completed_was_cancelled, crop_result_rows_for_student, crop_rows_from_answers,
    crop_targets_for_cli, detect_batch_command_error, emit_ready_for_empty_batch_continuation,
    ensure_submission_row, failed_resume_point, feedback_request_rows,
    filtered_completed_by_student, finish_batch_grading, finish_question_regrade,
    grading_rows_or_stop, intake_map, mark_refs_failed_with_message, mark_refs_stopped_and_emit,
    mark_refs_stopped_for_error, persist_batch_feedback_rows, persist_batch_preliminary_rows,
    pii_identity_unavailable_warning, preliminary_grading_request_payload,
    preliminary_grading_runtime_config, prepare_pii_batch_inputs, record_submission_job_id,
    reset_submission_for_restart, resolve_pii_model_dir_candidates, rows_for_student,
    sanitized_pii_batch_request_payload, sanitized_pii_request_payload, save_workflow_state,
    save_workflow_state_and_emit, settle_empty_grading_refs, sorted_keys,
    split_detect_refs_by_reusable_crop_targets, FailedResumePoint,
};
use crate::models::{
    AppSettings, ExamWorkspaceState, InstructorProfile, ProjectConfig, ProjectSummary,
    QuestionRecord, RuntimeJobEvent, StudentIntakeState, StudentIntakeSummary,
    StudentWorkflowAlignmentPage, StudentWorkflowAnswer, StudentWorkflowDetectRegion,
    StudentWorkflowDetectReview, StudentWorkflowDetectReviewRow, StudentWorkflowHighlightSpan,
    StudentWorkflowPage, StudentWorkflowState, StudentWorkflowSubmission, StudentWorkflowTransform,
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
    let image = ImageBuffer::from_fn(width, height, |_x, _y| Rgba([32_u8, 64_u8, 96_u8, 255_u8]));
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
    submission.answers[0].stale = true;
    assert_eq!(
        failed_resume_point(&submission),
        FailedResumePoint::AfterPii
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
    let mut after_pii = workflow_submission("student_5");
    after_pii.answers = vec![answer_seed("q1")];
    after_pii.answers[0].verified = true;
    after_pii.answers[0].parse_status = "ok".into();
    after_pii.answers[0].stale = true;
    from_start.stage = "failed".into();
    after_alignment.stage = "failed".into();
    after_canonicalize.stage = "failed".into();
    after_parse.stage = "failed".into();
    after_pii.stage = "failed".into();
    let mut state = StudentWorkflowState {
        status: "attention".into(),
        latest_job_id: None,
        submissions: vec![
            from_start,
            after_alignment,
            after_canonicalize,
            after_parse,
            after_pii,
        ],
    };

    let plan = classify_batch_resume_points(
        &mut state,
        vec![
            "student_1".into(),
            "student_2".into(),
            "student_3".into(),
            "student_4".into(),
            "student_5".into(),
        ],
    )
    .expect("resume points should classify");

    assert_eq!(plan.from_start, vec!["student_1"]);
    assert_eq!(plan.after_alignment, vec!["student_2"]);
    assert_eq!(plan.after_canonicalize, vec!["student_3"]);
    assert_eq!(plan.after_pii, vec!["student_5"]);
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
fn canonicalize_batch_plan_fails_unmatched_pages_without_blocking_valid_students() {
    let workspace = minimal_workspace_with_template_pages();
    let mut intake_by_ref = HashMap::new();
    intake_by_ref.insert("student_1".to_string(), intake_summary("student_1"));
    intake_by_ref.insert("student_2".to_string(), intake_summary("student_2"));
    let mut valid = workflow_submission("student_1");
    valid.stage = "canonicalize".into();
    valid.alignment_pages = vec![alignment_page(1), alignment_page(2)];
    let mut extra_page = workflow_submission("student_2");
    extra_page.stage = "canonicalize".into();
    extra_page.alignment_pages = vec![alignment_page(1), alignment_page(3)];
    let mut state = StudentWorkflowState {
        status: "running".into(),
        latest_job_id: None,
        submissions: vec![valid, extra_page],
    };

    let plan = build_canonicalize_batch_plan(
        &workspace,
        &intake_by_ref,
        &mut state,
        &["student_1".to_string(), "student_2".to_string()],
    )
    .expect("canonicalize plan should isolate per-student target errors");

    assert_eq!(plan.runnable_refs, vec!["student_1"]);
    assert_eq!(plan.failed_refs, vec!["student_2"]);
    assert_eq!(plan.targets.len(), 2);
    assert_eq!(state.submissions[0].stage, "canonicalize");
    assert_eq!(state.submissions[1].stage, "failed");
    assert_eq!(
        state.submissions[1].failure_message.as_deref(),
        Some("Template page '3' was missing for canonicalization.")
    );
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

    let err =
        detect_batch_command_error(&completed).expect("detect command error should be recognized");
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
fn missing_parse_results_stops_affected_ref_without_host_failure() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-parse-batch-missing-results");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    let project_path = create_project(&test_root);
    let sink = RecordingEventSink::default();
    let mut submission = workflow_submission("student_1");
    submission.stage = "parse".into();
    submission.answers = vec![answer_seed("q1")];
    let mut workflow_state = StudentWorkflowState {
        status: "running".into(),
        latest_job_id: None,
        submissions: vec![submission],
    };
    let completed = CompletedWorkerJob {
        job_id: "parse-batch-missing".into(),
        result: WorkerJobResult {
            terminal_type: "job_finished".into(),
            terminal_payload: json!({}),
            envelope: json!({"data": {}}),
            events: Vec::new(),
        },
    };
    let refs = vec!["student_1".to_string()];

    let grading_refs =
        apply_parse_batch_results(&project_path, &sink, &mut workflow_state, &refs, &completed)
            .expect("missing parse rows should be handled as stopped");

    assert!(grading_refs.is_empty());
    assert_eq!(workflow_state.submissions[0].stage, "stopped");
    assert!(workflow_state.submissions[0]
        .failure_message
        .as_deref()
        .unwrap_or_default()
        .contains("Command result was missing parse_results."));
    assert_eq!(sink.snapshot()[0].payload["workflowStatus"], "ready");

    let _ = std::fs::remove_dir_all(&test_root);
}

#[test]
fn malformed_crop_success_data_stops_ref_with_protocol_message() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-crop-malformed-success");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    let project_path = create_project(&test_root);
    let sink = RecordingEventSink::default();
    let refs = vec!["student_1".to_string()];
    let mut workflow_state = StudentWorkflowState {
        status: "running".into(),
        latest_job_id: None,
        submissions: vec![workflow_submission("student_1")],
    };
    let completed = CompletedWorkerJob {
        job_id: "crop-malformed".into(),
        result: WorkerJobResult {
            terminal_type: "job_finished".into(),
            terminal_payload: json!({}),
            envelope: json!({}),
            events: Vec::new(),
        },
    };

    let err = crop_result_rows_for_student(&completed)
        .expect_err("malformed crop success should be protocol error");
    mark_refs_stopped_for_error(&project_path, &mut workflow_state, &sink, &refs, &err)
        .expect("protocol error should stop refs");

    assert_eq!(workflow_state.submissions[0].stage, "stopped");
    assert!(workflow_state.submissions[0]
        .failure_message
        .as_deref()
        .unwrap_or_default()
        .contains("Command success envelope was missing data"));

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
    let trigger_words_by_ref = HashMap::from([("student_3".to_string(), vec!["Ada".to_string()])]);

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
fn missing_pii_result_for_active_student_stops_ref() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-pii-batch-missing-student-row");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    let project_path = create_project(&test_root);
    let sink = RecordingEventSink::default();
    let workspace = minimal_workspace_with_question();
    let mut submission = workflow_submission("student_1");
    submission.stage = "pii".into();
    let mut state = StudentWorkflowState {
        status: "running".into(),
        latest_job_id: None,
        submissions: vec![submission],
    };
    let crop_rows_by_ref = HashMap::from([(
        "student_1".to_string(),
        vec![json!({
            "student_ref": "student_1",
            "question_id": "q1",
            "status": "ok",
            "question_crop_path": "/tmp/student_1-q1.png"
        })],
    )]);
    let completed = CompletedWorkerJob {
        job_id: "pii-batch-missing-row".into(),
        result: WorkerJobResult {
            terminal_type: "job_finished".into(),
            terminal_payload: json!({}),
            envelope: json!({"data": {"pii_results": []}}),
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
    .expect("missing active pii result should stop the row");

    assert!(parse_refs.is_empty());
    assert_eq!(state.submissions[0].stage, "stopped");
    assert!(state.submissions[0]
        .failure_message
        .as_deref()
        .unwrap_or_default()
        .contains("Command result was missing pii_results rows for student 'student_1'."));
    assert_eq!(sink.snapshot()[0].payload["workflowStatus"], "ready");

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

#[test]
fn missing_grading_rows_key_stops_ready_refs() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-batch-grading-missing-rows");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    let project_path = create_project(&test_root);
    let sink = RecordingEventSink::default();
    let refs = vec!["student_1".to_string()];
    let mut submission = workflow_submission("student_1");
    submission.stage = "grading".into();
    let mut workflow_state = StudentWorkflowState {
        status: "running".into(),
        latest_job_id: None,
        submissions: vec![submission],
    };
    let completed = CompletedWorkerJob {
        job_id: "grading-missing-rows".into(),
        result: WorkerJobResult {
            terminal_type: "job_finished".into(),
            terminal_payload: json!({}),
            envelope: json!({"data": {}}),
            events: Vec::new(),
        },
    };

    let rows = grading_rows_or_stop(
        &project_path,
        &sink,
        &mut workflow_state,
        &refs,
        &completed,
        "preliminary_scores",
    )
    .expect("missing grading rows should be handled");

    assert!(rows.is_empty());
    assert_eq!(workflow_state.submissions[0].stage, "stopped");
    assert!(workflow_state.submissions[0]
        .failure_message
        .as_deref()
        .unwrap_or_default()
        .contains("Command result was missing preliminary_scores."));

    let _ = std::fs::remove_dir_all(&test_root);
}

#[test]
fn regrade_request_builder_only_includes_graded_submissions() {
    let workspace = minimal_workspace_with_question();
    let question_by_id = workspace
        .questions
        .iter()
        .map(|question| (question.question_id.clone(), question))
        .collect::<HashMap<_, _>>();
    let mut graded = workflow_submission("student_graded");
    graded.stage = "graded".into();
    graded.answers = vec![answer_seed("q1")];
    graded.answers[0].verified = true;
    graded.answers[0].verified_text = Some("graded stale answer".into());
    graded.answers[0].stale = true;
    let mut parse_review = workflow_submission("student_parse_review");
    parse_review.stage = "parse_review".into();
    parse_review.answers = vec![answer_seed("q1")];
    parse_review.answers[0].verified = true;
    parse_review.answers[0].verified_text = Some("pending review stale answer".into());
    parse_review.answers[0].stale = true;
    let mut grading = workflow_submission("student_grading");
    grading.stage = "grading".into();
    grading.answers = vec![answer_seed("q1")];
    grading.answers[0].verified = true;
    grading.answers[0].verified_text = Some("in-flight stale answer".into());
    grading.answers[0].stale = true;
    let mut state = StudentWorkflowState {
        status: "attention".into(),
        latest_job_id: None,
        submissions: vec![graded, parse_review, grading],
    };

    let requests =
        build_regrade_answer_score_requests(&workspace, &mut state, &question_by_id, "q1")
            .expect("regrade score requests should build");

    assert_eq!(requests.len(), 1);
    assert_eq!(requests["student_graded"].len(), 1);
    assert!(!requests.contains_key("student_parse_review"));
    assert!(!requests.contains_key("student_grading"));
}

#[test]
fn question_regrade_preserves_other_answer_highlights() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-question-regrade");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    let project_path = create_project(&test_root);
    let sink = RecordingEventSink::default();
    let mut q1 = answer_seed("q1");
    q1.verified = true;
    q1.verified_text = Some("Updated answer".into());
    q1.stale = true;
    q1.question_max_points = Some(3);
    let mut q2 = answer_seed("q2");
    q2.verified = true;
    q2.verified_text = Some("Already graded answer".into());
    q2.grading_status = "draft_ready".into();
    q2.total_points_awarded = Some(4);
    q2.highlights = vec![StudentWorkflowHighlightSpan {
        kind: "strength".into(),
        start_char: 0,
        end_char: 7,
        text: "Already".into(),
    }];
    let mut submission = workflow_submission("student_1");
    submission.stage = "grading".into();
    submission.answers = vec![q1, q2];
    let mut state = StudentWorkflowState {
        status: "running".into(),
        latest_job_id: None,
        submissions: vec![submission],
    };
    let refs = vec!["student_1".to_string()];
    let final_rows = vec![json!({
        "student_ref": "student_1",
        "question_id": "q1",
        "criterion_results": [],
        "total_points_awarded": 5,
        "question_max_points": 6
    })];
    let feedback_rows = vec![json!({
        "student_ref": "student_1",
        "question_id": "q1",
        "feedback_text": "Updated feedback."
    })];
    let highlight_rows = vec![json!({
        "student_ref": "student_1",
        "question_id": "q1",
        "highlights": [
            {
                "kind": "strength",
                "start_char": 0,
                "end_char": 7,
                "text": "Updated"
            }
        ]
    })];

    finish_question_regrade(
        &project_path,
        &sink,
        &mut state,
        &refs,
        final_rows,
        &feedback_rows,
        &highlight_rows,
    )
    .expect("question regrade should finish");

    let answers = &state.submissions[0].answers;
    assert_eq!(state.submissions[0].stage, "graded");
    assert!(!answers[0].stale);
    assert_eq!(answers[0].total_points_awarded, Some(5));
    assert_eq!(answers[0].question_max_points, Some(6));
    assert_eq!(
        answers[0].feedback_text.as_deref(),
        Some("Updated feedback.")
    );
    assert_eq!(answers[0].highlights[0].text, "Updated");
    assert_eq!(answers[1].total_points_awarded, Some(4));
    assert_eq!(answers[1].highlights[0].text, "Already");

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
    write_paddle_model_layout(&env_dir);
    write_paddle_model_layout(&settings_dir);
    write_paddle_model_layout(&bundled_dir);
    write_paddle_model_layout(&dev_dir);

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
fn resolve_pii_model_dir_candidates_skips_incomplete_existing_dirs() {
    let root = temp_root("scriptscore-pii-model-incomplete");
    let incomplete_dir = root.join("incomplete");
    let complete_dir = root.join("complete");
    fs::create_dir_all(&incomplete_dir).expect("incomplete model root should exist");
    write_paddle_model_layout(&complete_dir);

    let resolved = resolve_pii_model_dir_candidates([
        Some(incomplete_dir),
        Some(complete_dir.clone()),
        None,
        None,
    ])
    .expect("complete model dir should win");

    assert_eq!(resolved, complete_dir);
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

#[test]
fn pii_model_resolution_error_stops_active_pii_rows_with_fix_message() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-pii-model-stop");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    let project_path = create_project(&test_root);
    let sink = RecordingEventSink::default();
    let refs = vec!["student_1".to_string()];
    let mut submission = workflow_submission("student_1");
    submission.stage = "pii".into();
    let mut workflow_state = StudentWorkflowState {
        status: "running".into(),
        latest_job_id: None,
        submissions: vec![submission],
    };
    let error = resolve_pii_model_dir_candidates([
        Some(test_root.join("missing-env")),
        Some(test_root.join("missing-settings")),
        Some(test_root.join("missing-bundled")),
        Some(test_root.join("missing-dev")),
    ])
    .expect_err("missing model dirs should fail");

    mark_refs_stopped_for_error(&project_path, &mut workflow_state, &sink, &refs, &error)
        .expect("missing model error should stop active pii rows");

    assert_eq!(workflow_state.submissions[0].stage, "stopped");
    let message = workflow_state.submissions[0]
        .failure_message
        .as_deref()
        .unwrap_or_default();
    assert!(message.contains("Paddle model directory was not found"));
    assert!(message.contains("SCRIPTSCORE_PII_PADDLE_MODEL_DIR"));
    assert_eq!(sink.snapshot()[0].payload["workflowStatus"], "ready");

    let _ = std::fs::remove_dir_all(&test_root);
}

#[cfg(windows)]
#[test]
fn worker_payload_path_strips_windows_verbatim_prefix() {
    use super::WorkerPayloadPath;

    let path =
        Path::new(r"\\?\C:\scriptscore\scriptscore\desktop\src-tauri\target\debug\models\paddle");

    assert_eq!(
        path.to_worker_payload_path(),
        r"C:\scriptscore\scriptscore\desktop\src-tauri\target\debug\models\paddle"
    );

    let unc_path = Path::new(r"\\?\UNC\server\share\models\paddle");
    assert_eq!(
        unc_path.to_worker_payload_path(),
        r"\\server\share\models\paddle"
    );
}

fn write_paddle_model_layout(root: &Path) {
    for name in ["det", "rec"] {
        let model_dir = root.join(name);
        fs::create_dir_all(&model_dir).expect("model dir should create");
        fs::write(model_dir.join("inference.pdmodel"), "model").expect("model should write");
        fs::write(model_dir.join("inference.pdiparams"), "params").expect("params should write");
    }
}
