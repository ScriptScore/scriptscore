// SPDX-License-Identifier: AGPL-3.0-only
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Transaction};

use crate::errors::{HostError, HostResult};
use crate::models::{InstructorProfile, ProjectSummary};

use super::project_config::initialize_project_config;
use super::schema::{
    artifacts_root, initialize_schema, project_db_path, slugify, unique_suffix, PROJECT_DB_NAME,
    SCHEMA_VERSION,
};

pub fn default_projects_root() -> HostResult<PathBuf> {
    if let Ok(explicit_root) = std::env::var("SCRIPTSCORE_PROJECTS_ROOT") {
        return Ok(PathBuf::from(explicit_root));
    }
    Ok(default_projects_parent_dir()?.join("ScoreScript Projects"))
}

fn default_projects_parent_dir() -> HostResult<PathBuf> {
    let current_dir = std::env::current_dir().ok();
    select_default_projects_parent_dir(&[
        dirs::document_dir(),
        dirs::home_dir(),
        dirs::data_local_dir(),
        current_dir,
    ])
}

fn select_default_projects_parent_dir(candidates: &[Option<PathBuf>]) -> HostResult<PathBuf> {
    candidates
        .iter()
        .flatten()
        .next()
        .cloned()
        .ok_or_else(|| HostError::Project("Could not resolve a default project directory.".into()))
}

pub fn create_project(
    display_name: &str,
    subject: Option<String>,
    course_code: Option<String>,
    lms_course_id: Option<String>,
    instructor_profile: &InstructorProfile,
) -> HostResult<ProjectSummary> {
    create_project_in_root(
        default_projects_root()?,
        display_name,
        subject,
        course_code,
        lms_course_id,
        instructor_profile,
    )
}

pub fn create_project_in_root(
    root: PathBuf,
    display_name: &str,
    subject: Option<String>,
    course_code: Option<String>,
    lms_course_id: Option<String>,
    instructor_profile: &InstructorProfile,
) -> HostResult<ProjectSummary> {
    let display_name = display_name.trim();
    if display_name.is_empty() {
        return Err(HostError::Validation("display_name is required.".into()));
    }
    fs::create_dir_all(&root)?;
    let suffix = unique_suffix();
    let folder_name = format!("{}-{}", slugify(display_name), suffix);
    let project_path = root.join(folder_name);
    fs::create_dir_all(artifacts_root(&project_path))?;

    let connection = Connection::open(project_db_path(&project_path))?;
    initialize_schema(&connection)?;
    let project_id = format!("proj_{suffix}");
    initialize_project_config(
        &connection,
        &project_id,
        display_name,
        subject,
        course_code,
        lms_course_id,
        instructor_profile,
    )?;
    load_project_summary(&project_path)
}

pub fn open_project(project_path: &Path) -> HostResult<ProjectSummary> {
    let db_path = project_db_path(project_path);
    if !db_path.is_file() {
        return Err(HostError::Project(format!(
            "The selected folder does not contain {}.",
            PROJECT_DB_NAME
        )));
    }
    let connection = Connection::open(&db_path)?;
    initialize_schema(&connection)?;
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version != SCHEMA_VERSION {
        return Err(HostError::Project(format!(
            "Unsupported project schema version {version}. Expected {SCHEMA_VERSION}."
        )));
    }
    load_project_summary(project_path)
}

pub(crate) fn load_project_summary(project_path: &Path) -> HostResult<ProjectSummary> {
    let connection = Connection::open(project_db_path(project_path))?;
    let metadata = connection
        .query_row(
            "SELECT project_id, display_name, subject, course_code, lms_course_id, created_at, updated_at
             FROM project
             LIMIT 1",
            [],
            |row| {
                Ok(ProjectSummary {
                    project_id: row.get(0)?,
                    display_name: row.get(1)?,
                    subject: row.get(2)?,
                    course_code: row.get(3)?,
                    lms_course_id: row.get(4)?,
                    project_path: project_path.to_string_lossy().into_owned(),
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .optional()?;
    metadata.ok_or_else(|| {
        HostError::Project("The selected project is missing project configuration.".into())
    })
}

pub(crate) fn project_id(connection: &Connection) -> HostResult<String> {
    connection
        .query_row("SELECT project_id FROM project LIMIT 1", [], |row| {
            row.get(0)
        })
        .map_err(Into::into)
}

pub(crate) fn touch_project(transaction: &Transaction<'_>) -> HostResult<()> {
    transaction.execute("UPDATE project SET updated_at = CURRENT_TIMESTAMP", [])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{select_default_projects_parent_dir, HostResult};

    #[test]
    fn default_projects_parent_prefers_documents_candidate() -> HostResult<()> {
        let selected = select_default_projects_parent_dir(&[
            Some(PathBuf::from("/home/test/Documents")),
            Some(PathBuf::from("/home/test")),
        ])?;

        assert_eq!(selected, PathBuf::from("/home/test/Documents"));
        Ok(())
    }

    #[test]
    fn default_projects_parent_falls_back_when_documents_missing() -> HostResult<()> {
        let selected = select_default_projects_parent_dir(&[
            None,
            Some(PathBuf::from("/home/test")),
            Some(PathBuf::from("/home/test/.local/share")),
        ])?;

        assert_eq!(selected, PathBuf::from("/home/test"));
        Ok(())
    }

    #[test]
    fn default_projects_parent_errors_without_candidates() {
        let error = select_default_projects_parent_dir(&[None, None]).unwrap_err();

        assert_eq!(
            error.to_string(),
            "Could not resolve a default project directory."
        );
    }
}
