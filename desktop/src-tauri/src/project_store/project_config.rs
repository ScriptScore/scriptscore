// SPDX-License-Identifier: AGPL-3.0-only
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::errors::{HostError, HostResult};
use crate::models::{InstructorProfile, ProjectConfig, ProjectTraceRefs};

use super::projects::touch_project;
use super::schema::{initialize_schema, normalize_optional, project_db_path};

pub fn load_project_config(connection: &Connection) -> HostResult<ProjectConfig> {
    connection
        .query_row(
            "SELECT project_id, display_name, subject, course_code, lms_course_id, lms_assignment_id, redaction_required,
                    instructor_profile_json, trace_refs_json, created_at, updated_at
             FROM project
             LIMIT 1",
            [],
            |row| {
                Ok(ProjectConfig {
                    project_id: row.get(0)?,
                    display_name: row.get(1)?,
                    subject: row.get(2)?,
                    course_code: row.get(3)?,
                    lms_course_id: row.get(4)?,
                    lms_assignment_id: row.get(5)?,
                    redaction_required: row.get(6)?,
                    instructor_profile: serde_json::from_str::<InstructorProfile>(
                        &row.get::<_, String>(7)?,
                    )
                    .unwrap_or_default(),
                    trace_refs: serde_json::from_str::<ProjectTraceRefs>(&row.get::<_, String>(8)?)
                        .unwrap_or_default(),
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| {
            HostError::Project("The selected project is missing project configuration.".into())
        })
}

pub fn initialize_project_config(
    connection: &Connection,
    project_id: &str,
    display_name: &str,
    subject: Option<String>,
    course_code: Option<String>,
    lms_course_id: Option<String>,
    instructor_profile: &InstructorProfile,
) -> HostResult<()> {
    let instructor_profile_json = serde_json::to_string(instructor_profile)?;
    let trace_refs_json = serde_json::to_string(&ProjectTraceRefs::default())?;
    connection.execute(
        "INSERT INTO project (
            project_id,
            display_name,
            subject,
            course_code,
            lms_course_id,
            lms_assignment_id,
            redaction_required,
            instructor_profile_json,
            trace_refs_json,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 1, ?6, ?7, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        params![
            project_id,
            display_name,
            normalize_optional(subject),
            normalize_optional(course_code),
            normalize_optional(lms_course_id),
            instructor_profile_json,
            trace_refs_json,
        ],
    )?;
    Ok(())
}

/// Prefer `project.lms_course_id` from `scriptscore.db` so binding tokens stay stable when the UI
/// draft course id differs from the saved project.
pub fn resolve_canvas_course_id_for_binding(
    project_path: &Path,
    course_id_from_request: &str,
) -> HostResult<String> {
    let connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let cfg = load_project_config(&connection)?;
    if let Some(ref id) = cfg.lms_course_id {
        let t = id.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    let t = course_id_from_request.trim();
    if t.is_empty() {
        return Err(HostError::Validation(
            "Canvas course id is required for LMS binding.".into(),
        ));
    }
    Ok(t.to_string())
}

pub fn save_project_config(
    transaction: &Transaction<'_>,
    config: &ProjectConfig,
) -> HostResult<()> {
    let display_name = config.display_name.trim();
    if display_name.is_empty() {
        return Err(HostError::Validation("display_name is required.".into()));
    }
    let instructor_profile_json = serde_json::to_string(&config.instructor_profile)?;
    let trace_refs_json = serde_json::to_string(&config.trace_refs)?;
    transaction.execute(
        "UPDATE project
         SET display_name = ?1,
             subject = ?2,
             course_code = ?3,
             lms_course_id = ?4,
             lms_assignment_id = ?5,
             redaction_required = ?6,
             instructor_profile_json = ?7,
             trace_refs_json = ?8,
             updated_at = CURRENT_TIMESTAMP
         WHERE project_id = ?9",
        params![
            display_name,
            normalize_optional(config.subject.clone()),
            normalize_optional(config.course_code.clone()),
            normalize_optional(config.lms_course_id.clone()),
            normalize_optional(config.lms_assignment_id.clone()),
            config.redaction_required,
            instructor_profile_json,
            trace_refs_json,
            config.project_id,
        ],
    )?;
    touch_project(transaction)?;
    Ok(())
}

pub fn update_setup_trace_ref(
    transaction: &Transaction<'_>,
    project_id: &str,
    job_id: &str,
) -> HostResult<()> {
    let mut trace_refs = load_project_config(transaction)?.trace_refs;
    trace_refs.setup_job_id = Some(job_id.to_string());
    let trace_refs_json = serde_json::to_string(&trace_refs)?;
    transaction.execute(
        "UPDATE project SET trace_refs_json = ?1, updated_at = CURRENT_TIMESTAMP WHERE project_id = ?2",
        params![trace_refs_json, project_id],
    )?;
    Ok(())
}

pub fn update_batch_analyze_trace_ref(
    transaction: &Transaction<'_>,
    project_id: &str,
    job_id: &str,
) -> HostResult<()> {
    let mut trace_refs = load_project_config(transaction)?.trace_refs;
    trace_refs.batch_analyze_job_id = Some(job_id.to_string());
    let trace_refs_json = serde_json::to_string(&trace_refs)?;
    transaction.execute(
        "UPDATE project SET trace_refs_json = ?1, updated_at = CURRENT_TIMESTAMP WHERE project_id = ?2",
        params![trace_refs_json, project_id],
    )?;
    Ok(())
}
