// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::errors::{HostError, HostResult};
use crate::models::{
    ExamWorkspaceState, StudentIntakeInput, StudentIntakePageOrderUpdateInput, StudentIntakeState,
    StudentIntakeSummary, StudentWorkflowState, StudentWorkflowSubmission, WorkspaceWarning,
};
use crate::project_store;
use crate::state::runtime::ReservedJob;
use crate::worker::CompletedWorkerJob;

use super::shared::{parse_warnings, required_string, success_data};
use super::{
    run_reserved_job, start_runtime_job, AppStateInner, RuntimeEventSink, RuntimeJobRequest,
};

pub(crate) fn build_intake_default_pdf_rects_request(
    project_path: &Path,
) -> HostResult<Option<Value>> {
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    let redactions = workspace.redaction_regions;
    if redactions.is_empty() {
        return Ok(None);
    }
    let raster_sizes = template_raster_sizes_for_redaction_pages(project_path, &redactions)?;
    let raster_sizes_by_page = raster_sizes
        .into_iter()
        .map(|(page_number, (width_px, height_px))| {
            (
                page_number.to_string(),
                json!({
                    "width_px": width_px,
                    "height_px": height_px,
                }),
            )
        })
        .collect::<serde_json::Map<String, Value>>();
    Ok(Some(json!({
        "regions": redactions
            .iter()
            .map(|region| {
                json!({
                    "page_number": region.page_number,
                    "x": region.x,
                    "y": region.y,
                    "width": region.width,
                    "height": region.height,
                })
            })
            .collect::<Vec<Value>>(),
        "raster_sizes_by_page": raster_sizes_by_page,
    })))
}

struct IntakePipelineProgressGuard<'a> {
    state: &'a Arc<AppStateInner>,
}

impl<'a> IntakePipelineProgressGuard<'a> {
    fn start(state: &'a Arc<AppStateInner>, redact_jobs: u32) -> Self {
        {
            let mut app = state.lock();
            app.intake_pipeline_active = true;
            app.intake_pipeline_redact_total = redact_jobs;
        }
        Self { state }
    }
}

impl Drop for IntakePipelineProgressGuard<'_> {
    fn drop(&mut self) {
        let mut app = self.state.lock();
        app.intake_pipeline_active = false;
        app.intake_pipeline_redact_total = 0;
        app.intake_pipeline_redact_index = 0;
    }
}

struct StudentIntakePreparedInputs {
    previous_by_ref: HashMap<String, StudentIntakeSummary>,
    input_refs: HashSet<String>,
    default_regions_json: Vec<Value>,
    default_raster_json: Value,
    template_page_count: usize,
    persist_local_display_names: bool,
}

const INTAKE_RUNTIME_JOB_WAIT_TIMEOUT: Duration = Duration::from_secs(120);
const INTAKE_RUNTIME_JOB_POLL_INTERVAL: Duration = Duration::from_millis(100);

pub(crate) fn run_student_intake(
    state: &Arc<AppStateInner>,
    project_path: &Path,
    inputs: Vec<StudentIntakeInput>,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<ExamWorkspaceState> {
    if inputs.is_empty() {
        return Err(HostError::Validation(
            "At least one submission PDF is required.".into(),
        ));
    }

    let prepared = prepare_student_intake_inputs(project_path, &inputs)?;
    validate_selected_page_counts(&inputs, prepared.template_page_count)?;
    let _intake_progress = start_intake_pipeline_progress(state, inputs.len());

    let mut canonical_targets = Vec::new();
    let mut summaries = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        set_intake_redact_index(state, index);
        let (canonical_target, mut summary) = redact_and_promote_submission(
            state,
            event_sink,
            project_path,
            input,
            &prepared.default_regions_json,
            &prepared.default_raster_json,
            prepared.persist_local_display_names,
        )?;
        add_replaced_warning_if_needed(&prepared.previous_by_ref, &mut summary);
        canonical_targets.push(canonical_target);
        summaries.push(summary);
    }

    mark_intake_ingest_phase(state);

    let completed = run_ingest_for_canonical_targets(
        state,
        event_sink,
        project_path,
        &inputs,
        &canonical_targets,
    )?;
    let mut next_state = persist_student_ingest_results(project_path, summaries, &completed)?;
    merge_prior_intake_items(
        prepared.previous_by_ref,
        &prepared.input_refs,
        &mut next_state,
    );
    project_store::save_student_intake_state(project_path, &next_state)?;
    project_store::load_exam_workspace_state(project_path)
}

pub(crate) fn save_student_intake_page_order(
    project_path: &Path,
    input: StudentIntakePageOrderUpdateInput,
) -> HostResult<ExamWorkspaceState> {
    validate_page_order_update_input(&input)?;

    let mut intake_state = project_store::load_student_intake_state(project_path)?;
    let mut workflow_state = project_store::load_student_workflow_state(project_path)?;
    if page_order_change_is_locked(&workflow_state, &input.student_ref) {
        return Err(HostError::Validation(
            "Page order cannot be changed after downstream workflow processing has started.".into(),
        ));
    }

    let item = intake_item_for_page_order(&mut intake_state, &input.student_ref)?;
    let reordered_paths = normalized_reordered_paths(input.exam_page_paths);
    validate_reordered_paths_match_existing(item, &reordered_paths)?;
    let order_changed = item.exam_page_paths != reordered_paths;
    item.exam_page_paths = reordered_paths;
    item.page_count = i64::try_from(item.exam_page_paths.len()).map_err(|_| {
        HostError::Validation("Updated intake page count exceeded supported range.".into())
    })?;
    project_store::save_student_intake_state(project_path, &intake_state)?;
    if order_changed {
        invalidate_workflow_progress_for_intake_reorder(&mut workflow_state, &input.student_ref);
        workflow_state.status = workflow_status_for_submissions(&workflow_state);
        project_store::save_student_workflow_state(project_path, &workflow_state)?;
    }
    project_store::load_exam_workspace_state(project_path)
}

fn validate_page_order_update_input(input: &StudentIntakePageOrderUpdateInput) -> HostResult<()> {
    if input.student_ref.trim().is_empty() {
        return Err(HostError::Validation(
            "student_ref is required to save intake page order.".into(),
        ));
    }
    if input.exam_page_paths.is_empty() {
        return Err(HostError::Validation(
            "exam_page_paths must contain at least one page image.".into(),
        ));
    }
    Ok(())
}

fn page_order_change_is_locked(workflow_state: &StudentWorkflowState, student_ref: &str) -> bool {
    workflow_state.submissions.iter().any(|submission| {
        submission.student_ref == student_ref
            && !matches!(submission.stage.as_str(), "" | "intake_ready" | "failed")
    })
}

fn intake_item_for_page_order<'a>(
    intake_state: &'a mut StudentIntakeState,
    student_ref: &str,
) -> HostResult<&'a mut StudentIntakeSummary> {
    let item = intake_state
        .items
        .iter_mut()
        .find(|item| item.student_ref == student_ref)
        .ok_or_else(|| {
            HostError::Validation(format!(
                "No intake submission exists for '{}'.",
                student_ref
            ))
        })?;
    if item.exam_page_paths.is_empty() {
        return Err(HostError::Validation(
            "This intake submission does not have ingested exam pages yet.".into(),
        ));
    }
    Ok(item)
}

fn normalized_reordered_paths(exam_page_paths: Vec<String>) -> Vec<String> {
    exam_page_paths
        .into_iter()
        .map(|path| path.trim().to_string())
        .collect()
}

fn validate_reordered_paths_match_existing(
    item: &StudentIntakeSummary,
    reordered_paths: &[String],
) -> HostResult<()> {
    if reordered_paths.iter().any(|path| path.is_empty()) {
        return Err(HostError::Validation(
            "exam_page_paths must not contain empty paths.".into(),
        ));
    }

    let existing = item.exam_page_paths.iter().cloned().collect::<HashSet<_>>();
    let mut incoming = HashSet::new();
    for path in reordered_paths {
        if !incoming.insert(path.clone()) {
            return Err(HostError::Validation(
                "exam_page_paths must not contain duplicate paths.".into(),
            ));
        }
        if !existing.contains(path) {
            return Err(HostError::Validation(
                "exam_page_paths must contain only existing ingested pages.".into(),
            ));
        }
    }
    if reordered_paths.is_empty() {
        return Err(HostError::Validation(
            "exam_page_paths must contain at least one existing ingested page.".into(),
        ));
    }
    Ok(())
}

fn invalidate_workflow_progress_for_intake_reorder(
    workflow_state: &mut StudentWorkflowState,
    student_ref: &str,
) {
    let Some(submission) = workflow_state
        .submissions
        .iter_mut()
        .find(|submission| submission.student_ref == student_ref)
    else {
        return;
    };
    if !submission_has_downstream_progress(submission) {
        return;
    }
    submission.stage = "intake_ready".into();
    submission.latest_job_id = None;
    submission.failure_message = None;
    submission.page_artifacts.clear();
    submission.alignment_pages.clear();
    submission.answers.clear();
}

fn submission_has_downstream_progress(submission: &StudentWorkflowSubmission) -> bool {
    !submission.alignment_pages.is_empty()
        || !submission.page_artifacts.is_empty()
        || !submission.answers.is_empty()
}

fn workflow_status_for_submissions(workflow_state: &StudentWorkflowState) -> String {
    if workflow_state.submissions.is_empty() {
        "not_started".into()
    } else if workflow_state.submissions.iter().any(|submission| {
        matches!(
            submission.stage.as_str(),
            "alignment_review" | "parse_review" | "manual_grading" | "failed"
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
        .any(|submission| submission.stage != "intake_ready")
    {
        "running".into()
    } else {
        "ready".into()
    }
}

pub(crate) fn exam_page_paths_from_ingest_pdf_result(result: &Value) -> Vec<String> {
    let Some(pages) = result.get("pages").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for page in pages {
        if let Some(p) = page.get("image_path").and_then(Value::as_str) {
            if !p.trim().is_empty() {
                entries.push(p.to_string());
            }
        }
    }
    entries
}

fn template_redaction_regions_json(
    regions: &[crate::models::TemplateRedactionRegion],
) -> Vec<Value> {
    regions
        .iter()
        .map(|r| {
            json!({
                "page_number": r.page_number,
                "x": r.x,
                "y": r.y,
                "width": r.width,
                "height": r.height,
            })
        })
        .collect()
}

fn raster_sizes_by_page_json(sizes: &HashMap<i64, (i64, i64)>) -> Value {
    let mut map = serde_json::Map::new();
    for (page, (w, h)) in sizes {
        map.insert(page.to_string(), json!({ "width_px": w, "height_px": h }));
    }
    Value::Object(map)
}

fn input_redaction_regions_json(input: &StudentIntakeInput) -> Vec<Value> {
    input
        .redaction_regions_px
        .iter()
        .map(|region| {
            json!({
                "page_number": region.page_number,
                "x": region.x,
                "y": region.y,
                "width": region.width,
                "height": region.height,
            })
        })
        .collect()
}

fn input_raster_sizes_json(input: &StudentIntakeInput) -> Value {
    let mut map = serde_json::Map::new();
    for (page_number, size) in &input.raster_sizes_by_page {
        map.insert(
            page_number.to_string(),
            json!({
                "width_px": size.width_px,
                "height_px": size.height_px,
            }),
        );
    }
    Value::Object(map)
}

fn resolved_redaction_payloads(
    input: &StudentIntakeInput,
    default_regions_json: &[Value],
    default_raster_json: &Value,
) -> (Vec<Value>, Value) {
    let override_regions_json = input_redaction_regions_json(input);
    if override_regions_json.is_empty() {
        return (default_regions_json.to_vec(), default_raster_json.clone());
    }
    (override_regions_json, input_raster_sizes_json(input))
}

fn prepare_student_intake_inputs(
    project_path: &Path,
    inputs: &[StudentIntakeInput],
) -> HostResult<StudentIntakePreparedInputs> {
    let workspace = project_store::load_exam_workspace_state(project_path)?;
    let redactions = workspace.redaction_regions;
    let previous_state = project_store::load_student_intake_state(project_path)?;
    let previous_by_ref: HashMap<String, StudentIntakeSummary> = previous_state
        .items
        .into_iter()
        .map(|item| (item.student_ref.clone(), item))
        .collect();
    let input_refs: HashSet<String> = inputs
        .iter()
        .map(|item| item.student_ref.trim().to_string())
        .collect();
    let raster_sizes = if redactions.is_empty() {
        HashMap::new()
    } else {
        template_raster_sizes_for_redaction_pages(project_path, &redactions)?
    };
    Ok(StudentIntakePreparedInputs {
        previous_by_ref,
        input_refs,
        default_regions_json: template_redaction_regions_json(&redactions),
        default_raster_json: raster_sizes_by_page_json(&raster_sizes),
        template_page_count: workspace
            .template_preview_artifacts
            .iter()
            .filter_map(|artifact| usize::try_from(artifact.page_number).ok())
            .max()
            .unwrap_or(0),
        persist_local_display_names: workspace
            .project_config
            .lms_course_id
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty(),
    })
}

fn start_intake_pipeline_progress(
    state: &Arc<AppStateInner>,
    redact_jobs: usize,
) -> IntakePipelineProgressGuard<'_> {
    IntakePipelineProgressGuard::start(state, redact_jobs as u32)
}

fn set_intake_redact_index(state: &Arc<AppStateInner>, index: usize) {
    let mut app = state.lock();
    app.intake_pipeline_redact_index = index as u32;
}

fn mark_intake_ingest_phase(state: &Arc<AppStateInner>) {
    let mut app = state.lock();
    app.intake_pipeline_redact_total = 0;
}

fn canonical_targets_job_payload(canonical_targets: &[Value]) -> Value {
    json!({
        "pdf_targets": canonical_targets,
    })
}

fn validate_selected_page_counts(
    inputs: &[StudentIntakeInput],
    template_page_count: usize,
) -> HostResult<()> {
    if template_page_count == 0 {
        return Ok(());
    }
    for input in inputs {
        let page_order = normalized_desired_page_order(input);
        if !page_order.is_empty() && page_order.len() < template_page_count {
            return Err(HostError::Validation(format!(
                "Submission '{}' has fewer selected pages than the template. Replace or rescan the PDF before finalizing intake.",
                input.student_ref
            )));
        }
    }
    Ok(())
}

fn sanitized_ingest_request_payload(inputs: &[StudentIntakeInput]) -> Value {
    json!({
        "pdf_targets": inputs
            .iter()
            .map(|item| {
                let page_order = normalized_desired_page_order(item);
                if page_order.is_empty() {
                    json!({
                        "student_ref": item.student_ref,
                    })
                } else {
                    json!({
                        "student_ref": item.student_ref,
                        "page_order": page_order,
                    })
                }
            })
            .collect::<Vec<Value>>(),
    })
}

fn normalized_desired_page_order(input: &StudentIntakeInput) -> Vec<i64> {
    input
        .desired_page_order
        .iter()
        .copied()
        .filter(|page_number| *page_number > 0)
        .collect()
}

fn start_runtime_job_when_idle(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    command_name: &str,
) -> HostResult<ReservedJob> {
    let started_waiting = Instant::now();
    loop {
        match start_runtime_job(state, event_sink, command_name) {
            Ok(reserved) => return Ok(reserved),
            Err(HostError::Conflict(_))
                if started_waiting.elapsed() < INTAKE_RUNTIME_JOB_WAIT_TIMEOUT =>
            {
                thread::sleep(INTAKE_RUNTIME_JOB_POLL_INTERVAL);
            }
            Err(HostError::Conflict(_)) => {
                return Err(HostError::Conflict(format!(
                    "Timed out waiting for the desktop worker before starting '{}'.",
                    command_name
                )));
            }
            Err(err) => return Err(err),
        }
    }
}

fn redact_and_promote_submission(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    project_path: &Path,
    input: &StudentIntakeInput,
    default_regions_json: &[Value],
    default_raster_json: &Value,
    persist_local_display_name: bool,
) -> HostResult<(Value, StudentIntakeSummary)> {
    let canonical_pdf_path =
        project_store::canonical_intake_pdf_path(project_path, &input.student_ref);
    if let Some(parent) = canonical_pdf_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let (regions_json, raster_json) =
        resolved_redaction_payloads(input, default_regions_json, default_raster_json);

    let reserved_redact =
        start_runtime_job_when_idle(state, event_sink, "scans.pdf-create-redacted")?;
    let job_dir = project_store::command_output_dir(
        project_path,
        "scans.pdf-create-redacted",
        &reserved_redact.job_id,
    );
    fs::create_dir_all(&job_dir)?;
    let temp_redacted = job_dir.join("redacted.pdf");
    let completed_redact = run_reserved_job(
        state,
        event_sink,
        reserved_redact,
        RuntimeJobRequest {
            command_name: "scans.pdf-create-redacted",
            worker_request_payload: json!({
                "input_pdf_path": input.raw_pdf_path,
                "output_pdf_path": temp_redacted.to_string_lossy(),
                "regions": regions_json,
                "raster_sizes_by_page": raster_json,
                "page_order": normalized_desired_page_order(input),
                "student_ref": input.student_ref.trim(),
            }),
            persisted_request_payload: json!({
                "student_ref": input.student_ref,
                "command": "scans.pdf-create-redacted",
            }),
            output_artifacts_dir: Some(&job_dir),
            project_path: Some(project_path),
            stdin_bytes: None,
        },
    )?;
    let _ = success_data(&completed_redact.result.envelope)?;
    fs::copy(&temp_redacted, &canonical_pdf_path).map_err(|err| {
        HostError::Project(format!(
            "Could not copy redacted PDF to canonical path: {err}"
        ))
    })?;

    let canonical_target = json!({
        "student_ref": input.student_ref,
        "pdf_path": canonical_pdf_path.to_string_lossy().into_owned(),
    });
    let summary = StudentIntakeSummary {
        student_ref: input.student_ref.clone(),
        local_display_name: persisted_local_display_name(input, persist_local_display_name),
        canonical_pdf_path: canonical_pdf_path.to_string_lossy().into_owned(),
        ingest_status: "prepared".into(),
        page_count: 0,
        exam_page_paths: Vec::new(),
        warnings: Vec::new(),
        binding_token_hex: None,
    };
    Ok((canonical_target, summary))
}

fn normalized_local_student_name(input: &StudentIntakeInput) -> Option<String> {
    input.local_student_name.as_ref().and_then(|name| {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn persisted_local_display_name(
    input: &StudentIntakeInput,
    persist_local_display_name: bool,
) -> Option<String> {
    if persist_local_display_name {
        normalized_local_student_name(input)
    } else {
        None
    }
}

fn add_replaced_warning_if_needed(
    previous_by_ref: &HashMap<String, StudentIntakeSummary>,
    summary: &mut StudentIntakeSummary,
) {
    if let Some(existing) = previous_by_ref.get(&summary.student_ref) {
        if !existing.canonical_pdf_path.trim().is_empty() {
            summary.warnings.push(WorkspaceWarning {
                code: Some("student_intake_replaced".into()),
                message: format!(
                    "Student '{}' already had a canonical submission; replacing it.",
                    summary.student_ref
                ),
                scope: Some("student_intake".into()),
            });
        }
    }
}

fn run_ingest_for_canonical_targets(
    state: &Arc<AppStateInner>,
    event_sink: &dyn RuntimeEventSink,
    project_path: &Path,
    inputs: &[StudentIntakeInput],
    canonical_targets: &[Value],
) -> HostResult<CompletedWorkerJob> {
    let reserved = start_runtime_job_when_idle(state, event_sink, "scans.ingest")?;
    let output_artifacts_dir =
        project_store::command_output_dir(project_path, "scans.ingest", &reserved.job_id);
    fs::create_dir_all(&output_artifacts_dir)?;
    run_reserved_job(
        state,
        event_sink,
        reserved,
        RuntimeJobRequest {
            command_name: "scans.ingest",
            worker_request_payload: canonical_targets_job_payload(canonical_targets),
            persisted_request_payload: sanitized_ingest_request_payload(inputs),
            output_artifacts_dir: Some(&output_artifacts_dir),
            project_path: Some(project_path),
            stdin_bytes: None,
        },
    )
}

fn merge_prior_intake_items(
    previous_by_ref: HashMap<String, StudentIntakeSummary>,
    input_refs: &HashSet<String>,
    next_state: &mut StudentIntakeState,
) {
    let mut merged_items = Vec::new();
    for (student_ref, item) in previous_by_ref {
        if !input_refs.contains(&student_ref) {
            merged_items.push(item);
        }
    }
    merged_items.append(&mut next_state.items);
    next_state.items = merged_items;
}

fn persist_student_ingest_results(
    _project_path: &Path,
    mut existing: Vec<StudentIntakeSummary>,
    completed: &CompletedWorkerJob,
) -> HostResult<StudentIntakeState> {
    let data = success_data(&completed.result.envelope)?;
    let pdf_results = data
        .get("pdf_results")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            HostError::Protocol("scans.ingest success envelope was missing pdf_results.".into())
        })?;
    for result in pdf_results {
        let student_ref = required_string(result, "student_ref")?;
        let page_count = result
            .get("pages")
            .and_then(Value::as_array)
            .map(|pages| pages.len() as i64)
            .unwrap_or(0);
        let warnings = parse_warnings(result.get("warnings"))?;
        if let Some(item) = existing
            .iter_mut()
            .find(|item| item.student_ref == student_ref)
        {
            item.ingest_status = result
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("prepared")
                .to_string();
            item.page_count = page_count;
            item.exam_page_paths = exam_page_paths_from_ingest_pdf_result(result);
            item.warnings.extend(warnings);
        }
    }
    Ok(StudentIntakeState {
        status: crate::workflow_status::StudentIntakeStatus::Ready
            .as_str()
            .to_string(),
        latest_job_id: Some(completed.job_id.clone()),
        unresolved_count: 0,
        items: existing,
    })
}

fn read_png_raster_dimensions(path: &Path) -> HostResult<(i64, i64)> {
    let mut file = fs::File::open(path).map_err(|err| {
        HostError::Validation(format!(
            "Could not open template preview image '{}': {err}",
            path.display()
        ))
    })?;
    let mut buf = [0u8; 24];
    file.read_exact(&mut buf).map_err(|err| {
        HostError::Validation(format!(
            "Could not read template preview image '{}': {err}",
            path.display()
        ))
    })?;
    if buf[0..8] != [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a] {
        return Err(HostError::Validation(format!(
            "Template preview at '{}' is not a PNG (expected IHDR).",
            path.display()
        )));
    }
    let w = u32::from_be_bytes(buf[16..20].try_into().expect("IHDR width"));
    let h = u32::from_be_bytes(buf[20..24].try_into().expect("IHDR height"));
    Ok((i64::from(w), i64::from(h)))
}

fn template_raster_sizes_for_redaction_pages(
    project_path: &Path,
    redactions: &[crate::models::TemplateRedactionRegion],
) -> HostResult<HashMap<i64, (i64, i64)>> {
    let needed: HashSet<i64> = redactions.iter().map(|r| r.page_number).collect();
    let connection =
        rusqlite::Connection::open(project_store::schema::project_db_path(project_path))?;
    let artifacts = project_store::load_template_page_artifacts(&connection, project_path)?;
    let mut out = HashMap::new();
    for art in artifacts {
        if needed.contains(&art.page_number) {
            let dim = read_png_raster_dimensions(Path::new(&art.image_path))?;
            out.insert(art.page_number, dim);
        }
    }
    for page_number in &needed {
        if !out.contains_key(page_number) {
            return Err(HostError::Validation(format!(
                "Missing template preview image for redaction page {page_number}. Re-run template setup or redraw regions."
            )));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{
        add_replaced_warning_if_needed, build_intake_default_pdf_rects_request,
        exam_page_paths_from_ingest_pdf_result, input_raster_sizes_json,
        input_redaction_regions_json, invalidate_workflow_progress_for_intake_reorder,
        merge_prior_intake_items, normalized_desired_page_order, page_order_change_is_locked,
        persist_student_ingest_results, persisted_local_display_name, raster_sizes_by_page_json,
        read_png_raster_dimensions, resolved_redaction_payloads, sanitized_ingest_request_payload,
        submission_has_downstream_progress, template_redaction_regions_json,
        validate_page_order_update_input, validate_reordered_paths_match_existing,
        validate_selected_page_counts, workflow_status_for_submissions,
    };
    use crate::models::{
        InstructorProfile, StudentIntakeInput, StudentIntakePageOrderUpdateInput,
        StudentIntakeRasterSize, StudentIntakeRedactionRegionInput, StudentIntakeState,
        StudentIntakeSummary, StudentWorkflowAlignmentPage, StudentWorkflowAnswer,
        StudentWorkflowPage, StudentWorkflowState, StudentWorkflowSubmission,
        StudentWorkflowTransform, TemplateRedactionRegion, TemplateRedactionRegionInput,
        WorkerJobResult,
    };
    use crate::worker::CompletedWorkerJob;
    use std::collections::{HashMap, HashSet};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn summary(student_ref: &str, canonical_pdf_path: &str) -> StudentIntakeSummary {
        StudentIntakeSummary {
            student_ref: student_ref.into(),
            local_display_name: None,
            canonical_pdf_path: canonical_pdf_path.into(),
            ingest_status: "prepared".into(),
            page_count: 0,
            exam_page_paths: Vec::new(),
            warnings: Vec::new(),
            binding_token_hex: None,
        }
    }

    fn temp_png_path(prefix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}.png",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_millis()
        ))
    }

    fn temp_project_root(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_millis()
        ))
    }

    fn tiny_png_header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![
            0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
        ];
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes
    }

    fn insert_template_page_artifact(project_path: &Path, page_number: i64, image_path: &Path) {
        let relative_path = image_path
            .strip_prefix(project_path)
            .expect("image should live in project")
            .to_string_lossy()
            .into_owned();
        let connection =
            rusqlite::Connection::open(crate::project_store::schema::project_db_path(project_path))
                .expect("project db should open");
        connection
            .execute(
                "INSERT INTO artifact (
                    artifact_id,
                    job_id,
                    kind,
                    role,
                    relative_path,
                    mime_type,
                    sha256,
                    byte_size,
                    metadata_json,
                    created_at
                ) VALUES (?1, ?2, 'image', 'rendered_template_page', ?3, 'image/png', NULL, ?4, ?5, CURRENT_TIMESTAMP)",
                rusqlite::params![
                    format!("template-page-{page_number}"),
                    "job-1",
                    relative_path,
                    fs::metadata(image_path).expect("png metadata").len() as i64,
                    serde_json::json!({
                        "page_number": page_number,
                        "label": format!("Page {page_number}")
                    })
                    .to_string(),
                ],
            )
            .expect("artifact should insert");
    }

    #[test]
    fn add_replaced_warning_only_when_prior_canonical_exists() {
        let mut next_summary = summary("student_1", "/tmp/new.pdf");
        let previous = HashMap::from([(
            "student_1".to_string(),
            summary("student_1", "/tmp/old.pdf"),
        )]);

        add_replaced_warning_if_needed(&previous, &mut next_summary);

        assert_eq!(next_summary.warnings.len(), 1);
        assert_eq!(
            next_summary.warnings[0].code.as_deref(),
            Some("student_intake_replaced")
        );
    }

    #[test]
    fn default_redaction_payload_uses_template_regions_and_png_sizes() {
        let test_root = temp_project_root("scriptscore-intake-default-redaction");
        let created = crate::project_store::create_project_in_root(
            test_root.clone(),
            "Intake Redaction",
            Some("Biology".into()),
            None,
            None,
            &InstructorProfile::default(),
        )
        .expect("project should be created");
        let project_path = PathBuf::from(&created.project_path);
        let template_dir = project_path.join("artifacts").join("template_pages");
        fs::create_dir_all(&template_dir).expect("template dir should create");
        let page_path = template_dir.join("page-1.png");
        fs::write(&page_path, tiny_png_header(320, 240)).expect("png should write");
        insert_template_page_artifact(&project_path, 1, &page_path);
        crate::project_store::save_redaction_regions(
            &project_path,
            &[TemplateRedactionRegionInput {
                region_id: Some("redact-1".into()),
                page_number: 1,
                x: 7,
                y: 8,
                width: 90,
                height: 30,
            }],
        )
        .expect("redaction region should save");

        let payload = build_intake_default_pdf_rects_request(&project_path)
            .expect("default redaction payload should build")
            .expect("redaction payload should be present");

        assert_eq!(
            payload,
            serde_json::json!({
                "regions": [
                    {
                        "page_number": 1,
                        "x": 7,
                        "y": 8,
                        "width": 90,
                        "height": 30
                    }
                ],
                "raster_sizes_by_page": {
                    "1": {
                        "width_px": 320,
                        "height_px": 240
                    }
                }
            })
        );

        fs::remove_dir_all(&test_root).expect("test project should clean up");
    }

    #[test]
    fn page_order_update_input_rejects_blank_student_and_empty_paths() {
        assert!(
            validate_page_order_update_input(&StudentIntakePageOrderUpdateInput {
                student_ref: "  ".into(),
                exam_page_paths: vec!["/tmp/a.png".into()],
            })
            .expect_err("blank student ref should fail")
            .to_string()
            .contains("student_ref is required")
        );

        assert!(
            validate_page_order_update_input(&StudentIntakePageOrderUpdateInput {
                student_ref: "student_1".into(),
                exam_page_paths: Vec::new(),
            })
            .expect_err("empty page paths should fail")
            .to_string()
            .contains("at least one page image")
        );
    }

    #[test]
    fn merge_prior_intake_items_keeps_non_replaced_students() {
        let previous = HashMap::from([
            (
                "student_1".to_string(),
                summary("student_1", "/tmp/one.pdf"),
            ),
            (
                "student_2".to_string(),
                summary("student_2", "/tmp/two.pdf"),
            ),
        ]);
        let input_refs = HashSet::from(["student_2".to_string()]);
        let mut next_state = StudentIntakeState {
            status: "ready".into(),
            latest_job_id: None,
            items: vec![summary("student_2", "/tmp/replaced.pdf")],
            unresolved_count: 0,
        };

        merge_prior_intake_items(previous, &input_refs, &mut next_state);

        assert_eq!(next_state.items.len(), 2);
        assert_eq!(next_state.items[0].student_ref, "student_1");
        assert_eq!(next_state.items[1].canonical_pdf_path, "/tmp/replaced.pdf");
    }

    #[test]
    fn persist_student_ingest_results_updates_matching_items() {
        let state = persist_student_ingest_results(
            std::path::Path::new("/tmp/unused"),
            vec![summary("student_1", "/tmp/student_1.pdf")],
            &CompletedWorkerJob {
                job_id: "job-1".into(),
                result: WorkerJobResult {
                    terminal_type: "job_finished".into(),
                    terminal_payload: serde_json::json!({}),
                    envelope: serde_json::json!({
                        "data": {
                            "pdf_results": [
                                {
                                    "student_ref": "student_1",
                                    "status": "ready",
                                    "pages": [
                                        {"page_number": 2, "image_path": "/tmp/b.png"},
                                        {"page_number": 1, "image_path": "/tmp/a.png"}
                                    ],
                                    "warnings": [
                                        {"message": "Minor issue"}
                                    ]
                                }
                            ]
                        }
                    }),
                    events: Vec::new(),
                },
            },
        )
        .expect("ingest results should persist");

        assert_eq!(state.status, "ready");
        assert_eq!(state.latest_job_id.as_deref(), Some("job-1"));
        assert_eq!(state.items[0].ingest_status, "ready");
        assert_eq!(state.items[0].page_count, 2);
        assert_eq!(
            state.items[0].exam_page_paths,
            vec!["/tmp/b.png".to_string(), "/tmp/a.png".to_string()]
        );
        assert_eq!(state.items[0].warnings.len(), 1);
    }

    #[test]
    fn persist_student_ingest_results_rejects_missing_pdf_results() {
        let error = persist_student_ingest_results(
            std::path::Path::new("/tmp/unused"),
            vec![summary("student_1", "/tmp/student_1.pdf")],
            &CompletedWorkerJob {
                job_id: "job-1".into(),
                result: WorkerJobResult {
                    terminal_type: "job_finished".into(),
                    terminal_payload: serde_json::json!({}),
                    envelope: serde_json::json!({
                        "data": {
                            "other_results": []
                        }
                    }),
                    events: Vec::new(),
                },
            },
        )
        .expect_err("missing pdf_results should be a protocol error");

        assert!(error.to_string().contains("missing pdf_results"));
    }

    #[test]
    fn exam_page_paths_from_ingest_result_filters_blank_paths_without_reordering() {
        let paths = exam_page_paths_from_ingest_pdf_result(&serde_json::json!({
            "pages": [
                {"page_number": 3, "image_path": "/tmp/page-3.png"},
                {"page_number": 1, "image_path": ""},
                {"page_number": 2, "image_path": "/tmp/page-2.png"}
            ]
        }));

        assert_eq!(
            paths,
            vec!["/tmp/page-3.png".to_string(), "/tmp/page-2.png".to_string()]
        );
    }

    #[test]
    fn normalized_desired_page_order_filters_non_positive_entries() {
        let input = StudentIntakeInput {
            student_ref: "student_1".into(),
            local_student_name: None,
            raw_pdf_path: "/tmp/student_1.pdf".into(),
            desired_page_order: vec![3, 0, -1, 2],
            redaction_regions_px: Vec::new(),
            raster_sizes_by_page: HashMap::new(),
        };

        assert_eq!(normalized_desired_page_order(&input), vec![3, 2]);
    }

    #[test]
    fn selected_page_count_validation_blocks_missing_pages() {
        let input = StudentIntakeInput {
            student_ref: "student_1".into(),
            local_student_name: None,
            raw_pdf_path: "/tmp/student_1.pdf".into(),
            desired_page_order: vec![1, 2],
            redaction_regions_px: Vec::new(),
            raster_sizes_by_page: HashMap::new(),
        };

        assert!(validate_selected_page_counts(std::slice::from_ref(&input), 2).is_ok());
        assert!(validate_selected_page_counts(&[input], 3)
            .expect_err("fewer selected pages than template should fail")
            .to_string()
            .contains("Replace or rescan"));
    }

    #[test]
    fn redaction_region_and_raster_json_helpers_preserve_numeric_fields() {
        let template_regions = vec![TemplateRedactionRegion {
            region_id: "r1".into(),
            page_number: 2,
            x: 11,
            y: 12,
            width: 101,
            height: 52,
            label: "privacy_protection".into(),
            sort_order: 1,
        }];
        assert_eq!(
            template_redaction_regions_json(&template_regions),
            vec![serde_json::json!({
                "page_number": 2,
                "x": 11,
                "y": 12,
                "width": 101,
                "height": 52
            })]
        );
        assert_eq!(
            raster_sizes_by_page_json(&HashMap::from([(2, (640, 480))])),
            serde_json::json!({
                "2": {
                    "width_px": 640,
                    "height_px": 480
                }
            })
        );

        let input = StudentIntakeInput {
            student_ref: "student_1".into(),
            local_student_name: None,
            raw_pdf_path: "/tmp/student_1.pdf".into(),
            desired_page_order: Vec::new(),
            redaction_regions_px: vec![StudentIntakeRedactionRegionInput {
                page_number: 3,
                x: 21,
                y: 22,
                width: 31,
                height: 32,
            }],
            raster_sizes_by_page: HashMap::from([(
                3,
                StudentIntakeRasterSize {
                    width_px: 800,
                    height_px: 600,
                },
            )]),
        };
        assert_eq!(
            input_redaction_regions_json(&input),
            vec![serde_json::json!({
                "page_number": 3,
                "x": 21,
                "y": 22,
                "width": 31,
                "height": 32
            })]
        );
        assert_eq!(
            input_raster_sizes_json(&input),
            serde_json::json!({
                "3": {
                    "width_px": 800,
                    "height_px": 600
                }
            })
        );
    }

    #[test]
    fn reordered_page_paths_may_remove_but_not_add_pages() {
        let item = StudentIntakeSummary {
            exam_page_paths: vec!["/tmp/a.png".into(), "/tmp/b.png".into()],
            ..summary("student_1", "/tmp/student_1.pdf")
        };

        assert!(validate_reordered_paths_match_existing(
            &item,
            &["/tmp/b.png".to_string(), "/tmp/a.png".to_string()]
        )
        .is_ok());
        assert!(
            validate_reordered_paths_match_existing(&item, &["/tmp/b.png".to_string()]).is_ok()
        );
        assert!(validate_reordered_paths_match_existing(
            &item,
            &["/tmp/b.png".to_string(), "/tmp/c.png".to_string()]
        )
        .expect_err("added path should be invalid")
        .to_string()
        .contains("only existing ingested pages"));
        assert!(validate_reordered_paths_match_existing(
            &item,
            &["/tmp/b.png".to_string(), "/tmp/b.png".to_string()]
        )
        .expect_err("duplicate path should be invalid")
        .to_string()
        .contains("duplicate paths"));
        assert!(validate_reordered_paths_match_existing(
            &item,
            &["/tmp/b.png".to_string(), "".to_string()]
        )
        .expect_err("blank path should be invalid")
        .to_string()
        .contains("must not contain empty paths"));
    }

    #[test]
    fn persisted_local_display_name_is_gated_by_project_mode() {
        let input = StudentIntakeInput {
            student_ref: "student_1".into(),
            local_student_name: Some(" Ada Local ".into()),
            raw_pdf_path: "/tmp/student_1.pdf".into(),
            desired_page_order: Vec::new(),
            redaction_regions_px: Vec::new(),
            raster_sizes_by_page: HashMap::new(),
        };

        assert_eq!(
            persisted_local_display_name(&input, true).as_deref(),
            Some("Ada Local")
        );
        assert_eq!(persisted_local_display_name(&input, false), None);
    }

    #[test]
    fn sanitized_ingest_request_payload_keeps_requested_page_order_without_pdf_paths() {
        let payload = sanitized_ingest_request_payload(&[
            StudentIntakeInput {
                student_ref: "student_1".into(),
                local_student_name: None,
                raw_pdf_path: "/tmp/student_1.pdf".into(),
                desired_page_order: vec![3, 1, 2],
                redaction_regions_px: Vec::new(),
                raster_sizes_by_page: HashMap::new(),
            },
            StudentIntakeInput {
                student_ref: "student_2".into(),
                local_student_name: None,
                raw_pdf_path: "/tmp/student_2.pdf".into(),
                desired_page_order: Vec::new(),
                redaction_regions_px: Vec::new(),
                raster_sizes_by_page: HashMap::new(),
            },
        ]);

        assert_eq!(
            payload,
            serde_json::json!({
                "pdf_targets": [
                    {
                        "student_ref": "student_1",
                        "page_order": [3, 1, 2]
                    },
                    {
                        "student_ref": "student_2"
                    }
                ]
            })
        );
    }

    #[test]
    fn resolved_redaction_payloads_prefer_submission_specific_regions_when_present() {
        let input = StudentIntakeInput {
            student_ref: "student_1".into(),
            local_student_name: None,
            raw_pdf_path: "/tmp/student_1.pdf".into(),
            desired_page_order: Vec::new(),
            redaction_regions_px: vec![StudentIntakeRedactionRegionInput {
                page_number: 2,
                x: 15,
                y: 25,
                width: 35,
                height: 45,
            }],
            raster_sizes_by_page: HashMap::from([(
                2,
                StudentIntakeRasterSize {
                    width_px: 610,
                    height_px: 810,
                },
            )]),
        };

        let (regions_json, raster_json) = resolved_redaction_payloads(
            &input,
            &[serde_json::json!({"page_number": 1, "x": 1, "y": 2, "width": 3, "height": 4})],
            &serde_json::json!({"1": {"width_px": 100, "height_px": 200}}),
        );

        assert_eq!(
            regions_json,
            vec![serde_json::json!({
                "page_number": 2,
                "x": 15,
                "y": 25,
                "width": 35,
                "height": 45
            })]
        );
        assert_eq!(
            raster_json,
            serde_json::json!({
                "2": {
                    "width_px": 610,
                    "height_px": 810
                }
            })
        );
    }

    #[test]
    fn read_png_raster_dimensions_reads_ihdr_size() {
        let path = temp_png_path("scriptscore-intake-raster");
        fs::write(
            &path,
            [
                0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, b'I', b'H', b'D',
                b'R', 0, 0, 0, 16, 0, 0, 0, 32,
            ],
        )
        .expect("png header should write");

        let dims = read_png_raster_dimensions(&path).expect("dimensions should load");
        assert_eq!(dims, (16, 32));

        fs::remove_file(&path).expect("temp png should clean up");
    }

    fn workflow_submission(student_ref: &str) -> StudentWorkflowSubmission {
        StudentWorkflowSubmission {
            student_ref: student_ref.into(),
            canonical_pdf_path: format!("/tmp/{student_ref}.pdf"),
            page_count: 2,
            stage: "failed".into(),
            latest_job_id: Some("job-1".into()),
            failure_message: Some("Detect failed".into()),
            warnings: Vec::new(),
            page_artifacts: Vec::new(),
            alignment_pages: Vec::new(),
            detect_review: None,
            answers: Vec::new(),
        }
    }

    #[test]
    fn invalidate_workflow_progress_for_intake_reorder_clears_saved_downstream_state() {
        let mut workflow_state = StudentWorkflowState {
            status: "attention".into(),
            latest_job_id: None,
            submissions: vec![workflow_submission("student_1")],
        };
        workflow_state.submissions[0]
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
        workflow_state.submissions[0]
            .page_artifacts
            .push(StudentWorkflowPage {
                page_number: 1,
                image_path: "/tmp/canonical.png".into(),
                source_pdf_path: Some("/tmp/student_1.pdf".into()),
                ocr_metadata_path: Some("/tmp/ocr.json".into()),
            });
        workflow_state.submissions[0]
            .answers
            .push(StudentWorkflowAnswer {
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
                feedback_text: Some("Nice".into()),
                criterion_results: Vec::new(),
                highlights: Vec::new(),
                warnings: Vec::new(),
            });

        assert!(submission_has_downstream_progress(
            &workflow_state.submissions[0]
        ));

        invalidate_workflow_progress_for_intake_reorder(&mut workflow_state, "student_1");

        let submission = &workflow_state.submissions[0];
        assert_eq!(submission.stage, "intake_ready");
        assert!(submission.latest_job_id.is_none());
        assert!(submission.failure_message.is_none());
        assert!(submission.alignment_pages.is_empty());
        assert!(submission.page_artifacts.is_empty());
        assert!(submission.answers.is_empty());
    }

    #[test]
    fn page_order_change_locks_after_downstream_workflow_starts() {
        let mut workflow_state = StudentWorkflowState {
            status: "running".into(),
            latest_job_id: None,
            submissions: vec![workflow_submission("student_1")],
        };

        workflow_state.submissions[0].stage = "intake_ready".into();
        assert!(!page_order_change_is_locked(&workflow_state, "student_1"));

        workflow_state.submissions[0].stage = "failed".into();
        assert!(!page_order_change_is_locked(&workflow_state, "student_1"));

        workflow_state.submissions[0].stage = "parse".into();
        assert!(page_order_change_is_locked(&workflow_state, "student_1"));
        assert!(!page_order_change_is_locked(&workflow_state, "student_2"));
    }

    #[test]
    fn workflow_status_for_submissions_covers_empty_ready_running_attention_and_graded() {
        let mut state = StudentWorkflowState {
            status: String::new(),
            latest_job_id: None,
            submissions: Vec::new(),
        };
        assert_eq!(workflow_status_for_submissions(&state), "not_started");

        state.submissions = vec![workflow_submission("student_1")];
        state.submissions[0].stage = "intake_ready".into();
        assert_eq!(workflow_status_for_submissions(&state), "ready");

        state.submissions[0].stage = "detect".into();
        assert_eq!(workflow_status_for_submissions(&state), "running");

        state.submissions[0].stage = "alignment_review".into();
        assert_eq!(workflow_status_for_submissions(&state), "attention");

        state.submissions[0].stage = "graded".into();
        assert_eq!(workflow_status_for_submissions(&state), "graded");

        state.submissions.push(workflow_submission("student_2"));
        state.submissions[1].stage = "manual_grading".into();
        assert_eq!(workflow_status_for_submissions(&state), "attention");
    }
}
