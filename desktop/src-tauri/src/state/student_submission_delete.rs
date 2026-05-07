// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::errors::{HostError, HostResult};
use crate::models::{
    DeleteStudentSubmissionInput, ExamWorkspaceState, StudentIntakeSummary,
    StudentWorkflowSubmission,
};
use crate::project_store;

pub(crate) fn delete_student_submission(
    project_path: &Path,
    input: DeleteStudentSubmissionInput,
) -> HostResult<ExamWorkspaceState> {
    let student_ref = input.student_ref.trim();
    if student_ref.is_empty() {
        return Err(HostError::Validation("studentRef is required.".into()));
    }

    let mut intake = project_store::load_student_intake_state(project_path)?;
    let mut workflow = project_store::load_student_workflow_state(project_path)?;
    let mut moderation = project_store::load_moderation_state(project_path)?;
    let mut results = project_store::load_results_lms_state(project_path)?;

    let removed_intake = remove_intake_item(&mut intake.items, student_ref);
    let removed_workflow = remove_workflow_submission(&mut workflow.submissions, student_ref);
    if removed_intake.is_none() && removed_workflow.is_none() {
        return Err(HostError::Validation(format!(
            "Student submission '{student_ref}' was not found."
        )));
    }

    moderation
        .score_overrides
        .retain(|entry| entry.student_ref != student_ref);
    moderation
        .feedback_overrides
        .retain(|entry| entry.student_ref != student_ref);
    results
        .finalization_records
        .retain(|record| record.student_ref != student_ref);
    results
        .asset_bindings
        .retain(|binding| binding.student_ref != student_ref);

    let project_owned_paths = project_owned_artifact_paths(
        project_path,
        removed_intake.as_ref(),
        removed_workflow.as_ref(),
    );

    project_store::save_student_intake_state(project_path, &intake)?;
    project_store::save_student_workflow_state(project_path, &workflow)?;
    project_store::save_moderation_state(project_path, &moderation)?;
    project_store::save_results_lms_state(project_path, &results)?;
    delete_project_owned_paths_best_effort(&project_owned_paths);
    delete_student_report_asset_dir_best_effort(project_path, student_ref);

    project_store::load_exam_workspace_state(project_path)
}

fn remove_intake_item(
    items: &mut Vec<StudentIntakeSummary>,
    student_ref: &str,
) -> Option<StudentIntakeSummary> {
    items
        .iter()
        .position(|item| item.student_ref == student_ref)
        .map(|index| items.remove(index))
}

fn remove_workflow_submission(
    submissions: &mut Vec<StudentWorkflowSubmission>,
    student_ref: &str,
) -> Option<StudentWorkflowSubmission> {
    submissions
        .iter()
        .position(|submission| submission.student_ref == student_ref)
        .map(|index| submissions.remove(index))
}

fn project_owned_artifact_paths(
    project_path: &Path,
    intake: Option<&StudentIntakeSummary>,
    workflow: Option<&StudentWorkflowSubmission>,
) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    intake_artifact_paths(intake)
        .into_iter()
        .chain(workflow_artifact_paths(workflow))
        .filter(|path| is_under_project_artifacts(project_path, path))
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

fn intake_artifact_paths(intake: Option<&StudentIntakeSummary>) -> Vec<PathBuf> {
    let Some(intake) = intake else {
        return Vec::new();
    };
    let mut paths = vec![PathBuf::from(&intake.canonical_pdf_path)];
    paths.extend(intake.exam_page_paths.iter().map(PathBuf::from));
    paths
}

fn workflow_artifact_paths(workflow: Option<&StudentWorkflowSubmission>) -> Vec<PathBuf> {
    let Some(workflow) = workflow else {
        return Vec::new();
    };
    let mut paths = vec![PathBuf::from(&workflow.canonical_pdf_path)];
    paths.extend(workflow.page_artifacts.iter().flat_map(workflow_page_paths));
    paths.extend(
        workflow
            .answers
            .iter()
            .filter_map(|answer| answer.crop_image_path.as_ref())
            .map(PathBuf::from),
    );
    paths
}

fn workflow_page_paths(page: &crate::models::StudentWorkflowPage) -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from(&page.image_path)];
    if let Some(path) = &page.ocr_metadata_path {
        paths.push(PathBuf::from(path));
    }
    paths
}

fn is_under_project_artifacts(project_path: &Path, path: &Path) -> bool {
    let artifacts_root = project_path.join(crate::project_store::schema::ARTIFACTS_DIR_NAME);
    crate::path_utils::is_under_existing_dir(&artifacts_root, path)
}

fn delete_project_owned_paths_best_effort(paths: &[PathBuf]) {
    for path in paths {
        if path.is_file() {
            let _ = fs::remove_file(path);
        }
    }
}

fn delete_student_report_asset_dir_best_effort(project_path: &Path, student_ref: &str) {
    let sanitized = student_ref
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let path = project_path
        .join(crate::project_store::schema::ARTIFACTS_DIR_NAME)
        .join("results_lms")
        .join("report_images")
        .join(sanitized);
    if path.is_dir() {
        let _ = fs::remove_dir_all(path);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use rusqlite::{params, Connection};

    use crate::models::{
        DeleteStudentSubmissionInput, LmsUploadStudentStatus, ModerationFeedbackOverride,
        ModerationScoreOverride, ModerationState, ResultFinalizationRecord, ResultsLmsAssetBinding,
        ResultsLmsState, StudentIntakeState, StudentIntakeSummary, StudentWorkflowAnswer,
        StudentWorkflowPage, StudentWorkflowState, StudentWorkflowSubmission,
    };
    use crate::project_store::schema::{initialize_schema, project_db_path};
    use crate::project_store::{
        load_moderation_state, load_results_lms_state, load_student_intake_state,
        load_student_workflow_state, save_moderation_state, save_results_lms_state,
        save_student_intake_state, save_student_workflow_state,
    };
    use crate::test_support::lock_env_vars;

    use super::delete_student_submission;

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
            Connection::open(project_db_path(project_path)).expect("project db should open");
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
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, '{}', '{}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
                params![
                    "proj_test",
                    "Midterm 1",
                    Option::<String>::None,
                    Option::<String>::None,
                    Option::<String>::None,
                    true
                ],
            )
            .expect("project should insert");
    }

    fn intake_summary(project_path: &std::path::Path, student_ref: &str) -> StudentIntakeSummary {
        let artifact_dir = project_path.join("artifacts").join("student_intake");
        std::fs::create_dir_all(&artifact_dir).expect("artifact dir should exist");
        let canonical = artifact_dir.join(format!("{student_ref}.pdf"));
        let page = artifact_dir.join(format!("{student_ref}-page-1.png"));
        std::fs::write(&canonical, b"pdf").expect("canonical pdf should write");
        std::fs::write(&page, b"png").expect("page png should write");
        StudentIntakeSummary {
            student_ref: student_ref.into(),
            local_display_name: None,
            canonical_pdf_path: canonical.to_string_lossy().into_owned(),
            ingest_status: "ok".into(),
            page_count: 1,
            exam_page_paths: vec![page.to_string_lossy().into_owned()],
            warnings: Vec::new(),
            binding_token_hex: None,
        }
    }

    fn workflow_submission(
        project_path: &std::path::Path,
        student_ref: &str,
    ) -> StudentWorkflowSubmission {
        let artifact_dir = project_path.join("artifacts").join("workflow");
        std::fs::create_dir_all(&artifact_dir).expect("artifact dir should exist");
        let canonical = artifact_dir.join(format!("{student_ref}-canonical.pdf"));
        let page = artifact_dir.join(format!("{student_ref}-canonical-page.png"));
        let crop = artifact_dir.join(format!("{student_ref}-q1-crop.png"));
        let external = std::env::temp_dir().join(format!("{student_ref}-external-source.pdf"));
        std::fs::write(&canonical, b"pdf").expect("canonical pdf should write");
        std::fs::write(&page, b"png").expect("page png should write");
        std::fs::write(&crop, b"png").expect("crop png should write");
        std::fs::write(&external, b"raw").expect("external source should write");
        StudentWorkflowSubmission {
            student_ref: student_ref.into(),
            canonical_pdf_path: canonical.to_string_lossy().into_owned(),
            page_count: 1,
            stage: "graded".into(),
            latest_job_id: None,
            failure_message: None,
            warnings: Vec::new(),
            page_artifacts: vec![StudentWorkflowPage {
                page_number: 1,
                image_path: page.to_string_lossy().into_owned(),
                source_pdf_path: Some(external.to_string_lossy().into_owned()),
                ocr_metadata_path: None,
            }],
            alignment_pages: Vec::new(),
            detect_review: None,
            answers: vec![StudentWorkflowAnswer {
                question_id: "question_1".into(),
                question_number: 1,
                crop_image_path: Some(crop.to_string_lossy().into_owned()),
                pii_prescreen: None,
                manual_grading_required: false,
                manual_grading_reason: None,
                moderation_eligible: true,
                parse_status: "ok".into(),
                parse_confidence: None,
                parse_confidence_source: None,
                raw_parsed_text: Some("answer".into()),
                verified_text: Some("answer".into()),
                review_required: false,
                verified: true,
                stale: false,
                grading_status: "draft_ready".into(),
                grading_confidence: None,
                grading_confidence_reason: None,
                question_max_points: Some(5),
                total_points_awarded: Some(4),
                feedback_text: Some("Good".into()),
                criterion_results: Vec::new(),
                highlights: Vec::new(),
                warnings: Vec::new(),
            }],
        }
    }

    struct DeletionFixture {
        project_path: PathBuf,
        target_canonical: String,
        target_page: String,
        target_workflow_canonical: String,
        target_workflow_page: String,
        target_crop: String,
        external_source: String,
    }

    fn fixture_with_two_students() -> DeletionFixture {
        let project_path = temp_root("scriptscore-delete-student");
        bootstrap_project(&project_path);
        let target_intake = intake_summary(&project_path, "student_1");
        let keep_intake = intake_summary(&project_path, "student_2");
        let target_workflow = workflow_submission(&project_path, "student_1");
        let keep_workflow = workflow_submission(&project_path, "student_2");
        let fixture = DeletionFixture {
            project_path: project_path.clone(),
            target_canonical: target_intake.canonical_pdf_path.clone(),
            target_page: target_intake.exam_page_paths[0].clone(),
            target_workflow_canonical: target_workflow.canonical_pdf_path.clone(),
            target_workflow_page: target_workflow.page_artifacts[0].image_path.clone(),
            target_crop: target_workflow.answers[0].crop_image_path.clone().unwrap(),
            external_source: target_workflow.page_artifacts[0]
                .source_pdf_path
                .clone()
                .unwrap(),
        };
        save_two_student_current_state(
            &project_path,
            target_intake,
            keep_intake,
            target_workflow,
            keep_workflow,
        );
        save_student_scoped_project_state(&project_path);
        fixture
    }

    fn save_two_student_current_state(
        project_path: &std::path::Path,
        target_intake: StudentIntakeSummary,
        keep_intake: StudentIntakeSummary,
        target_workflow: StudentWorkflowSubmission,
        keep_workflow: StudentWorkflowSubmission,
    ) {
        save_student_intake_state(
            project_path,
            &StudentIntakeState {
                status: "ready".into(),
                latest_job_id: None,
                items: vec![target_intake, keep_intake],
                unresolved_count: 0,
            },
        )
        .expect("intake state should save");
        save_student_workflow_state(
            project_path,
            &StudentWorkflowState {
                status: "graded".into(),
                latest_job_id: None,
                submissions: vec![target_workflow, keep_workflow],
            },
        )
        .expect("workflow state should save");
    }

    fn save_student_scoped_project_state(project_path: &std::path::Path) {
        save_moderation_state(
            project_path,
            &ModerationState {
                score_overrides: vec![
                    ModerationScoreOverride {
                        student_ref: "student_1".into(),
                        question_id: "question_1".into(),
                        moderated_total_points: 5,
                    },
                    ModerationScoreOverride {
                        student_ref: "student_2".into(),
                        question_id: "question_1".into(),
                        moderated_total_points: 3,
                    },
                ],
                feedback_overrides: vec![ModerationFeedbackOverride {
                    student_ref: "student_1".into(),
                    question_id: "question_1".into(),
                    feedback_text: "Edited".into(),
                }],
                question_reviews: Vec::new(),
            },
        )
        .expect("moderation state should save");
        save_results_lms_state(project_path, &results_state_with_student_refs())
            .expect("results state should save");
    }

    fn results_state_with_student_refs() -> ResultsLmsState {
        ResultsLmsState {
            finalization_records: vec![ResultFinalizationRecord {
                student_ref: "student_1".into(),
                result_fingerprint: "fingerprint_1".into(),
                finalized_at: "1".into(),
            }],
            asset_bindings: vec![
                ResultsLmsAssetBinding {
                    provider: "canvas".into(),
                    course_id: "course_1".into(),
                    assignment_id: "assignment_1".into(),
                    student_ref: "student_1".into(),
                    local_asset_name: "q1.jpg".into(),
                    asset_fingerprint: "asset_1".into(),
                    provider_file_id: "file_1".into(),
                },
                ResultsLmsAssetBinding {
                    provider: "canvas".into(),
                    course_id: "course_1".into(),
                    assignment_id: "assignment_1".into(),
                    student_ref: "student_2".into(),
                    local_asset_name: "q1.jpg".into(),
                    asset_fingerprint: "asset_2".into(),
                    provider_file_id: "file_2".into(),
                },
            ],
            upload_attempts: vec![crate::models::LmsUploadAttemptResult {
                attempt_id: "attempt_1".into(),
                mode: Default::default(),
                provider: "canvas".into(),
                course_id: "course_1".into(),
                assignment_id: "assignment_1".into(),
                started_at: "1".into(),
                finished_at: "2".into(),
                attempted_count: 1,
                success_count: 1,
                failure_count: 0,
                student_results: vec![crate::models::LmsUploadStudentResult {
                    student_ref: "student_1".into(),
                    result_fingerprint: "fingerprint_1".into(),
                    status: LmsUploadStudentStatus::Uploaded,
                    sanitized_error: None,
                }],
            }],
            ..ResultsLmsState::default()
        }
    }

    fn assert_only_student_two_remains(workspace: &crate::models::ExamWorkspaceState) {
        assert_eq!(workspace.student_intake.items.len(), 1);
        assert_eq!(workspace.student_intake.items[0].student_ref, "student_2");
        assert_eq!(workspace.student_workflow.submissions.len(), 1);
        assert_eq!(
            workspace.student_workflow.submissions[0].student_ref,
            "student_2"
        );
    }

    fn assert_project_scoped_student_refs_removed(project_path: &std::path::Path) {
        assert!(load_moderation_state(project_path)
            .expect("moderation should load")
            .score_overrides
            .iter()
            .all(|entry| entry.student_ref != "student_1"));
        let results = load_results_lms_state(project_path).expect("results should load");
        assert!(results
            .finalization_records
            .iter()
            .all(|record| record.student_ref != "student_1"));
        assert!(results
            .asset_bindings
            .iter()
            .all(|binding| binding.student_ref != "student_1"));
        assert_eq!(results.upload_attempts.len(), 1);
    }

    fn assert_project_owned_files_removed(fixture: &DeletionFixture) {
        assert!(!PathBuf::from(&fixture.target_canonical).exists());
        assert!(!PathBuf::from(&fixture.target_page).exists());
        assert!(!PathBuf::from(&fixture.target_workflow_canonical).exists());
        assert!(!PathBuf::from(&fixture.target_workflow_page).exists());
        assert!(!PathBuf::from(&fixture.target_crop).exists());
        assert!(PathBuf::from(&fixture.external_source).exists());
    }

    fn assert_persisted_student_scoped_rows_rewritten(project_path: &std::path::Path) {
        assert_eq!(
            load_student_intake_state(project_path)
                .expect("intake should load")
                .items
                .len(),
            1
        );
        assert_eq!(
            load_student_workflow_state(project_path)
                .expect("workflow should load")
                .submissions
                .len(),
            1
        );
    }

    #[test]
    fn deletes_student_submission_current_state_and_project_scoped_refs() {
        let _guard = lock_env_vars();
        let fixture = fixture_with_two_students();

        let workspace = delete_student_submission(
            &fixture.project_path,
            DeleteStudentSubmissionInput {
                student_ref: "student_1".into(),
            },
        )
        .expect("delete should succeed");

        assert_only_student_two_remains(&workspace);
        assert_project_scoped_student_refs_removed(&fixture.project_path);
        assert_project_owned_files_removed(&fixture);
        assert_persisted_student_scoped_rows_rewritten(&fixture.project_path);

        std::fs::remove_dir_all(&fixture.project_path).expect("test project should clean up");
    }

    #[test]
    fn deleting_missing_student_returns_validation_error() {
        let _guard = lock_env_vars();
        let project_path = temp_root("scriptscore-delete-missing-student");
        bootstrap_project(&project_path);

        let err = delete_student_submission(
            &project_path,
            DeleteStudentSubmissionInput {
                student_ref: "student_missing".into(),
            },
        )
        .expect_err("missing student should error");

        assert!(err.to_string().contains("was not found"));
        std::fs::remove_dir_all(&project_path).expect("test project should clean up");
    }
}
