// SPDX-License-Identifier: AGPL-3.0-only
mod edits;
mod host_workflows;
mod jobs;
mod project_config;
mod projects;
pub(crate) mod schema;
mod student_roster;
mod template_setup;
mod trace_index;
mod workflow_state;
mod workspace;

pub use edits::{
    approve_template_setup, save_question_edits, save_redaction_regions, skip_template_redaction,
};
pub(crate) use host_workflows::{
    complete_host_workflow, insert_host_workflow, insert_host_workflow_child_job,
    mark_abandoned_host_workflows,
};
pub(crate) use jobs::mark_abandoned_runtime_jobs;
pub use jobs::{append_job_event, complete_job, insert_job_run, mark_job_running};
pub use project_config::{
    load_project_config, resolve_canvas_course_id_for_binding, save_project_config,
    update_batch_analyze_trace_ref, update_setup_trace_ref,
};
pub use projects::{create_project, create_project_in_root, default_projects_root, open_project};
pub(crate) use schema::command_output_dir;
pub use student_roster::{
    load_student_ref_for_binding_token_hex, load_student_roster, sync_student_roster_tokens,
};
pub(crate) use template_setup::{
    load_canonical_template_pdf_path, persist_template_setup_failure,
    persist_template_setup_success, prepare_template_setup, replace_template_pdf_and_pages,
    update_template_aruco_status,
};
pub use workflow_state::{
    append_generated_rubric, canonical_intake_pdf_path, list_job_traces, load_job_trace,
    load_latest_job_trace_for_command, load_moderation_state, load_results_lms_state,
    load_rubric_state, load_student_intake_state, load_student_workflow_state,
    mark_student_answers_stale_for_questions, persist_question_analysis_state,
    save_moderation_state, save_results_lms_state, save_rubric_state, save_student_intake_state,
    save_student_workflow_state, save_student_workflow_submissions,
};
pub use workspace::load_exam_workspace_state;
pub(crate) use workspace::load_template_page_artifacts;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use rusqlite::Connection;

    use crate::models::{CreateProjectInput, InstructorProfile};
    use crate::test_support::{lock_env_vars, EnvVarGuard};

    use super::schema::{project_db_path, SCHEMA_VERSION};
    use super::{create_project, load_exam_workspace_state, open_project};

    fn test_create_input() -> CreateProjectInput {
        CreateProjectInput {
            display_name: "Midterm 1".into(),
            subject: Some("Physics".into()),
            course_code: Some("PHYS 221".into()),
            lms_course_id: None,
            project_root: None,
            template_pdf_path: "/tmp/template.pdf".into(),
            instructor_profile: None,
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

    #[test]
    fn create_and_open_project_round_trip() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-project-store-test");
        let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
        let input = test_create_input();

        let created = create_project(
            &input.display_name,
            input.subject.clone(),
            input.course_code.clone(),
            None,
            &InstructorProfile::default(),
        )
        .expect("project should be created");
        let reopened = open_project(std::path::Path::new(&created.project_path))
            .expect("project should reopen");
        let workspace = load_exam_workspace_state(std::path::Path::new(&created.project_path))
            .expect("workspace should load");

        assert_eq!(created.project_id, reopened.project_id);
        assert_eq!(created.display_name, "Midterm 1");
        assert_eq!(reopened.course_code.as_deref(), Some("PHYS 221"));
        assert_eq!(workspace.status, "not_started");

        std::fs::remove_dir_all(&test_root).expect("test project should clean up");
    }

    #[test]
    fn create_project_bootstraps_current_schema() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-project-store-schema");
        let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
        let input = test_create_input();

        let created = create_project(
            &input.display_name,
            input.subject.clone(),
            input.course_code.clone(),
            None,
            &InstructorProfile::default(),
        )
        .expect("project should be created");
        let connection =
            Connection::open(project_db_path(std::path::Path::new(&created.project_path)))
                .expect("project db should open");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("schema version should load");
        let mut statement = connection
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name ASC",
            )
            .expect("table query should prepare");
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .expect("table rows should load");
        let tables = rows
            .map(|row| row.expect("table name"))
            .collect::<Vec<String>>();

        assert_eq!(version, SCHEMA_VERSION);
        assert_eq!(
            tables,
            vec![
                "artifact",
                "host_workflow_child_job",
                "host_workflow_job",
                "job_event",
                "job_run",
                "job_trace_student_ref",
                "project",
                "question",
                "student_roster",
                "template_redaction_region",
                "workflow_state",
            ]
        );
        let mut col_stmt = connection
            .prepare("PRAGMA table_info(student_roster)")
            .expect("pragma should prepare");
        let cols: Vec<String> = col_stmt
            .query_map([], |row| row.get::<_, String>(1))
            .expect("columns")
            .map(|r| r.expect("column name"))
            .collect();
        assert_eq!(
            cols,
            vec![
                "student_ref",
                "binding_token_hex",
                "created_at",
                "updated_at"
            ],
            "student roster must remain pseudonymous and must not store LMS names or raw LMS ids"
        );
    }

    #[test]
    fn open_project_rejects_unsupported_schema_version() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-project-store-version");
        let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
        let input = test_create_input();

        let created = create_project(
            &input.display_name,
            input.subject.clone(),
            input.course_code.clone(),
            None,
            &InstructorProfile::default(),
        )
        .expect("project should be created");
        let project_path = PathBuf::from(&created.project_path);
        let connection = Connection::open(project_db_path(&project_path)).expect("db should open");
        connection
            .execute_batch("PRAGMA user_version = 999;")
            .expect("schema version should update");

        let error = open_project(&project_path).expect_err("schema mismatch should fail");
        let message = error.to_string();
        assert!(message.contains("Unsupported"));
        assert!(message.contains("999"));
    }
}
