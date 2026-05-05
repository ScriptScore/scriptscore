// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageReader};
use sha2::{Digest, Sha256};

use crate::errors::{HostError, HostResult};
use crate::models::{
    ExamWorkspaceState, LmsUploadReportAsset, QuestionRecord, ResultQuestionRow, ResultStudentRow,
    ResultsLmsReportPreview, StudentWorkflowAnswer, StudentWorkflowState,
    StudentWorkflowSubmission,
};
use crate::project_store::schema::ARTIFACTS_DIR_NAME;

const REPORT_MARKER: &str = "results-report:v1";
const REPORT_ZERO_WIDTH_MARKER: &str =
    "\u{2063}\u{2063}\u{200B}\u{200D}\u{2060}\u{200C}\u{2063}\u{200B}";
const REPORT_IMAGE_ARTIFACT_DIR: &str = "results_lms/report_images";
const MAX_EMBEDDED_IMAGE_WIDTH_PX: u32 = 640;
const MAX_EMBEDDED_IMAGE_HEIGHT_PX: u32 = 420;
const MAX_EMBEDDED_IMAGE_BYTES: usize = 50 * 1024;
const MAX_REPORT_ASSET_PREWARM_WORKERS: usize = 4;
const ENCODE_ATTEMPTS: [(u32, u32, u8); 5] = [
    (640, 420, 58),
    (560, 360, 48),
    (480, 320, 40),
    (360, 240, 34),
    (300, 200, 30),
];

pub(crate) fn build_report_preview(
    workspace: &ExamWorkspaceState,
    student_ref: &str,
) -> HostResult<ResultsLmsReportPreview> {
    let row = workspace
        .results_lms_rows
        .iter()
        .find(|row| row.student_ref == student_ref)
        .ok_or_else(|| {
            HostError::Validation(format!("Result row '{}' was not found.", student_ref))
        })?;
    let submission = workspace
        .student_workflow
        .submissions
        .iter()
        .find(|submission| submission.student_ref == student_ref)
        .ok_or_else(|| {
            HostError::Validation(format!(
                "Submission '{}' was not found in student workflow state.",
                student_ref
            ))
        })?;
    let questions_by_id = workspace
        .questions
        .iter()
        .map(|question| (question.question_id.as_str(), question))
        .collect::<HashMap<_, _>>();
    let report_assets =
        prepare_report_assets(Path::new(&workspace.project.project_path), submission)?;
    let comment_template = build_canvas_report_comment_template(
        &workspace.project.display_name,
        row,
        submission,
        &questions_by_id,
        &report_assets,
    );
    let preview_sources = preview_asset_sources(&report_assets)?;

    Ok(ResultsLmsReportPreview {
        student_ref: student_ref.to_string(),
        result_fingerprint: row.result_fingerprint.clone(),
        html: substitute_report_asset_sources(&comment_template, &preview_sources),
    })
}

pub(crate) fn build_report_upload_materials(
    project_path: &Path,
    exam_title: &str,
    row: &ResultStudentRow,
    submission: &StudentWorkflowSubmission,
    questions_by_id: &HashMap<&str, &QuestionRecord>,
) -> HostResult<(String, Vec<LmsUploadReportAsset>)> {
    let report_assets = prepare_report_assets(project_path, submission)?;
    let report_html_template = build_canvas_report_comment_template(
        exam_title,
        row,
        submission,
        questions_by_id,
        &report_assets,
    );
    Ok((report_html_template, report_assets))
}

pub(crate) fn canvas_comment_contains_generated_report(comment: &str) -> bool {
    comment.contains(REPORT_MARKER) || comment.contains(REPORT_ZERO_WIDTH_MARKER)
}

pub(crate) fn build_canvas_report_comment_template(
    exam_title: &str,
    row: &ResultStudentRow,
    submission: &StudentWorkflowSubmission,
    questions_by_id: &HashMap<&str, &QuestionRecord>,
    report_assets: &[LmsUploadReportAsset],
) -> String {
    let image_src_by_question_id = report_assets
        .iter()
        .map(|asset| {
            (
                asset.question_id.as_str(),
                report_asset_placeholder(&asset.local_asset_name),
            )
        })
        .collect::<HashMap<_, _>>();
    let report_html = build_report_html(
        exam_title,
        row,
        submission,
        questions_by_id,
        &image_src_by_question_id,
    );
    wrap_generated_report_comment(&report_html, row.result_fingerprint.as_deref())
}

pub(crate) fn substitute_report_asset_sources(
    template: &str,
    asset_sources: &HashMap<String, String>,
) -> String {
    let mut rendered = template.to_string();
    for (asset_name, source) in asset_sources {
        rendered = rendered.replace(&report_asset_placeholder(asset_name), source);
    }
    rendered
}

pub(crate) fn prepare_report_assets(
    project_path: &Path,
    submission: &StudentWorkflowSubmission,
) -> HostResult<Vec<LmsUploadReportAsset>> {
    let mut assets = Vec::new();
    for answer in &submission.answers {
        let Some(crop_image_path) = answer.crop_image_path.as_deref() else {
            continue;
        };
        if let Some(asset) = ensure_report_asset(
            project_path,
            &submission.student_ref,
            answer,
            crop_image_path,
        )? {
            assets.push(asset);
        }
    }
    Ok(assets)
}

pub(crate) fn prewarm_report_assets_for_workflow_state(
    project_path: &Path,
    workflow_state: &StudentWorkflowState,
) -> HostResult<()> {
    let submissions = prewarmable_submissions(workflow_state);
    if submissions.is_empty() {
        return Ok(());
    }

    match report_asset_prewarm_worker_count(submissions.len()) {
        1 => prewarm_report_assets_serial(project_path, &submissions),
        worker_count => prewarm_report_assets_parallel(project_path, submissions, worker_count),
    }
}

pub(crate) fn spawn_report_asset_prewarm(
    project_path: PathBuf,
    workflow_state: StudentWorkflowState,
) {
    thread::spawn(move || {
        let _ = prewarm_report_assets_for_workflow_state(&project_path, &workflow_state);
    });
}

fn answer_has_report_asset_work(answer: &StudentWorkflowAnswer) -> bool {
    answer
        .crop_image_path
        .as_deref()
        .is_some_and(|path| !path.trim().is_empty())
}

fn prewarmable_submissions(
    workflow_state: &StudentWorkflowState,
) -> Vec<StudentWorkflowSubmission> {
    workflow_state
        .submissions
        .iter()
        .filter(|submission| submission.answers.iter().any(answer_has_report_asset_work))
        .cloned()
        .collect()
}

fn prewarm_report_assets_serial(
    project_path: &Path,
    submissions: &[StudentWorkflowSubmission],
) -> HostResult<()> {
    for submission in submissions {
        let _ = prepare_report_assets(project_path, submission)?;
    }
    Ok(())
}

fn prewarm_report_assets_parallel(
    project_path: &Path,
    submissions: Vec<StudentWorkflowSubmission>,
    worker_count: usize,
) -> HostResult<()> {
    let shared = Arc::new(ReportAssetPrewarmState {
        project_path: project_path.to_path_buf(),
        submissions,
        next_index: AtomicUsize::new(0),
        first_error: Mutex::new(None),
    });

    thread::scope(|scope| {
        for _ in 0..worker_count {
            let shared = Arc::clone(&shared);
            scope.spawn(move || prewarm_report_assets_worker(shared));
        }
    });

    finish_report_asset_prewarm(&shared)
}

fn prewarm_report_assets_worker(shared: Arc<ReportAssetPrewarmState>) {
    while let Some(index) = next_prewarm_submission_index(&shared) {
        if let Err(err) = prepare_report_assets(&shared.project_path, &shared.submissions[index]) {
            record_report_asset_prewarm_error(&shared, err);
        }
    }
}

fn next_prewarm_submission_index(shared: &ReportAssetPrewarmState) -> Option<usize> {
    let index = shared.next_index.fetch_add(1, Ordering::SeqCst);
    (index < shared.submissions.len()).then_some(index)
}

fn record_report_asset_prewarm_error(shared: &ReportAssetPrewarmState, err: HostError) {
    let mut slot = shared.first_error.lock().expect("prewarm error lock");
    if slot.is_none() {
        *slot = Some(err.to_string());
    }
}

fn finish_report_asset_prewarm(shared: &ReportAssetPrewarmState) -> HostResult<()> {
    if let Some(error_message) = shared
        .first_error
        .lock()
        .expect("prewarm error lock")
        .take()
    {
        return Err(HostError::Project(error_message));
    }
    Ok(())
}

fn report_asset_prewarm_worker_count(submission_count: usize) -> usize {
    let bounded_parallelism = thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(2)
        .clamp(1, MAX_REPORT_ASSET_PREWARM_WORKERS);
    submission_count.clamp(1, bounded_parallelism)
}

struct ReportAssetPrewarmState {
    project_path: PathBuf,
    submissions: Vec<StudentWorkflowSubmission>,
    next_index: AtomicUsize,
    first_error: Mutex<Option<String>>,
}

fn ensure_report_asset(
    project_path: &Path,
    student_ref: &str,
    answer: &StudentWorkflowAnswer,
    crop_image_path: &str,
) -> HostResult<Option<LmsUploadReportAsset>> {
    let local_asset_name = format!("{}.jpg", sanitize_asset_component(&answer.question_id));
    let local_file_path = report_asset_path(project_path, student_ref, &local_asset_name);
    if cached_report_asset_is_current(&local_file_path, crop_image_path) {
        return Ok(Some(LmsUploadReportAsset {
            question_id: answer.question_id.clone(),
            local_asset_name,
            asset_fingerprint: file_sha256_hex(&local_file_path)?,
            local_file_path: local_file_path.to_string_lossy().into_owned(),
        }));
    }

    let Some(parent) = local_file_path.parent() else {
        return Ok(None);
    };
    fs::create_dir_all(parent).map_err(|err| {
        HostError::Project(format!(
            "Could not create Results/LMS report asset directory '{}': {err}",
            parent.display()
        ))
    })?;

    let Ok((_, encoded)) = decode_and_resize_image(crop_image_path) else {
        return Ok(None);
    };
    fs::write(&local_file_path, encoded).map_err(|err| {
        HostError::Project(format!(
            "Could not write Results/LMS report asset '{}': {err}",
            local_file_path.display()
        ))
    })?;

    Ok(Some(LmsUploadReportAsset {
        question_id: answer.question_id.clone(),
        local_asset_name,
        asset_fingerprint: file_sha256_hex(&local_file_path)?,
        local_file_path: local_file_path.to_string_lossy().into_owned(),
    }))
}

fn cached_report_asset_is_current(path: &Path, crop_image_path: &str) -> bool {
    if !path.exists() || !Path::new(crop_image_path).exists() {
        return false;
    }
    let cached_is_decodable = ImageReader::open(path)
        .ok()
        .and_then(|reader| reader.decode().ok())
        .is_some();
    if !cached_is_decodable {
        return false;
    }
    source_crop_is_not_newer(path, crop_image_path)
}

fn source_crop_is_not_newer(cached_path: &Path, crop_image_path: &str) -> bool {
    let Ok(cached_metadata) = fs::metadata(cached_path) else {
        return false;
    };
    let Ok(crop_metadata) = fs::metadata(crop_image_path) else {
        return false;
    };
    let Ok(cached_modified) = cached_metadata.modified() else {
        return false;
    };
    let Ok(crop_modified) = crop_metadata.modified() else {
        return false;
    };
    cached_modified >= crop_modified
}

fn file_sha256_hex(path: &Path) -> HostResult<String> {
    let bytes = fs::read(path).map_err(|err| {
        HostError::Project(format!(
            "Could not read Results/LMS report asset '{}': {err}",
            path.display()
        ))
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hex::encode(hasher.finalize()))
}

fn report_asset_path(project_path: &Path, student_ref: &str, local_asset_name: &str) -> PathBuf {
    project_path
        .join(ARTIFACTS_DIR_NAME)
        .join(REPORT_IMAGE_ARTIFACT_DIR)
        .join(sanitize_asset_component(student_ref))
        .join(local_asset_name)
}

fn sanitize_asset_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn preview_asset_sources(
    report_assets: &[LmsUploadReportAsset],
) -> HostResult<HashMap<String, String>> {
    let mut sources = HashMap::new();
    for asset in report_assets {
        let bytes = fs::read(&asset.local_file_path).map_err(|err| {
            HostError::Project(format!(
                "Could not read Results/LMS report asset '{}': {err}",
                asset.local_file_path
            ))
        })?;
        sources.insert(
            asset.local_asset_name.clone(),
            format!(
                "data:image/jpeg;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ),
        );
    }
    Ok(sources)
}

fn report_asset_placeholder(asset_name: &str) -> String {
    format!("__RESULTS_LMS_ASSET_{asset_name}__")
}

fn wrap_generated_report_comment(report_html: &str, result_fingerprint: Option<&str>) -> String {
    let fingerprint = result_fingerprint
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("pending");
    let hidden_marker = format!(
        "<div aria-hidden=\"true\" style=\"display:none;font-size:0;line-height:0;color:transparent;\">{REPORT_MARKER}; fingerprint={fingerprint}</div>"
    );
    format!(
        "{REPORT_ZERO_WIDTH_MARKER}<!-- {REPORT_MARKER}; fingerprint={fingerprint} -->\n{hidden_marker}\n{report_html}"
    )
}

fn build_report_html(
    exam_title: &str,
    row: &ResultStudentRow,
    submission: &StudentWorkflowSubmission,
    questions_by_id: &HashMap<&str, &QuestionRecord>,
    image_src_by_question_id: &HashMap<&str, String>,
) -> String {
    let answers_by_question = submission
        .answers
        .iter()
        .map(|answer| (answer.question_id.as_str(), answer))
        .collect::<HashMap<_, _>>();

    let mut sections = String::new();
    for question_row in &row.question_rows {
        sections.push_str(&build_question_section(
            question_row,
            answers_by_question
                .get(question_row.question_id.as_str())
                .copied(),
            questions_by_id
                .get(question_row.question_id.as_str())
                .copied(),
            image_src_by_question_id
                .get(question_row.question_id.as_str())
                .map(String::as_str),
        ));
    }

    let total_label = if row.aggregate_complete {
        let total_max_points = row
            .question_rows
            .iter()
            .map(|question_row| question_row.max_points)
            .collect::<Option<Vec<_>>>()
            .map(|values| values.into_iter().sum::<i64>());
        match total_max_points {
            Some(max_points) if max_points > 0 => format!("{} / {max_points}", row.aggregate_total),
            _ => row.aggregate_total.to_string(),
        }
    } else {
        "Pending".to_string()
    };

    format!(
        concat!(
            "<!doctype html>",
            "<html><head><meta charset=\"utf-8\"><title>{title}</title></head>",
            "<body style=\"margin:0;background:#ffffff;color:#111827;font-family:Inter,Arial,sans-serif;line-height:1.45;\">",
            "<main style=\"max-width:980px;margin:0 auto;padding:28px 28px 40px;\">",
            "<h1 style=\"margin:0 0 10px;font-size:26px;line-height:1.2;\">{title}</h1>",
            "<p style=\"margin:0 0 28px;font-size:16px;font-weight:700;\">Total Score: {total_label}</p>",
            "{sections}",
            "</main></body></html>"
        ),
        title = escape_html(exam_title),
        total_label = escape_html(&total_label),
        sections = sections,
    )
}

fn build_question_section(
    question_row: &ResultQuestionRow,
    answer: Option<&StudentWorkflowAnswer>,
    question: Option<&QuestionRecord>,
    image_src: Option<&str>,
) -> String {
    let prompt = question
        .map(|item| item.text.trim())
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("Question {}", question_row.question_number));
    let score_label = match (
        question_row.effective_total_points,
        question_row
            .max_points
            .or_else(|| question.and_then(|item| item.max_points)),
    ) {
        (Some(points), Some(max_points)) => format!("{points} / {max_points}"),
        (Some(points), None) => points.to_string(),
        (None, _) => "Pending".to_string(),
    };
    let blocked_html = question_row
        .blocked_reason
        .as_deref()
        .map(|reason| {
            format!(
                "<p style=\"margin:16px 0 0;font-size:14px;color:#92400e;\"><strong>Blocked:</strong> {}</p>",
                escape_html(reason)
            )
        })
        .unwrap_or_default();
    let feedback_html = if question_row.effective_feedback_text.trim().is_empty() {
        "<p style=\"margin:16px 0 0;font-size:16px;\"><strong>Feedback:</strong> No feedback available.</p>"
            .to_string()
    } else {
        format!(
            "<p style=\"margin:16px 0 0;font-size:16px;\"><strong>Feedback:</strong> {}</p>",
            escape_preserving_line_breaks(&question_row.effective_feedback_text)
        )
    };
    let image_html = if answer.is_some() {
        image_src
            .map(|src| {
                format!(
                    "<img src=\"{src}\" alt=\"Question {} answer image\" style=\"display:block;max-width:100%;height:auto;margin:18px 0 0;border:1px solid #d1d5db;border-radius:8px;\" />",
                    question_row.question_number
                )
            })
            .unwrap_or_default()
    } else {
        String::new()
    };

    format!(
        concat!(
            "<section style=\"padding:0 0 28px;\">",
            "<hr style=\"border:none;border-top:1px solid #d1d5db;margin:0 0 22px;\" />",
            "<h2 style=\"margin:0 0 12px;font-size:18px;line-height:1.35;\">Question {number}: {prompt}</h2>",
            "<p style=\"margin:0;font-size:16px;font-weight:700;\">Score: {score_label}</p>",
            "{blocked_html}",
            "{image_html}",
            "{feedback_html}",
            "</section>"
        ),
        number = question_row.question_number,
        prompt = escape_html(&prompt),
        score_label = escape_html(&score_label),
        blocked_html = blocked_html,
        image_html = image_html,
        feedback_html = feedback_html,
    )
}

fn decode_and_resize_image(image_path: &str) -> HostResult<(&'static str, Vec<u8>)> {
    let reader = ImageReader::open(image_path).map_err(|err| {
        HostError::Project(format!(
            "Could not open answer image '{}': {err}",
            image_path
        ))
    })?;
    let image = reader.decode().map_err(|err| {
        HostError::Project(format!(
            "Could not decode answer image '{}': {err}",
            image_path
        ))
    })?;
    encode_image_for_embed(image, image_path)
}

fn encode_image_for_embed(
    image: DynamicImage,
    image_path: &str,
) -> HostResult<(&'static str, Vec<u8>)> {
    let base = downscale_for_embed(image);
    let mut smallest = None::<Vec<u8>>;

    for (width, height, quality) in ENCODE_ATTEMPTS {
        let candidate = resize_for_attempt(&base, width, height);
        let encoded = encode_jpeg(&candidate, quality, image_path)?;
        if encoded.len() <= MAX_EMBEDDED_IMAGE_BYTES {
            return Ok(("image/jpeg", encoded));
        }
        if smallest
            .as_ref()
            .is_none_or(|current| encoded.len() < current.len())
        {
            smallest = Some(encoded);
        }
    }

    Ok((
        "image/jpeg",
        smallest.unwrap_or_else(|| Vec::with_capacity(0)),
    ))
}

fn encode_jpeg(image: &DynamicImage, quality: u8, image_path: &str) -> HostResult<Vec<u8>> {
    let mut output = Cursor::new(Vec::new());
    let rgb = image.to_rgb8();
    let mut encoder = JpegEncoder::new_with_quality(&mut output, quality);
    encoder
        .encode_image(&DynamicImage::ImageRgb8(rgb))
        .map_err(|err| {
            HostError::Project(format!(
                "Could not encode answer image '{}' as JPEG: {err}",
                image_path
            ))
        })?;
    Ok(output.into_inner())
}

fn downscale_for_embed(image: DynamicImage) -> DynamicImage {
    if image.width() <= MAX_EMBEDDED_IMAGE_WIDTH_PX
        && image.height() <= MAX_EMBEDDED_IMAGE_HEIGHT_PX
    {
        return image;
    }
    image.resize(
        MAX_EMBEDDED_IMAGE_WIDTH_PX,
        MAX_EMBEDDED_IMAGE_HEIGHT_PX,
        FilterType::CatmullRom,
    )
}

fn resize_for_attempt(image: &DynamicImage, width: u32, height: u32) -> DynamicImage {
    if image.width() <= width && image.height() <= height {
        return image.clone();
    }
    image.resize(width, height, FilterType::Triangle)
}

fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(character),
        }
    }
    out
}

fn escape_preserving_line_breaks(value: &str) -> String {
    escape_html(value).replace('\n', "<br />")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use image::{DynamicImage, ImageBuffer, Rgba};

    use crate::models::{
        ExamWorkspaceState, ProjectSummary, QuestionRecord, ResultQuestionRow, ResultStudentRow,
        StudentWorkflowAnswer, StudentWorkflowState, StudentWorkflowSubmission,
    };

    use super::{
        build_canvas_report_comment_template, build_report_preview, build_report_upload_materials,
        canvas_comment_contains_generated_report, downscale_for_embed, encode_image_for_embed,
        prepare_report_assets, report_asset_prewarm_worker_count, substitute_report_asset_sources,
        MAX_EMBEDDED_IMAGE_BYTES, MAX_EMBEDDED_IMAGE_HEIGHT_PX, MAX_EMBEDDED_IMAGE_WIDTH_PX,
        MAX_REPORT_ASSET_PREWARM_WORKERS,
    };

    fn temp_path(prefix: &str, extension: &str) -> String {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_millis();
        std::env::temp_dir()
            .join(format!("{prefix}-{unique}.{extension}"))
            .to_string_lossy()
            .into_owned()
    }

    fn temp_dir(prefix: &str) -> String {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_millis();
        let dir = std::env::temp_dir().join(format!("{prefix}-{unique}"));
        fs::create_dir_all(&dir).expect("temp dir should create");
        dir.to_string_lossy().into_owned()
    }

    fn write_sample_png(path: &str, width: u32, height: u32) {
        let image =
            ImageBuffer::from_fn(width, height, |_x, _y| Rgba([32_u8, 64_u8, 96_u8, 255_u8]));
        image.save(path).expect("sample png should save");
    }

    fn workspace_state(project_path: &str, image_path: &str) -> ExamWorkspaceState {
        ExamWorkspaceState {
            project: ProjectSummary {
                project_id: "proj_1".into(),
                display_name: "Spring 2026 - CS145 - Midterm".into(),
                subject: Some("Computer Science".into()),
                course_code: Some("CS145".into()),
                lms_course_id: None,
                project_path: project_path.into(),
                created_at: "1".into(),
                updated_at: "1".into(),
            },
            status: "approved".into(),
            status_label: "Approved".into(),
            failure_message: None,
            template_preview_artifacts: Vec::new(),
            aruco_status: Default::default(),
            questions: vec![QuestionRecord {
                question_id: "question_1".into(),
                question_number: 1,
                page_number: 1,
                max_points: Some(5),
                text: "Give an example of a primitive data type.".into(),
                baseline_pdf_text: "Give an example of a primitive data type.".into(),
                region: None,
                source_artifact_id: None,
                image_path: None,
                analysis: Default::default(),
                rubric: Default::default(),
            }],
            redaction_regions: Vec::new(),
            warnings: Vec::new(),
            can_approve: true,
            can_approve_rubric: true,
            project_config: Default::default(),
            student_roster: Vec::new(),
            student_intake: Default::default(),
            student_workflow: StudentWorkflowState {
                status: "graded".into(),
                latest_job_id: None,
                submissions: vec![StudentWorkflowSubmission {
                    student_ref: "student_1".into(),
                    canonical_pdf_path: "/tmp/student_1.pdf".into(),
                    page_count: 1,
                    stage: "graded".into(),
                    latest_job_id: None,
                    failure_message: None,
                    warnings: Vec::new(),
                    page_artifacts: Vec::new(),
                    alignment_pages: Vec::new(),
                    detect_review: None,
                    answers: vec![StudentWorkflowAnswer {
                        question_id: "question_1".into(),
                        question_number: 1,
                        crop_image_path: Some(image_path.into()),
                        pii_prescreen: None,
                        manual_grading_required: false,
                        manual_grading_reason: None,
                        moderation_eligible: true,
                        parse_status: "ok".into(),
                        parse_confidence: Some("high".into()),
                        parse_confidence_source: Some("combined".into()),
                        raw_parsed_text: None,
                        verified_text: None,
                        review_required: false,
                        verified: true,
                        stale: false,
                        grading_status: "draft_ready".into(),
                        grading_confidence: Some("high".into()),
                        grading_confidence_reason: None,
                        question_max_points: Some(5),
                        total_points_awarded: Some(5),
                        feedback_text: Some("Correct.".into()),
                        criterion_results: Vec::new(),
                        highlights: Vec::new(),
                        warnings: Vec::new(),
                    }],
                }],
            },
            moderation_state: Default::default(),
            results_lms_state: Default::default(),
            results_lms_rows: vec![ResultStudentRow {
                student_ref: "student_1".into(),
                aggregate_total: 100,
                aggregate_complete: true,
                ready_to_finalize: true,
                blocked_reasons: Vec::new(),
                question_rows: vec![ResultQuestionRow {
                    question_id: "question_1".into(),
                    question_number: 1,
                    max_points: Some(5),
                    effective_total_points: Some(5),
                    effective_feedback_text: "Correct.".into(),
                    uses_moderated_total: false,
                    uses_moderated_feedback: false,
                    blocked_reason: None,
                }],
                result_fingerprint: Some("fp_1".into()),
                finalized: true,
                stale_finalization: false,
                finalized_at: Some("1".into()),
                uploaded: false,
                upload_failed: false,
                latest_upload_error: None,
                last_upload_attempt_id: None,
            }],
            results_lms_metrics: None,
            results_lms_review_summary: None,
            workflow_stage: "results_upload_ready".into(),
            workflow_label: "Ready".into(),
        }
    }

    #[test]
    fn report_preview_embeds_the_generated_report_html() {
        let project_path = temp_dir("scriptscore-results-preview");
        let image_path = temp_path("scriptscore-results-report", "png");
        write_sample_png(&image_path, 1280, 900);
        let workspace = workspace_state(&project_path, &image_path);

        let preview = build_report_preview(&workspace, "student_1").expect("preview should build");

        assert_eq!(preview.student_ref, "student_1");
        assert_eq!(preview.result_fingerprint.as_deref(), Some("fp_1"));
        assert!(preview.html.starts_with("\u{2063}\u{2063}"));
        assert!(preview.html.contains("Question 1"));
        assert!(preview.html.contains("data:image/jpeg;base64,"));
        assert!(preview.html.contains("<!-- results-report:v1;"));
        assert!(preview.html.contains("display:none"));

        let _ = fs::remove_file(&image_path);
        let _ = fs::remove_dir_all(&project_path);
    }

    #[test]
    fn canvas_report_detection_matches_marker_only() {
        assert!(canvas_comment_contains_generated_report(
            "<!-- results-report:v1 -->"
        ));
        assert!(canvas_comment_contains_generated_report(
            "\u{2063}\u{2063}\u{200B}\u{200D}\u{2060}\u{200C}\u{2063}\u{200B}<p>anything</p>"
        ));
        assert!(canvas_comment_contains_generated_report(
            "<div style=\"display:none\">results-report:v1; fingerprint=fp_1</div>"
        ));
        assert!(!canvas_comment_contains_generated_report("Teacher comment"));
    }

    #[test]
    fn build_report_upload_materials_uses_placeholders_for_assets() {
        let project_path = temp_dir("scriptscore-results-upload-materials");
        let image_path = temp_path("scriptscore-results-comment", "png");
        write_sample_png(&image_path, 40, 40);
        let workspace = workspace_state(&project_path, &image_path);
        let row = &workspace.results_lms_rows[0];
        let submission = &workspace.student_workflow.submissions[0];
        let questions_by_id = workspace
            .questions
            .iter()
            .map(|question| (question.question_id.as_str(), question))
            .collect::<HashMap<_, _>>();

        let (template, assets) = build_report_upload_materials(
            Path::new(&project_path),
            &workspace.project.display_name,
            row,
            submission,
            &questions_by_id,
        )
        .expect("upload materials should build");

        assert_eq!(assets.len(), 1);
        assert!(template.contains("__RESULTS_LMS_ASSET_question_1.jpg__"));

        let sources = HashMap::from([(
            assets[0].local_asset_name.clone(),
            "https://canvas.example/files/1/download?download_frd=1".to_string(),
        )]);
        let rendered = substitute_report_asset_sources(&template, &sources);
        assert!(rendered.contains("https://canvas.example/files/1/download?download_frd=1"));

        let _ = fs::remove_file(&image_path);
        let _ = fs::remove_dir_all(&project_path);
    }

    #[test]
    fn prepare_report_assets_reuses_cached_jpeg() {
        let project_path = temp_dir("scriptscore-results-asset-cache");
        let image_path = temp_path("scriptscore-results-cache-source", "png");
        write_sample_png(&image_path, 300, 200);
        let workspace = workspace_state(&project_path, &image_path);
        let submission = &workspace.student_workflow.submissions[0];

        let first_assets =
            prepare_report_assets(Path::new(&project_path), submission).expect("first asset pass");
        let metadata_before = fs::metadata(&first_assets[0].local_file_path)
            .expect("cached jpeg should exist")
            .modified()
            .expect("jpeg modified time should exist");
        let second_assets =
            prepare_report_assets(Path::new(&project_path), submission).expect("second asset pass");
        let metadata_after = fs::metadata(&second_assets[0].local_file_path)
            .expect("cached jpeg should still exist")
            .modified()
            .expect("jpeg modified time should exist");

        assert_eq!(
            first_assets[0].local_asset_name,
            second_assets[0].local_asset_name
        );
        assert_eq!(metadata_before, metadata_after);

        let _ = fs::remove_file(&image_path);
        let _ = fs::remove_dir_all(&project_path);
    }

    #[test]
    fn prepare_report_assets_refreshes_cached_jpeg_when_crop_changes() {
        let project_path = temp_dir("scriptscore-results-asset-refresh");
        let image_path = temp_path("scriptscore-results-refresh-source", "png");
        write_sample_png(&image_path, 300, 200);
        let workspace = workspace_state(&project_path, &image_path);
        let submission = &workspace.student_workflow.submissions[0];

        let first_assets =
            prepare_report_assets(Path::new(&project_path), submission).expect("first asset pass");
        let first_fingerprint = first_assets[0].asset_fingerprint.clone();
        std::thread::sleep(Duration::from_millis(20));
        write_sample_png(&image_path, 640, 420);

        let second_assets =
            prepare_report_assets(Path::new(&project_path), submission).expect("second asset pass");

        assert_eq!(
            first_assets[0].local_asset_name,
            second_assets[0].local_asset_name
        );
        assert_ne!(first_fingerprint, second_assets[0].asset_fingerprint);

        let _ = fs::remove_file(&image_path);
        let _ = fs::remove_dir_all(&project_path);
    }

    #[test]
    fn canvas_comment_template_wraps_report_with_marker() {
        let project_path = temp_dir("scriptscore-results-comment-template");
        let image_path = temp_path("scriptscore-results-template", "png");
        write_sample_png(&image_path, 40, 40);
        let workspace = workspace_state(&project_path, &image_path);
        let row = &workspace.results_lms_rows[0];
        let submission = &workspace.student_workflow.submissions[0];
        let questions_by_id = workspace
            .questions
            .iter()
            .map(|question| (question.question_id.as_str(), question))
            .collect::<HashMap<_, _>>();
        let report_assets =
            prepare_report_assets(Path::new(&project_path), submission).expect("assets");

        let comment = build_canvas_report_comment_template(
            &workspace.project.display_name,
            row,
            submission,
            &questions_by_id,
            &report_assets,
        );

        assert!(comment.starts_with("\u{2063}\u{2063}"));
        assert!(comment.contains("<!-- results-report:v1;"));
        assert!(comment.contains("display:none"));
        assert!(!comment.contains("ScriptScore"));

        let _ = fs::remove_file(&image_path);
        let _ = fs::remove_dir_all(&project_path);
    }

    #[test]
    fn downscale_caps_large_images_before_embedding() {
        let image = DynamicImage::ImageRgba8(ImageBuffer::from_fn(1400, 1000, |_x, _y| {
            Rgba([255_u8, 255_u8, 255_u8, 255_u8])
        }));

        let resized = downscale_for_embed(image);

        assert!(resized.width() <= MAX_EMBEDDED_IMAGE_WIDTH_PX);
        assert!(resized.height() <= MAX_EMBEDDED_IMAGE_HEIGHT_PX);
    }

    #[test]
    fn embedded_image_encoding_targets_fifty_kilobytes_or_less() {
        let image = DynamicImage::ImageRgba8(ImageBuffer::from_fn(1400, 1000, |x, y| {
            let red = ((x * 37 + y * 17) % 255) as u8;
            let green = ((x * 13 + y * 29) % 255) as u8;
            let blue = ((x * 19 + y * 11) % 255) as u8;
            Rgba([red, green, blue, 255_u8])
        }));

        let (mime_type, encoded) =
            encode_image_for_embed(downscale_for_embed(image), "fixture.png").unwrap();

        assert_eq!(mime_type, "image/jpeg");
        assert!(encoded.len() <= MAX_EMBEDDED_IMAGE_BYTES);
    }

    #[test]
    fn report_asset_prewarm_worker_count_stays_bounded_by_submission_count_and_limit() {
        assert_eq!(report_asset_prewarm_worker_count(1), 1);
        assert_eq!(report_asset_prewarm_worker_count(2), 2);
        assert!(report_asset_prewarm_worker_count(16) <= MAX_REPORT_ASSET_PREWARM_WORKERS);
    }
}
