// SPDX-License-Identifier: AGPL-3.0-only
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use serde_json::Value;

use crate::errors::{HostError, HostResult};

pub(crate) const SCHEMA_VERSION: i64 = 0;
pub(crate) const PROJECT_DB_NAME: &str = "scriptscore.db";
pub(crate) const ARTIFACTS_DIR_NAME: &str = "artifacts";
pub(crate) const TEMPLATE_SETUP_STATE_KEY: &str = "template_setup";

const BASELINE_SQL: &str = r#"
PRAGMA user_version = 0;

CREATE TABLE IF NOT EXISTS project (
  project_id TEXT PRIMARY KEY,
  display_name TEXT NOT NULL,
  subject TEXT,
  course_code TEXT,
  lms_course_id TEXT,
  lms_assignment_id TEXT,
  redaction_required INTEGER NOT NULL DEFAULT 1,
  instructor_profile_json TEXT NOT NULL DEFAULT '{}',
  trace_refs_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS workflow_state (
  scope_type TEXT NOT NULL,
  scope_id TEXT NOT NULL,
  state_key TEXT NOT NULL,
  status TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (scope_type, scope_id, state_key)
);

CREATE TABLE IF NOT EXISTS job_run (
  job_id TEXT PRIMARY KEY,
  command_name TEXT NOT NULL,
  request_id TEXT NOT NULL,
  state TEXT NOT NULL,
  submitted_at TEXT NOT NULL,
  started_at TEXT,
  finished_at TEXT,
  request_json TEXT NOT NULL,
  result_json TEXT,
  error_json TEXT
);

CREATE TABLE IF NOT EXISTS job_event (
  event_id INTEGER PRIMARY KEY AUTOINCREMENT,
  job_id TEXT NOT NULL,
  sequence INTEGER NOT NULL,
  event_type TEXT NOT NULL,
  progress_json TEXT,
  scope_json TEXT,
  data_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS job_trace_student_ref (
  job_id TEXT NOT NULL,
  student_ref TEXT NOT NULL,
  PRIMARY KEY (job_id, student_ref)
);

CREATE INDEX IF NOT EXISTS idx_job_trace_student_ref_student_ref
  ON job_trace_student_ref(student_ref);

CREATE TABLE IF NOT EXISTS host_workflow_job (
  workflow_id TEXT PRIMARY KEY,
  command_name TEXT NOT NULL,
  result_kind TEXT NOT NULL,
  state TEXT NOT NULL,
  submitted_at TEXT NOT NULL,
  started_at TEXT,
  finished_at TEXT,
  workspace_changed INTEGER NOT NULL DEFAULT 0,
  result_json TEXT,
  error_json TEXT
);

CREATE TABLE IF NOT EXISTS host_workflow_child_job (
  workflow_id TEXT NOT NULL,
  child_job_id TEXT NOT NULL,
  command_name TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (workflow_id, child_job_id)
);

CREATE TABLE IF NOT EXISTS artifact (
  artifact_id TEXT PRIMARY KEY,
  job_id TEXT,
  kind TEXT NOT NULL,
  role TEXT NOT NULL,
  relative_path TEXT NOT NULL,
  mime_type TEXT,
  sha256 TEXT,
  byte_size INTEGER,
  metadata_json TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS question (
  question_id TEXT PRIMARY KEY,
  question_number INTEGER NOT NULL,
  page_number INTEGER NOT NULL,
  max_points INTEGER,
  prompt_text TEXT NOT NULL,
  baseline_pdf_text TEXT,
  region_x INTEGER,
  region_y INTEGER,
  region_width INTEGER,
  region_height INTEGER,
  source_artifact_id TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS template_redaction_region (
  region_id TEXT PRIMARY KEY,
  page_number INTEGER NOT NULL,
  x INTEGER NOT NULL,
  y INTEGER NOT NULL,
  width INTEGER NOT NULL,
  height INTEGER NOT NULL,
  -- Semantic label: 'name_identification' for the first region (lowest sort_order),
  -- 'privacy_protection' for all subsequent regions.
  label TEXT NOT NULL,
  sort_order INTEGER NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS student_roster (
  student_ref TEXT PRIMARY KEY,
  binding_token_hex TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
"#;

pub(crate) fn initialize_schema(connection: &Connection) -> HostResult<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    match version {
        0 => connection.execute_batch(BASELINE_SQL)?,
        other => {
            return Err(HostError::Project(format!(
                "Unsupported prerelease project schema version {other}. Expected {SCHEMA_VERSION}. Recreate the local desktop project database from the current baseline."
            )));
        }
    }
    Ok(())
}

pub(crate) fn command_output_dir(project_path: &Path, command_name: &str, job_id: &str) -> PathBuf {
    artifacts_root(project_path)
        .join("jobs")
        .join(command_name.replace('.', "_"))
        .join(job_id)
}

pub(crate) fn artifacts_root(project_path: &Path) -> PathBuf {
    project_path.join(ARTIFACTS_DIR_NAME)
}

pub(crate) fn project_db_path(project_path: &Path) -> PathBuf {
    project_path.join(PROJECT_DB_NAME)
}

pub(crate) fn parse_json(source: Option<&str>) -> rusqlite::Result<Value> {
    match source {
        Some(text) if !text.is_empty() => serde_json::from_str(text).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
        }),
        _ => Ok(Value::Null),
    }
}

pub(crate) fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

pub(crate) fn slugify(display_name: &str) -> String {
    let mut slug = String::new();
    let mut previous_was_dash = false;
    for character in display_name.chars() {
        let lower = character.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            slug.push(lower);
            previous_was_dash = false;
        } else if !previous_was_dash {
            slug.push('-');
            previous_was_dash = true;
        }
    }
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "project".into()
    } else {
        trimmed.into()
    }
}

pub(crate) fn sanitize_file_component(value: &str) -> String {
    let mut sanitized = String::new();
    for character in value.chars() {
        let lower = character.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() || matches!(lower, '.' | '-' | '_') {
            sanitized.push(lower);
        } else {
            sanitized.push('_');
        }
    }
    sanitized
}

pub(crate) fn unique_suffix() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis();
    format!("{:x}", timestamp)
}

pub(crate) fn current_timestamp() -> String {
    format!(
        "{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs()
    )
}
