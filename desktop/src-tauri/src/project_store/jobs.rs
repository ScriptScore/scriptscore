// SPDX-License-Identifier: AGPL-3.0-only
use std::path::Path;

use rusqlite::{params, Connection};
use serde_json::json;

use crate::errors::HostResult;
use crate::models::{JobCompletion, JobProgressRecord, JobRunRecord};

use super::schema::{initialize_schema, project_db_path};
use super::trace_index::upsert_job_student_refs;

pub fn append_job_event(
    project_path: &Path,
    job_id: &str,
    record: &JobProgressRecord,
) -> HostResult<()> {
    let connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    connection.execute(
        "INSERT INTO job_event (
            job_id,
            sequence,
            event_type,
            progress_json,
            scope_json,
            data_json,
            created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            job_id,
            record.sequence,
            record.event_type,
            record.progress_json,
            record.scope_json,
            record.data_json,
            record.created_at,
        ],
    )?;
    let mut values =
        vec![serde_json::from_str(&record.data_json).unwrap_or(serde_json::Value::Null)];
    if let Some(progress_json) = record.progress_json.as_deref() {
        values.push(serde_json::from_str(progress_json).unwrap_or(serde_json::Value::Null));
    }
    if let Some(scope_json) = record.scope_json.as_deref() {
        values.push(serde_json::from_str(scope_json).unwrap_or(serde_json::Value::Null));
    }
    upsert_job_student_refs(&connection, job_id, &values)?;
    Ok(())
}

pub fn complete_job(
    project_path: &Path,
    job_id: &str,
    completion: &JobCompletion,
) -> HostResult<()> {
    let connection = Connection::open(project_db_path(project_path))?;
    connection.execute(
        "UPDATE job_run
         SET state = ?2,
             finished_at = ?3,
             result_json = ?4,
             error_json = ?5
         WHERE job_id = ?1",
        params![
            job_id,
            completion.state,
            completion.finished_at,
            completion.result_json,
            completion.error_json,
        ],
    )?;
    Ok(())
}

pub fn insert_job_run(project_path: &Path, record: &JobRunRecord) -> HostResult<()> {
    let connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    connection.execute(
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
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            record.job_id,
            record.command_name,
            record.request_id,
            record.state,
            record.submitted_at,
            record.started_at,
            record.finished_at,
            record.request_json,
            record.result_json,
            record.error_json,
        ],
    )?;
    let request = serde_json::from_str(&record.request_json).unwrap_or(serde_json::Value::Null);
    upsert_job_student_refs(&connection, &record.job_id, &[request])?;
    Ok(())
}

pub fn mark_job_running(project_path: &Path, job_id: &str, started_at: &str) -> HostResult<()> {
    let connection = Connection::open(project_db_path(project_path))?;
    connection.execute(
        "UPDATE job_run
         SET state = 'running',
             started_at = ?2
         WHERE job_id = ?1",
        params![job_id, started_at],
    )?;
    Ok(())
}

pub(crate) fn mark_abandoned_runtime_jobs(
    project_path: &Path,
    finished_at: &str,
    message: &str,
) -> HostResult<usize> {
    let connection = Connection::open(project_db_path(project_path))?;
    let error_json = json!({
        "message": message,
        "recovered": true,
        "reason": "desktop_session_ended"
    })
    .to_string();
    let changed = connection.execute(
        "UPDATE job_run
         SET state = 'failed',
             finished_at = ?1,
             error_json = ?2
         WHERE state IN ('submitted', 'running')",
        params![finished_at, error_json],
    )?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use rusqlite::Connection;

    use crate::models::{
        CreateProjectInput, InstructorProfile, JobCompletion, JobProgressRecord, JobRunRecord,
    };
    use crate::project_store::create_project;
    use crate::test_support::{lock_env_vars, EnvVarGuard};

    use super::{
        append_job_event, complete_job, insert_job_run, mark_abandoned_runtime_jobs,
        mark_job_running, project_db_path,
    };

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
    fn job_store_persists_run_lifecycle_and_progress_order() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-project-store-jobs");
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
        insert_job_run(
            &project_path,
            &JobRunRecord {
                job_id: "job_1".into(),
                command_name: "smoke.ping".into(),
                request_id: "req_1".into(),
                state: "submitted".into(),
                submitted_at: "1".into(),
                started_at: None,
                finished_at: None,
                request_json: "{\"message\":\"hello\"}".into(),
                result_json: None,
                error_json: None,
            },
        )
        .expect("job run should insert");
        mark_job_running(&project_path, "job_1", "2").expect("job should mark running");
        append_job_event(
            &project_path,
            "job_1",
            &JobProgressRecord {
                sequence: 1,
                event_type: "job_started".into(),
                progress_json: Some("{\"completed\":0,\"total\":1}".into()),
                scope_json: None,
                data_json: "{\"stage\":\"start\"}".into(),
                created_at: "2".into(),
            },
        )
        .expect("first event should append");
        append_job_event(
            &project_path,
            "job_1",
            &JobProgressRecord {
                sequence: 2,
                event_type: "job_finished".into(),
                progress_json: Some("{\"completed\":1,\"total\":1}".into()),
                scope_json: None,
                data_json: "{\"stage\":\"done\"}".into(),
                created_at: "3".into(),
            },
        )
        .expect("second event should append");
        complete_job(
            &project_path,
            "job_1",
            &JobCompletion {
                state: "succeeded".into(),
                finished_at: "3".into(),
                result_json: Some("{\"ok\":true}".into()),
                error_json: None,
            },
        )
        .expect("job should complete");

        let connection =
            Connection::open(project_db_path(&project_path)).expect("project db should open");
        let (state, started_at, finished_at, result_json): (
            String,
            Option<String>,
            Option<String>,
            Option<String>,
        ) = connection
            .query_row(
                "SELECT state, started_at, finished_at, result_json FROM job_run WHERE job_id = 'job_1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("job run should load");
        let sequences = {
            let mut statement = connection
                .prepare(
                    "SELECT sequence FROM job_event WHERE job_id = 'job_1' ORDER BY sequence ASC",
                )
                .expect("event query should prepare");
            let rows = statement
                .query_map([], |row| row.get::<_, i64>(0))
                .expect("event rows should load");
            rows.collect::<Result<Vec<i64>, _>>()
                .expect("event sequences should collect")
        };

        assert_eq!(state, "succeeded");
        assert_eq!(started_at.as_deref(), Some("2"));
        assert_eq!(finished_at.as_deref(), Some("3"));
        assert_eq!(result_json.as_deref(), Some("{\"ok\":true}"));
        assert_eq!(sequences, vec![1, 2]);
    }

    #[test]
    fn mark_abandoned_runtime_jobs_fails_only_non_terminal_rows() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-project-store-abandoned-jobs");
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
        for (job_id, state) in [
            ("submitted_job", "submitted"),
            ("running_job", "running"),
            ("succeeded_job", "succeeded"),
        ] {
            insert_job_run(
                &project_path,
                &JobRunRecord {
                    job_id: job_id.into(),
                    command_name: "smoke.ping".into(),
                    request_id: format!("{job_id}_request"),
                    state: state.into(),
                    submitted_at: "1".into(),
                    started_at: if state == "running" {
                        Some("2".into())
                    } else {
                        None
                    },
                    finished_at: if state == "succeeded" {
                        Some("3".into())
                    } else {
                        None
                    },
                    request_json: "{}".into(),
                    result_json: None,
                    error_json: None,
                },
            )
            .expect("job run should insert");
        }

        let changed =
            mark_abandoned_runtime_jobs(&project_path, "4", "Recovered stale desktop job.")
                .expect("abandoned rows should update");

        let connection =
            Connection::open(project_db_path(&project_path)).expect("project db should open");
        let rows = {
            let mut statement = connection
                .prepare(
                    "SELECT job_id, state, finished_at, error_json FROM job_run ORDER BY job_id",
                )
                .expect("job query should prepare");
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                })
                .expect("job rows should load");
            rows.collect::<Result<Vec<_>, _>>()
                .expect("job rows should collect")
        };

        assert_eq!(changed, 2);
        assert_eq!(rows[0].0, "running_job");
        assert_eq!(rows[0].1, "failed");
        assert_eq!(rows[0].2.as_deref(), Some("4"));
        assert!(rows[0]
            .3
            .as_deref()
            .unwrap_or_default()
            .contains("desktop_session_ended"));
        assert_eq!(rows[1].0, "submitted_job");
        assert_eq!(rows[1].1, "failed");
        assert_eq!(rows[2].0, "succeeded_job");
        assert_eq!(rows[2].1, "succeeded");
    }
}
