// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};
use zip::write::SimpleFileOptions;

use crate::errors::{HostError, HostResult};
use crate::models::{
    ExamWorkspaceState, ResultStudentRow, ResultsExportFormat, ResultsExportResponse,
    RunResultsExportInput, RuntimeJobEvent, StudentIntakeSummary, StudentWorkflowSubmission,
    WorkerStatus,
};
use crate::project_store;

use super::{results_lms_report, AppStateInner, RuntimeEventSink};

const RESULTS_EXPORT_COMMAND_NAME: &str = "results.export";

pub(crate) fn run_results_export(
    state: &Arc<AppStateInner>,
    input: RunResultsExportInput,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<ResultsExportResponse> {
    let project_path = state.lock().current_project_path()?;
    let workspace = project_store::load_exam_workspace_state(&project_path)?;
    let destination_path = validate_destination_path(&input.destination_path)?;
    let rows = selected_ready_rows(&workspace, &input.student_refs)?;

    match input.format {
        ResultsExportFormat::Csv => {
            run_csv_export_with_runtime_events(event_sink, &workspace, &rows, &destination_path)?
        }
        ResultsExportFormat::HtmlZip => write_html_zip_export(
            event_sink,
            &project_path,
            &workspace,
            &rows,
            &destination_path,
        )?,
    }

    Ok(ResultsExportResponse {
        format: input.format,
        destination_path: destination_path.to_string_lossy().into_owned(),
        exported_count: rows.len() as i64,
    })
}

fn validate_destination_path(raw_path: &str) -> HostResult<PathBuf> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err(HostError::Validation(
            "Choose a destination path before exporting Results.".into(),
        ));
    }
    let path = PathBuf::from(trimmed);
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|err| {
            HostError::Project(format!(
                "Could not create Results export destination directory '{}': {err}",
                parent.display()
            ))
        })?;
    }
    Ok(path)
}

fn selected_ready_rows<'a>(
    workspace: &'a ExamWorkspaceState,
    requested_student_refs: &[String],
) -> HostResult<Vec<&'a ResultStudentRow>> {
    if requested_student_refs.is_empty() {
        return Err(HostError::Validation(
            "Select at least one ready result before exporting.".into(),
        ));
    }

    let rows_by_ref = results_rows_by_ref(workspace);
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    for student_ref in requested_student_refs {
        let trimmed = student_ref.trim();
        if trimmed.is_empty() || !seen.insert(trimmed.to_string()) {
            continue;
        }
        rows.push(ready_export_row(&rows_by_ref, trimmed)?);
    }

    if rows.is_empty() {
        return Err(HostError::Validation(
            "Select at least one ready result before exporting.".into(),
        ));
    }
    Ok(rows)
}

fn results_rows_by_ref(workspace: &ExamWorkspaceState) -> HashMap<&str, &ResultStudentRow> {
    workspace
        .results_lms_rows
        .iter()
        .map(|row| (row.student_ref.as_str(), row))
        .collect::<HashMap<_, _>>()
}

fn ready_export_row<'a>(
    rows_by_ref: &HashMap<&str, &'a ResultStudentRow>,
    student_ref: &str,
) -> HostResult<&'a ResultStudentRow> {
    let row = rows_by_ref.get(student_ref).copied().ok_or_else(|| {
        HostError::Validation(format!("Result row '{student_ref}' was not found."))
    })?;
    if !row.ready_to_finalize || !row.aggregate_complete {
        return Err(HostError::Validation(format!(
            "Result row '{student_ref}' is not ready to export."
        )));
    }
    Ok(row)
}

fn write_csv_export(
    workspace: &ExamWorkspaceState,
    rows: &[&ResultStudentRow],
    destination_path: &Path,
) -> HostResult<()> {
    let local_names = local_display_names_by_ref(&workspace.student_intake.items);
    let questions = csv_question_columns(workspace);
    let mut csv = String::new();
    let header = csv_header(&questions);
    push_csv_record(&mut csv, &header);

    for row in rows {
        let record = csv_record_for_row(row, &local_names, &questions);
        push_csv_record(&mut csv, &record);
    }

    fs::write(destination_path, csv).map_err(|err| {
        HostError::Project(format!(
            "Could not write Results CSV export '{}': {err}",
            destination_path.display()
        ))
    })
}

fn csv_question_columns(workspace: &ExamWorkspaceState) -> Vec<(&str, i64)> {
    workspace
        .questions
        .iter()
        .map(|question| (question.question_id.as_str(), question.question_number))
        .collect::<Vec<_>>()
}

fn csv_header(questions: &[(&str, i64)]) -> Vec<String> {
    let mut header = vec![
        "student_ref".to_string(),
        "display_name".to_string(),
        "aggregate_score".to_string(),
        "aggregate_percent".to_string(),
    ];
    for (_, question_number) in questions {
        header.push(format!("q{question_number}_score"));
        header.push(format!("q{question_number}_feedback"));
    }
    header
}

fn csv_record_for_row(
    row: &ResultStudentRow,
    local_names: &HashMap<&str, String>,
    questions: &[(&str, i64)],
) -> Vec<String> {
    let mut record = vec![
        row.student_ref.clone(),
        local_names
            .get(row.student_ref.as_str())
            .cloned()
            .unwrap_or_default(),
        row.aggregate_total.to_string(),
        aggregate_percent(row).unwrap_or_default(),
    ];
    for (question_id, _) in questions {
        push_question_csv_fields(row, question_id, &mut record);
    }
    record
}

fn push_question_csv_fields(row: &ResultStudentRow, question_id: &str, record: &mut Vec<String>) {
    let Some(question_row) = row
        .question_rows
        .iter()
        .find(|entry| entry.question_id == question_id)
    else {
        record.push(String::new());
        record.push(String::new());
        return;
    };
    record.push(
        question_row
            .effective_total_points
            .map(|value| value.to_string())
            .unwrap_or_default(),
    );
    record.push(question_row.effective_feedback_text.clone());
}

fn local_display_names_by_ref(items: &[StudentIntakeSummary]) -> HashMap<&str, String> {
    items
        .iter()
        .filter_map(|item| {
            let name = item.local_display_name.as_deref()?.trim();
            (!name.is_empty()).then(|| (item.student_ref.as_str(), name.to_string()))
        })
        .collect()
}

fn aggregate_percent(row: &ResultStudentRow) -> Option<String> {
    let max_points = row
        .question_rows
        .iter()
        .map(|question| question.max_points)
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .sum::<i64>();
    (max_points > 0).then(|| {
        format!(
            "{:.2}",
            (row.aggregate_total as f64 / max_points as f64) * 100.0
        )
    })
}

fn push_csv_record(output: &mut String, values: &[String]) {
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push_str(&escape_csv_field(value));
    }
    output.push('\n');
}

fn escape_csv_field(value: &str) -> String {
    let safe_value = neutralize_csv_formula(value);
    if safe_value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", safe_value.replace('"', "\"\""))
    } else {
        safe_value
    }
}

fn neutralize_csv_formula(value: &str) -> String {
    match value.trim_start().chars().next() {
        Some('=' | '+' | '-' | '@') => format!("'{value}"),
        _ => value.to_string(),
    }
}

fn run_csv_export_with_runtime_events(
    event_sink: &dyn RuntimeEventSink,
    workspace: &ExamWorkspaceState,
    rows: &[&ResultStudentRow],
    destination_path: &Path,
) -> HostResult<()> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let job_id = format!("results_export_{}", uuid::Uuid::new_v4());
    emit_results_export_job_event(
        event_sink,
        "job_started",
        WorkerStatus::Busy,
        &request_id,
        &job_id,
        json!({
            "format": "csv",
            "progress": { "percent": 0 },
        }),
    );
    match write_csv_export(workspace, rows, destination_path) {
        Ok(()) => {
            emit_results_export_job_event(
                event_sink,
                "job_finished",
                WorkerStatus::Ready,
                &request_id,
                &job_id,
                json!({
                    "format": "csv",
                    "exportedCount": rows.len(),
                    "destinationPath": destination_path.to_string_lossy(),
                    "progress": { "percent": 100 },
                }),
            );
            Ok(())
        }
        Err(err) => {
            emit_results_export_job_event(
                event_sink,
                "job_failed",
                WorkerStatus::Error,
                &request_id,
                &job_id,
                json!({
                    "format": "csv",
                    "message": err.to_string(),
                }),
            );
            Err(err)
        }
    }
}

fn emit_results_export_job_event(
    event_sink: &dyn RuntimeEventSink,
    event_type: &str,
    worker_status: WorkerStatus,
    request_id: &str,
    job_id: &str,
    payload: Value,
) {
    event_sink.emit_runtime_event(RuntimeJobEvent {
        event_type: event_type.into(),
        command_name: RESULTS_EXPORT_COMMAND_NAME.into(),
        worker_status,
        request_id: Some(request_id.into()),
        job_id: Some(job_id.into()),
        payload,
    });
}

fn write_html_zip_export(
    event_sink: &dyn RuntimeEventSink,
    project_path: &Path,
    workspace: &ExamWorkspaceState,
    rows: &[&ResultStudentRow],
    destination_path: &Path,
) -> HostResult<()> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let job_id = format!("results_export_{}", uuid::Uuid::new_v4());
    let output_artifacts_dir = project_store::schema::command_output_dir(
        project_path,
        RESULTS_EXPORT_COMMAND_NAME,
        &job_id,
    );

    emit_results_export_job_event(
        event_sink,
        "job_started",
        WorkerStatus::Busy,
        &request_id,
        &job_id,
        json!({
            "format": "html_zip",
            "progress": { "percent": 0 },
        }),
    );

    match write_html_zip_export_artifacts(
        event_sink,
        &request_id,
        &job_id,
        workspace,
        rows,
        &output_artifacts_dir,
        destination_path,
    ) {
        Ok(()) => {
            emit_results_export_job_event(
                event_sink,
                "job_finished",
                WorkerStatus::Ready,
                &request_id,
                &job_id,
                json!({
                    "format": "html_zip",
                    "exportedCount": rows.len(),
                    "destinationPath": destination_path.to_string_lossy(),
                    "progress": { "percent": 100 },
                }),
            );
            Ok(())
        }
        Err(err) => {
            emit_results_export_job_event(
                event_sink,
                "job_failed",
                WorkerStatus::Error,
                &request_id,
                &job_id,
                json!({
                    "format": "html_zip",
                    "message": err.to_string(),
                }),
            );
            Err(err)
        }
    }
}

fn write_html_zip_export_artifacts(
    event_sink: &dyn RuntimeEventSink,
    request_id: &str,
    job_id: &str,
    workspace: &ExamWorkspaceState,
    rows: &[&ResultStudentRow],
    output_artifacts_dir: &Path,
    destination_path: &Path,
) -> HostResult<()> {
    fs::create_dir_all(output_artifacts_dir).map_err(|err| {
        HostError::Project(format!(
            "Could not create Results HTML export directory '{}': {err}",
            output_artifacts_dir.display()
        ))
    })?;
    emit_results_export_job_event(
        event_sink,
        "job_progress",
        WorkerStatus::Busy,
        request_id,
        job_id,
        json!({
            "format": "html_zip",
            "progress": { "percent": 1 },
        }),
    );
    let html_paths = write_results_report_html_files(
        event_sink,
        request_id,
        job_id,
        workspace,
        rows,
        output_artifacts_dir,
    )?;
    write_zip_from_html_paths(&html_paths, destination_path)
}

fn write_results_report_html_files(
    event_sink: &dyn RuntimeEventSink,
    request_id: &str,
    job_id: &str,
    workspace: &ExamWorkspaceState,
    rows: &[&ResultStudentRow],
    output_artifacts_dir: &Path,
) -> HostResult<Vec<PathBuf>> {
    let total = rows.len().max(1);
    let mut html_paths = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        validate_html_export_sources(workspace, row)?;
        let preview = results_lms_report::build_report_preview(workspace, &row.student_ref)?;
        let html_path = output_artifacts_dir.join(format!(
            "{}_result.html",
            sanitize_file_component(&row.student_ref)
        ));
        fs::write(&html_path, preview.html).map_err(|err| {
            HostError::Project(format!(
                "Could not write Results HTML export '{}': {err}",
                html_path.display()
            ))
        })?;
        html_paths.push(html_path);
        emit_results_export_job_event(
            event_sink,
            "job_progress",
            WorkerStatus::Busy,
            request_id,
            job_id,
            json!({
                "format": "html_zip",
                "progress": {
                    "percent": (((index + 1) * 90) / total) as i64,
                    "completed": index + 1,
                    "total": rows.len(),
                },
            }),
        );
    }
    Ok(html_paths)
}

fn validate_html_export_sources(
    workspace: &ExamWorkspaceState,
    row: &ResultStudentRow,
) -> HostResult<()> {
    let submission = workspace
        .student_workflow
        .submissions
        .iter()
        .find(|submission| submission.student_ref == row.student_ref)
        .ok_or_else(|| {
            HostError::Validation(format!(
                "Submission '{}' was not found for Results export.",
                row.student_ref
            ))
        })?;
    validate_html_export_question_sources(row, submission)
}

fn validate_html_export_question_sources(
    row: &ResultStudentRow,
    submission: &StudentWorkflowSubmission,
) -> HostResult<()> {
    let answers_by_id = submission
        .answers
        .iter()
        .map(|answer| (answer.question_id.as_str(), answer))
        .collect::<HashMap<_, _>>();
    for question_row in &row.question_rows {
        let answer = answers_by_id
            .get(question_row.question_id.as_str())
            .copied()
            .ok_or_else(|| {
                HostError::Validation(format!(
                    "Question {} is missing from submission '{}'.",
                    question_row.question_number, row.student_ref
                ))
            })?;
        let crop_path = answer
            .crop_image_path
            .as_deref()
            .filter(|path| !path.trim().is_empty())
            .ok_or_else(|| {
                HostError::Validation(format!(
                    "Question {} for '{}' has no crop image for export.",
                    question_row.question_number, row.student_ref
                ))
            })?;
        let path = Path::new(crop_path);
        if !path.is_file() {
            return Err(HostError::Validation(format!(
                "Question {} crop image for '{}' was not found at '{}'.",
                question_row.question_number,
                row.student_ref,
                path.display()
            )));
        }
    }
    Ok(())
}

fn write_zip_from_html_paths(html_paths: &[PathBuf], destination_path: &Path) -> HostResult<()> {
    let file = File::create(destination_path).map_err(|err| {
        HostError::Project(format!(
            "Could not create Results ZIP export '{}': {err}",
            destination_path.display()
        ))
    })?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for html_path in html_paths {
        let file_name = html_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                HostError::Project(format!(
                    "Could not determine export HTML file name for '{}'.",
                    html_path.display()
                ))
            })?;
        let mut html = Vec::new();
        File::open(html_path)
            .and_then(|mut file| file.read_to_end(&mut html))
            .map_err(|err| {
                HostError::Project(format!(
                    "Could not read generated Results HTML export '{}': {err}",
                    html_path.display()
                ))
            })?;
        zip.start_file(sanitize_zip_entry_name(file_name), options)
            .map_err(|err| HostError::Project(format!("Could not add Results ZIP entry: {err}")))?;
        zip.write_all(&html).map_err(|err| {
            HostError::Project(format!("Could not write Results ZIP entry: {err}"))
        })?;
    }

    zip.finish()
        .map_err(|err| HostError::Project(format!("Could not finish Results ZIP export: {err}")))?;
    Ok(())
}

fn sanitize_zip_entry_name(file_name: &str) -> String {
    sanitize_file_component(file_name)
}

fn sanitize_file_component(file_name: &str) -> String {
    file_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;
    use crate::models::{
        ModerationState, ProjectConfig, ProjectSummary, QuestionAnalysisState, QuestionRecord,
        ResultQuestionRow, ResultsLmsState, RubricState, StudentIntakeState, StudentRosterRow,
        StudentWorkflowAnswer, StudentWorkflowState, TemplatePageArtifactSummary,
        TemplateRedactionRegion, WorkspaceWarning,
    };

    fn ready_row(student_ref: &str) -> ResultStudentRow {
        ResultStudentRow {
            student_ref: student_ref.into(),
            aggregate_total: 8,
            aggregate_complete: true,
            ready_to_finalize: true,
            question_rows: vec![ResultQuestionRow {
                question_id: "q1".into(),
                question_number: 1,
                max_points: Some(10),
                effective_total_points: Some(8),
                effective_feedback_text: "Good, \"clear\"\nwork".into(),
                ..ResultQuestionRow::default()
            }],
            ..ResultStudentRow::default()
        }
    }

    fn workspace_with_rows(rows: Vec<ResultStudentRow>) -> ExamWorkspaceState {
        ExamWorkspaceState {
            project: ProjectSummary {
                project_id: "project".into(),
                display_name: "Exam".into(),
                subject: None,
                course_code: None,
                lms_course_id: None,
                project_path: "/tmp/project".into(),
                created_at: "now".into(),
                updated_at: "now".into(),
            },
            project_config: ProjectConfig {
                display_name: "Exam".into(),
                ..ProjectConfig::default()
            },
            aruco_status: Default::default(),
            questions: vec![QuestionRecord {
                question_id: "q1".into(),
                question_number: 1,
                text: "Explain".into(),
                max_points: Some(10),
                page_number: 1,
                baseline_pdf_text: "Explain".into(),
                region: None,
                source_artifact_id: None,
                image_path: None,
                analysis: QuestionAnalysisState::default(),
                rubric: RubricState::default(),
            }],
            student_intake: StudentIntakeState {
                items: vec![StudentIntakeSummary {
                    student_ref: "student_1".into(),
                    local_display_name: Some("Ada, Local".into()),
                    canonical_pdf_path: "/tmp/a.pdf".into(),
                    ingest_status: "ready".into(),
                    page_count: 1,
                    exam_page_paths: Vec::new(),
                    warnings: Vec::new(),
                    binding_token_hex: Some("do-not-export".into()),
                }],
                ..StudentIntakeState::default()
            },
            results_lms_rows: rows,
            student_workflow: StudentWorkflowState::not_started(),
            status: "approved".into(),
            status_label: "Approved".into(),
            failure_message: None,
            template_preview_artifacts: Vec::<TemplatePageArtifactSummary>::new(),
            redaction_regions: Vec::<TemplateRedactionRegion>::new(),
            warnings: Vec::<WorkspaceWarning>::new(),
            can_approve: true,
            can_approve_rubric: true,
            student_roster: Vec::<StudentRosterRow>::new(),
            moderation_state: ModerationState::default(),
            results_lms_state: ResultsLmsState::default(),
            results_lms_metrics: None,
            results_lms_review_summary: None,
            workflow_stage: "results_upload_ready".into(),
            workflow_label: "Ready".into(),
        }
    }

    #[test]
    fn results_export_rejects_empty_or_non_ready_selection() {
        let blocked = ResultStudentRow {
            student_ref: "student_1".into(),
            aggregate_complete: false,
            ready_to_finalize: false,
            ..ResultStudentRow::default()
        };
        let workspace = workspace_with_rows(vec![blocked]);
        assert!(selected_ready_rows(&workspace, &[]).is_err());
        assert!(selected_ready_rows(&workspace, &["student_1".into()]).is_err());
    }

    #[test]
    fn results_export_csv_includes_local_names_and_escapes_fields() {
        let temp_path = std::env::temp_dir().join(format!(
            "scriptscore-results-export-{}.csv",
            uuid::Uuid::new_v4()
        ));
        let mut workspace = workspace_with_rows(vec![ready_row("student_1")]);
        workspace.student_intake.items[0].local_display_name = Some("=Ada, Local".into());
        workspace.results_lms_rows[0].question_rows[0].effective_feedback_text =
            "+Good, \"clear\"\nwork".into();
        let rows = selected_ready_rows(&workspace, &["student_1".into()]).unwrap();
        write_csv_export(&workspace, &rows, &temp_path).unwrap();
        let csv = fs::read_to_string(&temp_path).unwrap();
        let _ = fs::remove_file(&temp_path);
        assert!(csv.contains("\"'=Ada, Local\""));
        assert!(csv.contains("\"'+Good, \"\"clear\"\"\nwork\""));
        assert!(!csv.contains("do-not-export"));
    }

    #[test]
    fn results_export_zip_contains_one_html_per_student() {
        let root = std::env::temp_dir().join(format!(
            "scriptscore-results-export-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let html_a = root.join("student_1_result.html");
        let html_b = root.join("student_2_result.html");
        fs::write(&html_a, "<html>A</html>").unwrap();
        fs::write(&html_b, "<html>B</html>").unwrap();
        let zip_path = root.join("results.zip");
        write_zip_from_html_paths(&[html_a, html_b], &zip_path).unwrap();
        let file = File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        assert_eq!(archive.len(), 2);
        let mut first = String::new();
        archive
            .by_name("student_1_result.html")
            .unwrap()
            .read_to_string(&mut first)
            .unwrap();
        assert_eq!(first, "<html>A</html>");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn results_export_zip_fails_when_generated_html_is_missing() {
        let root = std::env::temp_dir().join(format!(
            "scriptscore-results-export-missing-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let zip_path = root.join("results.zip");
        let missing = root.join("missing.html");
        let err = write_zip_from_html_paths(&[missing], &zip_path).unwrap_err();
        assert!(err
            .to_string()
            .contains("Could not read generated Results HTML export"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn results_export_html_rejects_missing_crop_artifacts() {
        let mut workspace = workspace_with_rows(vec![ready_row("student_1")]);
        workspace.student_workflow = StudentWorkflowState {
            status: "complete".into(),
            latest_job_id: None,
            submissions: vec![StudentWorkflowSubmission {
                student_ref: "student_1".into(),
                canonical_pdf_path: "/tmp/student.pdf".into(),
                page_count: 1,
                stage: "complete".into(),
                latest_job_id: None,
                failure_message: None,
                warnings: Vec::new(),
                page_artifacts: Vec::new(),
                alignment_pages: Vec::new(),
                detect_review: None,
                answers: vec![StudentWorkflowAnswer {
                    question_id: "q1".into(),
                    question_number: 1,
                    crop_image_path: Some("/tmp/scriptscore-missing-crop.png".into()),
                    pii_prescreen: None,
                    manual_grading_required: false,
                    manual_grading_reason: None,
                    moderation_eligible: true,
                    parse_status: "complete".into(),
                    parse_confidence: None,
                    parse_confidence_source: None,
                    raw_parsed_text: Some("answer".into()),
                    verified_text: None,
                    review_required: false,
                    verified: false,
                    stale: false,
                    grading_status: "complete".into(),
                    grading_confidence: None,
                    grading_confidence_reason: None,
                    question_max_points: Some(10),
                    total_points_awarded: Some(8),
                    feedback_text: Some("Good".into()),
                    criterion_results: Vec::new(),
                    highlights: Vec::new(),
                    warnings: Vec::new(),
                }],
            }],
        };

        let err = validate_html_export_sources(&workspace, &workspace.results_lms_rows[0])
            .expect_err("missing crop should fail HTML export validation");
        assert!(err.to_string().contains("was not found"));
    }
}
