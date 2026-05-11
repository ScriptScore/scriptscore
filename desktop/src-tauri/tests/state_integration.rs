// SPDX-License-Identifier: AGPL-3.0-only
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use serde_json::json;

use scriptscore_desktop_host::errors::HostError;
use scriptscore_desktop_host::models::{
    AppSettings, CreateProjectInput, InstructorProfile, ProjectConfig, QuestionEdit,
    RubricCriterion, RubricUpdateInput, RuntimeJobEvent, TemplateRedactionRegionInput,
    WorkerStatus,
};
use scriptscore_desktop_host::state::AppState;
use scriptscore_desktop_host::test_support::{lock_env_vars, EnvVarGuard};

const POST_SETUP_EVENT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Default)]
struct RecordingEventSink {
    events: Arc<Mutex<Vec<RuntimeJobEvent>>>,
}

impl RecordingEventSink {
    fn snapshot(&self) -> Vec<RuntimeJobEvent> {
        self.events.lock().expect("event sink lock").clone()
    }

    fn wait_for(&self, event_type: &str, command_name: &str) {
        self.wait_for_with_timeout(event_type, command_name, Duration::from_secs(5));
    }

    fn wait_for_with_timeout(&self, event_type: &str, command_name: &str, timeout: Duration) {
        self.wait_for_with_timeout_and_diagnostics(event_type, command_name, timeout, None);
    }

    fn wait_for_with_timeout_and_diagnostics(
        &self,
        event_type: &str,
        command_name: &str,
        timeout: Duration,
        diagnostics_root: Option<&Path>,
    ) {
        let deadline = Instant::now() + timeout;
        loop {
            if self
                .snapshot()
                .iter()
                .any(|event| event.event_type == event_type && event.command_name == command_name)
            {
                return;
            }
            if Instant::now() >= deadline {
                panic!(
                    "timed out waiting for runtime event {event_type} for {command_name}; observed events: {}; diagnostics: {}",
                    self.event_summary(),
                    timeout_diagnostics(diagnostics_root)
                );
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn wait_for_terminal(&self, command_name: &str, timeout: Duration) -> RuntimeJobEvent {
        self.wait_for_terminal_with_diagnostics(command_name, timeout, None)
    }

    fn wait_for_terminal_with_diagnostics(
        &self,
        command_name: &str,
        timeout: Duration,
        diagnostics_root: Option<&Path>,
    ) -> RuntimeJobEvent {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(event) = self.snapshot().into_iter().find(|event| {
                event.command_name == command_name
                    && matches!(
                        event.event_type.as_str(),
                        "job_finished" | "job_failed" | "job_cancelled"
                    )
            }) {
                return event;
            }
            if Instant::now() >= deadline {
                panic!(
                    "timed out waiting for terminal runtime event for {command_name}; observed events: {}; diagnostics: {}",
                    self.event_summary(),
                    timeout_diagnostics(diagnostics_root)
                );
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn event_summary(&self) -> String {
        let events = self.snapshot();
        if events.is_empty() {
            return "none".into();
        }
        events
            .iter()
            .map(|event| format!("{}:{}", event.command_name, event.event_type))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn timeout_diagnostics(root: Option<&Path>) -> String {
    root.map(durable_project_state_summary)
        .unwrap_or_else(|| "no durable project diagnostics root provided".into())
}

fn durable_project_state_summary(root: &Path) -> String {
    let mut db_paths = Vec::new();
    collect_project_db_paths(root, &mut db_paths);
    db_paths.sort();
    if db_paths.is_empty() {
        return format!("no scriptscore.db files found under {}", root.display());
    }

    let mut summary = String::new();
    for db_path in db_paths {
        let project_path = db_path.parent().unwrap_or(root);
        let _ = write!(summary, "[project: {}]", project_path.display());
        match Connection::open(&db_path) {
            Ok(connection) => {
                append_job_run_summary(&connection, &mut summary);
                append_template_setup_summary(&connection, &mut summary);
            }
            Err(err) => {
                let _ = write!(summary, " db_open_error={err}");
            }
        }
        summary.push(' ');
    }
    summary.trim().to_string()
}

fn collect_project_db_paths(root: &Path, db_paths: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().and_then(|name| name.to_str()) == Some("scriptscore.db") {
            db_paths.push(path);
        } else if path.is_dir() {
            collect_project_db_paths(&path, db_paths);
        }
    }
}

fn append_job_run_summary(connection: &Connection, summary: &mut String) {
    let mut statement = match connection.prepare(
        "SELECT command_name, state, started_at, finished_at, error_json
         FROM job_run
         ORDER BY rowid ASC",
    ) {
        Ok(statement) => statement,
        Err(err) => {
            let _ = write!(summary, " job_run_query_error={err}");
            return;
        }
    };
    let rows = match statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    }) {
        Ok(rows) => rows,
        Err(err) => {
            let _ = write!(summary, " job_run_rows_error={err}");
            return;
        }
    };

    let mut count = 0;
    summary.push_str(" job_run=[");
    for row in rows {
        let Ok((command_name, state, started_at, finished_at, error_json)) = row else {
            continue;
        };
        if count > 0 {
            summary.push_str("; ");
        }
        let _ = write!(
            summary,
            "{command_name}:{state}:started={}:finished={}",
            started_at.as_deref().unwrap_or("null"),
            finished_at.as_deref().unwrap_or("null")
        );
        if let Some(error_json) = error_json.as_deref().filter(|value| !value.is_empty()) {
            let _ = write!(summary, ":error={error_json}");
        }
        count += 1;
    }
    if count == 0 {
        summary.push_str("none");
    }
    summary.push(']');
}

fn append_template_setup_summary(connection: &Connection, summary: &mut String) {
    let mut statement = match connection.prepare(
        "SELECT scope_id, payload_json
         FROM workflow_state
         WHERE state_key = 'template_setup'
         ORDER BY rowid ASC",
    ) {
        Ok(statement) => statement,
        Err(err) => {
            let _ = write!(summary, " template_setup_query_error={err}");
            return;
        }
    };
    let rows = match statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }) {
        Ok(rows) => rows,
        Err(err) => {
            let _ = write!(summary, " template_setup_rows_error={err}");
            return;
        }
    };

    let mut count = 0;
    summary.push_str(" template_setup=[");
    for row in rows {
        let Ok((scope_id, payload_json)) = row else {
            continue;
        };
        let payload = serde_json::from_str::<serde_json::Value>(&payload_json)
            .unwrap_or(serde_json::Value::Null);
        let failure = payload
            .get("failureMessage")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let last_setup_job_id = payload
            .get("lastSetupJobId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("null");
        let question_count = payload
            .get("questionCount")
            .and_then(serde_json::Value::as_i64)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".into());
        if count > 0 {
            summary.push_str("; ");
        }
        let _ = write!(
            summary,
            "{scope_id}:lastSetupJobId={last_setup_job_id}:questionCount={question_count}"
        );
        if !failure.is_empty() {
            let _ = write!(summary, ":failure={failure}");
        }
        count += 1;
    }
    if count == 0 {
        summary.push_str("none");
    }
    summary.push(']');
}

impl scriptscore_desktop_host::state::RuntimeEventSink for RecordingEventSink {
    fn emit_runtime_event(&self, event: RuntimeJobEvent) {
        self.events.lock().expect("event sink lock").push(event);
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("repo root")
        .to_path_buf()
}

fn test_python() -> PathBuf {
    let repo_root = repo_root();
    let unix = repo_root
        .join("cli")
        .join(".venv")
        .join("bin")
        .join("python");
    if unix.is_file() {
        return unix;
    }
    let windows = repo_root
        .join("cli")
        .join(".venv")
        .join("Scripts")
        .join("python.exe");
    if windows.is_file() {
        return windows;
    }
    PathBuf::from(if cfg!(windows) { "python" } else { "python3" })
}

fn temp_root(prefix: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis();
    std::env::temp_dir().join(format!("{prefix}-{timestamp}"))
}

fn create_template_pdf(path: &std::path::Path) {
    create_template_pdf_with_question(path, "1. Explain westward expansion.", 5);
}

fn create_template_pdf_with_question(path: &std::path::Path, prompt_text: &str, max_points: i64) {
    path.parent()
        .expect("template pdf parent")
        .create_dir_all()
        .expect("template parent should exist");
    let script = r#"
import sys
import fitz

document = fitz.open()
page = document.new_page()
page.insert_text((72, 120), sys.argv[2], fontsize=12)
page.insert_text((460, 120), f"{sys.argv[3]} pts", fontsize=12)
document.save(sys.argv[1])
"#;
    let output = Command::new(test_python())
        .arg("-c")
        .arg(script)
        .arg(path)
        .arg(prompt_text)
        .arg(max_points.to_string())
        .output()
        .expect("template pdf generation should start");
    assert!(
        output.status.success(),
        "template pdf generation failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

trait CreateDirAllExt {
    fn create_dir_all(&self) -> std::io::Result<()>;
}

impl CreateDirAllExt for &std::path::Path {
    fn create_dir_all(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self)
    }
}

fn create_project_input(template_pdf_path: &std::path::Path) -> CreateProjectInput {
    CreateProjectInput {
        display_name: "Midterm 1".into(),
        subject: Some("Physics".into()),
        course_code: Some("PHYS 221".into()),
        lms_course_id: None,
        project_root: None,
        template_pdf_path: template_pdf_path.to_string_lossy().into_owned(),
        instructor_profile: Some(InstructorProfile::default()),
    }
}

fn create_project_input_with_guidance(
    template_pdf_path: &std::path::Path,
    guidance: &str,
) -> CreateProjectInput {
    CreateProjectInput {
        display_name: "Midterm 1".into(),
        subject: Some("Physics".into()),
        course_code: Some("PHYS 221".into()),
        lms_course_id: None,
        project_root: None,
        template_pdf_path: template_pdf_path.to_string_lossy().into_owned(),
        instructor_profile: Some(InstructorProfile {
            additional_guidance: guidance.to_string(),
            ..Default::default()
        }),
    }
}

fn app_settings() -> AppSettings {
    AppSettings::default()
}

fn project_path_from_shell(shell: &scriptscore_desktop_host::models::ShellState) -> PathBuf {
    PathBuf::from(
        shell
            .current_project
            .as_ref()
            .expect("project should be open")
            .project_path
            .clone(),
    )
}

fn open_project_db(project_path: &std::path::Path) -> Connection {
    Connection::open(project_path.join("scriptscore.db")).expect("project db should open")
}

fn latest_job_request_json(project_path: &std::path::Path, command_name: &str) -> String {
    open_project_db(project_path)
        .query_row(
            "SELECT request_json FROM job_run WHERE command_name = ?1 ORDER BY rowid DESC LIMIT 1",
            [command_name],
            |row| row.get(0),
        )
        .expect("job request should exist")
}

fn update_template_setup_payload(
    project_path: &std::path::Path,
    mutator: impl FnOnce(&mut serde_json::Value),
) {
    let connection = open_project_db(project_path);
    let (scope_id, payload_json): (String, String) = connection
        .query_row(
            "SELECT scope_id, payload_json FROM workflow_state WHERE state_key = 'template_setup' LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("template setup state should exist");
    let mut payload: serde_json::Value =
        serde_json::from_str(&payload_json).expect("payload json should parse");
    mutator(&mut payload);
    connection
        .execute(
            "UPDATE workflow_state SET payload_json = ?1 WHERE scope_id = ?2 AND state_key = 'template_setup'",
            params![payload.to_string(), scope_id],
        )
        .expect("template setup payload should update");
}

fn artifact_relative_paths(project_path: &std::path::Path) -> Vec<String> {
    let connection = open_project_db(project_path);
    let mut statement = connection
        .prepare("SELECT relative_path FROM artifact ORDER BY artifact_id ASC")
        .expect("artifact query should prepare");
    let rows = statement
        .query_map([], |row| row.get(0))
        .expect("artifact rows should load");
    rows.map(|row| row.expect("artifact relative path"))
        .collect::<Vec<String>>()
}

fn question_edit(question_id: &str, text: &str, max_points: i64) -> QuestionEdit {
    QuestionEdit {
        question_id: question_id.to_string(),
        question_number: 1,
        page_number: 1,
        max_points: Some(max_points),
        text: text.to_string(),
        question_context: None,
        rubric_edit_impact: None,
    }
}

fn single_criterion(points: i64) -> RubricCriterion {
    RubricCriterion {
        criterion_id: "criterion_1".into(),
        label: "Correctness".into(),
        points,
        partial_credit_guidance: format!("Award up to {points} points."),
        source: "manual".into(),
    }
}

#[test]
fn saving_project_setup_updates_project_metadata_summary() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-state-project-setup");
    let template_pdf = test_root.join("fixtures").join("template.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&template_pdf);
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());

    let state = AppState::bootstrap();
    let setup_events = RecordingEventSink::default();
    let shell = state
        .create_project(create_project_input(&template_pdf), &setup_events)
        .expect("project should be created");
    let project_path = project_path_from_shell(&shell);

    let current = state
        .workspace_state()
        .expect("workspace should load")
        .project_config;
    let updated_workspace = state
        .save_project_config(
            ProjectConfig {
                display_name: "Chemistry Final".into(),
                subject: Some("Chemistry".into()),
                course_code: Some("CHEM 301".into()),
                redaction_required: false,
                ..current
            },
            &app_settings(),
        )
        .expect("project config should save");

    assert_eq!(updated_workspace.project.display_name, "Chemistry Final");
    assert_eq!(
        updated_workspace.project.subject.as_deref(),
        Some("Chemistry")
    );
    assert_eq!(
        updated_workspace.project.course_code.as_deref(),
        Some("CHEM 301")
    );
    assert!(!updated_workspace.project_config.redaction_required);
    assert_eq!(
        updated_workspace.status_label,
        "Redaction skipped, review questions"
    );

    let reopened = state
        .open_project(project_path.clone(), &app_settings())
        .expect("project should reopen");
    let reopened_project = reopened.current_project.expect("project should be open");
    assert_eq!(reopened_project.display_name, "Chemistry Final");
    assert_eq!(reopened_project.subject.as_deref(), Some("Chemistry"));
    assert_eq!(reopened_project.course_code.as_deref(), Some("CHEM 301"));

    let connection = open_project_db(&project_path);
    let persisted: (String, Option<String>, Option<String>, bool) = connection
        .query_row(
            "SELECT display_name, subject, course_code, redaction_required FROM project LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("project should load");
    assert_eq!(persisted.0, "Chemistry Final");
    assert_eq!(persisted.1.as_deref(), Some("Chemistry"));
    assert_eq!(persisted.2.as_deref(), Some("CHEM 301"));
    assert!(!persisted.3);
}

#[test]
fn question_edit_after_rubric_approval_blocks_intake_until_reapproval() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-state-stale-rubric");
    let template_pdf = test_root.join("fixtures").join("template.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&template_pdf);
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());

    let state = AppState::bootstrap();
    let setup_events = RecordingEventSink::default();
    let shell = state
        .create_project(create_project_input(&template_pdf), &setup_events)
        .expect("project should be created");
    let project_path = project_path_from_shell(&shell);
    let question_id = state
        .workspace_state()
        .expect("workspace should load")
        .questions[0]
        .question_id
        .clone();
    open_project_db(&project_path)
        .execute(
            "INSERT OR REPLACE INTO workflow_state (
                scope_type, scope_id, state_key, status, payload_json, updated_at
            ) VALUES ('question', ?1, 'question_analysis', 'ok', ?2, CURRENT_TIMESTAMP)",
            params![
                question_id.clone(),
                json!({
                    "status": "ok",
                    "questionTextClean": "1. Explain westward expansion.",
                    "questionContext": "",
                    "warnings": [],
                    "latestJobId": null
                })
                .to_string()
            ],
        )
        .expect("analysis state should persist");
    state
        .save_redaction_regions(vec![sample_redaction()])
        .expect("redactions should save");
    let approved = state
        .save_rubric_update(RubricUpdateInput {
            question_id: question_id.clone(),
            criteria: vec![single_criterion(5)],
            approve: true,
            rubric_edit_impact: None,
        })
        .expect("rubric should approve");
    assert_eq!(approved.workflow_stage, "student_intake_ready");

    let stale = state
        .save_question_edits(vec![QuestionEdit {
            rubric_edit_impact: Some("grading".into()),
            ..question_edit(&question_id, "1. Explain westward migration.", 5)
        }])
        .expect("question edit should save");
    assert_eq!(stale.workflow_stage, "rubric_authoring");
    assert_eq!(stale.questions[0].rubric.approved_at, None);

    let reapproved = state
        .save_rubric_update(RubricUpdateInput {
            question_id: question_id.clone(),
            criteria: vec![single_criterion(5)],
            approve: true,
            rubric_edit_impact: None,
        })
        .expect("rubric should reapprove");
    assert_eq!(reapproved.workflow_stage, "student_intake_ready");

    let minor = state
        .save_question_edits(vec![QuestionEdit {
            rubric_edit_impact: Some("minor".into()),
            ..question_edit(&question_id, "1. Explain westward migration briefly.", 5)
        }])
        .expect("minor question edit should save");
    assert_eq!(minor.workflow_stage, "student_intake_ready");
    assert!(minor.questions[0].rubric.approved_at.is_some());
}

fn sample_redaction() -> TemplateRedactionRegionInput {
    TemplateRedactionRegionInput {
        region_id: None,
        page_number: 1,
        x: 16,
        y: 24,
        width: 180,
        height: 42,
    }
}

#[test]
fn host_side_reanalyze_validation_failure_keeps_worker_status_ready() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-state-reanalyze-validation");
    let template_pdf = test_root.join("fixtures").join("template.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&template_pdf);
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());

    let state = AppState::bootstrap();
    let setup_events = RecordingEventSink::default();
    state
        .create_project(create_project_input(&template_pdf), &setup_events)
        .expect("project should be created");

    let action_events = RecordingEventSink::default();
    let job_id = state
        .start_reanalyze_question_job(
            "missing-question".into(),
            app_settings(),
            Arc::new(action_events.clone()),
        )
        .expect("re-analyze should return a background job id");
    assert!(!job_id.is_empty());

    action_events.wait_for("job_failed", "reanalyze_question");
    let failed_event = action_events
        .snapshot()
        .into_iter()
        .find(|event| {
            event.event_type == "job_failed" && event.command_name == "reanalyze_question"
        })
        .expect("job_failed event should be emitted");

    assert!(matches!(failed_event.worker_status, WorkerStatus::Ready));
    assert!(matches!(
        state
            .shell_state()
            .expect("shell state should load")
            .worker_status,
        WorkerStatus::Ready
    ));
}

#[test]
fn host_side_reanalyze_rejects_approved_rubric_until_approval_is_rescinded() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-state-reanalyze-approved-rubric");
    let template_pdf = test_root.join("fixtures").join("template.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&template_pdf);
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());

    let state = AppState::bootstrap();
    let setup_events = RecordingEventSink::default();
    state
        .create_project(create_project_input(&template_pdf), &setup_events)
        .expect("project should be created");
    let question_id = state
        .workspace_state()
        .expect("workspace should load")
        .questions[0]
        .question_id
        .clone();
    state
        .save_rubric_update(RubricUpdateInput {
            question_id: question_id.clone(),
            criteria: vec![single_criterion(5)],
            approve: true,
            rubric_edit_impact: None,
        })
        .expect("rubric should approve");

    let action_events = RecordingEventSink::default();
    state
        .start_reanalyze_question_job(question_id, app_settings(), Arc::new(action_events.clone()))
        .expect("re-analyze should return a background job id");

    action_events.wait_for("job_failed", "reanalyze_question");
    let failed_event = action_events
        .snapshot()
        .into_iter()
        .find(|event| {
            event.event_type == "job_failed" && event.command_name == "reanalyze_question"
        })
        .expect("job_failed event should be emitted");
    let message = failed_event
        .payload
        .get("message")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    assert!(message.contains("rescind rubric approval first"));
    assert!(matches!(failed_event.worker_status, WorkerStatus::Ready));
}

#[test]
fn create_project_runs_exam_setup_and_supports_open_close_lifecycle() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-state-create");
    let template_pdf = test_root.join("fixtures").join("template.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&template_pdf);
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());
    let event_sink = RecordingEventSink::default();

    let state = AppState::bootstrap();
    let shell = state
        .create_project(
            create_project_input_with_guidance(&template_pdf, "Prefer concise rubric criteria."),
            &event_sink,
        )
        .expect("project should be created");
    let project_path = project_path_from_shell(&shell);
    let workspace = state.workspace_state().expect("workspace should load");

    assert!(matches!(
        state.shell_state().expect("shell").worker_status,
        WorkerStatus::Ready
    ));
    assert_eq!(workspace.status, "draft");
    assert_eq!(workspace.questions.len(), 1);
    assert_eq!(workspace.template_preview_artifacts.len(), 1);
    assert_eq!(
        workspace
            .project_config
            .instructor_profile
            .additional_guidance,
        "Prefer concise rubric criteria."
    );
    assert!(event_sink
        .snapshot()
        .iter()
        .any(|event| { event.command_name == "exam.setup" && event.event_type == "job_finished" }));

    let closed = state.close_current_project().expect("project should close");
    assert!(closed.current_project.is_none());

    let reopened = state
        .open_project(project_path, &app_settings())
        .expect("project should reopen");
    assert_eq!(
        reopened
            .current_project
            .as_ref()
            .expect("project should reopen")
            .display_name,
        "Midterm 1"
    );
}

#[test]
fn open_project_recovers_abandoned_runtime_and_host_workflow_rows() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-state-abandoned-jobs");
    let template_pdf = test_root.join("fixtures").join("template.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&template_pdf);
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());
    let event_sink = RecordingEventSink::default();

    let state = AppState::bootstrap();
    let shell = state
        .create_project(create_project_input(&template_pdf), &event_sink)
        .expect("project should be created");
    let project_path = project_path_from_shell(&shell);
    state.close_current_project().expect("project should close");
    {
        let connection = open_project_db(&project_path);
        connection
            .execute(
                "INSERT INTO job_run (
                    job_id,
                    command_name,
                    request_id,
                    state,
                    submitted_at,
                    started_at,
                    finished_at,
                    request_json,
                    result_json,
                    error_json
                ) VALUES (?1, 'smoke.ping', 'req_stale', 'running', '1', '2', NULL, '{}', NULL, NULL)",
                params!["stale_job"],
            )
            .expect("stale runtime job should insert");
        connection
            .execute(
                "INSERT INTO host_workflow_job (
                    workflow_id,
                    command_name,
                    result_kind,
                    state,
                    submitted_at,
                    started_at,
                    finished_at,
                    workspace_changed,
                    result_json,
                    error_json
                ) VALUES (?1, 'begin_student_workflow', 'workspace', 'running', '1', '1', NULL, 0, NULL, NULL)",
                params!["stale_workflow"],
            )
            .expect("stale host workflow should insert");
    }

    let reopened = state
        .open_project(project_path.clone(), &app_settings())
        .expect("project should reopen");

    assert!(reopened
        .last_runtime_error
        .as_deref()
        .unwrap_or_default()
        .contains("Recovered 2 stale desktop job records"));
    let connection = open_project_db(&project_path);
    let runtime_state: (String, Option<String>, Option<String>) = connection
        .query_row(
            "SELECT state, finished_at, error_json FROM job_run WHERE job_id = 'stale_job'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("runtime row should load");
    let workflow_state: (String, Option<String>, Option<String>) = connection
        .query_row(
            "SELECT state, finished_at, error_json FROM host_workflow_job WHERE workflow_id = 'stale_workflow'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("workflow row should load");

    assert_eq!(runtime_state.0, "failed");
    assert!(runtime_state.1.is_some());
    assert!(runtime_state
        .2
        .as_deref()
        .unwrap_or_default()
        .contains("desktop_session_ended"));
    assert_eq!(workflow_state.0, "failed");
    assert!(workflow_state.1.is_some());
    assert!(workflow_state
        .2
        .as_deref()
        .unwrap_or_default()
        .contains("desktop_session_ended"));
}

#[test]
fn start_create_project_job_enqueues_exam_analyze_with_matching_job_history_payload() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-post-setup-queue");
    let template_pdf = test_root.join("fixtures").join("template.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&template_pdf);
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());

    let state = AppState::bootstrap();
    let events = RecordingEventSink::default();
    let input = create_project_input(&template_pdf);
    let mut settings = app_settings();
    settings.ai_assist_categories.question_analysis = true;
    state
        .start_create_project_job(&input, settings, Arc::new(events.clone()))
        .expect("create project job should start");

    let create_project_event = events.wait_for_terminal_with_diagnostics(
        "create_project",
        POST_SETUP_EVENT_TIMEOUT,
        Some(&test_root),
    );
    assert_eq!(
        create_project_event.event_type,
        "job_finished",
        "create project should finish before post-setup jobs; diagnostics: {}",
        durable_project_state_summary(&test_root)
    );
    events.wait_for_with_timeout_and_diagnostics(
        "job_started",
        "scans.pdf-detect-aruco",
        POST_SETUP_EVENT_TIMEOUT,
        Some(&test_root),
    );
    events.wait_for_with_timeout_and_diagnostics(
        "job_started",
        "exam.analyze",
        POST_SETUP_EVENT_TIMEOUT,
        Some(&test_root),
    );

    let emitted = events.snapshot();
    let create_finished_index = emitted
        .iter()
        .position(|event| {
            event.command_name == "create_project" && event.event_type == "job_finished"
        })
        .expect("create project should finish");
    let aruco_started_index = emitted
        .iter()
        .position(|event| {
            event.command_name == "scans.pdf-detect-aruco" && event.event_type == "job_started"
        })
        .expect("ArUco detection should start after project creation");
    let analyze_started_index = emitted
        .iter()
        .position(|event| event.command_name == "exam.analyze" && event.event_type == "job_started")
        .expect("analysis should start after ArUco detection");
    assert!(
        create_finished_index < aruco_started_index,
        "project creation should finish before background ArUco detection starts"
    );
    assert!(
        aruco_started_index < analyze_started_index,
        "background ArUco detection should run ahead of automatic analysis"
    );

    assert!(
        !events
            .snapshot()
            .iter()
            .any(|e| { e.command_name == "exam.generate-rubric" && e.event_type == "job_queued" }),
        "automatic rubric generation is deferred until review and analysis readiness"
    );

    let project_path =
        project_path_from_shell(&state.shell_state().expect("shell state should load"));
    let request = latest_job_request_json(&project_path, "exam.analyze");
    assert!(request.contains("question_target_ids"));
    assert!(request.contains("question_targets"));
}

#[test]
fn start_create_project_job_skips_exam_analyze_when_exam_analysis_is_disabled() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-no-exam-analysis-post-setup");
    let template_pdf = test_root.join("fixtures").join("template.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&template_pdf);
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());

    let state = AppState::bootstrap();
    let events = RecordingEventSink::default();
    let input = create_project_input(&template_pdf);
    let mut settings = app_settings();
    settings.ai_assist_categories.question_analysis = false;
    state
        .start_create_project_job(&input, settings, Arc::new(events.clone()))
        .expect("create project job should start");

    let create_project_event = events.wait_for_terminal_with_diagnostics(
        "create_project",
        POST_SETUP_EVENT_TIMEOUT,
        Some(&test_root),
    );
    assert_eq!(
        create_project_event.event_type,
        "job_finished",
        "create project should finish before post-setup jobs; diagnostics: {}",
        durable_project_state_summary(&test_root)
    );
    events.wait_for_with_timeout_and_diagnostics(
        "job_finished",
        "scans.pdf-detect-aruco",
        POST_SETUP_EVENT_TIMEOUT,
        Some(&test_root),
    );

    assert!(
        !events
            .snapshot()
            .iter()
            .any(|e| e.command_name == "exam.analyze"),
        "post-setup AI analysis follows the Exam Analysis category toggle"
    );
}

#[test]
fn smoke_ping_persists_job_history_and_emits_runtime_events() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-state-smoke");
    let template_pdf = test_root.join("fixtures").join("template.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&template_pdf);
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());

    let state = AppState::bootstrap();
    let setup_events = RecordingEventSink::default();
    let shell = state
        .create_project(create_project_input(&template_pdf), &setup_events)
        .expect("project should be created");
    let project_path = project_path_from_shell(&shell);

    let event_sink = RecordingEventSink::default();
    let result = state
        .run_smoke_ping(&event_sink)
        .expect("smoke ping should succeed");
    assert_eq!(result.command, "smoke.ping");
    assert!(result.event_count >= 1);

    let events = event_sink.snapshot();
    assert!(events
        .iter()
        .any(|event| event.event_type == "job_submitted" && event.command_name == "smoke.ping"));
    assert!(events
        .iter()
        .any(|event| event.event_type == "job_started" && event.command_name == "smoke.ping"));
    assert!(events
        .iter()
        .any(|event| event.event_type == "job_finished" && event.command_name == "smoke.ping"));

    let connection = open_project_db(&project_path);
    let (job_id, state_name, request_json): (String, String, String) = connection
        .query_row(
            "SELECT job_id, state, request_json FROM job_run WHERE command_name = 'smoke.ping' ORDER BY rowid DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("job run should exist");
    let event_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM job_event WHERE job_id = ?1",
            [job_id],
            |row| row.get(0),
        )
        .expect("job events should exist");
    assert_eq!(state_name, "succeeded");
    assert!(request_json.contains("desktop runtime ready"));
    assert!(event_count >= 1);

    state.close_current_project().expect("project should close");
    state
        .open_project(project_path.clone(), &app_settings())
        .expect("project should reopen");
    let reopened_connection = open_project_db(&project_path);
    let reopened_job_count: i64 = reopened_connection
        .query_row(
            "SELECT COUNT(*) FROM job_run WHERE command_name = 'smoke.ping'",
            [],
            |row| row.get(0),
        )
        .expect("reopened job count should load");
    assert!(reopened_job_count >= 1);
}

#[test]
fn question_edits_and_redactions_rehydrate_after_reopen() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-state-rehydrate");
    let template_pdf = test_root.join("fixtures").join("template.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&template_pdf);
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());

    let state = AppState::bootstrap();
    let setup_events = RecordingEventSink::default();
    let shell = state
        .create_project(create_project_input(&template_pdf), &setup_events)
        .expect("project should be created");
    let project_path = project_path_from_shell(&shell);
    let original_workspace = state.workspace_state().expect("workspace should load");
    let edited_workspace = state
        .save_question_edits(vec![question_edit(
            &original_workspace.questions[0].question_id,
            "1. Describe orbital shapes.",
            8,
        )])
        .expect("question edits should save");
    let redacted_workspace = state
        .save_redaction_regions(vec![sample_redaction()])
        .expect("redactions should save");

    assert_eq!(
        edited_workspace.questions[0].text,
        "1. Describe orbital shapes."
    );
    assert_eq!(edited_workspace.questions[0].max_points, Some(8));
    assert_eq!(redacted_workspace.redaction_regions.len(), 1);
    assert!(redacted_workspace.can_approve);

    state.close_current_project().expect("project should close");
    state
        .open_project(project_path, &app_settings())
        .expect("project should reopen");

    let reopened_workspace = state.workspace_state().expect("workspace should rehydrate");
    assert_eq!(reopened_workspace.status, "draft");
    assert_eq!(
        reopened_workspace.questions[0].text,
        "1. Describe orbital shapes."
    );
    assert_eq!(reopened_workspace.questions[0].max_points, Some(8));
    assert_eq!(reopened_workspace.redaction_regions.len(), 1);
    assert!(reopened_workspace.can_approve);
}

#[test]
fn approved_template_setup_rehydrates_after_close_and_reopen() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-state-approved");
    let template_pdf = test_root.join("fixtures").join("template.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&template_pdf);
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());

    let state = AppState::bootstrap();
    let setup_events = RecordingEventSink::default();
    let shell = state
        .create_project(create_project_input(&template_pdf), &setup_events)
        .expect("project should be created");
    let project_path = project_path_from_shell(&shell);

    state
        .save_redaction_regions(vec![sample_redaction()])
        .expect("redactions should save");
    let approved = state
        .approve_template_setup(&app_settings())
        .expect("template setup should approve");

    assert_eq!(approved.status, "approved");
    assert_eq!(approved.failure_message, None);
    assert!(!approved.can_approve);
    assert!(approved.can_approve_rubric);

    state.close_current_project().expect("project should close");
    state
        .open_project(project_path, &app_settings())
        .expect("project should reopen");

    let reopened = state.workspace_state().expect("workspace should rehydrate");
    assert_eq!(reopened.status, "approved");
    assert_eq!(reopened.failure_message, None);
    assert!(!reopened.can_approve);
    assert!(reopened.can_approve_rubric);
}

#[test]
fn skipped_redaction_rehydrates_and_allows_approval_without_regions() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-state-skip");
    let template_pdf = test_root.join("fixtures").join("template.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&template_pdf);
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());
    let setup_events = RecordingEventSink::default();

    let state = AppState::bootstrap();
    let shell = state
        .create_project(create_project_input(&template_pdf), &setup_events)
        .expect("project should be created");
    let project_path = project_path_from_shell(&shell);

    let skipped = state
        .skip_template_redaction()
        .expect("redaction skip should persist");
    assert_eq!(skipped.status, "draft");
    assert_eq!(skipped.status_label, "Redaction skipped, review questions");
    assert!(skipped.redaction_regions.is_empty());
    assert!(skipped.can_approve);
    assert!(skipped
        .warnings
        .iter()
        .any(|warning| warning.code.as_deref() == Some("redaction_skipped")));

    state.close_current_project().expect("project should close");
    state
        .open_project(project_path, &app_settings())
        .expect("project should reopen");

    let reopened = state.workspace_state().expect("workspace should rehydrate");
    assert_eq!(reopened.status_label, "Redaction skipped, review questions");
    assert!(reopened.can_approve);
    assert!(reopened
        .warnings
        .iter()
        .any(|warning| warning.code.as_deref() == Some("redaction_skipped")));

    let approved = state
        .approve_template_setup(&app_settings())
        .expect("skip acknowledgement should allow approval");
    assert_eq!(approved.status, "approved");
}

#[test]
fn saving_redactions_clears_skip_acknowledgement() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-state-skip-clear");
    let template_pdf = test_root.join("fixtures").join("template.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&template_pdf);
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());
    let setup_events = RecordingEventSink::default();

    let state = AppState::bootstrap();
    let shell = state
        .create_project(create_project_input(&template_pdf), &setup_events)
        .expect("project should be created");
    let project_path = project_path_from_shell(&shell);

    state
        .skip_template_redaction()
        .expect("redaction skip should persist");
    let redacted = state
        .save_redaction_regions(vec![sample_redaction()])
        .expect("redaction save should succeed");

    assert_eq!(redacted.status_label, "Redaction review needed");
    assert!(redacted.can_approve);
    assert!(redacted
        .warnings
        .iter()
        .all(|warning| warning.code.as_deref() != Some("redaction_skipped")));

    state.close_current_project().expect("project should close");
    state
        .open_project(project_path, &app_settings())
        .expect("project should reopen");

    let reopened = state.workspace_state().expect("workspace should rehydrate");
    assert_eq!(reopened.status_label, "Redaction review needed");
    assert!(reopened
        .warnings
        .iter()
        .all(|warning| warning.code.as_deref() != Some("redaction_skipped")));
}

#[test]
fn replace_template_pdf_clears_previous_setup_state_and_uses_project_relative_artifacts() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-state-replace");
    let original_template_pdf = test_root.join("fixtures").join("template-a.pdf");
    let replacement_template_pdf = test_root.join("fixtures").join("template-b.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&original_template_pdf);
    create_template_pdf_with_question(
        &replacement_template_pdf,
        "1. Describe Newton's first law.",
        9,
    );
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());
    let event_sink = RecordingEventSink::default();

    let state = AppState::bootstrap();
    let shell = state
        .create_project(create_project_input(&original_template_pdf), &event_sink)
        .expect("project should be created");
    let project_path = project_path_from_shell(&shell);
    let original_workspace = state.workspace_state().expect("workspace should load");
    state
        .save_question_edits(vec![question_edit(
            &original_workspace.questions[0].question_id,
            "1. Old edited template question.",
            7,
        )])
        .expect("question edits should save");
    state
        .save_redaction_regions(vec![sample_redaction()])
        .expect("redactions should save");

    let replaced = state
        .replace_template_pdf(
            replacement_template_pdf.to_string_lossy().into_owned(),
            &event_sink,
        )
        .expect("replacement template should succeed");

    assert_eq!(replaced.status, "draft");
    assert_eq!(replaced.failure_message, None);
    assert_eq!(replaced.questions.len(), 1);
    assert!(replaced.questions[0]
        .text
        .contains("1. Describe Newton's first law."));
    assert_eq!(replaced.questions[0].max_points, Some(9));
    assert!(replaced.redaction_regions.is_empty());

    for relative_path in artifact_relative_paths(&project_path) {
        assert!(!std::path::Path::new(&relative_path).is_absolute());
        assert!(project_path.join(&relative_path).exists());
    }

    let request_json = latest_job_request_json(&project_path, "exam.setup");
    assert!(request_json.contains("template_source_name"));
    assert!(request_json.contains("template_artifact_id"));
    assert!(!request_json.contains(&replacement_template_pdf.to_string_lossy().to_string()));
}

#[test]
fn exporting_template_stamps_once_and_preserves_setup_state() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-state-aruco-export");
    let template_pdf = test_root.join("fixtures").join("template.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&template_pdf);
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());
    let event_sink = RecordingEventSink::default();

    let state = AppState::bootstrap();
    state
        .create_project(create_project_input(&template_pdf), &event_sink)
        .expect("project should be created");
    let before = state.workspace_state().expect("workspace should load");
    assert_eq!(before.aruco_status.state, "unknown");
    assert_eq!(before.aruco_status.total_marker_count, 0);
    let original_preview_path = before.template_preview_artifacts[0].image_path.clone();

    let exported_path = test_root.join("exports").join("stamped-template.pdf");
    let stamped = state
        .export_stamped_template_pdf(exported_path.to_string_lossy().into_owned(), &event_sink)
        .expect("template export should stamp and copy");

    assert!(exported_path.is_file());
    assert_eq!(stamped.status, "draft");
    assert_eq!(stamped.questions.len(), before.questions.len());
    assert_eq!(
        stamped.redaction_regions.len(),
        before.redaction_regions.len()
    );
    assert_eq!(stamped.aruco_status.state, "detected");
    assert_eq!(stamped.aruco_status.total_marker_count, 4);
    assert_ne!(
        stamped.template_preview_artifacts[0].image_path,
        original_preview_path
    );
    assert!(event_sink.snapshot().iter().any(|event| {
        event.command_name == "scans.pdf-stamp-aruco" && event.event_type == "job_finished"
    }));

    let stamp_event_count = event_sink
        .snapshot()
        .iter()
        .filter(|event| event.command_name == "scans.pdf-stamp-aruco")
        .count();
    let second_exported_path = test_root
        .join("exports")
        .join("already-stamped-template.pdf");
    let exported_again = state
        .export_stamped_template_pdf(
            second_exported_path.to_string_lossy().into_owned(),
            &event_sink,
        )
        .expect("already-stamped template export should copy only");

    assert!(second_exported_path.is_file());
    assert_eq!(exported_again.aruco_status.total_marker_count, 4);
    assert_eq!(
        event_sink
            .snapshot()
            .iter()
            .filter(|event| event.command_name == "scans.pdf-stamp-aruco")
            .count(),
        stamp_event_count
    );
}

#[test]
fn export_stamped_template_job_returns_while_active_runtime_job_finishes_before_stamping() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-state-aruco-export-waits");
    let template_pdf = test_root.join("fixtures").join("template.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&template_pdf);
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());

    let state = Arc::new(AppState::bootstrap());
    let event_sink = RecordingEventSink::default();
    state
        .create_project(create_project_input(&template_pdf), &event_sink)
        .expect("project should be created");

    let slow_state = Arc::clone(&state);
    let slow_events = event_sink.clone();
    let slow_handle = thread::spawn(move || {
        slow_state.run_smoke_ping_job(
            &slow_events,
            json!({"message": "slow", "steps": 10, "sleep_ms": 3_000}),
        )
    });
    event_sink.wait_for("job_started", "smoke.ping");

    let exported_path = test_root
        .join("exports")
        .join("queued-stamped-template.pdf");
    let export_job_id = state
        .start_export_stamped_template_pdf_job(
            exported_path.to_string_lossy().into_owned(),
            Arc::new(event_sink.clone()),
        )
        .expect("template export job should be accepted while runtime is busy");
    assert!(
        !event_sink.snapshot().iter().any(|event| {
            event.command_name == "smoke.ping" && event.event_type == "job_finished"
        }),
        "template export command should return before the active runtime job finishes"
    );
    assert!(
        !export_job_id.is_empty(),
        "background export job id should be returned"
    );
    thread::sleep(Duration::from_millis(150));
    assert!(
        !event_sink.snapshot().iter().any(|event| {
            event.command_name == "scans.pdf-stamp-aruco" && event.event_type == "job_submitted"
        }),
        "stamp job should not start while another runtime job is active"
    );

    slow_handle
        .join()
        .expect("slow job thread should join")
        .expect("slow job should complete");
    let export_event =
        event_sink.wait_for_terminal("export_stamped_template_pdf", Duration::from_secs(45));
    assert_eq!(
        export_event.event_type, "job_finished",
        "background export job should finish successfully: {}",
        export_event.payload
    );
    assert!(exported_path.is_file());
    let exported = state.workspace_state().expect("workspace should load");
    assert_eq!(exported.aruco_status.state, "detected");
    assert_eq!(exported.aruco_status.total_marker_count, 4);

    let events = event_sink.snapshot();
    let smoke_finished_index = events
        .iter()
        .position(|event| event.command_name == "smoke.ping" && event.event_type == "job_finished")
        .expect("slow job should finish");
    let stamp_submitted_index = events
        .iter()
        .position(|event| {
            event.command_name == "scans.pdf-stamp-aruco" && event.event_type == "job_submitted"
        })
        .expect("stamp job should start after the worker is idle");
    assert!(
        smoke_finished_index < stamp_submitted_index,
        "stamp job should wait until the active runtime job completes"
    );
}

#[test]
fn failed_template_setup_rehydrates_after_close_and_reopen() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-state-failure");
    let original_template_pdf = test_root.join("fixtures").join("template-a.pdf");
    let replacement_template_pdf = test_root.join("fixtures").join("template-b.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&original_template_pdf);
    create_template_pdf_with_question(
        &replacement_template_pdf,
        "1. Explain conservation of momentum.",
        6,
    );
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());
    let event_sink = RecordingEventSink::default();

    let state = AppState::bootstrap();
    let shell = state
        .create_project(create_project_input(&original_template_pdf), &event_sink)
        .expect("project should be created");
    let project_path = project_path_from_shell(&shell);
    state.__test_reset_worker_state();

    let _bad_python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", "/nonexistent/python3");
    let failed = state
        .replace_template_pdf(
            replacement_template_pdf.to_string_lossy().into_owned(),
            &event_sink,
        )
        .expect("failed template setup should still return workspace state");

    assert_eq!(failed.status, "failed");
    assert!(failed.failure_message.is_some());
    assert!(failed.questions.is_empty());
    assert!(failed.redaction_regions.is_empty());

    state.close_current_project().expect("project should close");
    state
        .open_project(project_path, &app_settings())
        .expect("project should reopen");

    let reopened = state.workspace_state().expect("workspace should rehydrate");
    assert_eq!(reopened.status, "failed");
    assert_eq!(reopened.failure_message, failed.failure_message);
    assert!(reopened.questions.is_empty());
    assert!(reopened.redaction_regions.is_empty());
}

#[test]
fn persisted_exam_setup_warnings_rehydrate_after_reopen() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-state-warnings");
    let template_pdf = test_root.join("fixtures").join("template.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&template_pdf);
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());
    let setup_events = RecordingEventSink::default();

    let state = AppState::bootstrap();
    let shell = state
        .create_project(create_project_input(&template_pdf), &setup_events)
        .expect("project should be created");
    let project_path = project_path_from_shell(&shell);

    update_template_setup_payload(&project_path, |payload| {
        payload["warnings"] = json!([
            {
                "code": "question_detection_partial",
                "message": "One question label needed manual review.",
                "scope": "{\"page_number\":1}"
            }
        ]);
    });

    let workspace = state.workspace_state().expect("workspace should load");
    assert_eq!(workspace.warnings.len(), 1);
    assert_eq!(
        workspace.warnings[0].message,
        "One question label needed manual review."
    );

    state.close_current_project().expect("project should close");
    state
        .open_project(project_path, &app_settings())
        .expect("project should reopen");

    let reopened = state.workspace_state().expect("workspace should rehydrate");
    assert_eq!(reopened.warnings.len(), 1);
    assert_eq!(
        reopened.warnings[0].code.as_deref(),
        Some("question_detection_partial")
    );
}

#[test]
fn replacing_template_clears_skip_acknowledgement_and_persisted_warnings() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-state-replace-skip");
    let original_template_pdf = test_root.join("fixtures").join("template-a.pdf");
    let replacement_template_pdf = test_root.join("fixtures").join("template-b.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&original_template_pdf);
    create_template_pdf_with_question(
        &replacement_template_pdf,
        "1. Describe Newton's third law.",
        6,
    );
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());
    let event_sink = RecordingEventSink::default();

    let state = AppState::bootstrap();
    let shell = state
        .create_project(create_project_input(&original_template_pdf), &event_sink)
        .expect("project should be created");
    let project_path = project_path_from_shell(&shell);

    state
        .skip_template_redaction()
        .expect("redaction skip should persist");
    update_template_setup_payload(&project_path, |payload| {
        payload["warnings"] = json!([
            {
                "code": "template_layout_notice",
                "message": "Header text was not matched to a question.",
                "scope": null
            }
        ]);
    });

    let replaced = state
        .replace_template_pdf(
            replacement_template_pdf.to_string_lossy().into_owned(),
            &event_sink,
        )
        .expect("replacement template should succeed");

    assert_eq!(replaced.status, "draft");
    assert_eq!(replaced.status_label, "Redaction review needed");
    assert!(replaced.redaction_regions.is_empty());
    assert!(!replaced.can_approve);
    assert!(replaced.warnings.is_empty());
}

#[test]
fn shared_runtime_enforces_single_flight_and_supports_cancellation() {
    let _guard = lock_env_vars();
    let test_root = temp_root("scriptscore-desktop-state-cancel");
    let template_pdf = test_root.join("fixtures").join("template.pdf");
    std::fs::create_dir_all(&test_root).expect("test root should exist");
    create_template_pdf(&template_pdf);
    let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", test_python());

    let state = Arc::new(AppState::bootstrap());
    let setup_events = RecordingEventSink::default();
    state
        .create_project(create_project_input(&template_pdf), &setup_events)
        .expect("project should be created");

    let slow_events = RecordingEventSink::default();
    let thread_state = Arc::clone(&state);
    let thread_events = slow_events.clone();
    let handle = thread::spawn(move || {
        thread_state.run_smoke_ping_job(
            &thread_events,
            json!({"message": "slow", "steps": 3, "sleep_ms": 1500}),
        )
    });

    slow_events.wait_for("job_started", "smoke.ping");
    let conflict = state
        .run_smoke_ping(&slow_events)
        .expect_err("second job should conflict");
    assert!(matches!(conflict, HostError::Conflict(_)));

    state
        .cancel_active_job(None)
        .expect("active job should accept cancellation");

    let completed = handle
        .join()
        .expect("worker thread should join")
        .expect("cancelled job should still return a terminal message");
    assert_eq!(completed.result.terminal_type, "job_cancelled");
    assert!(slow_events
        .snapshot()
        .iter()
        .any(|event| event.event_type == "job_cancelled"));
    assert!(matches!(
        state.shell_state().expect("shell").worker_status,
        WorkerStatus::Ready
    ));
}

#[test]
fn runtime_errors_are_emitted_when_the_worker_cannot_be_reached() {
    let _guard = lock_env_vars();
    let missing_python =
        temp_root("scriptscore-desktop-missing-python").join("python-does-not-exist");
    let _python = EnvVarGuard::set("SCRIPTSCORE_PYTHON", &missing_python);
    let event_sink = RecordingEventSink::default();

    let state = AppState::bootstrap();
    let error = state
        .run_smoke_ping(&event_sink)
        .expect_err("runtime check should fail");

    assert!(matches!(error, HostError::Io(_) | HostError::Worker(_)));
    assert!(event_sink.snapshot().iter().any(|event| {
        event.event_type == "runtime_error" && event.command_name == "smoke.ping"
    }));
    assert!(matches!(
        state.shell_state().expect("shell").worker_status,
        WorkerStatus::Error
    ));
}
