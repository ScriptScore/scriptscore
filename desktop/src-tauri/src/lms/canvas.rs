// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::HashMap;
use std::fs;

use serde::Deserialize;
use url::Url;

use super::{LmsCourseSummary, LmsRosterRow};
use crate::errors::HostResult;
use crate::models::{
    LmsAssignmentSummary, LmsUploadMode, LmsUploadPreparationRow, LmsUploadPublishOutcome,
    LmsUploadStudentResult, LmsUploadStudentStatus, ResultsLmsAssetBinding,
};
use crate::state::results_lms::{
    emit_results_lms_upload_student_finished, emit_results_lms_upload_student_started,
};
use crate::state::results_lms_report::canvas_comment_contains_generated_report;
use crate::state::RuntimeEventSink;

const MAX_CANVAS_ERROR_BODY_CHARS: usize = 240;

#[derive(Debug, Deserialize)]
struct CanvasCourseRow {
    id: i64,
    name: String,
    #[serde(default)]
    course_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CanvasUserRow {
    id: i64,
    name: String,
    #[serde(default)]
    sortable_name: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    login_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CanvasAssignmentRow {
    id: i64,
    name: String,
    #[serde(default)]
    points_possible: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct CanvasSubmissionComment {
    id: i64,
    #[serde(default)]
    comment: Option<String>,
    #[serde(default)]
    html_comment: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CanvasFileUploadTarget {
    upload_url: String,
    #[serde(default)]
    upload_params: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct CanvasFileRow {
    id: i64,
}

#[derive(Debug, Deserialize)]
struct CanvasSubmissionRow {
    #[serde(default)]
    submission_comments: Vec<CanvasSubmissionComment>,
}

pub struct CanvasResultsPublisherConfig<'a> {
    pub base_url: &'a str,
    pub access_token: &'a str,
    pub course_id: &'a str,
    pub assignment_id: &'a str,
}

pub struct CanvasUploadProgress<'a> {
    pub event_sink: &'a dyn RuntimeEventSink,
    pub batch_id: &'a str,
}

struct CanvasAssignmentContext<'a> {
    client: &'a reqwest::Client,
    base_url: &'a str,
    access_token: &'a str,
    course_numeric: i64,
    assignment_numeric: i64,
}

struct CanvasSubmissionTarget<'a> {
    user_id: &'a str,
}

struct GeneratedReportCommentPlan {
    generated_comment_ids: Vec<i64>,
}

struct CanvasPreparedAsset {
    binding: ResultsLmsAssetBinding,
    url: String,
    reused: bool,
}

fn normalize_base_url(raw: &str) -> HostResult<Url> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(crate::errors::HostError::Validation(
            "Canvas base URL is required.".into(),
        ));
    }
    let parsed = Url::parse(trimmed).map_err(|_| {
        crate::errors::HostError::Validation("Canvas base URL is not a valid URL.".into())
    })?;
    if parsed.scheme() != "https" {
        return Err(crate::errors::HostError::Validation(
            "Only HTTPS Canvas base URLs are allowed.".into(),
        ));
    }
    Ok(parsed)
}

fn canvas_client() -> HostResult<reqwest::Client> {
    reqwest::Client::builder()
        .https_only(true)
        .build()
        .map_err(|e| crate::errors::HostError::Project(format!("HTTP client init failed: {e}")))
}

/// List courses the current user teaches (first page, up to Canvas default page size).
pub async fn list_teacher_courses(
    base_url: &str,
    access_token: &str,
) -> HostResult<Vec<LmsCourseSummary>> {
    let token = access_token.trim();
    if token.is_empty() {
        return Err(crate::errors::HostError::Validation(
            "Canvas access token is required.".into(),
        ));
    }

    let mut base = normalize_base_url(base_url)?;
    base.set_path("/api/v1/courses");
    base.query_pairs_mut()
        .append_pair("per_page", "100")
        .append_pair("enrollment_type", "teacher");
    let url = base;

    let client = canvas_client()?;

    let response = client
        .get(url)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| crate::errors::HostError::Project(format!("Canvas request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(crate::errors::HostError::Project(format!(
            "Canvas API error {status}: {body}"
        )));
    }

    let rows: Vec<CanvasCourseRow> = response.json().await.map_err(|e| {
        crate::errors::HostError::Project(format!("Canvas response parse failed: {e}"))
    })?;

    let mut out: Vec<LmsCourseSummary> = rows
        .into_iter()
        .map(|row| LmsCourseSummary {
            lms_course_id: row.id.to_string(),
            name: row.name,
            course_code: row.course_code,
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn validate_course_id(course_id: &str) -> HostResult<i64> {
    let trimmed = course_id.trim();
    if trimmed.is_empty() {
        return Err(crate::errors::HostError::Validation(
            "Canvas course id is required.".into(),
        ));
    }
    trimmed.parse::<i64>().map_err(|_| {
        crate::errors::HostError::Validation("Canvas course id must be a numeric course id.".into())
    })
}

fn validate_assignment_id(assignment_id: &str) -> HostResult<i64> {
    let trimmed = assignment_id.trim();
    if trimmed.is_empty() {
        return Err(crate::errors::HostError::Validation(
            "Canvas assignment id is required.".into(),
        ));
    }
    trimmed.parse::<i64>().map_err(|_| {
        crate::errors::HostError::Validation(
            "Canvas assignment id must be a numeric assignment id.".into(),
        )
    })
}

/// Students enrolled in the course (first page, up to `per_page`), sorted by `sort_key`.
pub async fn list_course_roster(
    base_url: &str,
    access_token: &str,
    course_id: &str,
) -> HostResult<Vec<LmsRosterRow>> {
    let token = access_token.trim();
    if token.is_empty() {
        return Err(crate::errors::HostError::Validation(
            "Canvas access token is required.".into(),
        ));
    }
    let course_numeric = validate_course_id(course_id)?;

    let mut base = normalize_base_url(base_url)?;
    base.set_path(&format!("/api/v1/courses/{course_numeric}/users"));
    base.query_pairs_mut()
        .append_pair("per_page", "100")
        .append_pair("enrollment_type[]", "student");

    let client = canvas_client()?;

    let response = client
        .get(base)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| crate::errors::HostError::Project(format!("Canvas request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(crate::errors::HostError::Project(format!(
            "Canvas API error {status}: {body}"
        )));
    }

    let rows: Vec<CanvasUserRow> = response.json().await.map_err(|e| {
        crate::errors::HostError::Project(format!("Canvas roster response parse failed: {e}"))
    })?;

    let mut out: Vec<LmsRosterRow> = rows
        .into_iter()
        .map(|row| {
            let sort_source = row.sortable_name.unwrap_or_else(|| row.name.clone());
            let sort_key = sort_source.to_lowercase();
            LmsRosterRow {
                user_id: row.id.to_string(),
                display_name: row.name,
                sort_key,
                email: row.email,
                login_id: row.login_id,
            }
        })
        .collect();
    out.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
    Ok(out)
}

pub async fn list_course_assignments(
    base_url: &str,
    access_token: &str,
    course_id: &str,
) -> HostResult<Vec<LmsAssignmentSummary>> {
    let token = access_token.trim();
    if token.is_empty() {
        return Err(crate::errors::HostError::Validation(
            "Canvas access token is required.".into(),
        ));
    }
    let course_numeric = validate_course_id(course_id)?;

    let mut base = normalize_base_url(base_url)?;
    base.set_path(&format!("/api/v1/courses/{course_numeric}/assignments"));
    base.query_pairs_mut().append_pair("per_page", "100");

    let client = canvas_client()?;
    let response = client
        .get(base)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| crate::errors::HostError::Project(format!("Canvas request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        return Err(crate::errors::HostError::Project(format!(
            "Canvas API error {status} while loading assignments."
        )));
    }

    let rows: Vec<CanvasAssignmentRow> = response.json().await.map_err(|e| {
        crate::errors::HostError::Project(format!("Canvas assignments response parse failed: {e}"))
    })?;

    let mut out = rows
        .into_iter()
        .map(|row| LmsAssignmentSummary {
            assignment_id: row.id.to_string(),
            name: row.name,
            points_possible: row.points_possible,
        })
        .collect::<Vec<_>>();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub async fn publish_assignment_results(
    config: CanvasResultsPublisherConfig<'_>,
    mode: LmsUploadMode,
    rows: &[LmsUploadPreparationRow],
    progress: CanvasUploadProgress<'_>,
) -> HostResult<Vec<LmsUploadPublishOutcome>> {
    let token = config.access_token.trim();
    if token.is_empty() {
        return Err(crate::errors::HostError::Validation(
            "Canvas access token is required.".into(),
        ));
    }
    let course_numeric = validate_course_id(config.course_id)?;
    let assignment_numeric = validate_assignment_id(config.assignment_id)?;
    let client = canvas_client()?;
    let context = CanvasAssignmentContext {
        client: &client,
        base_url: config.base_url,
        access_token: token,
        course_numeric,
        assignment_numeric,
    };
    validate_assignment_access(&context).await?;

    match mode {
        LmsUploadMode::DryRun => publish_dry_run_results(&context, rows, &progress).await,
        LmsUploadMode::Live => publish_live_results(&context, rows, &progress).await,
    }
}

async fn validate_assignment_access(context: &CanvasAssignmentContext<'_>) -> HostResult<()> {
    let mut assignment_url = normalize_base_url(context.base_url)?;
    assignment_url.set_path(&format!(
        "/api/v1/courses/{}/assignments/{}",
        context.course_numeric, context.assignment_numeric
    ));
    let assignment_response = context
        .client
        .get(assignment_url)
        .header("Authorization", format!("Bearer {}", context.access_token))
        .send()
        .await
        .map_err(|e| crate::errors::HostError::Project(format!("Canvas request failed: {e}")))?;
    if !assignment_response.status().is_success() {
        let status = assignment_response.status();
        return Err(crate::errors::HostError::Project(format!(
            "Canvas API error {status} while validating the selected assignment."
        )));
    }
    Ok(())
}

async fn publish_dry_run_results(
    context: &CanvasAssignmentContext<'_>,
    rows: &[LmsUploadPreparationRow],
    progress: &CanvasUploadProgress<'_>,
) -> HostResult<Vec<LmsUploadPublishOutcome>> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        start_student_upload(progress, &row.student_ref);
        let outcome = match validate_dry_run_row(context, row).await {
            Ok(()) => build_publish_outcome(row, LmsUploadStudentStatus::Ready, None, Vec::new()),
            Err(error) => build_publish_outcome(
                row,
                LmsUploadStudentStatus::Failed,
                Some(error.to_string()),
                Vec::new(),
            ),
        };
        finish_student_upload(progress, &outcome.student_result);
        out.push(outcome);
    }
    Ok(out)
}

async fn publish_live_results(
    context: &CanvasAssignmentContext<'_>,
    rows: &[LmsUploadPreparationRow],
    progress: &CanvasUploadProgress<'_>,
) -> HostResult<Vec<LmsUploadPublishOutcome>> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        start_student_upload(progress, &row.student_ref);
        let outcome = match publish_live_row(context, row).await {
            Ok(active_asset_bindings) => build_publish_outcome(
                row,
                LmsUploadStudentStatus::Uploaded,
                None,
                active_asset_bindings,
            ),
            Err(error) => build_publish_outcome(
                row,
                LmsUploadStudentStatus::Failed,
                Some(error.to_string()),
                Vec::new(),
            ),
        };
        finish_student_upload(progress, &outcome.student_result);
        out.push(outcome);
    }
    Ok(out)
}

fn start_student_upload(progress: &CanvasUploadProgress<'_>, student_ref: &str) {
    emit_results_lms_upload_student_started(progress.event_sink, progress.batch_id, student_ref);
}

fn finish_student_upload(progress: &CanvasUploadProgress<'_>, result: &LmsUploadStudentResult) {
    emit_results_lms_upload_student_finished(progress.event_sink, progress.batch_id, result);
}

fn build_student_result(
    row: &LmsUploadPreparationRow,
    status: LmsUploadStudentStatus,
    sanitized_error: Option<String>,
) -> LmsUploadStudentResult {
    LmsUploadStudentResult {
        student_ref: row.student_ref.clone(),
        result_fingerprint: row.result_fingerprint.clone(),
        status,
        sanitized_error,
    }
}

fn build_publish_outcome(
    row: &LmsUploadPreparationRow,
    status: LmsUploadStudentStatus,
    sanitized_error: Option<String>,
    active_asset_bindings: Vec<ResultsLmsAssetBinding>,
) -> LmsUploadPublishOutcome {
    LmsUploadPublishOutcome {
        student_result: build_student_result(row, status, sanitized_error),
        active_asset_bindings,
    }
}

fn submission_target<'a>(row: &'a LmsUploadPreparationRow) -> CanvasSubmissionTarget<'a> {
    CanvasSubmissionTarget {
        user_id: &row.user_id,
    }
}

async fn validate_dry_run_row(
    context: &CanvasAssignmentContext<'_>,
    row: &LmsUploadPreparationRow,
) -> HostResult<()> {
    let submission = submission_target(row);
    list_submission_comments(context, &submission)
        .await
        .map(|_| ())
}

async fn publish_live_row(
    context: &CanvasAssignmentContext<'_>,
    row: &LmsUploadPreparationRow,
) -> HostResult<Vec<ResultsLmsAssetBinding>> {
    let submission = submission_target(row);
    let plan = generated_report_comment_plan(context, &submission).await?;
    let mut prepared_assets = prepare_report_assets_for_publish(context, row).await?;
    let comment_html = render_report_comment_html(row, &prepared_assets)?;

    if let Err(error) = publish_generated_comment(
        context,
        &submission,
        row.score,
        &comment_html,
        &prepared_assets,
    )
    .await
    {
        if should_retry_with_fresh_assets(&error)
            && prepared_assets.iter().any(|asset| asset.reused)
        {
            delete_newly_uploaded_files(context, &prepared_assets).await?;
            prepared_assets = upload_all_report_assets(context, row).await?;
            let retry_comment_html = render_report_comment_html(row, &prepared_assets)?;
            publish_generated_comment(
                context,
                &submission,
                row.score,
                &retry_comment_html,
                &prepared_assets,
            )
            .await?;
        } else {
            delete_newly_uploaded_files(context, &prepared_assets).await?;
            return Err(error);
        }
    }

    // The new report is already live at this point, so prior-comment cleanup is best-effort.
    let _ = cleanup_prior_generated_comments(context, &submission, &plan).await;
    let _ = delete_stale_generated_files(context, row, &prepared_assets).await;

    Ok(prepared_assets
        .into_iter()
        .map(|asset| asset.binding)
        .collect::<Vec<_>>())
}

async fn generated_report_comment_plan(
    context: &CanvasAssignmentContext<'_>,
    submission: &CanvasSubmissionTarget<'_>,
) -> HostResult<GeneratedReportCommentPlan> {
    let mut existing_report_comments = list_submission_comments(context, submission)
        .await?
        .into_iter()
        .filter(comment_is_generated_report)
        .collect::<Vec<_>>();
    existing_report_comments.sort_by_key(|comment| comment.id);
    Ok(GeneratedReportCommentPlan {
        generated_comment_ids: existing_report_comments
            .into_iter()
            .map(|comment| comment.id)
            .collect::<Vec<_>>(),
    })
}

async fn cleanup_prior_generated_comments(
    context: &CanvasAssignmentContext<'_>,
    submission: &CanvasSubmissionTarget<'_>,
    plan: &GeneratedReportCommentPlan,
) -> HostResult<()> {
    for comment_id in &plan.generated_comment_ids {
        delete_submission_comment(context, submission, *comment_id).await?;
    }
    Ok(())
}

async fn publish_generated_comment(
    context: &CanvasAssignmentContext<'_>,
    submission: &CanvasSubmissionTarget<'_>,
    score: f64,
    comment_html: &str,
    prepared_assets: &[CanvasPreparedAsset],
) -> HostResult<()> {
    create_submission_grade_comment(
        context,
        submission,
        score,
        comment_html,
        &prepared_assets
            .iter()
            .map(|asset| asset.binding.provider_file_id.as_str())
            .collect::<Vec<_>>(),
    )
    .await
}

async fn prepare_report_assets_for_publish(
    context: &CanvasAssignmentContext<'_>,
    row: &LmsUploadPreparationRow,
) -> HostResult<Vec<CanvasPreparedAsset>> {
    let mut prepared_assets = Vec::with_capacity(row.report_assets.len());
    for asset in &row.report_assets {
        if let Some(binding) = row.existing_asset_bindings.iter().find(|binding| {
            binding.local_asset_name == asset.local_asset_name
                && binding.asset_fingerprint == asset.asset_fingerprint
        }) {
            prepared_assets.push(CanvasPreparedAsset {
                binding: binding.clone(),
                url: canvas_file_download_url(context, &binding.provider_file_id)?,
                reused: true,
            });
            continue;
        }

        let file_id = upload_submission_comment_file(context, row, asset).await?;
        prepared_assets.push(CanvasPreparedAsset {
            binding: ResultsLmsAssetBinding {
                provider: "canvas".into(),
                course_id: context.course_numeric.to_string(),
                assignment_id: context.assignment_numeric.to_string(),
                student_ref: row.student_ref.clone(),
                local_asset_name: asset.local_asset_name.clone(),
                asset_fingerprint: asset.asset_fingerprint.clone(),
                provider_file_id: file_id.clone(),
            },
            url: canvas_file_download_url(context, &file_id)?,
            reused: false,
        });
    }
    Ok(prepared_assets)
}

async fn upload_all_report_assets(
    context: &CanvasAssignmentContext<'_>,
    row: &LmsUploadPreparationRow,
) -> HostResult<Vec<CanvasPreparedAsset>> {
    let mut prepared_assets = Vec::with_capacity(row.report_assets.len());
    for asset in &row.report_assets {
        let file_id = upload_submission_comment_file(context, row, asset).await?;
        prepared_assets.push(CanvasPreparedAsset {
            binding: ResultsLmsAssetBinding {
                provider: "canvas".into(),
                course_id: context.course_numeric.to_string(),
                assignment_id: context.assignment_numeric.to_string(),
                student_ref: row.student_ref.clone(),
                local_asset_name: asset.local_asset_name.clone(),
                asset_fingerprint: asset.asset_fingerprint.clone(),
                provider_file_id: file_id.clone(),
            },
            url: canvas_file_download_url(context, &file_id)?,
            reused: false,
        });
    }
    Ok(prepared_assets)
}

fn render_report_comment_html(
    row: &LmsUploadPreparationRow,
    prepared_assets: &[CanvasPreparedAsset],
) -> HostResult<String> {
    let mut asset_sources = HashMap::new();
    for asset in prepared_assets {
        asset_sources.insert(asset.binding.local_asset_name.clone(), asset.url.clone());
    }
    let rendered = crate::state::results_lms_report::substitute_report_asset_sources(
        &row.report_html_template,
        &asset_sources,
    );
    if rendered.chars().count() > 65_535 {
        return Err(crate::errors::HostError::Project(
            "Canvas generated report comment is too long even without embedded images.".into(),
        ));
    }
    Ok(rendered)
}

fn should_retry_with_fresh_assets(error: &crate::errors::HostError) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("file_ids") || message.contains("file id")
}

async fn delete_newly_uploaded_files(
    context: &CanvasAssignmentContext<'_>,
    prepared_assets: &[CanvasPreparedAsset],
) -> HostResult<()> {
    for asset in prepared_assets.iter().filter(|asset| !asset.reused) {
        delete_file(context, &asset.binding.provider_file_id).await?;
    }
    Ok(())
}

async fn delete_stale_generated_files(
    context: &CanvasAssignmentContext<'_>,
    row: &LmsUploadPreparationRow,
    prepared_assets: &[CanvasPreparedAsset],
) -> HostResult<()> {
    let current_assets = prepared_assets
        .iter()
        .map(|asset| {
            (
                asset.binding.local_asset_name.as_str(),
                asset.binding.asset_fingerprint.as_str(),
            )
        })
        .collect::<std::collections::HashSet<_>>();
    for binding in row.existing_asset_bindings.iter().filter(|binding| {
        !current_assets.contains(&(
            binding.local_asset_name.as_str(),
            binding.asset_fingerprint.as_str(),
        ))
    }) {
        delete_file(context, &binding.provider_file_id).await?;
    }
    Ok(())
}

async fn upload_submission_comment_file(
    context: &CanvasAssignmentContext<'_>,
    row: &LmsUploadPreparationRow,
    asset: &crate::models::LmsUploadReportAsset,
) -> HostResult<String> {
    let bytes = fs::read(&asset.local_file_path).map_err(|err| {
        crate::errors::HostError::Project(format!(
            "Could not read Results/LMS report asset '{}': {err}",
            asset.local_file_path
        ))
    })?;
    let upload_target = create_submission_comment_file_target(
        context,
        &row.user_id,
        &asset.local_asset_name,
        bytes.len(),
    )
    .await?;
    finish_submission_comment_file_upload(&upload_target, &asset.local_asset_name, bytes).await
}

async fn create_submission_comment_file_target(
    context: &CanvasAssignmentContext<'_>,
    user_id: &str,
    asset_name: &str,
    byte_size: usize,
) -> HostResult<CanvasFileUploadTarget> {
    let mut upload_url = normalize_base_url(context.base_url)?;
    upload_url.set_path(&format!(
        "/api/v1/courses/{}/assignments/{}/submissions/{user_id}/comments/files",
        context.course_numeric, context.assignment_numeric
    ));
    let response = context
        .client
        .post(upload_url)
        .header("Authorization", format!("Bearer {}", context.access_token))
        .form(&vec![
            ("name", asset_name.to_string()),
            ("size", byte_size.to_string()),
            ("content_type", "image/jpeg".to_string()),
        ])
        .send()
        .await
        .map_err(|e| crate::errors::HostError::Project(format!("Canvas request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(crate::errors::HostError::Project(format!(
            "Canvas API error {status} while preparing a report image upload: {}",
            body.split_whitespace().collect::<Vec<_>>().join(" ")
        )));
    }

    response.json().await.map_err(|e| {
        crate::errors::HostError::Project(format!(
            "Canvas report image upload target parse failed: {e}"
        ))
    })
}

async fn finish_submission_comment_file_upload(
    upload_target: &CanvasFileUploadTarget,
    asset_name: &str,
    bytes: Vec<u8>,
) -> HostResult<String> {
    let upload_url = Url::parse(&upload_target.upload_url).map_err(|_| {
        crate::errors::HostError::Project(
            "Canvas returned an invalid report image upload URL.".into(),
        )
    })?;
    let client = reqwest::Client::builder()
        .https_only(true)
        .build()
        .map_err(|e| crate::errors::HostError::Project(format!("HTTP client init failed: {e}")))?;
    let mut form = reqwest::multipart::Form::new();
    for (key, value) in &upload_target.upload_params {
        form = form.text(key.clone(), value.clone());
    }
    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name(asset_name.to_string())
        .mime_str("image/jpeg")
        .map_err(|e| crate::errors::HostError::Project(format!("JPEG MIME init failed: {e}")))?;
    let response = client
        .post(upload_url)
        .multipart(form.part("file", part))
        .send()
        .await
        .map_err(|e| {
            crate::errors::HostError::Project(format!("Canvas file upload failed: {e}"))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(crate::errors::HostError::Project(format!(
            "Canvas API error {status} while uploading a report image: {}",
            body.split_whitespace().collect::<Vec<_>>().join(" ")
        )));
    }

    let file: CanvasFileRow = response.json().await.map_err(|e| {
        crate::errors::HostError::Project(format!(
            "Canvas uploaded report image response parse failed: {e}"
        ))
    })?;
    Ok(file.id.to_string())
}

fn canvas_file_download_url(
    context: &CanvasAssignmentContext<'_>,
    file_id: &str,
) -> HostResult<String> {
    let mut file_url = normalize_base_url(context.base_url)?;
    file_url.set_path(&format!("/files/{file_id}/download"));
    file_url.query_pairs_mut().append_pair("download_frd", "1");
    Ok(file_url.to_string())
}

async fn delete_file(context: &CanvasAssignmentContext<'_>, file_id: &str) -> HostResult<()> {
    let mut file_url = normalize_base_url(context.base_url)?;
    file_url.set_path(&format!("/api/v1/files/{file_id}"));
    let response = context
        .client
        .delete(file_url)
        .header("Authorization", format!("Bearer {}", context.access_token))
        .send()
        .await
        .map_err(|e| crate::errors::HostError::Project(format!("Canvas request failed: {e}")))?;

    if response.status().is_success() {
        return Ok(());
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(crate::errors::HostError::Project(format!(
        "Canvas API error {status} while deleting a prior generated report image: {}",
        body.split_whitespace().collect::<Vec<_>>().join(" ")
    )))
}

async fn list_submission_comments(
    context: &CanvasAssignmentContext<'_>,
    submission: &CanvasSubmissionTarget<'_>,
) -> HostResult<Vec<CanvasSubmissionComment>> {
    let mut submission_url = normalize_base_url(context.base_url)?;
    submission_url.set_path(&format!(
        "/api/v1/courses/{}/assignments/{}/submissions/{}",
        context.course_numeric, context.assignment_numeric, submission.user_id
    ));
    submission_url
        .query_pairs_mut()
        .append_pair("include[]", "submission_comments");
    let response = context
        .client
        .get(submission_url)
        .header("Authorization", format!("Bearer {}", context.access_token))
        .send()
        .await
        .map_err(|e| crate::errors::HostError::Project(format!("Canvas request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        return Err(crate::errors::HostError::Project(format!(
            "Canvas API error {status} while loading existing submission comments."
        )));
    }

    let submission: CanvasSubmissionRow = response.json().await.map_err(|e| {
        crate::errors::HostError::Project(format!(
            "Canvas submission comments response parse failed: {e}"
        ))
    })?;
    Ok(submission.submission_comments)
}

async fn delete_submission_comment(
    context: &CanvasAssignmentContext<'_>,
    submission: &CanvasSubmissionTarget<'_>,
    comment_id: i64,
) -> HostResult<()> {
    let mut comment_url = normalize_base_url(context.base_url)?;
    comment_url.set_path(&format!(
        "/api/v1/courses/{}/assignments/{}/submissions/{}/comments/{comment_id}",
        context.course_numeric, context.assignment_numeric, submission.user_id
    ));
    let response = context
        .client
        .delete(comment_url)
        .header("Authorization", format!("Bearer {}", context.access_token))
        .send()
        .await
        .map_err(|e| crate::errors::HostError::Project(format!("Canvas request failed: {e}")))?;

    if response.status().is_success() {
        return Ok(());
    }

    let status = response.status();
    Err(crate::errors::HostError::Project(format!(
        "Canvas API error {status} while deleting the prior generated report comment."
    )))
}

async fn create_submission_grade_comment(
    context: &CanvasAssignmentContext<'_>,
    submission: &CanvasSubmissionTarget<'_>,
    score: f64,
    report_html: &str,
    file_ids: &[&str],
) -> HostResult<()> {
    let mut submission_url = normalize_base_url(context.base_url)?;
    submission_url.set_path(&format!(
        "/api/v1/courses/{}/assignments/{}/submissions/{}",
        context.course_numeric, context.assignment_numeric, submission.user_id
    ));
    let mut request_body = vec![
        ("submission[posted_grade]", score.to_string()),
        ("comment[text_comment]", report_html.to_string()),
    ];
    request_body.extend(
        file_ids
            .iter()
            .map(|file_id| ("comment[file_ids][]", (*file_id).to_string())),
    );
    let response = context
        .client
        .put(submission_url)
        .header("Authorization", format!("Bearer {}", context.access_token))
        .form(&request_body)
        .send()
        .await
        .map_err(|e| crate::errors::HostError::Project(format!("Canvas request failed: {e}")))?;

    if response.status().is_success() {
        return Ok(());
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(crate::errors::HostError::Project(
        canvas_publish_error_message(status, &body),
    ))
}

fn comment_is_generated_report(comment: &CanvasSubmissionComment) -> bool {
    comment
        .comment
        .as_deref()
        .is_some_and(canvas_comment_contains_generated_report)
        || comment
            .html_comment
            .as_deref()
            .is_some_and(canvas_comment_contains_generated_report)
}

fn canvas_publish_error_message(status: reqwest::StatusCode, body: &str) -> String {
    let normalized_body = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed_body = normalized_body.trim();
    if trimmed_body.is_empty() {
        return format!("Canvas API error {status} while publishing this score.");
    }
    let snippet = if trimmed_body.chars().count() > MAX_CANVAS_ERROR_BODY_CHARS {
        let mut clipped = trimmed_body
            .chars()
            .take(MAX_CANVAS_ERROR_BODY_CHARS)
            .collect::<String>();
        clipped.push_str("...");
        clipped
    } else {
        trimmed_body.to_string()
    };
    format!("Canvas API error {status} while publishing this score: {snippet}")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopEventSink;

    impl crate::state::RuntimeEventSink for NoopEventSink {
        fn emit_runtime_event(&self, _event: crate::models::RuntimeJobEvent) {}
    }

    fn test_client() -> reqwest::Client {
        reqwest::Client::builder()
            .https_only(true)
            .build()
            .expect("client should build")
    }

    fn test_context<'a>(client: &'a reqwest::Client) -> CanvasAssignmentContext<'a> {
        CanvasAssignmentContext {
            client,
            base_url: "https://canvas.example.edu/courses/123",
            access_token: "token",
            course_numeric: 123,
            assignment_numeric: 456,
        }
    }

    #[test]
    fn roster_parses_canvas_users_json() {
        let json = r#"[
            {"id": 42, "name": "Zed Last", "sortable_name": "last, zed"},
            {"id": 7, "name": "Amy First"}
        ]"#;
        let rows: Vec<CanvasUserRow> = serde_json::from_str(json).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, 42);
        assert!(rows[1].sortable_name.is_none());
    }

    #[test]
    fn validate_course_id_accepts_numeric() {
        assert_eq!(validate_course_id(" 12345 ").unwrap(), 12345);
    }

    #[test]
    fn validate_assignment_id_accepts_trimmed_numeric_and_rejects_empty() {
        assert_eq!(validate_assignment_id(" 98765 ").unwrap(), 98765);
        assert!(validate_assignment_id("")
            .unwrap_err()
            .to_string()
            .contains("assignment id is required"));
    }

    #[test]
    fn normalize_base_url_requires_https_canvas_url() {
        let parsed = normalize_base_url(" https://canvas.example.edu/ ")
            .expect("https canvas URL should parse");
        assert_eq!(parsed.as_str(), "https://canvas.example.edu/");

        assert!(normalize_base_url("")
            .unwrap_err()
            .to_string()
            .contains("required"));
        assert!(normalize_base_url("not a url")
            .unwrap_err()
            .to_string()
            .contains("not a valid URL"));
        assert!(normalize_base_url("http://canvas.example.edu")
            .unwrap_err()
            .to_string()
            .contains("Only HTTPS"));
    }

    #[test]
    fn validate_course_id_rejects_non_numeric() {
        assert!(validate_course_id("abc").is_err());
    }

    #[test]
    fn validate_assignment_id_rejects_non_numeric() {
        assert!(validate_assignment_id("abc").is_err());
    }

    #[test]
    fn canvas_file_download_url_uses_normalized_canvas_file_path() {
        let client = test_client();
        let context = test_context(&client);

        let url = canvas_file_download_url(&context, "789").expect("download URL should build");

        assert_eq!(
            url,
            "https://canvas.example.edu/files/789/download?download_frd=1"
        );
    }

    #[test]
    fn identifies_generated_report_comments_by_marker() {
        let report_comment = CanvasSubmissionComment {
            id: 1,
            comment: Some("Generated report".into()),
            html_comment: None,
        };
        let zero_width_marked_comment = CanvasSubmissionComment {
            id: 11,
            comment: Some("\u{2063}\u{2063}\u{200B}\u{200D}\u{2060}\u{200C}\u{2063}\u{200B}<html>report</html>".into()),
            html_comment: None,
        };
        let marked_comment = CanvasSubmissionComment {
            id: 2,
            comment: Some("Teacher note".into()),
            html_comment: Some(
                "<div style=\"display:none\">results-report:v1; fingerprint=fp_1</div>".into(),
            ),
        };
        let legacy_report_comment = CanvasSubmissionComment {
            id: 12,
            comment: Some(
                "Spring 2026 - CS145 - Midterm Total Score: 100 / 100 Question 1: Prompt Feedback: Correct."
                    .into(),
            ),
            html_comment: None,
        };
        let regular_comment = CanvasSubmissionComment {
            id: 3,
            comment: Some("Nice improvement".into()),
            html_comment: None,
        };

        assert!(!comment_is_generated_report(&report_comment));
        assert!(comment_is_generated_report(&zero_width_marked_comment));
        assert!(comment_is_generated_report(&marked_comment));
        assert!(!comment_is_generated_report(&legacy_report_comment));
        assert!(!comment_is_generated_report(&regular_comment));
    }

    #[test]
    fn build_publish_outcome_carries_row_status_error_and_bindings() {
        let row = LmsUploadPreparationRow {
            student_ref: "student-a".into(),
            user_id: "canvas-user-42".into(),
            result_fingerprint: "result-fp".into(),
            ..LmsUploadPreparationRow::default()
        };
        let binding = ResultsLmsAssetBinding {
            provider: "canvas".into(),
            course_id: "123".into(),
            assignment_id: "456".into(),
            student_ref: "student-a".into(),
            local_asset_name: "q1.jpg".into(),
            asset_fingerprint: "asset-fp".into(),
            provider_file_id: "file-1".into(),
        };

        let target = submission_target(&row);
        let outcome = build_publish_outcome(
            &row,
            LmsUploadStudentStatus::Failed,
            Some("Canvas rejected the score".into()),
            vec![binding.clone()],
        );

        assert_eq!(target.user_id, "canvas-user-42");
        assert_eq!(outcome.student_result.student_ref, "student-a");
        assert_eq!(outcome.student_result.result_fingerprint, "result-fp");
        assert_eq!(
            outcome.student_result.status,
            LmsUploadStudentStatus::Failed
        );
        assert_eq!(
            outcome.student_result.sanitized_error.as_deref(),
            Some("Canvas rejected the score")
        );
        assert_eq!(outcome.active_asset_bindings[0].provider_file_id, "file-1");
        assert_eq!(
            outcome.active_asset_bindings[0].asset_fingerprint,
            "asset-fp"
        );
    }

    #[test]
    fn canvas_public_entrypoints_reject_missing_auth_and_bad_ids_before_network() {
        let courses =
            tauri::async_runtime::block_on(list_teacher_courses("https://canvas.example.edu", " "));
        assert!(courses
            .unwrap_err()
            .to_string()
            .contains("access token is required"));

        let roster_missing_auth = tauri::async_runtime::block_on(list_course_roster(
            "https://canvas.example.edu",
            "",
            "123",
        ));
        assert!(roster_missing_auth
            .unwrap_err()
            .to_string()
            .contains("access token is required"));

        let roster_bad_course = tauri::async_runtime::block_on(list_course_roster(
            "https://canvas.example.edu",
            "token",
            "abc",
        ));
        assert!(roster_bad_course
            .unwrap_err()
            .to_string()
            .contains("numeric course id"));

        let assignments_missing_auth = tauri::async_runtime::block_on(list_course_assignments(
            "https://canvas.example.edu",
            " ",
            "123",
        ));
        assert!(assignments_missing_auth
            .unwrap_err()
            .to_string()
            .contains("access token is required"));

        let assignments_bad_course = tauri::async_runtime::block_on(list_course_assignments(
            "https://canvas.example.edu",
            "token",
            "not-numeric",
        ));
        assert!(assignments_bad_course
            .unwrap_err()
            .to_string()
            .contains("numeric course id"));

        let sink = NoopEventSink;
        let publish = tauri::async_runtime::block_on(publish_assignment_results(
            CanvasResultsPublisherConfig {
                base_url: "https://canvas.example.edu",
                access_token: " ",
                course_id: "123",
                assignment_id: "456",
            },
            LmsUploadMode::DryRun,
            &[],
            CanvasUploadProgress {
                event_sink: &sink,
                batch_id: "batch-1",
            },
        ));
        assert!(publish
            .unwrap_err()
            .to_string()
            .contains("access token is required"));
    }

    #[test]
    fn prepare_report_assets_reuses_matching_bindings_without_uploading() {
        let client = test_client();
        let context = test_context(&client);
        let row = LmsUploadPreparationRow {
            student_ref: "student-a".into(),
            report_assets: vec![crate::models::LmsUploadReportAsset {
                question_id: "q1".into(),
                local_asset_name: "q1.jpg".into(),
                asset_fingerprint: "asset-fp".into(),
                local_file_path: "/unused/q1.jpg".into(),
            }],
            existing_asset_bindings: vec![ResultsLmsAssetBinding {
                provider: "canvas".into(),
                course_id: "123".into(),
                assignment_id: "456".into(),
                student_ref: "student-a".into(),
                local_asset_name: "q1.jpg".into(),
                asset_fingerprint: "asset-fp".into(),
                provider_file_id: "987".into(),
            }],
            ..LmsUploadPreparationRow::default()
        };

        let prepared =
            tauri::async_runtime::block_on(prepare_report_assets_for_publish(&context, &row))
                .expect("matching asset should be reused");

        assert_eq!(prepared.len(), 1);
        assert!(prepared[0].reused);
        assert_eq!(prepared[0].binding.provider_file_id, "987");
        assert_eq!(
            prepared[0].url,
            "https://canvas.example.edu/files/987/download?download_frd=1"
        );
    }

    #[test]
    fn asset_cleanup_helpers_skip_when_everything_is_reused_or_current() {
        let client = test_client();
        let context = test_context(&client);
        let binding = ResultsLmsAssetBinding {
            provider: "canvas".into(),
            course_id: "123".into(),
            assignment_id: "456".into(),
            student_ref: "student-a".into(),
            local_asset_name: "q1.jpg".into(),
            asset_fingerprint: "asset-fp".into(),
            provider_file_id: "987".into(),
        };
        let prepared_assets = vec![CanvasPreparedAsset {
            binding: binding.clone(),
            url: "https://canvas.example.edu/files/987/download?download_frd=1".into(),
            reused: true,
        }];
        let row = LmsUploadPreparationRow {
            existing_asset_bindings: vec![binding],
            ..LmsUploadPreparationRow::default()
        };

        tauri::async_runtime::block_on(delete_newly_uploaded_files(&context, &prepared_assets))
            .expect("reused assets should not be deleted");
        tauri::async_runtime::block_on(delete_stale_generated_files(
            &context,
            &row,
            &prepared_assets,
        ))
        .expect("current assets should not be deleted as stale");
    }

    #[test]
    fn generated_report_comment_plan_sorts_existing_generated_comment_ids() {
        let comments = vec![
            CanvasSubmissionComment {
                id: 3,
                comment: Some("Teacher note".into()),
                html_comment: Some(
                    "<div style=\"display:none\">results-report:v1; fingerprint=fp_3</div>".into(),
                ),
            },
            CanvasSubmissionComment {
                id: 1,
                comment: Some(
                    "\u{2063}\u{2063}\u{200B}\u{200D}\u{2060}\u{200C}\u{2063}\u{200B}report".into(),
                ),
                html_comment: None,
            },
            CanvasSubmissionComment {
                id: 2,
                comment: Some("Regular note".into()),
                html_comment: None,
            },
        ];
        let mut existing_report_comments = comments
            .into_iter()
            .filter(comment_is_generated_report)
            .collect::<Vec<_>>();
        existing_report_comments.sort_by_key(|comment| comment.id);
        let plan = GeneratedReportCommentPlan {
            generated_comment_ids: existing_report_comments
                .into_iter()
                .map(|comment| comment.id)
                .collect(),
        };

        assert_eq!(plan.generated_comment_ids, vec![1, 3]);
    }

    #[test]
    fn canvas_publish_error_message_includes_trimmed_body() {
        let message = canvas_publish_error_message(
            reqwest::StatusCode::UNPROCESSABLE_ENTITY,
            "{ \"errors\": [ { \"message\": \"Comment is too long\" } ] }",
        );

        assert!(message.contains("422"));
        assert!(message.contains("Comment is too long"));
    }

    #[test]
    fn canvas_publish_error_message_omits_empty_body_and_truncates_long_body() {
        let empty = canvas_publish_error_message(reqwest::StatusCode::BAD_GATEWAY, " \n\t ");
        assert_eq!(
            empty,
            "Canvas API error 502 Bad Gateway while publishing this score."
        );

        let long_body = "x".repeat(MAX_CANVAS_ERROR_BODY_CHARS + 20);
        let message = canvas_publish_error_message(reqwest::StatusCode::BAD_REQUEST, &long_body);

        assert!(message.contains("400"));
        assert!(message.ends_with("..."));
        assert!(message.contains(&"x".repeat(MAX_CANVAS_ERROR_BODY_CHARS)));
        assert!(!message.contains(&"x".repeat(MAX_CANVAS_ERROR_BODY_CHARS + 1)));
    }

    #[test]
    fn should_retry_with_fresh_assets_detects_canvas_file_id_failures() {
        assert!(should_retry_with_fresh_assets(
            &crate::errors::HostError::Project("invalid file_ids in comment".into())
        ));
        assert!(should_retry_with_fresh_assets(
            &crate::errors::HostError::Project("Canvas rejected stale file id".into())
        ));
        assert!(!should_retry_with_fresh_assets(
            &crate::errors::HostError::Project("Canvas score update failed".into())
        ));
    }

    #[test]
    fn render_submission_comment_replaces_only_known_asset_slots() {
        let row = LmsUploadPreparationRow {
            report_html_template:
                "<img src=\"__RESULTS_LMS_ASSET_q1.jpg__\"><img src=\"__RESULTS_LMS_ASSET_missing.jpg__\">".into(),
            ..LmsUploadPreparationRow::default()
        };
        let rendered = render_report_comment_html(
            &row,
            &[CanvasPreparedAsset {
                binding: ResultsLmsAssetBinding {
                    local_asset_name: "q1.jpg".into(),
                    ..ResultsLmsAssetBinding::default()
                },
                url: "https://canvas.example.edu/files/1".into(),
                reused: true,
            }],
        )
        .expect("comment should render");

        assert!(rendered.contains("https://canvas.example.edu/files/1"));
        assert!(rendered.contains("__RESULTS_LMS_ASSET_missing.jpg__"));
    }

    #[test]
    fn render_submission_comment_rejects_canvas_comment_limit() {
        let row = LmsUploadPreparationRow {
            report_html_template: "x".repeat(65_536),
            ..LmsUploadPreparationRow::default()
        };

        let error =
            render_report_comment_html(&row, &[]).expect_err("overlong Canvas comment should fail");

        assert!(error
            .to_string()
            .contains("Canvas generated report comment is too long"));
    }
}
