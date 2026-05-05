// SPDX-License-Identifier: AGPL-3.0-only
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateSetupStatus {
    NotStarted,
    Running,
    Draft,
    Approved,
    Failed,
}

impl TemplateSetupStatus {
    pub fn from_storage(status: &str) -> Self {
        match status {
            "running" => Self::Running,
            "draft" => Self::Draft,
            "approved" => Self::Approved,
            "failed" => Self::Failed,
            _ => Self::NotStarted,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::Running => "running",
            Self::Draft => "draft",
            Self::Approved => "approved",
            Self::Failed => "failed",
        }
    }

    pub fn is_draft(&self) -> bool {
        matches!(self, Self::Draft)
    }

    pub fn can_approve(
        &self,
        has_questions: bool,
        has_redactions: bool,
        skip_redaction: bool,
    ) -> bool {
        self.is_draft() && has_questions && (has_redactions || skip_redaction)
    }

    pub fn workspace_status_label(&self, skip_redaction: bool, has_no_redactions: bool) -> String {
        match self {
            Self::Running => "Running template setup".into(),
            Self::Draft if skip_redaction && has_no_redactions => {
                "Redaction skipped, review questions".into()
            }
            Self::Draft => "Redaction review needed".into(),
            Self::Approved => "Template setup approved".into(),
            Self::Failed => "Template setup failed".into(),
            Self::NotStarted => "Template setup not started".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuestionAnalysisStatus {
    NotStarted,
    Ok,
    Error,
    Other(String),
}

impl QuestionAnalysisStatus {
    pub fn from_storage(status: &str) -> Self {
        match status {
            "not_started" => Self::NotStarted,
            "ok" => Self::Ok,
            "error" => Self::Error,
            other => Self::Other(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::NotStarted => "not_started",
            Self::Ok => "ok",
            Self::Error => "error",
            Self::Other(other) => other.as_str(),
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Ok)
    }

    pub fn is_editable(&self) -> bool {
        self.is_complete()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RubricStatus {
    NotStarted,
    Draft,
    Approved,
    Other(String),
}

impl RubricStatus {
    pub fn from_storage(status: &str) -> Self {
        match status {
            "not_started" => Self::NotStarted,
            "draft" => Self::Draft,
            "approved" => Self::Approved,
            other => Self::Other(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::NotStarted => "not_started",
            Self::Draft => "draft",
            Self::Approved => "approved",
            Self::Other(other) => other.as_str(),
        }
    }

    pub fn is_draft(&self) -> bool {
        matches!(self, Self::Draft)
    }

    pub fn is_approved(&self) -> bool {
        matches!(self, Self::Approved)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StudentIntakeStatus {
    NotStarted,
    Ready,
    Other(String),
}

impl StudentIntakeStatus {
    pub fn from_storage(status: &str) -> Self {
        match status {
            "not_started" => Self::NotStarted,
            "ready" => Self::Ready,
            other => Self::Other(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::NotStarted => "not_started",
            Self::Ready => "ready",
            Self::Other(other) => other.as_str(),
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectWorkflowStage {
    TemplateSetupNotStarted,
    TemplateSetupRunning,
    TemplateSetupFailed,
    RedactionReview,
    QuestionReview,
    RubricAuthoring,
    StudentIntakeReady,
    StudentWorkflowRunning,
    StudentWorkflowReview,
    StudentGrading,
    StudentGradingComplete,
    ResultsFinalizationPending,
    ResultsFinalized,
    ResultsUploadReady,
    ResultsUploadAttention,
    ResultsUploaded,
}

impl ProjectWorkflowStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TemplateSetupNotStarted => "template_setup_not_started",
            Self::TemplateSetupRunning => "template_setup_running",
            Self::TemplateSetupFailed => "template_setup_failed",
            Self::RedactionReview => "redaction_review",
            Self::QuestionReview => "question_review",
            Self::RubricAuthoring => "rubric_authoring",
            Self::StudentIntakeReady => "student_intake_ready",
            Self::StudentWorkflowRunning => "student_workflow_running",
            Self::StudentWorkflowReview => "student_workflow_review",
            Self::StudentGrading => "student_grading",
            Self::StudentGradingComplete => "student_grading_complete",
            Self::ResultsFinalizationPending => "results_finalization_pending",
            Self::ResultsFinalized => "results_finalized",
            Self::ResultsUploadReady => "results_upload_ready",
            Self::ResultsUploadAttention => "results_upload_attention",
            Self::ResultsUploaded => "results_uploaded",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::TemplateSetupNotStarted => "Template setup not started",
            Self::TemplateSetupRunning => "Running template setup",
            Self::TemplateSetupFailed => "Template setup failed",
            Self::RedactionReview => "Redaction review needed",
            Self::QuestionReview => "Question review in progress",
            Self::RubricAuthoring => "Rubric authoring in progress",
            Self::StudentIntakeReady => "Ready for student intake",
            Self::StudentWorkflowRunning => "Processing student submissions",
            Self::StudentWorkflowReview => "Student submission needs review",
            Self::StudentGrading => "Grading student submissions",
            Self::StudentGradingComplete => "Grading complete",
            Self::ResultsFinalizationPending => "Grading complete, results need finalization",
            Self::ResultsFinalized => "Results finalized",
            Self::ResultsUploadReady => "Results finalized and upload-ready",
            Self::ResultsUploadAttention => "Results upload needs attention",
            Self::ResultsUploaded => "Results uploaded",
        }
    }
}

pub fn derive_project_workflow_stage(
    template_setup_status: &TemplateSetupStatus,
    redaction_required: bool,
    has_redaction_regions: bool,
    skip_redaction_acknowledged: bool,
    questions: &[crate::models::QuestionRecord],
) -> ProjectWorkflowStage {
    if let Some(base_stage) = template_base_workflow_stage(template_setup_status) {
        return base_stage;
    }
    if !is_redaction_satisfied(
        redaction_required,
        has_redaction_regions,
        skip_redaction_acknowledged,
    ) {
        return ProjectWorkflowStage::RedactionReview;
    }
    if !all_questions_analyzed(questions) {
        return ProjectWorkflowStage::QuestionReview;
    }
    if !all_rubrics_approved(questions) {
        return ProjectWorkflowStage::RubricAuthoring;
    }
    ProjectWorkflowStage::StudentIntakeReady
}

#[derive(Clone, Copy, Debug, Default)]
struct SubmissionStageSummary {
    any_review: bool,
    any_failed: bool,
    any_manual_grading: bool,
    any_grading: bool,
    any_processing: bool,
    any_past_intake: bool,
    all_graded: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SubmissionStageKind {
    IntakeReady,
    Stopped,
    Review,
    Failed,
    ManualGrading,
    Grading,
    Graded,
    Processing,
}

impl SubmissionStageSummary {
    fn for_submissions(submissions: &[crate::models::StudentWorkflowSubmission]) -> Option<Self> {
        if submissions.is_empty() {
            return None;
        }
        Some(submissions.iter().fold(
            Self {
                all_graded: true,
                ..Self::default()
            },
            |mut summary, submission| {
                let kind = classify_submission_stage(submission.stage.as_str());
                summary.any_past_intake |= !matches!(
                    kind,
                    SubmissionStageKind::IntakeReady | SubmissionStageKind::Stopped
                );
                summary.all_graded &= kind == SubmissionStageKind::Graded;
                match kind {
                    SubmissionStageKind::IntakeReady | SubmissionStageKind::Stopped => {}
                    SubmissionStageKind::Review => summary.any_review = true,
                    SubmissionStageKind::Failed => summary.any_failed = true,
                    SubmissionStageKind::ManualGrading => summary.any_manual_grading = true,
                    SubmissionStageKind::Grading => {
                        summary.any_grading = true;
                        summary.any_processing = true;
                    }
                    SubmissionStageKind::Graded => {}
                    SubmissionStageKind::Processing => summary.any_processing = true,
                }
                summary
            },
        ))
    }

    fn project_stage(self) -> Option<ProjectWorkflowStage> {
        if self.any_review || self.any_failed || self.any_manual_grading {
            return Some(ProjectWorkflowStage::StudentWorkflowReview);
        }
        if self.all_graded {
            return Some(ProjectWorkflowStage::StudentGradingComplete);
        }
        if self.any_grading {
            return Some(ProjectWorkflowStage::StudentGrading);
        }
        if self.any_processing || self.any_past_intake {
            return Some(ProjectWorkflowStage::StudentWorkflowRunning);
        }
        None
    }
}

fn classify_submission_stage(stage: &str) -> SubmissionStageKind {
    match stage {
        "intake_ready" => SubmissionStageKind::IntakeReady,
        "stopped" => SubmissionStageKind::Stopped,
        "alignment_review" | "detect_review" | "parse_review" => SubmissionStageKind::Review,
        "failed" => SubmissionStageKind::Failed,
        "manual_grading" => SubmissionStageKind::ManualGrading,
        "grading" => SubmissionStageKind::Grading,
        "graded" => SubmissionStageKind::Graded,
        _ => SubmissionStageKind::Processing,
    }
}

/// Derive the project-level workflow stage from the student workflow state.
///
/// Decision tree (first match wins):
///
/// 1. Empty submissions → None (no post-intake override)
/// 2. Any alignment_review / detect_review / parse_review / manual_grading / failed → StudentWorkflowReview
/// 3. All graded → StudentGradingComplete
/// 4. Any grading in flight → StudentGrading
/// 5. Any other post-intake progress → StudentWorkflowRunning
/// 6. All intake_ready → None (no post-intake override)
pub fn derive_post_intake_workflow_stage(
    workflow_state: &crate::models::StudentWorkflowState,
) -> Option<ProjectWorkflowStage> {
    SubmissionStageSummary::for_submissions(&workflow_state.submissions)?.project_stage()
}

pub fn derive_results_workflow_stage(
    rows: &[crate::models::ResultStudentRow],
    assignment_selected: bool,
) -> Option<ProjectWorkflowStage> {
    if rows.is_empty() {
        return None;
    }

    if has_ready_unfinalized_rows(rows) {
        return Some(ProjectWorkflowStage::ResultsFinalizationPending);
    }

    let current_finalized = rows
        .iter()
        .filter(|row| row.finalized && !row.stale_finalization)
        .collect::<Vec<_>>();
    if current_finalized.is_empty() {
        return Some(ProjectWorkflowStage::ResultsFinalizationPending);
    }
    if current_finalized.iter().any(|row| row.upload_failed) {
        return Some(ProjectWorkflowStage::ResultsUploadAttention);
    }
    if current_finalized.iter().all(|row| row.uploaded) {
        return Some(ProjectWorkflowStage::ResultsUploaded);
    }
    if assignment_selected {
        return Some(ProjectWorkflowStage::ResultsUploadReady);
    }
    Some(ProjectWorkflowStage::ResultsFinalized)
}

fn has_ready_unfinalized_rows(rows: &[crate::models::ResultStudentRow]) -> bool {
    rows.iter()
        .any(|row| row.ready_to_finalize && (!row.finalized || row.stale_finalization))
}

fn template_base_workflow_stage(
    template_setup_status: &TemplateSetupStatus,
) -> Option<ProjectWorkflowStage> {
    match template_setup_status {
        TemplateSetupStatus::Failed => Some(ProjectWorkflowStage::TemplateSetupFailed),
        TemplateSetupStatus::Running => Some(ProjectWorkflowStage::TemplateSetupRunning),
        TemplateSetupStatus::NotStarted => Some(ProjectWorkflowStage::TemplateSetupNotStarted),
        TemplateSetupStatus::Draft | TemplateSetupStatus::Approved => None,
    }
}

fn is_redaction_satisfied(
    redaction_required: bool,
    has_redaction_regions: bool,
    skip_redaction_acknowledged: bool,
) -> bool {
    !redaction_required || skip_redaction_acknowledged || has_redaction_regions
}

fn all_questions_analyzed(questions: &[crate::models::QuestionRecord]) -> bool {
    !questions.is_empty() && questions.iter().all(|q| q.analysis.is_complete())
}

fn all_rubrics_approved(questions: &[crate::models::QuestionRecord]) -> bool {
    !questions.is_empty() && questions.iter().all(rubric_is_currently_approved)
}

fn rubric_is_currently_approved(question: &crate::models::QuestionRecord) -> bool {
    if !RubricStatus::from_storage(&question.rubric.status).is_approved() {
        return false;
    }
    let question_context = question
        .analysis
        .question_context
        .as_deref()
        .unwrap_or_default();
    if question
        .rubric
        .approval_matches(&question.text, question_context, question.max_points)
    {
        return true;
    }
    if !matches!(
        QuestionAnalysisStatus::from_storage(&question.analysis.status),
        QuestionAnalysisStatus::Ok
    ) {
        return false;
    }
    let Some(clean_text) = question.analysis.question_text_clean.as_deref() else {
        return false;
    };
    question
        .rubric
        .approval_matches(clean_text, question_context, question.max_points)
}

#[cfg(test)]
mod tests {
    use super::{
        derive_post_intake_workflow_stage, derive_project_workflow_stage,
        derive_results_workflow_stage, ProjectWorkflowStage, QuestionAnalysisStatus, RubricStatus,
        StudentIntakeStatus, TemplateSetupStatus,
    };
    use crate::models::{
        QuestionAnalysisState, QuestionRecord, RubricApprovalBasis, RubricCriterion, RubricState,
        StudentWorkflowState, StudentWorkflowSubmission,
    };

    #[test]
    fn template_setup_status_helpers_are_explicit() {
        let status = TemplateSetupStatus::from_storage("draft");
        assert!(status.is_draft());
        assert!(status.can_approve(true, false, true));
        assert_eq!(
            status.workspace_status_label(true, true),
            "Redaction skipped, review questions"
        );
    }

    #[test]
    fn question_analysis_status_tracks_unknown_values_without_panicking() {
        let status = QuestionAnalysisStatus::from_storage("partial");
        assert_eq!(status.as_str(), "partial");
        assert!(!status.is_complete());
        assert!(!status.is_editable());
    }

    #[test]
    fn rubric_and_intake_status_helpers_cover_known_states() {
        assert!(RubricStatus::from_storage("approved").is_approved());
        assert!(StudentIntakeStatus::from_storage("ready").is_ready());
    }

    #[test]
    fn project_workflow_stage_uses_approved_rubrics_as_intake_gate() {
        let mut question = sample_question();
        assert_eq!(
            derive_project_workflow_stage(&TemplateSetupStatus::Draft, true, false, false, &[]),
            ProjectWorkflowStage::RedactionReview
        );

        question.analysis.status = "ok".into();
        question.rubric.status = "approved".into();

        assert_eq!(
            derive_project_workflow_stage(
                &TemplateSetupStatus::Draft,
                true,
                true,
                false,
                &[question.clone()],
            ),
            ProjectWorkflowStage::StudentIntakeReady
        );

        assert_eq!(
            derive_project_workflow_stage(
                &TemplateSetupStatus::Approved,
                true,
                true,
                false,
                &[question],
            ),
            ProjectWorkflowStage::StudentIntakeReady
        );
    }

    #[test]
    fn project_workflow_stage_accepts_approval_basis_matching_clean_analysis_text() {
        let mut question = sample_question();
        let criterion = RubricCriterion {
            criterion_id: "criterion_1".into(),
            label: "Accuracy".into(),
            points: 5,
            partial_credit_guidance: String::new(),
            source: "manual".into(),
        };
        question.text = "1. Question".into();
        question.analysis.status = "ok".into();
        question.analysis.question_text_clean = Some("Question".into());
        question.rubric.status = "approved".into();
        question.rubric.criteria = vec![criterion.clone()];
        question.rubric.approval_basis = Some(RubricApprovalBasis {
            question_text: "Question".into(),
            question_context: String::new(),
            max_points: Some(5),
            criteria: vec![criterion],
        });

        assert_eq!(
            derive_project_workflow_stage(
                &TemplateSetupStatus::Draft,
                true,
                true,
                false,
                &[question],
            ),
            ProjectWorkflowStage::StudentIntakeReady
        );
    }

    fn sample_question() -> QuestionRecord {
        QuestionRecord {
            question_id: "q1".into(),
            question_number: 1,
            page_number: 1,
            max_points: Some(5),
            text: "Question".into(),
            baseline_pdf_text: "Question".into(),
            region: None,
            source_artifact_id: Some("artifact-1".into()),
            image_path: Some("/tmp/q1.png".into()),
            analysis: QuestionAnalysisState::default(),
            rubric: RubricState::default(),
        }
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

    #[test]
    fn derive_post_intake_workflow_stage_returns_none_for_empty_submissions() {
        let state = StudentWorkflowState {
            status: "not_started".into(),
            latest_job_id: None,
            submissions: Vec::new(),
        };
        assert_eq!(derive_post_intake_workflow_stage(&state), None);
    }

    #[test]
    fn derive_results_workflow_stage_stays_pending_when_ready_rows_remain_unfinalized() {
        let stage = derive_results_workflow_stage(
            &[
                crate::models::ResultStudentRow {
                    student_ref: "student_1".into(),
                    ready_to_finalize: true,
                    finalized: true,
                    stale_finalization: false,
                    uploaded: true,
                    ..Default::default()
                },
                crate::models::ResultStudentRow {
                    student_ref: "student_2".into(),
                    ready_to_finalize: true,
                    finalized: false,
                    stale_finalization: false,
                    uploaded: false,
                    ..Default::default()
                },
            ],
            true,
        );

        assert_eq!(
            stage,
            Some(ProjectWorkflowStage::ResultsFinalizationPending)
        );
    }

    #[test]
    fn derive_post_intake_workflow_stage_returns_review_for_alignment_review() {
        let state = StudentWorkflowState {
            status: "attention".into(),
            latest_job_id: None,
            submissions: vec![workflow_submission("student_1", "alignment_review")],
        };
        assert_eq!(
            derive_post_intake_workflow_stage(&state),
            Some(ProjectWorkflowStage::StudentWorkflowReview)
        );
    }

    #[test]
    fn derive_post_intake_workflow_stage_returns_review_for_parse_review() {
        let state = StudentWorkflowState {
            status: "attention".into(),
            latest_job_id: None,
            submissions: vec![workflow_submission("student_1", "parse_review")],
        };
        assert_eq!(
            derive_post_intake_workflow_stage(&state),
            Some(ProjectWorkflowStage::StudentWorkflowReview)
        );
    }

    #[test]
    fn derive_post_intake_workflow_stage_returns_running_for_alignment() {
        let state = StudentWorkflowState {
            status: "running".into(),
            latest_job_id: None,
            submissions: vec![workflow_submission("student_1", "alignment")],
        };
        assert_eq!(
            derive_post_intake_workflow_stage(&state),
            Some(ProjectWorkflowStage::StudentWorkflowRunning)
        );
    }

    #[test]
    fn derive_post_intake_workflow_stage_treats_stopped_as_resumable_ready() {
        let state = StudentWorkflowState {
            status: "ready".into(),
            latest_job_id: None,
            submissions: vec![workflow_submission("student_1", "stopped")],
        };
        assert_eq!(derive_post_intake_workflow_stage(&state), None);
    }

    #[test]
    fn derive_post_intake_workflow_stage_returns_grading_for_grading_stage() {
        let state = StudentWorkflowState {
            status: "running".into(),
            latest_job_id: None,
            submissions: vec![workflow_submission("student_1", "grading")],
        };
        assert_eq!(
            derive_post_intake_workflow_stage(&state),
            Some(ProjectWorkflowStage::StudentGrading)
        );
    }

    #[test]
    fn derive_post_intake_workflow_stage_returns_complete_when_all_graded() {
        let state = StudentWorkflowState {
            status: "graded".into(),
            latest_job_id: None,
            submissions: vec![workflow_submission("student_1", "graded")],
        };
        assert_eq!(
            derive_post_intake_workflow_stage(&state),
            Some(ProjectWorkflowStage::StudentGradingComplete)
        );
    }

    #[test]
    fn derive_post_intake_workflow_stage_returns_review_when_manual_grading() {
        let state = StudentWorkflowState {
            status: "attention".into(),
            latest_job_id: None,
            submissions: vec![workflow_submission("student_1", "manual_grading")],
        };
        assert_eq!(
            derive_post_intake_workflow_stage(&state),
            Some(ProjectWorkflowStage::StudentWorkflowReview)
        );
    }

    #[test]
    fn derive_post_intake_workflow_stage_returns_review_when_graded_and_manual_grading_mix() {
        let state = StudentWorkflowState {
            status: "attention".into(),
            latest_job_id: None,
            submissions: vec![
                workflow_submission("student_1", "graded"),
                workflow_submission("student_2", "manual_grading"),
            ],
        };
        assert_eq!(
            derive_post_intake_workflow_stage(&state),
            Some(ProjectWorkflowStage::StudentWorkflowReview)
        );
    }

    #[test]
    fn derive_post_intake_workflow_stage_returns_review_for_failed() {
        let state = StudentWorkflowState {
            status: "attention".into(),
            latest_job_id: None,
            submissions: vec![workflow_submission("student_1", "failed")],
        };
        assert_eq!(
            derive_post_intake_workflow_stage(&state),
            Some(ProjectWorkflowStage::StudentWorkflowReview)
        );
    }

    #[test]
    fn derive_post_intake_workflow_stage_mixed_intake_ready_and_graded() {
        let state = StudentWorkflowState {
            status: "running".into(),
            latest_job_id: None,
            submissions: vec![
                workflow_submission("student_1", "intake_ready"),
                workflow_submission("student_2", "graded"),
            ],
        };
        assert_eq!(
            derive_post_intake_workflow_stage(&state),
            Some(ProjectWorkflowStage::StudentWorkflowRunning)
        );
    }

    #[test]
    fn derive_post_intake_workflow_stage_mixed_intake_ready_and_processing() {
        let state = StudentWorkflowState {
            status: "running".into(),
            latest_job_id: None,
            submissions: vec![
                workflow_submission("student_1", "intake_ready"),
                workflow_submission("student_2", "alignment"),
            ],
        };
        assert_eq!(
            derive_post_intake_workflow_stage(&state),
            Some(ProjectWorkflowStage::StudentWorkflowRunning)
        );
    }
}
