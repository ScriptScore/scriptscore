// SPDX-License-Identifier: AGPL-3.0-only
use std::fs;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::errors::{HostError, HostResult};
use crate::models::{
    ArtifactRecord, QuestionRecord, TemplateArucoStatus, TemplatePageArtifactSummary,
    TemplateSetupPayload,
};
use crate::workflow_status::TemplateSetupStatus;

use super::projects::{project_id, touch_project};
use super::schema::{
    initialize_schema, project_db_path, sanitize_file_component, TEMPLATE_SETUP_STATE_KEY,
};

pub fn prepare_template_setup(
    project_path: &Path,
    template_pdf_path: &Path,
    job_id: &str,
) -> HostResult<TemplateSetupPayload> {
    if !template_pdf_path.is_file() {
        return Err(HostError::Validation(format!(
            "Template PDF was not found at '{}'.",
            template_pdf_path.display()
        )));
    }
    let source_name = template_pdf_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("template.pdf")
        .to_string();
    let target_relative = Path::new(super::schema::ARTIFACTS_DIR_NAME)
        .join("template_inputs")
        .join(format!(
            "{job_id}-{}",
            sanitize_file_component(&source_name)
        ));
    let target_path = project_path.join(&target_relative);
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(template_pdf_path, &target_path)?;
    let metadata = fs::metadata(&target_path)?;
    let artifact_id = format!("template_pdf_{job_id}");
    let payload = TemplateSetupPayload {
        template_artifact_id: Some(artifact_id.clone()),
        template_source_name: Some(source_name),
        last_setup_job_id: Some(job_id.to_string()),
        page_count: 0,
        question_count: 0,
        failure_message: None,
        approved_at: None,
        redaction_skip_acknowledged_at: None,
        aruco_status: Default::default(),
        warnings: Vec::new(),
    };

    let mut connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let project_id = project_id(&connection)?;
    let transaction = connection.transaction()?;
    clear_template_setup_data(&transaction)?;
    insert_artifact(
        &transaction,
        &ArtifactRecord {
            artifact_id,
            job_id: Some(job_id.to_string()),
            kind: "document".into(),
            role: "canonical_template_pdf".into(),
            relative_path: target_relative.to_string_lossy().into_owned(),
            mime_type: Some("application/pdf".into()),
            byte_size: Some(metadata.len() as i64),
            metadata_json: Some(
                serde_json::json!({
                    "source_name": payload.template_source_name.clone(),
                })
                .to_string(),
            ),
        },
    )?;
    upsert_template_setup_state(
        &transaction,
        &project_id,
        TemplateSetupStatus::Running,
        &payload,
    )?;
    touch_project(&transaction)?;
    transaction.commit()?;
    Ok(payload)
}

pub fn persist_template_setup_success(
    project_path: &Path,
    payload: &TemplateSetupPayload,
    page_artifacts: &[TemplatePageArtifactSummary],
    artifact_records: &[ArtifactRecord],
    questions: &[QuestionRecord],
) -> HostResult<()> {
    let mut connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let project_id = project_id(&connection)?;
    let transaction = connection.transaction()?;
    for artifact in artifact_records {
        insert_artifact(&transaction, artifact)?;
    }
    for question in questions {
        transaction.execute(
            "INSERT INTO question (
                question_id,
                question_number,
                page_number,
                max_points,
                prompt_text,
                baseline_pdf_text,
                region_x,
                region_y,
                region_width,
                region_height,
                source_artifact_id,
                created_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
            params![
                question.question_id,
                question.question_number,
                question.page_number,
                question.max_points,
                question.text,
                question.baseline_pdf_text,
                question.region.as_ref().map(|region| region.x),
                question.region.as_ref().map(|region| region.y),
                question.region.as_ref().map(|region| region.width),
                question.region.as_ref().map(|region| region.height),
                question.source_artifact_id,
            ],
        )?;
    }
    let mut next_payload = payload.clone();
    next_payload.failure_message = None;
    next_payload.approved_at = None;
    next_payload.page_count = page_artifacts.len() as i64;
    next_payload.question_count = questions.len() as i64;
    upsert_template_setup_state(
        &transaction,
        &project_id,
        TemplateSetupStatus::Draft,
        &next_payload,
    )?;
    touch_project(&transaction)?;
    transaction.commit()?;
    Ok(())
}

pub fn persist_template_setup_failure(
    project_path: &Path,
    payload: &TemplateSetupPayload,
    failure_message: &str,
) -> HostResult<()> {
    let mut connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let project_id = project_id(&connection)?;
    let transaction = connection.transaction()?;
    let mut next_payload = payload.clone();
    next_payload.page_count = 0;
    next_payload.question_count = 0;
    next_payload.approved_at = None;
    next_payload.failure_message = Some(failure_message.to_string());
    upsert_template_setup_state(
        &transaction,
        &project_id,
        TemplateSetupStatus::Failed,
        &next_payload,
    )?;
    touch_project(&transaction)?;
    transaction.commit()?;
    Ok(())
}

pub(crate) fn load_template_setup_state(
    connection: &Connection,
    project_id: &str,
) -> HostResult<(TemplateSetupStatus, TemplateSetupPayload)> {
    let row = connection
        .query_row(
            "SELECT status, payload_json
             FROM workflow_state
             WHERE scope_type = 'project' AND scope_id = ?1 AND state_key = ?2",
            params![project_id, TEMPLATE_SETUP_STATE_KEY],
            |row| {
                let status: String = row.get(0)?;
                let payload_json: String = row.get(1)?;
                Ok((status, payload_json))
            },
        )
        .optional()?;
    let Some((status, payload_json)) = row else {
        return Ok((
            TemplateSetupStatus::NotStarted,
            TemplateSetupPayload::default(),
        ));
    };
    let payload: TemplateSetupPayload = serde_json::from_str(&payload_json)?;
    Ok((TemplateSetupStatus::from_storage(&status), payload))
}

pub(crate) fn upsert_template_setup_state(
    transaction: &Transaction<'_>,
    project_id: &str,
    status: TemplateSetupStatus,
    payload: &TemplateSetupPayload,
) -> HostResult<()> {
    transaction.execute(
        "INSERT INTO workflow_state (
            scope_type,
            scope_id,
            state_key,
            status,
            payload_json,
            updated_at
         ) VALUES ('project', ?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)
         ON CONFLICT(scope_type, scope_id, state_key)
         DO UPDATE SET
            status = excluded.status,
            payload_json = excluded.payload_json,
            updated_at = CURRENT_TIMESTAMP",
        params![
            project_id,
            TEMPLATE_SETUP_STATE_KEY,
            status.as_str(),
            serde_json::to_string(payload)?,
        ],
    )?;
    Ok(())
}

pub(crate) fn mark_template_setup_draft(
    transaction: &Transaction<'_>,
    project_id: &str,
) -> HostResult<()> {
    let (status, mut payload) = load_template_setup_state(transaction, project_id)?;
    if matches!(
        status,
        TemplateSetupStatus::NotStarted | TemplateSetupStatus::Failed
    ) {
        return Ok(());
    }
    payload.approved_at = None;
    payload.failure_message = None;
    upsert_template_setup_state(
        transaction,
        project_id,
        TemplateSetupStatus::Draft,
        &payload,
    )
}

pub(crate) fn load_canonical_template_pdf_path(
    project_path: &Path,
) -> HostResult<Option<std::path::PathBuf>> {
    let connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let relative_path = connection
        .query_row(
            "SELECT relative_path FROM artifact WHERE role = 'canonical_template_pdf' LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(relative_path.map(|path| project_path.join(path)))
}

pub(crate) fn update_template_aruco_status(
    project_path: &Path,
    aruco_status: TemplateArucoStatus,
) -> HostResult<()> {
    let mut connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let project_id = project_id(&connection)?;
    let (status, mut payload) = load_template_setup_state(&connection, &project_id)?;
    payload.aruco_status = aruco_status;
    let transaction = connection.transaction()?;
    upsert_template_setup_state(&transaction, &project_id, status, &payload)?;
    touch_project(&transaction)?;
    transaction.commit()?;
    Ok(())
}

pub(crate) fn replace_template_pdf_and_pages(
    project_path: &Path,
    job_id: &str,
    stamped_pdf_path: &Path,
    rendered_page_paths: &[std::path::PathBuf],
    aruco_status: TemplateArucoStatus,
) -> HostResult<()> {
    let mut connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let project_id = project_id(&connection)?;
    let (status, mut payload) = load_template_setup_state(&connection, &project_id)?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "DELETE FROM artifact WHERE role IN ('canonical_template_pdf', 'rendered_template_page')",
        [],
    )?;
    insert_artifact(
        &transaction,
        &ArtifactRecord {
            artifact_id: format!("stamped_template_pdf_{job_id}"),
            job_id: Some(job_id.to_string()),
            kind: "document".into(),
            role: "canonical_template_pdf".into(),
            relative_path: relative_project_path(project_path, stamped_pdf_path)?,
            mime_type: Some("application/pdf".into()),
            byte_size: Some(fs::metadata(stamped_pdf_path)?.len() as i64),
            metadata_json: Some(
                serde_json::json!({
                    "source_name": payload.template_source_name.clone(),
                    "stamped": true,
                })
                .to_string(),
            ),
        },
    )?;
    for (index, path) in rendered_page_paths.iter().enumerate() {
        let page_number = i64::try_from(index + 1)
            .map_err(|_| HostError::Project("Template page index overflowed.".into()))?;
        insert_artifact(
            &transaction,
            &ArtifactRecord {
                artifact_id: format!("stamped_template_page_{job_id}_{page_number}"),
                job_id: Some(job_id.to_string()),
                kind: "image".into(),
                role: "rendered_template_page".into(),
                relative_path: relative_project_path(project_path, path)?,
                mime_type: Some("image/png".into()),
                byte_size: Some(fs::metadata(path)?.len() as i64),
                metadata_json: Some(
                    serde_json::json!({
                        "page_number": page_number,
                        "label": format!("Page {page_number}"),
                        "stamped": true,
                    })
                    .to_string(),
                ),
            },
        )?;
    }
    payload.page_count = rendered_page_paths.len() as i64;
    payload.aruco_status = aruco_status;
    upsert_template_setup_state(&transaction, &project_id, status, &payload)?;
    touch_project(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn clear_template_setup_data(transaction: &Transaction<'_>) -> HostResult<()> {
    transaction.execute("DELETE FROM question", [])?;
    transaction.execute("DELETE FROM template_redaction_region", [])?;
    transaction.execute(
        "DELETE FROM artifact WHERE role IN (
            'canonical_template_pdf',
            'rendered_template_page',
            'template_question'
         )",
        [],
    )?;
    Ok(())
}

fn relative_project_path(project_path: &Path, absolute_path: &Path) -> HostResult<String> {
    absolute_path
        .strip_prefix(project_path)
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|_| {
            HostError::Project(format!(
                "Artifact '{}' is outside the project directory '{}'.",
                absolute_path.display(),
                project_path.display()
            ))
        })
}

fn insert_artifact(transaction: &Transaction<'_>, record: &ArtifactRecord) -> HostResult<()> {
    transaction.execute(
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
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?8, CURRENT_TIMESTAMP)",
        params![
            record.artifact_id,
            record.job_id,
            record.kind,
            record.role,
            record.relative_path,
            record.mime_type,
            record.byte_size,
            record.metadata_json,
        ],
    )?;
    Ok(())
}
