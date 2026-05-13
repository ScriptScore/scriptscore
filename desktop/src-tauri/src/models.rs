// SPDX-License-Identifier: AGPL-3.0-only
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::workflow_status::{QuestionAnalysisStatus, RubricStatus, StudentIntakeStatus};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    Starting,
    Ready,
    Busy,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub project_id: String,
    pub display_name: String,
    pub subject: Option<String>,
    pub course_code: Option<String>,
    #[serde(default)]
    pub lms_course_id: Option<String>,
    pub project_path: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellState {
    pub current_project: Option<ProjectSummary>,
    pub worker_status: WorkerStatus,
    pub worker_activity: WorkerActivity,
    pub last_runtime_error: Option<String>,
    pub debug_features: DebugFeatures,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerActivity {
    pub active_jobs: Vec<WorkerJobSummary>,
    pub pending_job_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerJobSummary {
    pub job_id: String,
    pub command_name: String,
    pub started_at: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugFeatures {
    pub redaction_toggle: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegalDisclosure {
    pub license_expression: String,
    pub source_url: String,
    pub local_notices_path: String,
    pub third_party_notices: String,
    pub policy_report_json: String,
    pub artifact_status: String,
}

/// Transient `scans.ocr` result (not persisted to the project job table).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScansOcrHintResult {
    pub hint_text: String,
    /// Rows returned by EasyOCR `readtext` (0 when test env short-circuits OCR).
    pub segment_count: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentRosterMatchResult {
    pub student_ref: String,
    pub binding_token_hex: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LmsRosterCacheStatus {
    #[default]
    Idle,
    Loading,
    Ready,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LmsRosterCacheSnapshot {
    pub status: LmsRosterCacheStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lms_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub course_id: Option<String>,
    #[serde(default)]
    pub rows: Vec<crate::lms::LmsRosterRow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub llm_provider: String,
    pub llm_base_url: String,
    pub llm_model: String,
    pub llm_api_key: Option<String>,
    #[serde(default)]
    pub lms_provider: String,
    #[serde(default)]
    pub lms_canvas_base_url: String,
    pub lms_canvas_api_key: Option<String>,
    #[serde(default)]
    pub lms_binding_secret_plaintext_fallback: bool,
    #[serde(default)]
    pub pii_paddle_model_dir: Option<String>,
    #[serde(default = "default_preliminary_grading_max_workers")]
    pub preliminary_grading_max_workers: i64,
    pub instructor_profile: InstructorProfile,
    pub ai_assist_enabled: bool,
    #[serde(default)]
    pub ai_assist_categories: AiAssistCategories,
    pub theme: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            llm_provider: "ollama_native".into(),
            llm_base_url: "http://127.0.0.1:11434".into(),
            llm_model: "qwen2.5vl:7b".into(),
            llm_api_key: None,
            lms_provider: "none".into(),
            lms_canvas_base_url: "https://canvas.instructure.com".into(),
            lms_canvas_api_key: None,
            lms_binding_secret_plaintext_fallback: false,
            pii_paddle_model_dir: None,
            preliminary_grading_max_workers: default_preliminary_grading_max_workers(),
            instructor_profile: InstructorProfile::default(),
            ai_assist_enabled: false,
            ai_assist_categories: AiAssistCategories::default(),
            theme: "dark".into(),
        }
    }
}

fn default_preliminary_grading_max_workers() -> i64 {
    1
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAssistCategories {
    pub rubrics: bool,
    pub question_analysis: bool,
    pub grading_feedback: bool,
    pub parsing_review: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstructorProfileEnabledTags {
    pub grading_strictness: bool,
    pub syntax_leniency: bool,
    pub ocr_tolerance: bool,
    pub partial_credit_style: bool,
    pub feedback_style: bool,
}

impl Default for InstructorProfileEnabledTags {
    fn default() -> Self {
        Self {
            grading_strictness: true,
            syntax_leniency: false,
            ocr_tolerance: false,
            partial_credit_style: false,
            feedback_style: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstructorProfile {
    pub grading_strictness: String,
    pub syntax_leniency: String,
    pub ocr_tolerance: String,
    pub partial_credit_style: String,
    pub feedback_style: String,
    #[serde(default)]
    pub enabled_tags: InstructorProfileEnabledTags,
    pub additional_guidance: String,
    pub include_minimum_credit_criterion: bool,
    /// Percent of question max points (1–100) used to compute the minimum-credit row when first applied.
    pub minimum_credit_percent: i64,
}

impl Default for InstructorProfile {
    fn default() -> Self {
        Self {
            grading_strictness: "balanced".into(),
            syntax_leniency: "medium".into(),
            ocr_tolerance: "medium".into(),
            partial_credit_style: "balanced".into(),
            feedback_style: "brief".into(),
            enabled_tags: InstructorProfileEnabledTags::default(),
            additional_guidance: String::new(),
            include_minimum_credit_criterion: false,
            minimum_credit_percent: 10,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectInput {
    pub display_name: String,
    pub subject: Option<String>,
    pub course_code: Option<String>,
    #[serde(default)]
    pub lms_course_id: Option<String>,
    #[serde(default)]
    pub project_root: Option<String>,
    pub template_pdf_path: String,
    pub instructor_profile: Option<InstructorProfile>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmokePingResult {
    pub command: String,
    pub message: String,
    pub steps: i64,
    pub event_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisionCapableModel {
    #[serde(alias = "model")]
    pub name: String,
    #[serde(alias = "display_name")]
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmModelValidation {
    pub model: String,
    #[serde(alias = "display_name")]
    pub display_name: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub valid: bool,
    pub reason: Option<String>,
    #[serde(default, alias = "missing_capabilities")]
    pub missing_capabilities: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeJobEvent {
    pub event_type: String,
    pub command_name: String,
    pub worker_status: WorkerStatus,
    pub request_id: Option<String>,
    pub job_id: Option<String>,
    pub payload: Value,
}

#[derive(Clone, Debug)]
pub struct JobProgressRecord {
    pub sequence: i64,
    pub event_type: String,
    pub progress_json: Option<String>,
    pub scope_json: Option<String>,
    pub data_json: String,
    pub created_at: String,
}

#[derive(Clone, Debug)]
pub struct JobRunRecord {
    pub job_id: String,
    pub command_name: String,
    pub request_id: String,
    pub state: String,
    pub submitted_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub request_json: String,
    pub result_json: Option<String>,
    pub error_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct JobCompletion {
    pub state: String,
    pub finished_at: String,
    pub result_json: Option<String>,
    pub error_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ArtifactRecord {
    pub artifact_id: String,
    pub job_id: Option<String>,
    pub kind: String,
    pub role: String,
    pub relative_path: String,
    pub mime_type: Option<String>,
    pub byte_size: Option<i64>,
    pub metadata_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct WorkerJobResult {
    pub terminal_type: String,
    pub terminal_payload: Value,
    pub envelope: Value,
    pub events: Vec<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplatePageArtifactSummary {
    pub artifact_id: String,
    pub page_number: i64,
    pub image_path: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateQuestionRegion {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionRecord {
    pub question_id: String,
    pub question_number: i64,
    pub page_number: i64,
    pub max_points: Option<i64>,
    pub text: String,
    pub baseline_pdf_text: String,
    pub region: Option<TemplateQuestionRegion>,
    pub source_artifact_id: Option<String>,
    pub image_path: Option<String>,
    #[serde(default)]
    pub analysis: QuestionAnalysisState,
    #[serde(default)]
    pub rubric: RubricState,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionEdit {
    pub question_id: String,
    pub question_number: i64,
    pub page_number: i64,
    pub max_points: Option<i64>,
    pub text: String,
    /// When set, merges into `question_analysis` workflow payload (does not replace analysis status).
    #[serde(default)]
    pub question_context: Option<String>,
    /// Optional operator classification for edits to questions with already-approved rubrics.
    /// Accepted values: "minor" or "grading".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rubric_edit_impact: Option<String>,
}

/// Semantic label values for redaction regions:
/// - `"name_identification"`: the first region (lowest sort_order), dedicated to identifying student names
/// - `"privacy_protection"`: all subsequent regions, used for pre-emptive privacy masking
pub const REDACTION_LABEL_NAME_IDENTIFICATION: &str = "name_identification";
pub const REDACTION_LABEL_PRIVACY_PROTECTION: &str = "privacy_protection";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateRedactionRegion {
    pub region_id: String,
    pub page_number: i64,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    /// Semantic label: `"name_identification"` for the first region, `"privacy_protection"` for others.
    pub label: String,
    pub sort_order: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateRedactionRegionInput {
    pub region_id: Option<String>,
    pub page_number: i64,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateArucoPageStatus {
    pub page_number: i64,
    pub marker_count: i64,
    pub marker_ids: Vec<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateArucoStatus {
    pub state: String,
    pub total_marker_count: i64,
    pub pages: Vec<TemplateArucoPageStatus>,
}

impl Default for TemplateArucoStatus {
    fn default() -> Self {
        Self {
            state: "unknown".into(),
            total_marker_count: 0,
            pages: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceWarning {
    pub code: Option<String>,
    pub message: String,
    pub scope: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateSetupPayload {
    pub template_artifact_id: Option<String>,
    pub template_source_name: Option<String>,
    pub last_setup_job_id: Option<String>,
    pub page_count: i64,
    pub question_count: i64,
    pub failure_message: Option<String>,
    pub approved_at: Option<String>,
    pub redaction_skip_acknowledged_at: Option<String>,
    #[serde(default)]
    pub aruco_status: TemplateArucoStatus,
    pub warnings: Vec<WorkspaceWarning>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTraceRefs {
    pub setup_job_id: Option<String>,
    pub batch_analyze_job_id: Option<String>,
    pub batch_rubric_job_id: Option<String>,
    pub intake_job_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectConfig {
    pub project_id: String,
    pub display_name: String,
    pub subject: Option<String>,
    pub course_code: Option<String>,
    #[serde(default)]
    pub lms_course_id: Option<String>,
    #[serde(default)]
    pub lms_assignment_id: Option<String>,
    pub redaction_required: bool,
    pub instructor_profile: InstructorProfile,
    pub trace_refs: ProjectTraceRefs,
    pub created_at: String,
    pub updated_at: String,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            display_name: String::new(),
            subject: None,
            course_code: None,
            lms_course_id: None,
            lms_assignment_id: None,
            redaction_required: true,
            instructor_profile: InstructorProfile::default(),
            trace_refs: ProjectTraceRefs::default(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSetupState {
    #[serde(default)]
    pub display_name: String,
    pub subject: Option<String>,
    pub course_code: Option<String>,
    #[serde(default = "default_redaction_required")]
    pub redaction_required: bool,
    #[serde(default)]
    pub instructor_profile: InstructorProfile,
    pub include_minimum_credit_criterion: bool,
    pub minimum_credit_percent: i64,
    pub last_setup_trace_job_id: Option<String>,
}

impl Default for ProjectSetupState {
    fn default() -> Self {
        Self {
            display_name: String::new(),
            subject: None,
            course_code: None,
            redaction_required: default_redaction_required(),
            instructor_profile: InstructorProfile::default(),
            include_minimum_credit_criterion: false,
            minimum_credit_percent: 10,
            last_setup_trace_job_id: None,
        }
    }
}

fn default_redaction_required() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionAnalysisState {
    pub status: String,
    pub question_text_clean: Option<String>,
    pub question_context: Option<String>,
    pub warnings: Vec<WorkspaceWarning>,
    pub latest_job_id: Option<String>,
}

impl Default for QuestionAnalysisState {
    fn default() -> Self {
        Self {
            status: QuestionAnalysisStatus::NotStarted.as_str().to_string(),
            question_text_clean: None,
            question_context: None,
            warnings: Vec::new(),
            latest_job_id: None,
        }
    }
}

impl QuestionAnalysisState {
    pub fn status_kind(&self) -> QuestionAnalysisStatus {
        QuestionAnalysisStatus::from_storage(&self.status)
    }

    pub fn is_complete(&self) -> bool {
        self.status_kind().is_complete()
    }

    pub fn is_editable(&self) -> bool {
        self.status_kind().is_editable()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RubricCriterion {
    pub criterion_id: String,
    pub label: String,
    pub points: i64,
    pub partial_credit_guidance: String,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RubricApprovalBasis {
    pub question_text: String,
    pub question_context: String,
    pub max_points: Option<i64>,
    pub criteria: Vec<RubricCriterion>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RubricState {
    pub status: String,
    pub criteria: Vec<RubricCriterion>,
    pub warnings: Vec<WorkspaceWarning>,
    pub approved_at: Option<String>,
    pub latest_job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_basis: Option<RubricApprovalBasis>,
}

impl Default for RubricState {
    fn default() -> Self {
        Self {
            status: RubricStatus::NotStarted.as_str().to_string(),
            criteria: Vec::new(),
            warnings: Vec::new(),
            approved_at: None,
            latest_job_id: None,
            approval_basis: None,
        }
    }
}

impl RubricState {
    pub fn status_kind(&self) -> RubricStatus {
        RubricStatus::from_storage(&self.status)
    }

    pub fn is_draft(&self) -> bool {
        self.status_kind().is_draft()
    }

    pub fn is_approved(&self) -> bool {
        self.status_kind().is_approved()
    }

    pub fn has_content(&self) -> bool {
        !self.criteria.is_empty()
    }

    pub fn mark_draft(&mut self) {
        self.status = RubricStatus::Draft.as_str().to_string();
        self.approved_at = None;
        self.approval_basis = None;
    }

    pub fn mark_approved(&mut self, approved_at: String, basis: RubricApprovalBasis) {
        self.status = RubricStatus::Approved.as_str().to_string();
        self.approved_at = Some(approved_at);
        self.approval_basis = Some(basis);
    }

    pub fn approval_matches(
        &self,
        question_text: &str,
        question_context: &str,
        max_points: Option<i64>,
    ) -> bool {
        let Some(basis) = self.approval_basis.as_ref() else {
            return self.is_approved();
        };
        self.is_approved()
            && basis.question_text == question_text
            && basis.question_context == question_context
            && basis.max_points == max_points
            && basis.criteria == self.criteria
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentIntakeSummary {
    pub student_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_display_name: Option<String>,
    pub canonical_pdf_path: String,
    pub ingest_status: String,
    pub page_count: i64,
    /// Absolute paths to `scans.ingest` page PNGs in operator-confirmed intake order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exam_page_paths: Vec<String>,
    pub warnings: Vec<WorkspaceWarning>,
    /// Pseudonymous HMAC token (hex); never contains raw LMS identifiers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_token_hex: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentIntakeState {
    pub status: String,
    pub latest_job_id: Option<String>,
    pub items: Vec<StudentIntakeSummary>,
    pub unresolved_count: i64,
}

impl StudentIntakeState {
    pub fn not_started() -> Self {
        Self {
            status: StudentIntakeStatus::NotStarted.as_str().to_string(),
            ..Self::default()
        }
    }

    pub fn status_kind(&self) -> StudentIntakeStatus {
        StudentIntakeStatus::from_storage(&self.status)
    }

    pub fn is_ready(&self) -> bool {
        self.status_kind().is_ready()
    }

    pub fn has_unresolved_items(&self) -> bool {
        self.unresolved_count > 0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentRosterRow {
    pub student_ref: String,
    pub binding_token_hex: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentWorkflowTransform {
    pub rotation: f64,
    pub scale: f64,
    pub translate_x: f64,
    pub translate_y: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentWorkflowPage {
    pub page_number: i64,
    pub image_path: String,
    pub source_pdf_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ocr_metadata_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentWorkflowDetectRegion {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    #[serde(default = "rendered_page_pixels_units")]
    pub units: String,
}

fn rendered_page_pixels_units() -> String {
    "rendered_page_pixels".into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentWorkflowDetectReviewRow {
    pub question_id: String,
    pub page_number: i64,
    pub source_page_image_path: String,
    pub template_region: StudentWorkflowDetectRegion,
    #[serde(default)]
    pub warnings: Vec<WorkspaceWarning>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_region: Option<StudentWorkflowDetectRegion>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentWorkflowDetectReview {
    #[serde(default)]
    pub pending_rows: Vec<StudentWorkflowDetectReviewRow>,
    #[serde(default)]
    pub trusted_crop_targets: Vec<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentWorkflowAlignmentPage {
    pub page_number: i64,
    pub confidence: Option<f64>,
    pub low_confidence: bool,
    #[serde(default)]
    pub review_exempt: bool,
    #[serde(default)]
    pub review_exempt_reason: Option<String>,
    #[serde(default)]
    pub question_count: i64,
    pub transform: StudentWorkflowTransform,
    pub warnings: Vec<WorkspaceWarning>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentWorkflowCriterionResult {
    pub criterion_index: i64,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub points: i64,
    pub points_awarded: i64,
    pub rationale: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentWorkflowHighlightSpan {
    pub kind: String,
    pub start_char: i64,
    pub end_char: i64,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentWorkflowPiiPrescreen {
    pub source_command: String,
    pub status: String,
    pub contains_handwriting: String,
    pub contains_pii: bool,
    #[serde(default)]
    pub pii_types_detected: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<WorkspaceWarning>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentWorkflowAnswer {
    pub question_id: String,
    pub question_number: i64,
    pub crop_image_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pii_prescreen: Option<StudentWorkflowPiiPrescreen>,
    #[serde(default)]
    pub manual_grading_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_grading_reason: Option<String>,
    #[serde(default)]
    pub moderation_eligible: bool,
    pub parse_status: String,
    pub parse_confidence: Option<String>,
    pub parse_confidence_source: Option<String>,
    pub raw_parsed_text: Option<String>,
    pub verified_text: Option<String>,
    pub review_required: bool,
    pub verified: bool,
    pub stale: bool,
    pub grading_status: String,
    pub grading_confidence: Option<String>,
    pub grading_confidence_reason: Option<String>,
    pub question_max_points: Option<i64>,
    pub total_points_awarded: Option<i64>,
    pub feedback_text: Option<String>,
    #[serde(default)]
    pub criterion_results: Vec<StudentWorkflowCriterionResult>,
    #[serde(default)]
    pub highlights: Vec<StudentWorkflowHighlightSpan>,
    #[serde(default)]
    pub warnings: Vec<WorkspaceWarning>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModerationScoreOverride {
    pub student_ref: String,
    pub question_id: String,
    pub moderated_total_points: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModerationFeedbackOverride {
    pub student_ref: String,
    pub question_id: String,
    pub feedback_text: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModerationQuestionReview {
    pub question_id: String,
    pub reviewed_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModerationState {
    #[serde(default)]
    pub score_overrides: Vec<ModerationScoreOverride>,
    #[serde(default)]
    pub feedback_overrides: Vec<ModerationFeedbackOverride>,
    #[serde(default)]
    pub question_reviews: Vec<ModerationQuestionReview>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultsLmsTarget {
    pub provider: String,
    pub course_id: String,
    pub assignment_id: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultFinalizationRecord {
    pub student_ref: String,
    pub result_fingerprint: String,
    pub finalized_at: String,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LmsUploadMode {
    #[default]
    DryRun,
    Live,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LmsUploadStudentStatus {
    #[default]
    Ready,
    Uploaded,
    Failed,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LmsAssignmentSummary {
    pub assignment_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points_possible: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultsLmsAssetBinding {
    pub provider: String,
    pub course_id: String,
    pub assignment_id: String,
    pub student_ref: String,
    pub local_asset_name: String,
    #[serde(default)]
    pub asset_fingerprint: String,
    pub provider_file_id: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LmsUploadReportAsset {
    pub question_id: String,
    pub local_asset_name: String,
    #[serde(default)]
    pub asset_fingerprint: String,
    pub local_file_path: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LmsUploadPreparationRow {
    pub student_ref: String,
    pub user_id: String,
    pub result_fingerprint: String,
    pub score: f64,
    pub report_html_template: String,
    #[serde(default)]
    pub report_assets: Vec<LmsUploadReportAsset>,
    #[serde(default)]
    pub existing_asset_bindings: Vec<ResultsLmsAssetBinding>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LmsUploadStudentResult {
    pub student_ref: String,
    pub result_fingerprint: String,
    pub status: LmsUploadStudentStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sanitized_error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LmsUploadPublishOutcome {
    pub student_result: LmsUploadStudentResult,
    #[serde(default)]
    pub active_asset_bindings: Vec<ResultsLmsAssetBinding>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LmsUploadAttemptResult {
    pub attempt_id: String,
    pub mode: LmsUploadMode,
    pub provider: String,
    pub course_id: String,
    pub assignment_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub attempted_count: i64,
    pub success_count: i64,
    pub failure_count: i64,
    #[serde(default)]
    pub student_results: Vec<LmsUploadStudentResult>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultsLmsState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_target: Option<ResultsLmsTarget>,
    #[serde(default)]
    pub finalization_records: Vec<ResultFinalizationRecord>,
    #[serde(default)]
    pub asset_bindings: Vec<ResultsLmsAssetBinding>,
    #[serde(default)]
    pub upload_attempts: Vec<LmsUploadAttemptResult>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultsQuestionMetric {
    pub question_id: String,
    pub question_number: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_points: Option<i64>,
    #[serde(default)]
    pub reviewed: bool,
    pub sample_size: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub average_points: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub average_percent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difficulty_percent: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultsExamMetrics {
    pub scored_student_count: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub average_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub median_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_score: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_score: Option<i64>,
    #[serde(default)]
    pub question_metrics: Vec<ResultsQuestionMetric>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultsLmsReviewSummary {
    pub total_reviewable_questions: i64,
    pub unreviewed_question_count: i64,
    #[serde(default)]
    pub has_unreviewed_questions: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentWorkflowSubmission {
    pub student_ref: String,
    pub canonical_pdf_path: String,
    pub page_count: i64,
    pub stage: String,
    pub latest_job_id: Option<String>,
    pub failure_message: Option<String>,
    #[serde(default)]
    pub warnings: Vec<WorkspaceWarning>,
    #[serde(default)]
    pub page_artifacts: Vec<StudentWorkflowPage>,
    #[serde(default)]
    pub alignment_pages: Vec<StudentWorkflowAlignmentPage>,
    #[serde(default)]
    pub detect_review: Option<StudentWorkflowDetectReview>,
    #[serde(default)]
    pub answers: Vec<StudentWorkflowAnswer>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentWorkflowState {
    pub status: String,
    pub latest_job_id: Option<String>,
    #[serde(default)]
    pub submissions: Vec<StudentWorkflowSubmission>,
}

impl StudentWorkflowState {
    pub fn not_started() -> Self {
        Self {
            status: "not_started".into(),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobTraceSummary {
    pub job_id: String,
    pub command_name: String,
    pub state: String,
    pub submitted_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub event_count: i64,
    pub student_refs: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobTraceEvent {
    pub sequence: i64,
    pub event_type: String,
    pub progress: Option<Value>,
    pub scope: Option<Value>,
    pub data: Value,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobTraceState {
    pub job_id: String,
    pub command_name: String,
    pub state: String,
    pub submitted_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub student_refs: Vec<String>,
    pub request: Value,
    pub result: Option<Value>,
    pub error: Option<Value>,
    pub events: Vec<JobTraceEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RubricUpdateInput {
    pub question_id: String,
    pub criteria: Vec<RubricCriterion>,
    pub approve: bool,
    /// Optional operator classification for edits to already-approved rubrics.
    /// Accepted values: "minor" or "grading".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rubric_edit_impact: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveCriterionScoreInput {
    pub student_ref: String,
    pub question_id: String,
    pub criterion_index: i64,
    pub points_awarded: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveModeratedScoreInput {
    pub student_ref: String,
    pub question_id: String,
    pub moderated_total_points: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetModerationQuestionReviewedInput {
    pub question_id: String,
    pub reviewed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveModeratedFeedbackInput {
    pub student_ref: String,
    pub question_id: String,
    pub feedback_text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResultsLmsAssignmentInput {
    #[serde(default)]
    pub assignment_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteStudentSubmissionInput {
    pub student_ref: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSubmissionResultFinalizedInput {
    pub student_ref: String,
    pub finalized: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalizeReadyResultsInput {
    #[serde(default)]
    pub student_refs: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResultsLmsUploadInput {
    pub mode: LmsUploadMode,
    #[serde(default)]
    pub student_refs: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryResultsLmsUploadInput {
    pub attempt_id: String,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResultsExportFormat {
    #[default]
    HtmlZip,
    Csv,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResultsExportInput {
    pub format: ResultsExportFormat,
    #[serde(default)]
    pub student_refs: Vec<String>,
    pub destination_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultsExportResponse {
    pub format: ResultsExportFormat,
    pub destination_path: String,
    pub exported_count: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultQuestionRow {
    pub question_id: String,
    pub question_number: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_points: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_total_points: Option<i64>,
    #[serde(default)]
    pub effective_feedback_text: String,
    #[serde(default)]
    pub uses_moderated_total: bool,
    #[serde(default)]
    pub uses_moderated_feedback: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultStudentRow {
    pub student_ref: String,
    pub aggregate_total: i64,
    pub aggregate_complete: bool,
    pub ready_to_finalize: bool,
    #[serde(default)]
    pub blocked_reasons: Vec<String>,
    #[serde(default)]
    pub question_rows: Vec<ResultQuestionRow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_fingerprint: Option<String>,
    #[serde(default)]
    pub finalized: bool,
    #[serde(default)]
    pub stale_finalization: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finalized_at: Option<String>,
    #[serde(default)]
    pub uploaded: bool,
    #[serde(default)]
    pub upload_failed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_upload_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_upload_attempt_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultsLmsUploadResponse {
    pub attempt: LmsUploadAttemptResult,
    pub workspace: ExamWorkspaceState,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultsLmsReportPreview {
    pub student_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_fingerprint: Option<String>,
    pub html: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentIntakeInput {
    pub student_ref: String,
    #[serde(default)]
    pub local_student_name: Option<String>,
    pub raw_pdf_path: String,
    #[serde(default)]
    pub desired_page_order: Vec<i64>,
    #[serde(default)]
    pub redaction_regions_px: Vec<StudentIntakeRedactionRegionInput>,
    #[serde(default)]
    pub raster_sizes_by_page: std::collections::HashMap<i64, StudentIntakeRasterSize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentIntakeRedactionRegionInput {
    pub page_number: i64,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentIntakeRasterSize {
    pub width_px: i64,
    pub height_px: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentIntakePageOrderUpdateInput {
    pub student_ref: String,
    pub exam_page_paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExamWorkspaceState {
    pub project: ProjectSummary,
    pub status: String,
    pub status_label: String,
    pub failure_message: Option<String>,
    pub template_preview_artifacts: Vec<TemplatePageArtifactSummary>,
    #[serde(default)]
    pub aruco_status: TemplateArucoStatus,
    pub questions: Vec<QuestionRecord>,
    pub redaction_regions: Vec<TemplateRedactionRegion>,
    pub warnings: Vec<WorkspaceWarning>,
    pub can_approve: bool,
    /// Draft or approved template: rubric save/approve is allowed (unlike template-setup approval).
    pub can_approve_rubric: bool,
    #[serde(default)]
    pub project_config: ProjectConfig,
    #[serde(default)]
    pub student_roster: Vec<StudentRosterRow>,
    #[serde(default)]
    pub student_intake: StudentIntakeState,
    #[serde(default)]
    pub student_workflow: StudentWorkflowState,
    #[serde(default)]
    pub moderation_state: ModerationState,
    #[serde(default)]
    pub results_lms_state: ResultsLmsState,
    #[serde(default)]
    pub results_lms_rows: Vec<ResultStudentRow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub results_lms_metrics: Option<ResultsExamMetrics>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub results_lms_review_summary: Option<ResultsLmsReviewSummary>,
    #[serde(default)]
    pub workflow_stage: String,
    #[serde(default)]
    pub workflow_label: String,
}
