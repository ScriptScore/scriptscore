// SPDX-License-Identifier: AGPL-3.0-only
use std::path::Path;

use rusqlite::{params, Connection};
use serde_json::json;

use crate::errors::HostResult;

use super::schema::project_db_path;

pub(crate) fn insert_host_workflow(
    project_path: &Path,
    workflow_id: &str,
    command_name: &str,
    result_kind: &str,
    submitted_at: &str,
) -> HostResult<()> {
    let connection = Connection::open(project_db_path(project_path))?;
    connection.execute(
        "INSERT OR IGNORE INTO host_workflow_job (
            workflow_id,
            command_name,
            result_kind,
            state,
            submitted_at,
            started_at
        ) VALUES (?1, ?2, ?3, 'running', ?4, ?4)",
        params![workflow_id, command_name, result_kind, submitted_at],
    )?;
    Ok(())
}

pub(crate) fn complete_host_workflow(
    project_path: &Path,
    workflow_id: &str,
    state: &str,
    finished_at: &str,
    workspace_changed: bool,
    result_json: Option<&str>,
    error_json: Option<&str>,
) -> HostResult<()> {
    let connection = Connection::open(project_db_path(project_path))?;
    connection.execute(
        "UPDATE host_workflow_job
         SET state = ?2,
             finished_at = ?3,
             workspace_changed = ?4,
             result_json = ?5,
             error_json = ?6
         WHERE workflow_id = ?1",
        params![
            workflow_id,
            state,
            finished_at,
            if workspace_changed { 1 } else { 0 },
            result_json,
            error_json
        ],
    )?;
    Ok(())
}

pub(crate) fn insert_host_workflow_child_job(
    project_path: &Path,
    workflow_id: &str,
    child_job_id: &str,
    command_name: &str,
) -> HostResult<()> {
    let connection = Connection::open(project_db_path(project_path))?;
    connection.execute(
        "INSERT OR IGNORE INTO host_workflow_child_job (
            workflow_id,
            child_job_id,
            command_name
        ) VALUES (?1, ?2, ?3)",
        params![workflow_id, child_job_id, command_name],
    )?;
    Ok(())
}

pub(crate) fn mark_abandoned_host_workflows(
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
        "UPDATE host_workflow_job
         SET state = 'failed',
             finished_at = ?1,
             error_json = ?2
         WHERE state = 'running'",
        params![finished_at, error_json],
    )?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use rusqlite::Connection;

    use crate::models::InstructorProfile;
    use crate::project_store::{create_project, schema::project_db_path};
    use crate::test_support::{lock_env_vars, EnvVarGuard};

    use super::{insert_host_workflow, mark_abandoned_host_workflows};

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
    fn mark_abandoned_host_workflows_fails_running_rows() {
        let _guard = lock_env_vars();
        let test_root = temp_root("scriptscore-project-store-abandoned-host-workflows");
        let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
        let created = create_project(
            "Midterm 1",
            Some("Physics".into()),
            Some("PHYS 221".into()),
            None,
            &InstructorProfile::default(),
        )
        .expect("project should be created");
        let project_path = PathBuf::from(&created.project_path);
        insert_host_workflow(
            &project_path,
            "workflow_1",
            "begin_student_workflow",
            "workspace",
            "1",
        )
        .expect("workflow should insert");

        let changed =
            mark_abandoned_host_workflows(&project_path, "2", "Recovered stale host workflow.")
                .expect("abandoned workflow should update");

        let connection =
            Connection::open(project_db_path(&project_path)).expect("project db should open");
        let (state, finished_at, error_json): (String, Option<String>, Option<String>) = connection
            .query_row(
                "SELECT state, finished_at, error_json FROM host_workflow_job WHERE workflow_id = 'workflow_1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("workflow row should load");

        assert_eq!(changed, 1);
        assert_eq!(state, "failed");
        assert_eq!(finished_at.as_deref(), Some("2"));
        assert!(error_json
            .as_deref()
            .unwrap_or_default()
            .contains("desktop_session_ended"));
    }
}
