// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::errors::{HostError, HostResult};
use crate::models::{
    AppSettings, CreateProjectInput, InstructorProfile, RubricCriterion, RubricUpdateInput,
    RuntimeJobEvent, StudentIntakeInput,
};
use crate::state::{AppState, RuntimeEventSink};

const RELEASE_SMOKE_ENV: &str = "SCRIPTSCORE_RELEASE_SMOKE";
const RESOURCE_DIR_ENV: &str = "SCRIPTSCORE_RELEASE_SMOKE_RESOURCE_DIR";
const DEFAULT_MODE: &str = "document";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseSmokeConfig {
    pub output_path: PathBuf,
    pub mode: String,
    pub resource_dir: Option<PathBuf>,
}

#[derive(Debug, Default)]
struct ReleaseSmokeArgs {
    enabled_arg: bool,
    output_path: Option<PathBuf>,
    mode: Option<String>,
    resource_dir: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseSmokeSummary {
    status: String,
    mode: String,
    app_version: String,
    platform: String,
    started_at_unix_ms: u128,
    finished_at_unix_ms: u128,
    synthetic: SyntheticSummary,
    template: TemplateSummary,
    intake: IntakeSummary,
    workflow: WorkflowSummary,
    events: EventSummary,
    result: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyntheticSummary {
    template_pages: i64,
    student_submissions: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TemplateSummary {
    status: String,
    question_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct IntakeSummary {
    status: String,
    row_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowSummary {
    status: String,
    submission_count: usize,
    terminal_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EventSummary {
    total_count: usize,
    command_counts: BTreeMap<String, usize>,
}

#[derive(Default)]
struct RecordingEventSink {
    events: Mutex<Vec<RuntimeJobEvent>>,
}

impl RecordingEventSink {
    fn snapshot(&self) -> Vec<RuntimeJobEvent> {
        self.events
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

impl RuntimeEventSink for RecordingEventSink {
    fn emit_runtime_event(&self, event: RuntimeJobEvent) {
        self.events
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(event);
    }
}

struct CleanupDir {
    path: PathBuf,
}

impl CleanupDir {
    fn new(prefix: &str) -> HostResult<Self> {
        let timestamp = now_unix_ms();
        let path =
            std::env::temp_dir().join(format!("{prefix}-{timestamp}-{}", std::process::id()));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for CleanupDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

pub fn run_from_env_args<I>(args: I) -> Option<i32>
where
    I: IntoIterator<Item = OsString>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    let config = match config_from_env_args(args.iter().map(OsString::as_os_str)) {
        Ok(Some(config)) => config,
        Ok(None) => return None,
        Err(error) => {
            eprintln!("scriptscore release smoke: {error}");
            return Some(2);
        }
    };

    let exit_code = match run_and_write_summary(&config) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("scriptscore release smoke failed: {error}");
            1
        }
    };
    Some(exit_code)
}

pub fn config_from_env_args<'a, I>(args: I) -> HostResult<Option<ReleaseSmokeConfig>>
where
    I: IntoIterator<Item = &'a OsStr>,
{
    let parsed = parse_args(args)?;
    if !release_smoke_env_enabled() || !parsed.enabled_arg {
        return Ok(None);
    }
    let output_path = parsed.output_path.ok_or_else(|| {
        HostError::Validation("--release-smoke-output is required for release smoke.".into())
    })?;
    let mode = parsed.mode.unwrap_or_else(|| DEFAULT_MODE.to_string());
    if mode != "document" && mode != "local-ai" {
        return Err(HostError::Validation(format!(
            "Unsupported release smoke mode '{mode}'. Expected 'document' or 'local-ai'."
        )));
    }
    let resource_dir = parsed
        .resource_dir
        .or_else(|| std::env::var_os(RESOURCE_DIR_ENV).map(PathBuf::from));
    Ok(Some(ReleaseSmokeConfig {
        output_path,
        mode,
        resource_dir,
    }))
}

fn parse_args<'a, I>(args: I) -> HostResult<ReleaseSmokeArgs>
where
    I: IntoIterator<Item = &'a OsStr>,
{
    let mut parsed = ReleaseSmokeArgs::default();
    let mut iter = args.into_iter();
    let _program = iter.next();
    while let Some(arg) = iter.next() {
        if arg == "--release-smoke" {
            parsed.enabled_arg = true;
        } else if arg == "--release-smoke-output" {
            parsed.output_path = Some(next_path_arg(&mut iter, "--release-smoke-output")?);
        } else if arg == "--release-smoke-mode" {
            parsed.mode = Some(next_string_arg(&mut iter, "--release-smoke-mode")?);
        } else if arg == "--release-smoke-resource-dir" {
            parsed.resource_dir = Some(next_path_arg(&mut iter, "--release-smoke-resource-dir")?);
        }
    }
    Ok(parsed)
}

fn next_path_arg<'a, I>(iter: &mut I, name: &str) -> HostResult<PathBuf>
where
    I: Iterator<Item = &'a OsStr>,
{
    Ok(PathBuf::from(next_os_arg(iter, name)?))
}

fn next_string_arg<'a, I>(iter: &mut I, name: &str) -> HostResult<String>
where
    I: Iterator<Item = &'a OsStr>,
{
    next_os_arg(iter, name)?
        .to_str()
        .map(String::from)
        .ok_or_else(|| HostError::Validation(format!("{name} must be valid UTF-8.")))
}

fn next_os_arg<'a, I>(iter: &mut I, name: &str) -> HostResult<&'a OsStr>
where
    I: Iterator<Item = &'a OsStr>,
{
    iter.next()
        .ok_or_else(|| HostError::Validation(format!("{name} requires a value.")))
}

fn release_smoke_env_enabled() -> bool {
    matches!(
        std::env::var(RELEASE_SMOKE_ENV).ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

fn run_and_write_summary(config: &ReleaseSmokeConfig) -> HostResult<()> {
    let summary = match config.mode.as_str() {
        "document" => run_document_smoke(config),
        "local-ai" => Ok(local_ai_skipped_summary()),
        _ => unreachable!("mode is validated before execution"),
    };
    let summary = match summary {
        Ok(summary) => summary,
        Err(error) => failure_summary(config, &error.to_string()),
    };
    write_summary(&config.output_path, &summary)?;
    if summary.status == "passed" || summary.status == "skipped" {
        Ok(())
    } else {
        Err(HostError::Validation(summary.result))
    }
}

pub fn run_document_smoke(config: &ReleaseSmokeConfig) -> HostResult<ReleaseSmokeSummary> {
    let started_at_unix_ms = now_unix_ms();
    let temp_dir = CleanupDir::new("scriptscore-release-smoke")?;
    let pdfs = prepare_smoke_pdfs(config.resource_dir.as_deref(), temp_dir.path())?;

    let state = AppState::bootstrap_for_release_smoke(config.resource_dir.clone());
    let event_sink = RecordingEventSink::default();
    let settings = AppSettings::default();
    create_smoke_project(&state, &event_sink, temp_dir.path(), &pdfs.template)?;
    approve_smoke_template_and_rubric(&state, &settings)?;
    run_smoke_intake(&state, &event_sink, &pdfs)?;
    let final_workspace = state
        .clone_inner()
        .begin_student_workflow(&settings, &event_sink)?;
    let terminal_counts = workflow_stage_counts(&final_workspace.student_workflow);
    ensure_expected_workflow_stages(&terminal_counts)?;

    Ok(document_smoke_summary(
        started_at_unix_ms,
        final_workspace,
        terminal_counts,
        &event_sink.snapshot(),
    ))
}

struct SmokePdfPaths {
    template: PathBuf,
    student_a: PathBuf,
    student_b: PathBuf,
}

fn prepare_smoke_pdfs(resource_dir: Option<&Path>, temp_root: &Path) -> HostResult<SmokePdfPaths> {
    let pdfs = SmokePdfPaths {
        template: temp_root.join("template.pdf"),
        student_a: temp_root.join("student-a.pdf"),
        student_b: temp_root.join("student-b.pdf"),
    };
    write_smoke_pdf(
        resource_dir,
        &pdfs.template,
        &[
            PositionedPdfText {
                x: 72,
                y: 120,
                text: "1. Explain westward expansion.",
            },
            PositionedPdfText {
                x: 460,
                y: 120,
                text: "5 pts",
            },
        ],
    )?;
    write_smoke_pdf(
        resource_dir,
        &pdfs.student_a,
        &[
            PositionedPdfText {
                x: 72,
                y: 120,
                text: "Student A",
            },
            PositionedPdfText {
                x: 72,
                y: 152,
                text: "1. Railroads and migration expanded settlements.",
            },
        ],
    )?;
    write_smoke_pdf(
        resource_dir,
        &pdfs.student_b,
        &[
            PositionedPdfText {
                x: 72,
                y: 120,
                text: "Student B",
            },
            PositionedPdfText {
                x: 72,
                y: 152,
                text: "1. Land policy and farming drove expansion.",
            },
        ],
    )?;
    Ok(pdfs)
}

fn create_smoke_project(
    state: &AppState,
    event_sink: &dyn RuntimeEventSink,
    temp_root: &Path,
    template_pdf: &Path,
) -> HostResult<()> {
    let shell = state.create_project(
        CreateProjectInput {
            display_name: "Release Smoke".into(),
            subject: Some("History".into()),
            course_code: Some("SMOKE 101".into()),
            lms_course_id: None,
            project_root: Some(temp_root.join("projects").to_string_lossy().into_owned()),
            template_pdf_path: template_pdf.to_string_lossy().into_owned(),
            instructor_profile: Some(InstructorProfile::default()),
        },
        event_sink,
    )?;
    if shell.current_project.is_none() {
        return Err(HostError::Project(
            "Release smoke project did not become current.".into(),
        ));
    }
    Ok(())
}

fn approve_smoke_template_and_rubric(state: &AppState, settings: &AppSettings) -> HostResult<()> {
    state.skip_template_redaction()?;
    let approved = state.approve_template_setup(settings)?;
    let question = approved
        .questions
        .first()
        .ok_or_else(|| HostError::Project("Release smoke found no template questions.".into()))?;
    state.save_rubric_update(RubricUpdateInput {
        question_id: question.question_id.clone(),
        criteria: vec![RubricCriterion {
            criterion_id: "criterion_1".into(),
            label: "Correctness".into(),
            points: question.max_points.unwrap_or(5),
            partial_credit_guidance: "Award credit for historically relevant explanation.".into(),
            source: "manual".into(),
        }],
        approve: true,
        rubric_edit_impact: None,
    })?;
    Ok(())
}

fn run_smoke_intake(
    state: &AppState,
    event_sink: &dyn RuntimeEventSink,
    pdfs: &SmokePdfPaths,
) -> HostResult<()> {
    state.run_student_intake(
        vec![
            StudentIntakeInput {
                student_ref: "release-smoke-a".into(),
                local_student_name: None,
                raw_pdf_path: pdfs.student_a.to_string_lossy().into_owned(),
                desired_page_order: vec![1],
                redaction_regions_px: Vec::new(),
                raster_sizes_by_page: Default::default(),
            },
            StudentIntakeInput {
                student_ref: "release-smoke-b".into(),
                local_student_name: None,
                raw_pdf_path: pdfs.student_b.to_string_lossy().into_owned(),
                desired_page_order: vec![1],
                redaction_regions_px: Vec::new(),
                raster_sizes_by_page: Default::default(),
            },
        ],
        event_sink,
    )?;
    Ok(())
}

fn workflow_stage_counts(
    workflow: &crate::models::StudentWorkflowState,
) -> BTreeMap<String, usize> {
    workflow.submissions.iter().fold(
        BTreeMap::<String, usize>::new(),
        |mut counts, submission| {
            *counts.entry(submission.stage.clone()).or_insert(0) += 1;
            counts
        },
    )
}

fn ensure_expected_workflow_stages(terminal_counts: &BTreeMap<String, usize>) -> HostResult<()> {
    if terminal_counts.is_empty()
        || terminal_counts.keys().any(|stage| {
            !matches!(
                stage.as_str(),
                "detect_review" | "manual_grading" | "graded" | "parse_review" | "stopped"
            )
        })
    {
        return Err(HostError::Project(format!(
            "Release smoke workflow ended in unexpected stage counts: {terminal_counts:?}"
        )));
    }
    Ok(())
}

fn document_smoke_summary(
    started_at_unix_ms: u128,
    final_workspace: crate::models::ExamWorkspaceState,
    terminal_counts: BTreeMap<String, usize>,
    events: &[RuntimeJobEvent],
) -> ReleaseSmokeSummary {
    let workflow = final_workspace.student_workflow;
    ReleaseSmokeSummary {
        status: "passed".into(),
        mode: "document".into(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        platform: std::env::consts::OS.into(),
        started_at_unix_ms,
        finished_at_unix_ms: now_unix_ms(),
        synthetic: SyntheticSummary {
            template_pages: 1,
            student_submissions: 2,
        },
        template: TemplateSummary {
            status: final_workspace.status,
            question_count: final_workspace.questions.len(),
        },
        intake: IntakeSummary {
            status: final_workspace.student_intake.status,
            row_count: final_workspace.student_intake.items.len(),
        },
        workflow: WorkflowSummary {
            status: workflow.status,
            submission_count: workflow.submissions.len(),
            terminal_counts,
        },
        events: event_summary(events),
        result: "Deterministic document release smoke reached an expected terminal or review workflow state.".into(),
    }
}

fn event_summary(events: &[RuntimeJobEvent]) -> EventSummary {
    let command_counts = events.iter().fold(BTreeMap::new(), |mut counts, event| {
        *counts.entry(event.command_name.clone()).or_insert(0) += 1;
        counts
    });
    EventSummary {
        total_count: events.len(),
        command_counts,
    }
}

fn local_ai_skipped_summary() -> ReleaseSmokeSummary {
    let now = now_unix_ms();
    ReleaseSmokeSummary {
        status: "skipped".into(),
        mode: "local-ai".into(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        platform: std::env::consts::OS.into(),
        started_at_unix_ms: now,
        finished_at_unix_ms: now,
        synthetic: SyntheticSummary {
            template_pages: 0,
            student_submissions: 0,
        },
        template: TemplateSummary {
            status: "skipped".into(),
            question_count: 0,
        },
        intake: IntakeSummary {
            status: "skipped".into(),
            row_count: 0,
        },
        workflow: WorkflowSummary {
            status: "skipped".into(),
            submission_count: 0,
            terminal_counts: BTreeMap::new(),
        },
        events: EventSummary {
            total_count: 0,
            command_counts: BTreeMap::new(),
        },
        result: "Full local AI release smoke is reserved for SCRIPTSCORE_SMOKE_OLLAMA_URL and SCRIPTSCORE_SMOKE_OLLAMA_MODEL configuration.".into(),
    }
}

fn failure_summary(config: &ReleaseSmokeConfig, message: &str) -> ReleaseSmokeSummary {
    let now = now_unix_ms();
    ReleaseSmokeSummary {
        status: "failed".into(),
        mode: config.mode.clone(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        platform: std::env::consts::OS.into(),
        started_at_unix_ms: now,
        finished_at_unix_ms: now,
        synthetic: SyntheticSummary {
            template_pages: 0,
            student_submissions: 0,
        },
        template: TemplateSummary {
            status: "failed".into(),
            question_count: 0,
        },
        intake: IntakeSummary {
            status: "failed".into(),
            row_count: 0,
        },
        workflow: WorkflowSummary {
            status: "failed".into(),
            submission_count: 0,
            terminal_counts: BTreeMap::new(),
        },
        events: EventSummary {
            total_count: 0,
            command_counts: BTreeMap::new(),
        },
        result: redact_local_paths(message),
    }
}

fn write_summary(path: &Path, summary: &ReleaseSmokeSummary) -> HostResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let contents = serde_json::to_string_pretty(summary)?;
    std::fs::write(path, format!("{contents}\n"))?;
    Ok(())
}

#[cfg(test)]
fn write_simple_pdf(path: &Path, lines: &[&str]) -> HostResult<()> {
    let positioned = lines
        .iter()
        .enumerate()
        .map(|(index, text)| PositionedPdfText {
            x: 72,
            y: 720 - (index as i64 * 16),
            text,
        })
        .collect::<Vec<_>>();
    write_positioned_pdf(path, &positioned)
}

fn write_smoke_pdf(
    resource_dir: Option<&Path>,
    path: &Path,
    lines: &[PositionedPdfText<'_>],
) -> HostResult<()> {
    let Some(python) = release_smoke_pdf_python(resource_dir) else {
        return write_positioned_pdf(path, lines);
    };
    write_positioned_pdf_with_python(&python, path, lines)
}

struct PositionedPdfText<'a> {
    x: i64,
    y: i64,
    text: &'a str,
}

fn write_positioned_pdf(path: &Path, lines: &[PositionedPdfText<'_>]) -> HostResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut content = String::new();
    for line in lines {
        content.push_str(&format!(
            "BT\n/F1 12 Tf\n{} {} Td\n({}) Tj\nET\n",
            line.x,
            line.y,
            escape_pdf_text(line.text)
        ));
    }
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{}endstream", content.len(), content),
    ];
    let mut pdf = Vec::<u8>::from("%PDF-1.4\n".as_bytes());
    let mut offsets = Vec::with_capacity(objects.len() + 1);
    offsets.push(0usize);
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
    }
    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    std::fs::write(path, pdf)?;
    Ok(())
}

fn write_positioned_pdf_with_python(
    python: &Path,
    path: &Path,
    lines: &[PositionedPdfText<'_>],
) -> HostResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let payload = lines
        .iter()
        .map(|line| {
            serde_json::json!({
                "x": line.x,
                "y": line.y,
                "text": line.text,
            })
        })
        .collect::<Vec<_>>();
    let script = r#"
import json
import sys
import fitz

document = fitz.open()
page = document.new_page()
for item in json.loads(sys.argv[2]):
    page.insert_text((item["x"], item["y"]), item["text"], fontsize=12)
document.save(sys.argv[1])
"#;
    let output = Command::new(python)
        .arg("-c")
        .arg(script)
        .arg(path)
        .arg(serde_json::to_string(&payload)?)
        .output()
        .map_err(|error| {
            HostError::Worker(format!(
                "Could not start release smoke PDF generator '{}': {error}",
                python.display()
            ))
        })?;
    if !output.status.success() {
        let details = String::from_utf8_lossy(if output.stderr.is_empty() {
            &output.stdout
        } else {
            &output.stderr
        });
        return Err(HostError::Worker(format!(
            "Release smoke PDF generation failed: {}",
            details.trim()
        )));
    }
    Ok(())
}

fn release_smoke_pdf_python(resource_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SCRIPTSCORE_PYTHON").map(PathBuf::from) {
        return Some(path);
    }
    let runtime_root = resource_dir?.join("runtime");
    let manifest_path = runtime_root.join("runtime-manifest.json");
    let manifest = std::fs::read_to_string(manifest_path).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest).ok()?;
    let python = manifest.get("pythonExecutable")?.as_str()?;
    let path = PathBuf::from(python);
    Some(if path.is_absolute() {
        path
    } else {
        runtime_root.join(path)
    })
}

fn escape_pdf_text(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '(' => "\\(".chars().collect(),
            ')' => "\\)".chars().collect(),
            _ => vec![ch],
        })
        .collect()
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis()
}

fn redact_local_paths(message: &str) -> String {
    let temp_dir = std::env::temp_dir().to_string_lossy().into_owned();
    message.replace(&temp_dir, "<temp>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{lock_env_vars, EnvVarGuard};

    #[test]
    fn config_requires_env_and_flag() {
        let _guard = lock_env_vars();
        let _env = EnvVarGuard::set(RELEASE_SMOKE_ENV, "1");
        let config = config_from_env_args([
            OsStr::new("scriptscore"),
            OsStr::new("--release-smoke-output"),
            OsStr::new("summary.json"),
        ])
        .expect("config should parse");
        assert!(config.is_none());
    }

    #[test]
    fn config_parses_document_mode_and_resource_dir() {
        let _guard = lock_env_vars();
        let _env = EnvVarGuard::set(RELEASE_SMOKE_ENV, "1");
        let config = config_from_env_args([
            OsStr::new("scriptscore"),
            OsStr::new("--release-smoke"),
            OsStr::new("--release-smoke-output"),
            OsStr::new("summary.json"),
            OsStr::new("--release-smoke-mode"),
            OsStr::new("document"),
            OsStr::new("--release-smoke-resource-dir"),
            OsStr::new("resources"),
        ])
        .expect("config should parse")
        .expect("config should be enabled");
        assert_eq!(config.output_path, PathBuf::from("summary.json"));
        assert_eq!(config.mode, "document");
        assert_eq!(config.resource_dir, Some(PathBuf::from("resources")));
    }

    #[test]
    fn config_rejects_unknown_mode() {
        let _guard = lock_env_vars();
        let _env = EnvVarGuard::set(RELEASE_SMOKE_ENV, "1");
        let error = config_from_env_args([
            OsStr::new("scriptscore"),
            OsStr::new("--release-smoke"),
            OsStr::new("--release-smoke-output"),
            OsStr::new("summary.json"),
            OsStr::new("--release-smoke-mode"),
            OsStr::new("surprise"),
        ])
        .expect_err("unknown mode should fail");
        assert!(error.to_string().contains("Unsupported release smoke mode"));
    }

    #[test]
    fn simple_pdf_contains_cross_reference_table() {
        let root = CleanupDir::new("scriptscore-release-smoke-pdf-test").expect("temp root");
        let path = root.path().join("sample.pdf");
        write_simple_pdf(&path, &["Hello (world)"]).expect("pdf should write");
        let bytes = std::fs::read(&path).expect("pdf should read");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("xref"));
        assert!(text.contains("Hello \\(world\\)"));
    }
}
