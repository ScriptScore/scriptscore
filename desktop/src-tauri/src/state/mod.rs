// SPDX-License-Identifier: AGPL-3.0-only
mod app_jobs;
mod host_workflow;
mod job_history;
mod lms_binding;
mod lms_roster_cache;
pub(crate) mod moderation;
pub(crate) mod project_lifecycle;
pub(crate) mod results_export;
pub(crate) mod results_lms;
pub(crate) mod results_lms_report;
mod runtime;
pub(crate) mod scheduler;
pub(crate) mod settings;
mod student_submission_delete;
pub(crate) mod student_workflow;
mod template_export;
mod template_setup;
mod transient_commands;
pub(crate) mod workspace_actions;

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::errors::{HostError, HostResult};
use crate::models::{
    AppSettings, DebugFeatures, DeleteStudentSubmissionInput, ExamWorkspaceState, JobTraceState,
    LmsRosterCacheSnapshot, ProjectConfig, ProjectSummary, QuestionEdit, RubricUpdateInput,
    RuntimeJobEvent, SaveCriterionScoreInput, SaveModeratedFeedbackInput, SaveModeratedScoreInput,
    SaveResultsLmsAssignmentInput, SetModerationQuestionReviewedInput,
    SetSubmissionResultFinalizedInput, ShellState, SmokePingResult, StudentIntakeInput,
    StudentRosterMatchResult, TemplateRedactionRegionInput, WorkerActivity, WorkerStatus,
};
use crate::project_store;
use crate::worker_runtime::WorkerRuntimeSource;
use scheduler::RuntimeScheduler;

pub const RUNTIME_JOB_EVENT_NAME: &str = "scriptscore:runtime-job";

pub trait RuntimeEventSink: Send + Sync {
    fn emit_runtime_event(&self, event: RuntimeJobEvent);
}

pub(crate) struct AppHandleRuntimeEventSink {
    app_handle: AppHandle,
}

impl AppHandleRuntimeEventSink {
    pub(crate) fn new(app_handle: AppHandle) -> Self {
        Self { app_handle }
    }
}

impl RuntimeEventSink for AppHandleRuntimeEventSink {
    fn emit_runtime_event(&self, event: RuntimeJobEvent) {
        let _ = self.app_handle.emit(RUNTIME_JOB_EVENT_NAME, event);
    }
}

pub struct AppState {
    inner: Arc<AppStateInner>,
}

pub(crate) struct AppStateInner {
    inner: Mutex<DesktopApp>,
}

impl AppStateInner {
    pub(crate) fn lock(&self) -> std::sync::MutexGuard<'_, DesktopApp> {
        self.inner.lock().expect("state lock poisoned")
    }

    pub(crate) fn bundled_resource_dir(&self) -> Option<PathBuf> {
        self.lock().bundled_resource_dir()
    }

    pub(crate) fn run_smoke_ping(
        self: &Arc<Self>,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<SmokePingResult> {
        runtime::run_smoke_ping(self, event_sink)
    }

    pub(crate) fn list_llm_models(
        self: &Arc<Self>,
        event_sink: &dyn RuntimeEventSink,
        provider_name: String,
        base_url: String,
        api_key: Option<String>,
    ) -> HostResult<Vec<crate::models::VisionCapableModel>> {
        settings::list_llm_models(self, event_sink, provider_name, base_url, api_key)
    }

    pub(crate) fn validate_llm_model(
        self: &Arc<Self>,
        event_sink: &dyn RuntimeEventSink,
        provider_name: String,
        base_url: String,
        model: String,
        api_key: Option<String>,
    ) -> HostResult<crate::models::LlmModelValidation> {
        settings::validate_llm_model(self, event_sink, provider_name, base_url, model, api_key)
    }

    pub(crate) fn replace_template_pdf(
        self: &Arc<Self>,
        template_pdf_path: String,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<ExamWorkspaceState> {
        project_lifecycle::replace_template_pdf(self, template_pdf_path, event_sink)
    }

    pub(crate) fn start_job(
        self: &Arc<Self>,
        command_name: &str,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<runtime::ReservedJob> {
        runtime::start_runtime_job(self, event_sink, command_name)
    }

    pub(crate) fn run_reserved_job(
        self: &Arc<Self>,
        reserved: runtime::ReservedJob,
        command_name: String,
        request_payload: serde_json::Value,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<()> {
        let project_path = self.lock().current_project_path_optional();
        runtime::run_reserved_job(
            self,
            event_sink,
            reserved,
            runtime::RuntimeJobRequest {
                command_name: &command_name,
                worker_request_payload: request_payload.clone(),
                persisted_request_payload: request_payload,
                output_artifacts_dir: None,
                project_path: project_path.as_deref(),
                stdin_bytes: None,
            },
        )
        .map(|_| ())
    }

    pub(crate) fn transient_scans_ocr_hint(
        self: &Arc<Self>,
        png_bytes: Vec<u8>,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<crate::models::ScansOcrHintResult> {
        transient_commands::transient_scans_ocr_hint(self, png_bytes, event_sink)
    }

    pub(crate) fn transient_render_pdf_page_png(
        self: &Arc<Self>,
        pdf_path: String,
        page_number: i64,
        zoom: f64,
        max_width_px: Option<i64>,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<workspace_actions::IntakePreviewPage> {
        transient_commands::transient_render_pdf_page_png(
            self,
            pdf_path,
            page_number,
            zoom,
            max_width_px,
            event_sink,
        )
    }

    pub(crate) fn transient_clip_pdf_rects_png_base64(
        self: &Arc<Self>,
        pdf_path: String,
        rects: Vec<workspace_actions::PdfPointRect>,
        zoom: f64,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<Vec<String>> {
        transient_commands::transient_clip_pdf_rects_png_base64(
            self, pdf_path, rects, zoom, event_sink,
        )
    }

    pub(crate) fn transient_pdf_clip_text(
        self: &Arc<Self>,
        input: workspace_actions::PdfTextClipInput,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<String> {
        transient_commands::transient_pdf_clip_text(self, input, event_sink)
    }

    pub(crate) fn intake_default_pdf_rects_from_template(
        self: &Arc<Self>,
        pdf_path: String,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<Vec<workspace_actions::PdfPointRect>> {
        transient_commands::intake_default_pdf_rects_from_template(self, pdf_path, event_sink)
    }

    pub(crate) fn start_create_project_job(
        self: &Arc<Self>,
        input: &crate::models::CreateProjectInput,
        settings: AppSettings,
        event_sink: Arc<dyn RuntimeEventSink>,
    ) -> HostResult<String> {
        app_jobs::start_create_project_job(self, input, settings, event_sink)
    }

    pub(crate) fn emit_runtime_error(
        self: &Arc<Self>,
        event_sink: &dyn RuntimeEventSink,
        command_name: &str,
        message: &str,
    ) {
        app_jobs::emit_runtime_error(event_sink, command_name, message);
    }

    pub(crate) fn begin_student_workflow(
        self: &Arc<Self>,
        settings: &AppSettings,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        student_workflow::begin_student_workflow(self, &project_path, settings, event_sink)
    }

    pub(crate) fn confirm_student_alignment(
        self: &Arc<Self>,
        input: student_workflow::StudentWorkflowAlignmentUpdateInput,
        settings: &AppSettings,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        student_workflow::confirm_student_alignment(
            self,
            &project_path,
            input,
            settings,
            event_sink,
        )
    }

    pub(crate) fn save_student_alignment_review(
        self: &Arc<Self>,
        input: student_workflow::StudentWorkflowAlignmentUpdateInput,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        student_workflow::save_student_alignment_review(&project_path, input)
    }

    pub(crate) fn confirm_student_parse_review(
        self: &Arc<Self>,
        input: student_workflow::StudentWorkflowParseReviewInput,
        settings: &AppSettings,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        student_workflow::confirm_student_parse_review(
            self,
            &project_path,
            input,
            settings,
            event_sink,
        )
    }

    pub(crate) fn save_student_parse_review(
        self: &Arc<Self>,
        input: student_workflow::StudentWorkflowParseReviewInput,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        student_workflow::save_student_parse_review(&project_path, input)
    }

    pub(crate) fn confirm_student_detect_review(
        self: &Arc<Self>,
        input: student_workflow::StudentWorkflowDetectReviewInput,
        settings: &AppSettings,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        student_workflow::confirm_student_detect_review(
            self,
            &project_path,
            input,
            settings,
            event_sink,
        )
    }

    pub(crate) fn save_student_detect_review(
        self: &Arc<Self>,
        input: student_workflow::StudentWorkflowDetectReviewInput,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        student_workflow::save_student_detect_review(&project_path, input)
    }
}

pub(crate) struct DesktopApp {
    current_project: Option<ProjectSummary>,
    debug_features: DebugFeatures,
    lms_roster_cache: lms_roster_cache::ProjectLmsRosterCache,
    worker_status: WorkerStatus,
    last_runtime_error: Option<String>,
    worker: Option<crate::worker::WorkerClient>,
    app_handle: Option<AppHandle>,
    scheduler: RuntimeScheduler,
    host_workflow_children: HashMap<String, Vec<String>>,
    /// Unified intake progress (frontend): first half redact, second half ingest.
    intake_pipeline_active: bool,
    /// Redact jobs in this intake run; `0` means ingest phase (or idle).
    intake_pipeline_redact_total: u32,
    /// 0-based index of the current redact job while `intake_pipeline_redact_total > 0`.
    intake_pipeline_redact_index: u32,
}

impl AppState {
    pub(crate) fn from_inner(inner: Arc<AppStateInner>) -> Self {
        Self { inner }
    }

    pub fn bootstrap() -> Self {
        Self::bootstrap_with_args(std::env::args_os())
    }

    #[doc(hidden)]
    pub fn bootstrap_with_args<I>(args: I) -> Self
    where
        I: IntoIterator<Item = OsString>,
    {
        let (worker, worker_status, last_runtime_error) =
            match crate::worker::WorkerClient::launch(None, WorkerRuntimeSource::RepoFallback) {
                Ok(worker) => (Some(worker), WorkerStatus::Ready, None),
                Err(err) => (None, WorkerStatus::Error, Some(err.to_string())),
            };
        let debug_features = parse_debug_features(args);
        Self {
            inner: Arc::new(AppStateInner {
                inner: Mutex::new(DesktopApp {
                    current_project: None,
                    debug_features,
                    lms_roster_cache: Default::default(),
                    worker_status,
                    last_runtime_error,
                    worker,
                    app_handle: None,
                    scheduler: RuntimeScheduler::default(),
                    host_workflow_children: HashMap::new(),
                    intake_pipeline_active: false,
                    intake_pipeline_redact_total: 0,
                    intake_pipeline_redact_index: 0,
                }),
            }),
        }
    }

    pub fn create_project(
        &self,
        input: crate::models::CreateProjectInput,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<ShellState> {
        project_lifecycle::create_project(&self.inner, input, event_sink)
    }

    pub fn start_create_project_job(
        &self,
        input: &crate::models::CreateProjectInput,
        settings: AppSettings,
        event_sink: Arc<dyn RuntimeEventSink>,
    ) -> HostResult<String> {
        AppStateInner::start_create_project_job(&self.inner, input, settings, event_sink)
    }

    pub fn open_project(
        &self,
        project_path: PathBuf,
        settings: &AppSettings,
    ) -> HostResult<ShellState> {
        let summary = project_store::open_project(&project_path)?;
        {
            let app = self.inner.lock();
            app.ensure_no_active_job()?;
        }
        let recovery_message = recover_abandoned_project_jobs(&project_path)?;
        {
            let mut app = self.inner.lock();
            app.current_project = Some(summary);
            app.last_runtime_error = runtime_error_after_project_open(
                app.last_runtime_error.take(),
                recovery_message,
                app.worker.is_some(),
            );
        }
        let _ = lms_roster_cache::ensure_project_preload(&self.inner, settings)?;
        Ok(self.inner.lock().shell_state())
    }

    pub fn close_current_project(&self) -> HostResult<ShellState> {
        let mut app = self.inner.lock();
        app.ensure_no_active_job()?;
        app.current_project = None;
        drop(app);
        lms_roster_cache::clear_for_closed_project(&self.inner);
        Ok(self.inner.lock().shell_state())
    }

    pub fn replace_template_pdf(
        &self,
        template_pdf_path: String,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<ExamWorkspaceState> {
        self.inner
            .replace_template_pdf(template_pdf_path, event_sink)
    }

    pub fn start_replace_template_pdf_job(
        &self,
        template_pdf_path: String,
        event_sink: Arc<dyn RuntimeEventSink>,
    ) -> HostResult<String> {
        let inner = Arc::clone(&self.inner);
        Ok(host_workflow::start_host_workflow_job(
            Arc::clone(&inner),
            "replace_template_pdf",
            host_workflow::HostWorkflowResultKind::Workspace,
            true,
            Arc::clone(&event_sink),
            move || {
                let workspace = project_lifecycle::replace_template_pdf(
                    &inner,
                    template_pdf_path,
                    &*event_sink,
                )?;
                let project_path = PathBuf::from(workspace.project.project_path.clone());
                let mut app = inner.lock();
                app.current_project = Some(workspace.project.clone());
                drop(app);
                app_jobs::spawn_template_aruco_detection_job(
                    Arc::clone(&inner),
                    project_path,
                    Arc::clone(&event_sink),
                );
                Ok(workspace)
            },
        ))
    }

    pub fn export_stamped_template_pdf(
        &self,
        destination_path: String,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<ExamWorkspaceState> {
        template_export::export_stamped_template_pdf(&self.inner, destination_path, event_sink)
    }

    pub fn start_export_stamped_template_pdf_job(
        &self,
        destination_path: String,
        event_sink: Arc<dyn RuntimeEventSink>,
    ) -> HostResult<String> {
        let inner = Arc::clone(&self.inner);
        Ok(host_workflow::start_host_workflow_job(
            Arc::clone(&inner),
            "export_stamped_template_pdf",
            host_workflow::HostWorkflowResultKind::Workspace,
            true,
            Arc::clone(&event_sink),
            move || {
                let workspace = template_export::export_stamped_template_pdf(
                    &inner,
                    destination_path,
                    &*event_sink,
                )?;
                let mut app = inner.lock();
                app.current_project = Some(workspace.project.clone());
                Ok(workspace)
            },
        ))
    }

    pub fn run_smoke_ping(&self, event_sink: &dyn RuntimeEventSink) -> HostResult<SmokePingResult> {
        self.inner.run_smoke_ping(event_sink)
    }

    pub fn run_results_export(
        &self,
        input: crate::models::RunResultsExportInput,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<crate::models::ResultsExportResponse> {
        results_export::run_results_export(&self.inner, input, event_sink)
    }

    pub fn list_llm_models(
        &self,
        event_sink: &dyn RuntimeEventSink,
        provider_name: String,
        base_url: String,
        api_key: Option<String>,
    ) -> HostResult<Vec<crate::models::VisionCapableModel>> {
        self.inner
            .list_llm_models(event_sink, provider_name, base_url, api_key)
    }

    #[doc(hidden)]
    pub fn run_smoke_ping_job(
        &self,
        event_sink: &dyn RuntimeEventSink,
        request_payload: Value,
    ) -> HostResult<crate::worker::CompletedWorkerJob> {
        runtime::run_smoke_ping_job(&self.inner, event_sink, request_payload)
    }

    pub fn cancel_active_job(&self, job_id: Option<String>) -> HostResult<ShellState> {
        let job_id = {
            let app = self.inner.lock();
            job_id.unwrap_or_else(|| {
                app.scheduler
                    .active_job_id()
                    .map(String::from)
                    .unwrap_or_default()
            })
        };
        let cancel_id =
            host_workflow::host_workflow_child_for_cancel(&self.inner, &job_id).unwrap_or(job_id);
        let mut app = self.inner.lock();
        app.scheduler.cancel_job(&cancel_id)?;
        Ok(app.shell_state())
    }

    pub fn workspace_state(&self) -> HostResult<ExamWorkspaceState> {
        let mut app = self.inner.lock();
        let project_path = app.current_project_path()?;
        let workspace = project_store::load_exam_workspace_state(&project_path)?;
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn recover_interrupted_student_workflow(&self) -> HostResult<ExamWorkspaceState> {
        let project_path = {
            let app = self.inner.lock();
            app.ensure_no_active_job()?;
            app.current_project_path()?
        };
        project_store::mark_interrupted_student_workflow_runs(
            &project_path,
            "Workflow was recovered because no desktop job was active. Start workflow again to retry.",
        )?;
        let mut app = self.inner.lock();
        let workspace = project_store::load_exam_workspace_state(&project_path)?;
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn current_project_path_for_commands(&self) -> HostResult<std::path::PathBuf> {
        let app = self.inner.lock();
        app.current_project_path()
    }

    pub fn save_question_edits(&self, edits: Vec<QuestionEdit>) -> HostResult<ExamWorkspaceState> {
        let mut app = self.inner.lock();
        let project_path = app.current_project_path()?;
        let workspace = project_store::save_question_edits(&project_path, &edits)?;
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn save_redaction_regions(
        &self,
        regions: Vec<TemplateRedactionRegionInput>,
    ) -> HostResult<ExamWorkspaceState> {
        let mut app = self.inner.lock();
        let project_path = app.current_project_path()?;
        let workspace = project_store::save_redaction_regions(&project_path, &regions)?;
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn approve_template_setup(&self, settings: &AppSettings) -> HostResult<ExamWorkspaceState> {
        lms_binding::sync_student_roster_tokens_from_cached_lms(
            self,
            settings,
            "Template approval",
        )?;
        let mut app = self.inner.lock();
        let project_path = app.current_project_path()?;
        let workspace = project_store::approve_template_setup(&project_path)?;
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn ensure_automatic_rubric_jobs(
        &self,
        settings: AppSettings,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<()> {
        let project_path = {
            let app = self.inner.lock();
            match app.current_project_path_optional() {
                Some(p) => p,
                None => return Ok(()),
            }
        };
        project_lifecycle::enqueue_auto_rubric_jobs_if_eligible(
            &self.inner,
            &project_path,
            event_sink,
            &settings,
        )
    }

    pub fn skip_template_redaction(&self) -> HostResult<ExamWorkspaceState> {
        let mut app = self.inner.lock();
        let project_path = app.current_project_path()?;
        let workspace = project_store::skip_template_redaction(&project_path)?;
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn shell_state(&self) -> HostResult<ShellState> {
        Ok(self.inner.lock().shell_state())
    }

    pub fn save_project_config(
        &self,
        config: ProjectConfig,
        settings: &AppSettings,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.inner.lock();
        let project_path = app.current_project_path()?;
        let _ = workspace_actions::save_project_config(&project_path, config)?;
        drop(app);
        let _ = results_lms::sync_results_lms_assignment_context(&project_path, settings)?;
        let workspace = project_store::load_exam_workspace_state(&project_path)?;
        let mut app = self.inner.lock();
        app.current_project = Some(workspace.project.clone());
        drop(app);
        let _ = lms_roster_cache::ensure_project_preload(&self.inner, settings)?;
        Ok(workspace)
    }

    pub fn generate_question_rubric(
        &self,
        question_id: String,
        replace_existing: bool,
        settings: AppSettings,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.inner.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        let workspace = workspace_actions::generate_question_rubric(
            &self.inner,
            &project_path,
            &question_id,
            replace_existing,
            &settings,
            event_sink,
        )?;
        let mut app = self.inner.lock();
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn start_generate_question_rubric_job(
        &self,
        question_id: String,
        replace_existing: bool,
        settings: AppSettings,
        event_sink: Arc<dyn RuntimeEventSink>,
    ) -> HostResult<String> {
        let inner = Arc::clone(&self.inner);
        Ok(host_workflow::start_host_workflow_job(
            Arc::clone(&inner),
            "generate_question_rubric",
            host_workflow::HostWorkflowResultKind::Workspace,
            true,
            Arc::clone(&event_sink),
            move || {
                let project_path = {
                    let app = inner.lock();
                    app.current_project_path()?
                };
                let workspace = workspace_actions::generate_question_rubric(
                    &inner,
                    &project_path,
                    &question_id,
                    replace_existing,
                    &settings,
                    &*event_sink,
                )?;
                let mut app = inner.lock();
                app.current_project = Some(workspace.project.clone());
                Ok(workspace)
            },
        ))
    }

    pub fn reanalyze_question(
        &self,
        question_id: String,
        settings: AppSettings,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.inner.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        let workspace = workspace_actions::reanalyze_question(
            &self.inner,
            &project_path,
            &question_id,
            &settings,
            event_sink,
        )?;
        let mut app = self.inner.lock();
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn start_reanalyze_question_job(
        &self,
        question_id: String,
        settings: AppSettings,
        event_sink: Arc<dyn RuntimeEventSink>,
    ) -> HostResult<String> {
        let inner = Arc::clone(&self.inner);
        Ok(host_workflow::start_host_workflow_job(
            Arc::clone(&inner),
            "reanalyze_question",
            host_workflow::HostWorkflowResultKind::Workspace,
            true,
            Arc::clone(&event_sink),
            move || {
                let project_path = {
                    let app = inner.lock();
                    app.current_project_path()?
                };
                let workspace = workspace_actions::reanalyze_question(
                    &inner,
                    &project_path,
                    &question_id,
                    &settings,
                    &*event_sink,
                )?;
                let mut app = inner.lock();
                app.current_project = Some(workspace.project.clone());
                Ok(workspace)
            },
        ))
    }

    pub fn save_rubric_update(&self, input: RubricUpdateInput) -> HostResult<ExamWorkspaceState> {
        let mut app = self.inner.lock();
        let project_path = app.current_project_path()?;
        let workspace = workspace_actions::save_rubric_update(&project_path, input)?;
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn save_criterion_score(
        &self,
        input: SaveCriterionScoreInput,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.inner.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        let workspace = moderation::save_criterion_score(&project_path, input)?;
        let mut app = self.inner.lock();
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn save_moderated_score(
        &self,
        input: SaveModeratedScoreInput,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.inner.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        let workspace = moderation::save_moderated_score(&project_path, input)?;
        let mut app = self.inner.lock();
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn save_moderated_feedback(
        &self,
        input: SaveModeratedFeedbackInput,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.inner.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        let workspace = moderation::save_moderated_feedback(&project_path, input)?;
        let mut app = self.inner.lock();
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn set_moderation_question_reviewed(
        &self,
        input: SetModerationQuestionReviewedInput,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.inner.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        let workspace = moderation::set_question_reviewed(&project_path, input)?;
        let mut app = self.inner.lock();
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn run_student_intake(
        &self,
        inputs: Vec<StudentIntakeInput>,
        event_sink: &dyn RuntimeEventSink,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.inner.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        let workspace =
            workspace_actions::run_student_intake(&self.inner, &project_path, inputs, event_sink)?;
        let mut app = self.inner.lock();
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn start_student_intake_job(
        &self,
        inputs: Vec<StudentIntakeInput>,
        event_sink: Arc<dyn RuntimeEventSink>,
    ) -> HostResult<String> {
        let inner = Arc::clone(&self.inner);
        Ok(host_workflow::start_host_workflow_job(
            Arc::clone(&inner),
            "run_student_intake",
            host_workflow::HostWorkflowResultKind::Workspace,
            true,
            Arc::clone(&event_sink),
            move || {
                let project_path = {
                    let app = inner.lock();
                    app.current_project_path()?
                };
                let workspace = workspace_actions::run_student_intake(
                    &inner,
                    &project_path,
                    inputs,
                    &*event_sink,
                )?;
                let mut app = inner.lock();
                app.current_project = Some(workspace.project.clone());
                Ok(workspace)
            },
        ))
    }

    pub fn save_student_intake_page_order(
        &self,
        input: crate::models::StudentIntakePageOrderUpdateInput,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.inner.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        let workspace = workspace_actions::save_student_intake_page_order(&project_path, input)?;
        let mut app = self.inner.lock();
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn delete_student_submission(
        &self,
        input: DeleteStudentSubmissionInput,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.inner.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        let workspace = student_submission_delete::delete_student_submission(&project_path, input)?;
        let mut app = self.inner.lock();
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn start_student_workflow_job(
        &self,
        settings: AppSettings,
        event_sink: Arc<dyn RuntimeEventSink>,
    ) -> HostResult<String> {
        let inner = Arc::clone(&self.inner);
        {
            let app = inner.lock();
            if app.scheduler.has_active_jobs() {
                return Err(HostError::Conflict(
                    "A desktop job is already active in this session.".into(),
                ));
            }
        }
        Ok(host_workflow::start_host_workflow_job(
            Arc::clone(&inner),
            "begin_student_workflow",
            host_workflow::HostWorkflowResultKind::Workspace,
            true,
            Arc::clone(&event_sink),
            move || {
                let workspace = inner.begin_student_workflow(&settings, &*event_sink)?;
                let mut app = inner.lock();
                app.current_project = Some(workspace.project.clone());
                Ok(workspace)
            },
        ))
    }

    pub fn start_confirm_student_alignment_job(
        &self,
        input: student_workflow::StudentWorkflowAlignmentUpdateInput,
        settings: AppSettings,
        event_sink: Arc<dyn RuntimeEventSink>,
    ) -> HostResult<String> {
        let inner = Arc::clone(&self.inner);
        Ok(host_workflow::start_host_workflow_job(
            Arc::clone(&inner),
            "confirm_student_alignment",
            host_workflow::HostWorkflowResultKind::Workspace,
            true,
            Arc::clone(&event_sink),
            move || {
                let workspace = inner.confirm_student_alignment(input, &settings, &*event_sink)?;
                let mut app = inner.lock();
                app.current_project = Some(workspace.project.clone());
                Ok(workspace)
            },
        ))
    }

    pub fn start_confirm_student_parse_review_job(
        &self,
        input: student_workflow::StudentWorkflowParseReviewInput,
        settings: AppSettings,
        event_sink: Arc<dyn RuntimeEventSink>,
    ) -> HostResult<String> {
        let inner = Arc::clone(&self.inner);
        Ok(host_workflow::start_host_workflow_job(
            Arc::clone(&inner),
            "confirm_student_parse_review",
            host_workflow::HostWorkflowResultKind::Workspace,
            true,
            Arc::clone(&event_sink),
            move || {
                let workspace =
                    inner.confirm_student_parse_review(input, &settings, &*event_sink)?;
                let mut app = inner.lock();
                app.current_project = Some(workspace.project.clone());
                Ok(workspace)
            },
        ))
    }

    pub fn start_confirm_student_detect_review_job(
        &self,
        input: student_workflow::StudentWorkflowDetectReviewInput,
        settings: AppSettings,
        event_sink: Arc<dyn RuntimeEventSink>,
    ) -> HostResult<String> {
        let inner = Arc::clone(&self.inner);
        Ok(host_workflow::start_host_workflow_job(
            Arc::clone(&inner),
            "confirm_student_detect_review",
            host_workflow::HostWorkflowResultKind::Workspace,
            true,
            Arc::clone(&event_sink),
            move || {
                let workspace =
                    inner.confirm_student_detect_review(input, &settings, &*event_sink)?;
                let mut app = inner.lock();
                app.current_project = Some(workspace.project.clone());
                Ok(workspace)
            },
        ))
    }

    pub fn save_student_alignment_review(
        &self,
        input: student_workflow::StudentWorkflowAlignmentUpdateInput,
    ) -> HostResult<ExamWorkspaceState> {
        let workspace = self.inner.save_student_alignment_review(input)?;
        let mut app = self.inner.lock();
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn save_student_parse_review(
        &self,
        input: student_workflow::StudentWorkflowParseReviewInput,
    ) -> HostResult<ExamWorkspaceState> {
        let workspace = self.inner.save_student_parse_review(input)?;
        let mut app = self.inner.lock();
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn save_student_detect_review(
        &self,
        input: student_workflow::StudentWorkflowDetectReviewInput,
    ) -> HostResult<ExamWorkspaceState> {
        let workspace = self.inner.save_student_detect_review(input)?;
        let mut app = self.inner.lock();
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn attach_app_handle(&self, app_handle: AppHandle) {
        let mut app = self.inner.lock();
        app.app_handle = Some(app_handle);
        // Bootstrap runs before Tauri can resolve the packaged resource directory, so once the
        // app handle exists we must relaunch against the current runtime source of truth.
        app.reload_worker_for_current_runtime();
    }

    pub fn compute_lms_binding_token(
        &self,
        course_id: String,
        canvas_user_id: String,
        settings: &crate::models::AppSettings,
    ) -> HostResult<String> {
        lms_binding::compute_lms_binding_token(self, course_id, canvas_user_id, settings)
    }

    /// True if this Canvas user already has a processed canonical submission in the current project
    /// (binding token match in `student_roster` + matching `student_intake` row with PDF on disk).
    pub fn prior_canonical_submission_exists_for_lms_student(
        &self,
        course_id: String,
        canvas_user_id: String,
        settings: &crate::models::AppSettings,
    ) -> HostResult<bool> {
        lms_binding::prior_canonical_submission_exists_for_lms_student(
            self,
            course_id,
            canvas_user_id,
            settings,
        )
    }

    pub fn resolve_lms_student_ref(
        &self,
        course_id: String,
        canvas_user_id: String,
        settings: &crate::models::AppSettings,
    ) -> HostResult<StudentRosterMatchResult> {
        lms_binding::resolve_lms_student_ref(self, course_id, canvas_user_id, settings)
    }

    pub fn student_roster_sync_course_id(&self) -> HostResult<Option<String>> {
        lms_binding::student_roster_sync_course_id(self)
    }

    pub fn lms_roster_cache_snapshot(
        &self,
        settings: &crate::models::AppSettings,
    ) -> HostResult<LmsRosterCacheSnapshot> {
        lms_roster_cache::snapshot(self, settings)
    }

    pub fn ensure_lms_roster_preload(
        &self,
        settings: &crate::models::AppSettings,
    ) -> HostResult<LmsRosterCacheSnapshot> {
        let project_path = {
            let app = self.inner.lock();
            app.current_project_path_optional()
        };
        if let Some(project_path) = project_path {
            let _ = results_lms::sync_results_lms_assignment_context(&project_path, settings)?;
        }
        let snapshot = lms_roster_cache::ensure_preload(self, settings)?;
        if matches!(snapshot.status, crate::models::LmsRosterCacheStatus::Ready) {
            lms_binding::sync_student_roster_tokens_from_cached_lms(
                self,
                settings,
                "LMS roster preload",
            )?;
        }
        Ok(snapshot)
    }

    pub fn save_results_lms_assignment(
        &self,
        settings: &crate::models::AppSettings,
        input: SaveResultsLmsAssignmentInput,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.inner.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        let workspace = results_lms::save_results_lms_assignment(&project_path, settings, input)?;
        let mut app = self.inner.lock();
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn set_submission_result_finalized(
        &self,
        input: SetSubmissionResultFinalizedInput,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.inner.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        let workspace = results_lms::set_submission_result_finalized(&project_path, input)?;
        let mut app = self.inner.lock();
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn finalize_ready_results(
        &self,
        input: crate::models::FinalizeReadyResultsInput,
    ) -> HostResult<ExamWorkspaceState> {
        let app = self.inner.lock();
        let project_path = app.current_project_path()?;
        drop(app);
        let workspace = results_lms::finalize_ready_results(&project_path, input)?;
        let mut app = self.inner.lock();
        app.current_project = Some(workspace.project.clone());
        Ok(workspace)
    }

    pub fn load_job_trace(
        &self,
        job_id: Option<String>,
        command_name: Option<String>,
    ) -> HostResult<Option<JobTraceState>> {
        let app = self.inner.lock();
        let project_path = app.current_project_path()?;
        workspace_actions::load_trace(&project_path, job_id, command_name)
    }

    pub fn list_job_traces(&self) -> HostResult<Vec<crate::models::JobTraceSummary>> {
        let app = self.inner.lock();
        let project_path = app.current_project_path()?;
        project_store::list_job_traces(&project_path)
    }

    pub(crate) fn clone_inner(&self) -> Arc<AppStateInner> {
        Arc::clone(&self.inner)
    }

    pub(crate) fn current_project_path(&self) -> HostResult<PathBuf> {
        let app = self.inner.lock();
        app.current_project_path()
    }

    #[doc(hidden)]
    pub fn __test_reset_worker_state(&self) {
        let mut app = self.inner.inner.lock().expect("lock poisoned");
        app.worker = None;
        app.worker_status = WorkerStatus::Starting;
        app.last_runtime_error = None;
    }
}

impl DesktopApp {
    fn bundled_resource_dir(&self) -> Option<PathBuf> {
        self.app_handle
            .as_ref()
            .and_then(|app_handle| app_handle.path().resource_dir().ok())
    }

    fn ensure_worker(&mut self) -> HostResult<()> {
        if self.worker.is_some() {
            return Ok(());
        }
        self.worker_status = WorkerStatus::Starting;
        match crate::worker::WorkerClient::launch(
            self.bundled_resource_dir().as_deref(),
            self.worker_runtime_source(),
        ) {
            Ok(worker) => {
                self.worker = Some(worker);
                self.worker_status = WorkerStatus::Ready;
                self.last_runtime_error = None;
                Ok(())
            }
            Err(err) => {
                self.worker_status = WorkerStatus::Error;
                self.last_runtime_error = Some(err.to_string());
                Err(err)
            }
        }
    }

    fn ensure_no_active_job(&self) -> HostResult<()> {
        if self.scheduler.has_active_jobs() {
            if let Some(job_id) = self.scheduler.active_job_id() {
                return Err(HostError::Conflict(format!(
                    "Desktop job '{}' is still active.",
                    job_id
                )));
            }
        }
        Ok(())
    }

    fn take_worker(&mut self) -> HostResult<crate::worker::WorkerClient> {
        self.worker
            .take()
            .ok_or_else(|| HostError::Worker("Desktop worker is not available.".into()))
    }

    fn reload_worker_for_current_runtime(&mut self) {
        self.worker_status = WorkerStatus::Starting;
        match crate::worker::WorkerClient::launch(
            self.bundled_resource_dir().as_deref(),
            self.worker_runtime_source(),
        ) {
            Ok(worker) => {
                self.worker = Some(worker);
                self.worker_status = WorkerStatus::Ready;
                self.last_runtime_error = None;
            }
            Err(err) => {
                self.worker = None;
                self.worker_status = WorkerStatus::Error;
                self.last_runtime_error = Some(err.to_string());
            }
        }
    }

    fn worker_runtime_source(&self) -> WorkerRuntimeSource {
        if !cfg!(debug_assertions) && self.bundled_resource_dir().is_some() {
            WorkerRuntimeSource::BundledRequired
        } else {
            WorkerRuntimeSource::RepoFallback
        }
    }

    fn shell_state(&self) -> ShellState {
        ShellState {
            current_project: self.current_project.clone(),
            worker_status: app_jobs::current_worker_status(self),
            worker_activity: WorkerActivity {
                active_jobs: self.scheduler.active_job_summaries(),
                pending_job_count: self.scheduler.pending_job_count(),
            },
            last_runtime_error: self.last_runtime_error.clone(),
            debug_features: self.debug_features.clone(),
        }
    }

    fn current_project_path(&self) -> HostResult<PathBuf> {
        self.current_project_path_optional()
            .ok_or_else(|| HostError::Project("No project is currently open.".into()))
    }

    fn current_project_path_optional(&self) -> Option<PathBuf> {
        self.current_project
            .as_ref()
            .map(|summary| PathBuf::from(&summary.project_path))
    }

    fn refresh_current_project(&mut self, project_path: &Path) -> HostResult<()> {
        self.current_project = Some(project_store::open_project(project_path)?);
        Ok(())
    }
}

fn parse_debug_features<I>(args: I) -> DebugFeatures
where
    I: IntoIterator<Item = OsString>,
{
    DebugFeatures {
        redaction_toggle: args
            .into_iter()
            .any(|arg| arg.to_string_lossy() == "--scriptscore-debug-redaction-toggle"),
    }
}

fn emit_runtime_job_event(event_sink: &dyn RuntimeEventSink, event: RuntimeJobEvent) {
    event_sink.emit_runtime_event(event);
}

fn current_run_timestamp() -> String {
    format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::from_secs(0))
            .as_secs()
    )
}

fn recover_abandoned_project_jobs(project_path: &Path) -> HostResult<Option<String>> {
    let finished_at = current_run_timestamp();
    let message =
        "Recovered desktop jobs that were still marked active after a prior app session ended.";
    let runtime_count =
        project_store::mark_abandoned_runtime_jobs(project_path, &finished_at, message)?;
    let workflow_count =
        project_store::mark_abandoned_host_workflows(project_path, &finished_at, message)?;
    let total = runtime_count + workflow_count;
    let interrupted_count = project_store::mark_interrupted_student_workflow_runs(
        project_path,
        "Workflow was interrupted when the previous app session ended. Start workflow again to retry.",
    )?;
    if total == 0 && interrupted_count == 0 {
        return Ok(None);
    }
    let job_record_notice = (total > 0).then(|| {
        format!(
            "Recovered {total} stale desktop job record{} from a prior session",
            if total == 1 { "" } else { "s" }
        )
    });
    let workflow_notice = (interrupted_count > 0).then(|| {
        format!(
            "marked {interrupted_count} interrupted student workflow{} as stopped",
            if interrupted_count == 1 { "" } else { "s" }
        )
    });
    Ok(Some(match (job_record_notice, workflow_notice) {
        (Some(job_record_notice), Some(workflow_notice)) => {
            format!("{job_record_notice} and {workflow_notice}.")
        }
        (Some(job_record_notice), None) => format!("{job_record_notice}."),
        (None, Some(workflow_notice)) => {
            format!("Recovered prior session state and {workflow_notice}.")
        }
        (None, None) => unreachable!("recovery message requires recovered state"),
    }))
}

fn runtime_error_after_project_open(
    current_error: Option<String>,
    recovery_message: Option<String>,
    worker_available: bool,
) -> Option<String> {
    if recovery_message.is_some() {
        return recovery_message;
    }
    if worker_available {
        return None;
    }
    current_error
}

#[cfg(test)]
mod tests {
    use super::{runtime_error_after_project_open, AppState};

    struct NoopEventSink;

    impl super::RuntimeEventSink for NoopEventSink {
        fn emit_runtime_event(&self, _event: crate::models::RuntimeJobEvent) {}
    }

    fn assert_no_open_project<T: std::fmt::Debug>(result: crate::errors::HostResult<T>) {
        let error = result.expect_err("operation should require an open project");
        assert!(
            error.to_string().contains("No project is currently open"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn project_open_runtime_error_policy_preserves_recovery_and_clears_stale_probe_errors() {
        assert_eq!(
            runtime_error_after_project_open(Some("Ollama is unreachable.".into()), None, true,),
            None
        );
        assert_eq!(
            runtime_error_after_project_open(Some("worker launch failed".into()), None, false,),
            Some("worker launch failed".into())
        );
        assert_eq!(
            runtime_error_after_project_open(
                Some("Ollama is unreachable.".into()),
                Some("Recovered abandoned jobs.".into()),
                true,
            ),
            Some("Recovered abandoned jobs.".into())
        );
    }

    #[test]
    fn app_state_project_scoped_facade_methods_reject_without_open_project() {
        let state = AppState::bootstrap_with_args([std::ffi::OsString::from("scriptscore")]);
        let sink = NoopEventSink;
        let settings = crate::models::AppSettings::default();

        assert_no_open_project(state.workspace_state());
        assert_no_open_project(state.recover_interrupted_student_workflow());
        assert_no_open_project(state.current_project_path_for_commands());
        assert_no_open_project(state.replace_template_pdf("template.pdf".into(), &sink));
        assert_no_open_project(state.export_stamped_template_pdf("template.pdf".into(), &sink));
        assert_no_open_project(state.run_results_export(
            crate::models::RunResultsExportInput {
                format: crate::models::ResultsExportFormat::Csv,
                student_refs: vec!["student_1".into()],
                destination_path: "results.csv".into(),
            },
            &sink,
        ));
        assert_no_open_project(state.save_question_edits(Vec::new()));
        assert_no_open_project(state.save_redaction_regions(Vec::new()));
        assert_no_open_project(state.approve_template_setup(&settings));
        assert_no_open_project(state.skip_template_redaction());
        assert_no_open_project(
            state.save_project_config(crate::models::ProjectConfig::default(), &settings),
        );
        assert_no_open_project(state.generate_question_rubric(
            "q1".into(),
            false,
            settings.clone(),
            &sink,
        ));
        assert_no_open_project(state.reanalyze_question("q1".into(), settings.clone(), &sink));
        assert_no_open_project(state.save_rubric_update(crate::models::RubricUpdateInput {
            question_id: "q1".into(),
            criteria: Vec::new(),
            approve: false,
            rubric_edit_impact: None,
        }));
        assert_no_open_project(state.save_criterion_score(
            crate::models::SaveCriterionScoreInput {
                student_ref: "student_1".into(),
                question_id: "q1".into(),
                criterion_index: 0,
                points_awarded: 0,
            },
        ));
        assert_no_open_project(state.save_moderated_score(
            crate::models::SaveModeratedScoreInput {
                student_ref: "student_1".into(),
                question_id: "q1".into(),
                moderated_total_points: 0,
            },
        ));
        assert_no_open_project(state.save_moderated_feedback(
            crate::models::SaveModeratedFeedbackInput {
                student_ref: "student_1".into(),
                question_id: "q1".into(),
                feedback_text: "feedback".into(),
            },
        ));
        assert_no_open_project(state.set_moderation_question_reviewed(
            crate::models::SetModerationQuestionReviewedInput {
                question_id: "q1".into(),
                reviewed: true,
            },
        ));
        assert_no_open_project(state.run_student_intake(Vec::new(), &sink));
        assert_no_open_project(state.save_student_intake_page_order(
            crate::models::StudentIntakePageOrderUpdateInput {
                student_ref: "student_1".into(),
                exam_page_paths: Vec::new(),
            },
        ));
        assert_no_open_project(state.delete_student_submission(
            crate::models::DeleteStudentSubmissionInput {
                student_ref: "student_1".into(),
            },
        ));
        assert_no_open_project(state.save_student_alignment_review(
            super::student_workflow::StudentWorkflowAlignmentUpdateInput {
                student_ref: "student_1".into(),
                pages: Vec::new(),
            },
        ));
        assert_no_open_project(state.save_student_parse_review(
            super::student_workflow::StudentWorkflowParseReviewInput {
                student_ref: "student_1".into(),
                question_id: "q1".into(),
                corrected_text: "answer".into(),
            },
        ));
        assert_no_open_project(state.save_student_detect_review(
            super::student_workflow::StudentWorkflowDetectReviewInput {
                student_ref: "student_1".into(),
                resolutions: Vec::new(),
            },
        ));
        assert_no_open_project(state.save_results_lms_assignment(
            &settings,
            crate::models::SaveResultsLmsAssignmentInput {
                assignment_id: Some("assignment_1".into()),
            },
        ));
        assert_no_open_project(state.set_submission_result_finalized(
            crate::models::SetSubmissionResultFinalizedInput {
                student_ref: "student_1".into(),
                finalized: true,
            },
        ));
        assert_no_open_project(state.finalize_ready_results(
            crate::models::FinalizeReadyResultsInput {
                student_refs: vec!["student_1".into()],
            },
        ));
        assert_no_open_project(state.load_job_trace(None, None));
        assert_no_open_project(state.list_job_traces());

        state
            .ensure_automatic_rubric_jobs(settings, &sink)
            .expect("no open project should make automatic rubric enqueue a no-op");
    }

    #[test]
    fn resolve_lms_student_ref_uses_persisted_student_roster() {
        use std::path::Path;
        use std::path::PathBuf;

        use crate::project_store::schema::project_db_path;
        use rusqlite::Connection;

        let _guard = crate::test_support::lock_env_vars();
        crate::secrets::__test_set_binding_hmac_secret_bytes(vec![0x11u8; 32]);
        let test_root = std::env::temp_dir().join(format!(
            "scriptscore-binding-persist-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(std::time::Duration::from_secs(0))
                .as_millis()
        ));
        let _projects_root =
            crate::test_support::EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);

        let created = crate::project_store::create_project(
            "Binding Test",
            None,
            None,
            Some("persisted-course-id".into()),
            &crate::models::InstructorProfile::default(),
        )
        .expect("project should be created");

        let state = AppState::bootstrap();
        let settings = crate::models::AppSettings::default();
        state
            .open_project(PathBuf::from(&created.project_path), &settings)
            .expect("project should open");

        let token_one = state
            .compute_lms_binding_token("persisted-course-id".into(), "canvas_42".into(), &settings)
            .expect("token should compute");
        let token_two = state
            .compute_lms_binding_token("persisted-course-id".into(), "canvas_99".into(), &settings)
            .expect("token should compute");
        crate::project_store::sync_student_roster_tokens(
            Path::new(&created.project_path),
            &[token_one.clone(), token_two.clone()],
        )
        .expect("student roster should persist");

        let resolved_one = state
            .resolve_lms_student_ref("persisted-course-id".into(), "canvas_42".into(), &settings)
            .expect("roster lookup should resolve first LMS student");
        assert_eq!(resolved_one.student_ref, "student_1");

        let resolved_two = state
            .resolve_lms_student_ref("persisted-course-id".into(), "canvas_99".into(), &settings)
            .expect("roster lookup should resolve second LMS student");
        assert_eq!(resolved_two.student_ref, "student_2");

        let project_path = Path::new(&created.project_path);
        let connection =
            Connection::open(project_db_path(project_path)).expect("project db should open");
        let count_student_1: i64 = connection
            .query_row(
                "SELECT COUNT(1) FROM student_roster WHERE student_ref = ?1 AND binding_token_hex = ?2",
                rusqlite::params!["student_1", token_one],
                |row| row.get(0),
            )
            .expect("count should query");
        assert_eq!(count_student_1, 1);
        let count_student_2: i64 = connection
            .query_row(
                "SELECT COUNT(1) FROM student_roster WHERE student_ref = ?1 AND binding_token_hex = ?2",
                rusqlite::params!["student_2", token_two],
                |row| row.get(0),
            )
            .expect("count should query");
        assert_eq!(count_student_2, 1);

        drop(connection);
        std::fs::remove_dir_all(&test_root).expect("test project should clean up");
    }

    #[test]
    fn lms_roster_preload_persists_student_roster_tokens_for_intake() {
        use std::path::PathBuf;
        use std::sync::Arc;
        use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

        use crate::lms::{__test_set_roster_fetch_override, ActiveLmsRosterLoader, LmsRosterRow};
        use crate::models::{AppSettings, InstructorProfile, LmsRosterCacheStatus};

        struct OverrideGuard;
        impl Drop for OverrideGuard {
            fn drop(&mut self) {
                __test_set_roster_fetch_override(None);
            }
        }

        let _guard = crate::test_support::lock_env_vars();
        let _override_guard = OverrideGuard;
        crate::secrets::__test_set_binding_hmac_secret_bytes(vec![0x22u8; 32]);
        let test_root = std::env::temp_dir().join(format!(
            "scriptscore-binding-preload-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_millis()
        ));
        let _projects_root =
            crate::test_support::EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
        __test_set_roster_fetch_override(Some(Arc::new(|loader| match loader {
            ActiveLmsRosterLoader::Canvas { .. } => Ok(vec![LmsRosterRow {
                user_id: "canvas_42".into(),
                display_name: "Student Forty Two".into(),
                sort_key: "forty two, student".into(),
                email: None,
                login_id: None,
            }]),
        })));

        let created = crate::project_store::create_project(
            "Binding Preload Test",
            None,
            None,
            Some("preload-course-id".into()),
            &InstructorProfile::default(),
        )
        .expect("project should be created");
        let state = AppState::bootstrap();
        let settings = AppSettings {
            lms_provider: "canvas".into(),
            lms_canvas_base_url: "https://canvas.example.test".into(),
            lms_canvas_api_key: Some("token".into()),
            ..AppSettings::default()
        };
        state
            .open_project(PathBuf::from(&created.project_path), &settings)
            .expect("project should open");

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = state
                .ensure_lms_roster_preload(&settings)
                .expect("preload should be ensured");
            if snapshot.status == LmsRosterCacheStatus::Ready {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "roster preload should become ready"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        let resolved = state
            .resolve_lms_student_ref("preload-course-id".into(), "canvas_42".into(), &settings)
            .expect("student intake should resolve from preload-synced roster");
        assert_eq!(resolved.student_ref, "student_1");

        let roster =
            crate::project_store::load_student_roster(std::path::Path::new(&created.project_path))
                .expect("student roster should load");
        assert_eq!(roster.len(), 1);
        assert_eq!(roster[0].student_ref, "student_1");

        std::fs::remove_dir_all(&test_root).expect("test project should clean up");
    }

    #[test]
    fn bootstrap_records_worker_error_when_python_is_invalid() {
        use std::path::PathBuf;

        let _guard = crate::test_support::lock_env_vars();
        let missing_python = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("repo root")
            .to_path_buf()
            .join("missing-python")
            .join("python-does-not-exist");
        let _python = crate::test_support::EnvVarGuard::set("SCRIPTSCORE_PYTHON", &missing_python);

        let state = AppState::bootstrap();
        let shell = state.shell_state().expect("shell state should load");

        assert!(matches!(
            shell.worker_status,
            crate::models::WorkerStatus::Error
        ));
        assert!(shell.last_runtime_error.is_some());
    }

    #[test]
    fn shell_state_debug_redaction_toggle_defaults_off() {
        let state = AppState::bootstrap_with_args([std::ffi::OsString::from("scriptscore")]);
        let shell = state.shell_state().expect("shell state should load");

        assert!(!shell.debug_features.redaction_toggle);
    }

    #[test]
    fn shell_state_debug_redaction_toggle_reads_startup_flag() {
        let state = AppState::bootstrap_with_args([
            std::ffi::OsString::from("scriptscore"),
            std::ffi::OsString::from("--scriptscore-debug-redaction-toggle"),
        ]);
        let shell = state.shell_state().expect("shell state should load");

        assert!(shell.debug_features.redaction_toggle);
    }
}
