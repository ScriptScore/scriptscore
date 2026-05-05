// SPDX-License-Identifier: AGPL-3.0-only
use std::path::PathBuf;

use crate::binding_token::{canvas_course_context, compute_binding_token_hex, TOKEN_VERSION};
use crate::errors::{HostError, HostResult};
use crate::models::{AppSettings, StudentRosterMatchResult};
use crate::project_store;

use super::{lms_roster_cache, AppState};

pub(super) fn compute_lms_binding_token(
    app_state: &AppState,
    course_id: String,
    canvas_user_id: String,
    settings: &AppSettings,
) -> HostResult<String> {
    let project_path = app_state.inner.lock().current_project_path_optional();
    let course_resolved = match project_path.as_ref() {
        Some(path) => project_store::resolve_canvas_course_id_for_binding(path, &course_id)?,
        None => {
            let t = course_id.trim();
            if t.is_empty() {
                return Err(HostError::Validation(
                    "Canvas course id is required for LMS binding.".into(),
                ));
            }
            t.to_string()
        }
    };
    let secret = crate::secrets::binding_hmac_secret_bytes(settings)?;
    let ctx = canvas_course_context(&course_resolved);
    compute_binding_token_hex(&secret, &ctx, canvas_user_id.trim(), TOKEN_VERSION)
}

pub(super) fn prior_canonical_submission_exists_for_lms_student(
    app_state: &AppState,
    course_id: String,
    canvas_user_id: String,
    settings: &AppSettings,
) -> HostResult<bool> {
    fn intake_row_has_canonical_outputs(item: &crate::models::StudentIntakeSummary) -> bool {
        if item.canonical_pdf_path.trim().is_empty() {
            return false;
        }
        item.ingest_status == "ok" || item.page_count > 0
    }

    let project_path = {
        let app = app_state.inner.lock();
        app.current_project_path()?
    };
    let course_resolved =
        project_store::resolve_canvas_course_id_for_binding(&project_path, &course_id)?;
    let secret = crate::secrets::binding_hmac_secret_bytes(settings)?;
    let ctx = canvas_course_context(&course_resolved);
    let token = compute_binding_token_hex(&secret, &ctx, canvas_user_id.trim(), TOKEN_VERSION)?;
    let Some(student_ref) =
        project_store::load_student_ref_for_binding_token_hex(&project_path, &token)?
    else {
        return Ok(false);
    };
    let intake = project_store::load_student_intake_state(&project_path)?;
    Ok(intake
        .items
        .iter()
        .any(|item| item.student_ref == student_ref && intake_row_has_canonical_outputs(item)))
}

pub(super) fn resolve_lms_student_ref(
    app_state: &AppState,
    course_id: String,
    canvas_user_id: String,
    settings: &AppSettings,
) -> HostResult<StudentRosterMatchResult> {
    let project_path: PathBuf = {
        let app = app_state.inner.lock();
        app.current_project_path()?
    };
    let course_resolved =
        project_store::resolve_canvas_course_id_for_binding(&project_path, &course_id)?;
    let secret = crate::secrets::binding_hmac_secret_bytes(settings)?;
    let ctx = canvas_course_context(&course_resolved);
    let token = compute_binding_token_hex(&secret, &ctx, canvas_user_id.trim(), TOKEN_VERSION)?;
    let mut student_ref =
        project_store::load_student_ref_for_binding_token_hex(&project_path, &token)?;
    if student_ref.is_none() {
        sync_student_roster_tokens_from_cached_lms(app_state, settings, "Student intake")?;
        student_ref = project_store::load_student_ref_for_binding_token_hex(&project_path, &token)?;
    }
    let student_ref = student_ref.ok_or_else(|| {
        HostError::Validation(
            "Selected LMS student is not present in the persisted project roster. Run LMS roster preload before intake."
                .into(),
        )
    })?;
    Ok(StudentRosterMatchResult {
        student_ref,
        binding_token_hex: token,
    })
}

pub(super) fn sync_student_roster_tokens_from_cached_lms(
    app_state: &AppState,
    settings: &AppSettings,
    action: &str,
) -> HostResult<bool> {
    let Some(_) = student_roster_sync_course_id(app_state)? else {
        return Ok(false);
    };
    let (course_id, rows) = lms_roster_cache::required_cached_rows(app_state, settings, action)?;
    let mut tokens = Vec::with_capacity(rows.len());
    for row in rows {
        tokens.push(compute_lms_binding_token(
            app_state,
            course_id.clone(),
            row.user_id,
            settings,
        )?);
    }
    let project_path = {
        let app = app_state.inner.lock();
        app.current_project_path()?
    };
    project_store::sync_student_roster_tokens(&project_path, &tokens)?;
    Ok(true)
}

pub(super) fn student_roster_sync_course_id(app_state: &AppState) -> HostResult<Option<String>> {
    let app = app_state.inner.lock();
    let project_path = app.current_project_path()?;
    let connection =
        rusqlite::Connection::open(crate::project_store::schema::project_db_path(&project_path))?;
    let project_config = project_store::load_project_config(&connection)?;
    let Some(course_id) = project_config.lms_course_id.as_ref() else {
        return Ok(None);
    };
    Ok(Some(course_id.clone()))
}
