// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::HashMap;

use serde_json::{json, Map, Value};

use crate::errors::{HostError, HostResult};
use crate::models::{
    ExamWorkspaceState, QuestionRecord, StudentWorkflowAlignmentPage, StudentWorkflowAnswer,
    StudentWorkflowCriterionResult, StudentWorkflowDetectRegion, StudentWorkflowDetectReview,
    StudentWorkflowDetectReviewRow, StudentWorkflowHighlightSpan, StudentWorkflowPage,
    StudentWorkflowPiiPrescreen, StudentWorkflowSubmission, WorkspaceWarning,
};
use crate::worker::CompletedWorkerJob;

use super::shared::{
    parse_warnings, required_array, required_f64_object, required_i64, required_i64_object,
    required_string, required_string_object, success_data,
};
use super::{cli_instructor_profile_json, StudentWorkflowTransform};

pub(super) fn parse_alignment_pages(
    completed: &CompletedWorkerJob,
) -> HostResult<Vec<StudentWorkflowAlignmentPage>> {
    let data = success_data(&completed.result.envelope)?;
    let rows = required_array(data, "alignment_results")?;
    rows.iter().map(alignment_page_from_row).collect()
}

fn alignment_page_from_row(row: &Value) -> HostResult<StudentWorkflowAlignmentPage> {
    let status = required_string(row, "status")?;
    let transform = alignment_transform_from_row(row, &status)?;
    Ok(StudentWorkflowAlignmentPage {
        page_number: required_i64(row, "page_number")?,
        confidence: row.get("confidence").and_then(Value::as_f64),
        low_confidence: status == "low_confidence" || status == "failed",
        review_exempt: false,
        review_exempt_reason: None,
        question_count: 0,
        transform,
        warnings: parse_warnings(row.get("warnings"))?,
    })
}

fn alignment_transform_from_row(row: &Value, status: &str) -> HostResult<StudentWorkflowTransform> {
    let Some(transform) = row.get("transform").and_then(Value::as_object) else {
        if status == "failed" {
            return Ok(StudentWorkflowTransform {
                rotation: 0.0,
                scale: 1.0,
                translate_x: 0.0,
                translate_y: 0.0,
            });
        }
        return Err(HostError::Protocol(
            "Alignment result was missing transform.".into(),
        ));
    };
    Ok(StudentWorkflowTransform {
        rotation: required_f64_object(transform, "rotation")?,
        scale: required_f64_object(transform, "scale")?,
        translate_x: required_f64_object(transform, "translate_x")?,
        translate_y: required_f64_object(transform, "translate_y")?,
    })
}

pub(super) fn template_pages_as_cli(workspace: &ExamWorkspaceState) -> Vec<Value> {
    workspace
        .template_preview_artifacts
        .iter()
        .map(|page| {
            json!({
                "page_type": "template",
                "page_number": page.page_number,
                "image_path": page.image_path,
            })
        })
        .collect()
}

pub(super) fn intake_pages_as_cli(
    intake: &crate::models::StudentIntakeSummary,
) -> HostResult<Vec<Value>> {
    intake
        .exam_page_paths
        .iter()
        .enumerate()
        .map(|(idx, image_path)| {
            Ok(json!({
                "page_type": "student_scan",
                "page_number": (idx as i64) + 1,
                "image_path": image_path,
                "student_ref": intake.student_ref,
                "source_pdf_path": intake.canonical_pdf_path,
            }))
        })
        .collect()
}

pub(super) fn build_canonicalize_targets(
    workspace: &ExamWorkspaceState,
    intake: &crate::models::StudentIntakeSummary,
    submission: &StudentWorkflowSubmission,
) -> HostResult<Vec<Value>> {
    let template_pages_by_number = workspace
        .template_preview_artifacts
        .iter()
        .map(|page| (page.page_number, page))
        .collect::<HashMap<_, _>>();
    submission
        .alignment_pages
        .iter()
        .map(|page| {
            let template_page =
                template_pages_by_number
                    .get(&page.page_number)
                    .ok_or_else(|| {
                        HostError::Validation(format!(
                            "Template page '{}' was missing for canonicalization.",
                            page.page_number
                        ))
                    })?;
            Ok(json!({
                "page": intake_page_cli(intake, page.page_number)?,
                "template_page": {
                    "page_type": "template",
                    "page_number": template_page.page_number,
                    "image_path": template_page.image_path,
                },
                "transform": {
                    "rotation": page.transform.rotation,
                    "scale": page.transform.scale,
                    "translate_x": page.transform.translate_x,
                    "translate_y": page.transform.translate_y,
                }
            }))
        })
        .collect()
}

pub(super) fn intake_page_cli(
    intake: &crate::models::StudentIntakeSummary,
    page_number: i64,
) -> HostResult<Value> {
    let idx = usize::try_from(page_number - 1).map_err(|_| {
        HostError::Validation(format!(
            "Invalid page number '{}' for '{}'.",
            page_number, intake.student_ref
        ))
    })?;
    let image_path = intake.exam_page_paths.get(idx).ok_or_else(|| {
        HostError::Validation(format!(
            "Missing ingested page {} for '{}'.",
            page_number, intake.student_ref
        ))
    })?;
    Ok(json!({
        "page_type": "student_scan",
        "page_number": page_number,
        "image_path": image_path,
        "student_ref": intake.student_ref,
        "source_pdf_path": intake.canonical_pdf_path,
    }))
}

pub(super) fn student_workflow_pages_as_cli(
    pages: &[StudentWorkflowPage],
    student_ref: &str,
) -> Vec<Value> {
    pages
        .iter()
        .map(|page| {
            json!({
                "page_type": "student_scan",
                "page_number": page.page_number,
                "image_path": page.image_path,
                "student_ref": student_ref,
                "source_pdf_path": page.source_pdf_path,
            })
        })
        .collect()
}

pub(super) fn parse_canonicalized_pages(
    data: &serde_json::Map<String, Value>,
) -> HostResult<Vec<StudentWorkflowPage>> {
    let rows = required_array(data, "canonicalize_results")?;
    rows.iter()
        .filter(|row| matches!(row.get("status").and_then(Value::as_str), Some("ok")))
        .map(|row| {
            let output_page = row
                .get("output_page")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    HostError::Protocol("Canonicalize result was missing output_page.".into())
                })?;
            Ok(StudentWorkflowPage {
                page_number: required_i64_object(output_page, "page_number")?,
                image_path: required_string_object(output_page, "image_path")?,
                source_pdf_path: output_page
                    .get("source_pdf_path")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                ocr_metadata_path: None,
            })
        })
        .collect()
}

pub(super) fn apply_detect_page_ocr_results(
    submission: &mut StudentWorkflowSubmission,
    data: &serde_json::Map<String, Value>,
) -> HostResult<()> {
    let rows = required_array(data, "page_ocr_results")?;
    let ocr_by_page = rows
        .iter()
        .map(|row| {
            Ok((
                required_i64(row, "page_number")?,
                required_string(row, "ocr_metadata_path")?,
            ))
        })
        .collect::<HostResult<HashMap<_, _>>>()?;
    for page in &mut submission.page_artifacts {
        page.ocr_metadata_path = ocr_by_page.get(&page.page_number).cloned();
    }
    Ok(())
}

pub(super) fn build_detect_targets(
    workspace: &ExamWorkspaceState,
    submission: &StudentWorkflowSubmission,
) -> HostResult<Vec<Value>> {
    let mut hints_by_page = HashMap::<i64, Vec<Value>>::new();
    for question in &workspace.questions {
        let region = question.region.as_ref().ok_or_else(|| {
            HostError::Validation(format!(
                "Question '{}' is missing template region geometry required for scan detection.",
                question.question_id
            ))
        })?;
        let question_text_hint = question
            .analysis
            .question_text_clean
            .clone()
            .unwrap_or_else(|| question.baseline_pdf_text.clone());
        hints_by_page
            .entry(question.page_number)
            .or_default()
            .push(json!({
                "question_id": question.question_id,
                "question_number": question.question_number,
                "question_label": question_label(question),
                "template_region": {
                    "x": region.x,
                    "y": region.y,
                    "width": region.width,
                    "height": region.height,
                    "units": "rendered_page_pixels",
                },
                "question_text_hint": question_text_hint,
            }));
    }
    Ok(submission
        .page_artifacts
        .iter()
        .filter_map(|page| {
            hints_by_page.get(&page.page_number).map(|question_hints| {
                let mut page_value = Map::new();
                page_value.insert("page_type".into(), Value::String("student_scan".into()));
                page_value.insert("page_number".into(), Value::from(page.page_number));
                page_value.insert("image_path".into(), Value::String(page.image_path.clone()));
                page_value.insert(
                    "student_ref".into(),
                    Value::String(submission.student_ref.clone()),
                );
                if let Some(source_pdf_path) = page.source_pdf_path.as_ref() {
                    page_value.insert(
                        "source_pdf_path".into(),
                        Value::String(source_pdf_path.clone()),
                    );
                }

                let mut target = Map::new();
                target.insert("page".into(), Value::Object(page_value));
                target.insert(
                    "question_hints".into(),
                    Value::Array(question_hints.clone()),
                );
                if let Some(path) = page.ocr_metadata_path.as_ref() {
                    target.insert("ocr_metadata_path".into(), Value::String(path.clone()));
                }
                Value::Object(target)
            })
        })
        .collect())
}

pub(super) fn build_crop_targets(data: &serde_json::Map<String, Value>) -> HostResult<Vec<Value>> {
    let rows = required_array(data, "detect_results")?;
    rows.iter()
        .filter(|row| {
            matches!(row.get("status").and_then(Value::as_str), Some("ok"))
                && matches!(
                    row.get("region_source").and_then(Value::as_str),
                    Some("ocr_refined")
                )
        })
        .map(|row| {
            let mut target = json!({
                "question_id": required_string(row, "question_id")?,
                "page_number": required_i64(row, "page_number")?,
                "region": row
                    .get("region")
                    .cloned()
                    .ok_or_else(|| HostError::Protocol("Detect result was missing region.".into()))?,
            });
            if let Some(student_ref) = row.get("student_ref").and_then(Value::as_str) {
                target["student_ref"] = Value::String(student_ref.to_string());
            }
            Ok(target)
        })
        .collect()
}

pub(super) fn build_detect_review(
    workspace: &ExamWorkspaceState,
    submission: &StudentWorkflowSubmission,
    data: &serde_json::Map<String, Value>,
) -> HostResult<Option<StudentWorkflowDetectReview>> {
    let rows = required_array(data, "detect_results")?;
    let trusted_crop_targets = build_crop_targets(data)?;
    let pending_rows = rows
        .iter()
        .filter(|row| detect_row_needs_review(row))
        .map(|row| detect_review_row(workspace, submission, row))
        .collect::<HostResult<Vec<_>>>()?;
    if pending_rows.is_empty() {
        return Ok(None);
    }
    Ok(Some(StudentWorkflowDetectReview {
        pending_rows,
        trusted_crop_targets,
    }))
}

fn detect_row_needs_review(row: &Value) -> bool {
    let status = row.get("status").and_then(Value::as_str);
    let source = row.get("region_source").and_then(Value::as_str);
    !matches!((status, source), (Some("ok"), Some("ocr_refined")))
}

fn detect_review_row(
    workspace: &ExamWorkspaceState,
    submission: &StudentWorkflowSubmission,
    row: &Value,
) -> HostResult<StudentWorkflowDetectReviewRow> {
    let question_id = required_string(row, "question_id")?;
    let page_number = required_i64(row, "page_number")?;
    let source_page_image_path = submission
        .page_artifacts
        .iter()
        .find(|page| page.page_number == page_number)
        .map(|page| page.image_path.clone())
        .ok_or_else(|| {
            HostError::Protocol(format!(
                "Detect review row referenced missing canonical page {page_number}."
            ))
        })?;
    let template_region = row
        .get("region")
        .map(region_from_value)
        .transpose()?
        .or_else(|| template_region_for_question(workspace, &question_id))
        .ok_or_else(|| {
            HostError::Protocol(format!(
                "Detect review row for question '{question_id}' was missing template geometry."
            ))
        })?;
    Ok(StudentWorkflowDetectReviewRow {
        question_id,
        page_number,
        source_page_image_path,
        template_region,
        warnings: parse_warnings(row.get("warnings"))?,
        resolved_region: None,
    })
}

fn question_label(question: &QuestionRecord) -> String {
    question
        .question_id
        .strip_prefix('q')
        .filter(|label| !label.is_empty() && label.chars().all(|ch| ch.is_ascii_alphanumeric()))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| question.question_number.to_string())
}

fn template_region_for_question(
    workspace: &ExamWorkspaceState,
    question_id: &str,
) -> Option<StudentWorkflowDetectRegion> {
    workspace.questions.iter().find_map(|question| {
        if question.question_id != question_id {
            return None;
        }
        let region = question.region.as_ref()?;
        Some(StudentWorkflowDetectRegion {
            x: region.x,
            y: region.y,
            width: region.width,
            height: region.height,
            units: "rendered_page_pixels".into(),
        })
    })
}

pub(super) fn crop_targets_from_detect_review(
    review: &StudentWorkflowDetectReview,
    student_ref: &str,
) -> HostResult<Vec<Value>> {
    let mut targets = review.trusted_crop_targets.clone();
    for row in &review.pending_rows {
        let region = row.resolved_region.as_ref().ok_or_else(|| {
            HostError::Validation(format!(
                "Question '{}' on page {} is missing a resolved detect region.",
                row.question_id, row.page_number
            ))
        })?;
        validate_detect_region(region)?;
        targets.push(json!({
            "student_ref": student_ref,
            "question_id": row.question_id,
            "page_number": row.page_number,
            "region": {
                "x": region.x,
                "y": region.y,
                "width": region.width,
                "height": region.height,
                "units": region.units,
            },
        }));
    }
    Ok(targets)
}

pub(super) fn validate_detect_region(region: &StudentWorkflowDetectRegion) -> HostResult<()> {
    if region.units != "rendered_page_pixels" {
        return Err(HostError::Validation(
            "Resolved detect region must use rendered_page_pixels units.".into(),
        ));
    }
    if region.x < 0 || region.y < 0 || region.width <= 0 || region.height <= 0 {
        return Err(HostError::Validation(
            "Resolved detect region must have non-negative origin and positive size.".into(),
        ));
    }
    Ok(())
}

fn region_from_value(value: &Value) -> HostResult<StudentWorkflowDetectRegion> {
    Ok(StudentWorkflowDetectRegion {
        x: required_i64(value, "x")?,
        y: required_i64(value, "y")?,
        width: required_i64(value, "width")?,
        height: required_i64(value, "height")?,
        units: value
            .get("units")
            .and_then(Value::as_str)
            .unwrap_or("rendered_page_pixels")
            .to_string(),
    })
}

pub(super) fn build_parse_targets(
    workspace: &ExamWorkspaceState,
    submission: &StudentWorkflowSubmission,
    crop_rows: &[Value],
) -> HostResult<Vec<Value>> {
    let questions_by_id = workspace
        .questions
        .iter()
        .map(|question| (question.question_id.as_str(), question))
        .collect::<HashMap<_, _>>();
    crop_rows
        .iter()
        .filter(|row| matches!(row.get("status").and_then(Value::as_str), Some("ok")))
        .map(|row| {
            let question_id = required_string(row, "question_id")?;
            let answer = workflow_answer(submission, &question_id)?;
            let question = questions_by_id.get(question_id.as_str()).ok_or_else(|| {
                HostError::Validation(format!(
                    "Question '{}' was missing from the workspace.",
                    question_id
                ))
            })?;
            let question_crop_path = row
                .get("question_crop_path")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    HostError::Protocol("Crop result was missing question_crop_path.".into())
                })?;
            let template_question_png_path = question.image_path.clone().ok_or_else(|| {
                HostError::Validation(format!(
                    "Question '{}' is missing template crop image.",
                    question_id
                ))
            })?;
            let mut target = json!({
                "student_ref": submission.student_ref,
                "question_id": question_id,
                "parse_question_context": {
                    "question_number": question.question_number,
                    "question_text_clean": question
                        .analysis
                        .question_text_clean
                        .clone()
                        .unwrap_or_else(|| question.baseline_pdf_text.clone()),
                },
                "question_crop_path": question_crop_path,
                "template_question_png_path": template_question_png_path,
            });
            if let Some(prescreen) = answer
                .pii_prescreen
                .as_ref()
                .filter(|item| pii_prescreen_can_skip_parse_handwriting_check(item))
            {
                target["pii_prescreen"] = pii_prescreen_json(prescreen);
            }
            Ok(target)
        })
        .collect()
}

pub(super) fn build_answers_from_pii_results(
    workspace: &ExamWorkspaceState,
    crop_rows: &[Value],
    pii_data: &serde_json::Map<String, Value>,
) -> HostResult<Vec<StudentWorkflowAnswer>> {
    let prescreens = pii_prescreens_by_question(pii_data)?;
    seed_answers_from_crop_rows(workspace, crop_rows, |question_id| {
        let prescreen = prescreens.get(question_id).cloned();
        let warnings = prescreen
            .as_ref()
            .map(|item| item.warnings.clone())
            .unwrap_or_default();
        let (manual_grading_required, manual_grading_reason, parse_status, grading_status) =
            match prescreen.as_ref() {
                Some(item) if pii_prescreen_allows_automated_progress(item) => (
                    false,
                    None,
                    "not_started".to_string(),
                    "not_started".to_string(),
                ),
                Some(item) if item.contains_pii => (
                    true,
                    Some("pii_detected".to_string()),
                    "blocked".to_string(),
                    "manual_required".to_string(),
                ),
                Some(_) => (
                    true,
                    Some("pii_ambiguous".to_string()),
                    "blocked".to_string(),
                    "manual_required".to_string(),
                ),
                None => (
                    true,
                    Some("pii_ambiguous".to_string()),
                    "blocked".to_string(),
                    "manual_required".to_string(),
                ),
            };
        AnswerSeedState {
            pii_prescreen: prescreen,
            manual_grading_required,
            manual_grading_reason,
            moderation_eligible: true,
            parse_status,
            grading_status,
            warnings,
        }
    })
}

pub(super) fn build_answers_for_manual_pii_block(
    workspace: &ExamWorkspaceState,
    crop_rows: &[Value],
    warning: WorkspaceWarning,
) -> HostResult<Vec<StudentWorkflowAnswer>> {
    seed_answers_from_crop_rows(workspace, crop_rows, |_question_id| AnswerSeedState {
        pii_prescreen: None,
        manual_grading_required: true,
        manual_grading_reason: Some("pii_ambiguous".to_string()),
        moderation_eligible: true,
        parse_status: "blocked".to_string(),
        grading_status: "manual_required".to_string(),
        warnings: vec![warning.clone()],
    })
}

pub(super) fn merge_parse_results_into_answers(
    submission: &mut StudentWorkflowSubmission,
    parse_data: &serde_json::Map<String, Value>,
) -> HostResult<()> {
    let rows = required_array(parse_data, "parse_results")?;
    for row in rows {
        let question_id = required_string(row, "question_id")?;
        let answer = submission
            .answers
            .iter_mut()
            .find(|item| item.question_id == question_id)
            .ok_or_else(|| {
                HostError::Validation(format!(
                    "Submission '{}' is missing answer state for '{}'.",
                    submission.student_ref, question_id
                ))
            })?;
        let parse_status = required_string(row, "status")?;
        let parse_confidence = row
            .get("confidence")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let parse_confidence_source = row
            .get("confidence_source")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let raw_parsed_text = row
            .get("parsed_text")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let warnings = parse_warnings(row.get("warnings"))?;
        let review_required =
            parse_review_required(&parse_status, parse_confidence.as_deref(), &warnings);
        answer.parse_status = parse_status.clone();
        answer.parse_confidence = parse_confidence;
        answer.parse_confidence_source = parse_confidence_source;
        answer.raw_parsed_text = raw_parsed_text.clone();
        answer.verified_text = if review_required {
            raw_parsed_text.clone()
        } else {
            raw_parsed_text.clone().or_else(|| Some(String::new()))
        };
        answer.review_required = review_required;
        answer.verified =
            !review_required && parse_status != "error" && parse_status != "cancelled";
        answer.stale = false;
        answer.warnings = warnings;
    }
    Ok(())
}

fn pii_prescreen_allows_automated_progress(prescreen: &StudentWorkflowPiiPrescreen) -> bool {
    !prescreen.contains_pii && matches!(prescreen.status.as_str(), "ok" | "warning")
}

fn pii_prescreen_can_skip_parse_handwriting_check(prescreen: &StudentWorkflowPiiPrescreen) -> bool {
    !prescreen.contains_pii && prescreen.status == "ok"
}

fn pii_prescreen_json(prescreen: &StudentWorkflowPiiPrescreen) -> Value {
    json!({
        "source_command": prescreen.source_command,
        "status": prescreen.status,
        "contains_handwriting": prescreen.contains_handwriting,
        "contains_pii": prescreen.contains_pii,
        "pii_types_detected": prescreen.pii_types_detected,
        "warnings": prescreen.warnings,
    })
}

fn pii_prescreens_by_question(
    pii_data: &serde_json::Map<String, Value>,
) -> HostResult<HashMap<String, StudentWorkflowPiiPrescreen>> {
    let rows = required_array(pii_data, "pii_results")?;
    rows.iter()
        .map(|row| {
            Ok((
                required_string(row, "question_id")?,
                StudentWorkflowPiiPrescreen {
                    source_command: "scans.pii".to_string(),
                    status: required_string(row, "status")?,
                    contains_handwriting: required_string(row, "contains_handwriting")?,
                    contains_pii: row
                        .get("contains_pii")
                        .and_then(Value::as_bool)
                        .ok_or_else(|| {
                            HostError::Protocol(
                                "PII result row was missing contains_pii.".to_string(),
                            )
                        })?,
                    pii_types_detected: row
                        .get("pii_types_detected")
                        .and_then(Value::as_array)
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(Value::as_str)
                                .map(ToOwned::to_owned)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default(),
                    warnings: parse_warnings(row.get("warnings"))?,
                },
            ))
        })
        .collect()
}

#[derive(Clone)]
struct AnswerSeedState {
    pii_prescreen: Option<StudentWorkflowPiiPrescreen>,
    manual_grading_required: bool,
    manual_grading_reason: Option<String>,
    moderation_eligible: bool,
    parse_status: String,
    grading_status: String,
    warnings: Vec<WorkspaceWarning>,
}

fn seed_answers_from_crop_rows<F>(
    workspace: &ExamWorkspaceState,
    crop_rows: &[Value],
    build_state: F,
) -> HostResult<Vec<StudentWorkflowAnswer>>
where
    F: Fn(&str) -> AnswerSeedState,
{
    let questions_by_id = workspace
        .questions
        .iter()
        .map(|question| (question.question_id.clone(), question))
        .collect::<HashMap<_, _>>();
    let mut answers = crop_rows
        .iter()
        .map(|row| {
            let question_id = required_string(row, "question_id")?;
            let question = questions_by_id.get(&question_id).ok_or_else(|| {
                HostError::Validation(format!(
                    "Question '{}' was missing from the workspace.",
                    question_id
                ))
            })?;
            let crop_path = row
                .get("question_crop_path")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let state = if matches!(row.get("status").and_then(Value::as_str), Some("ok")) {
                build_state(&question_id)
            } else {
                crop_failed_seed_state(row)?
            };
            Ok(StudentWorkflowAnswer {
                question_id,
                question_number: question.question_number,
                crop_image_path: crop_path,
                pii_prescreen: state.pii_prescreen,
                manual_grading_required: state.manual_grading_required,
                manual_grading_reason: state.manual_grading_reason,
                moderation_eligible: state.moderation_eligible,
                parse_status: state.parse_status,
                parse_confidence: None,
                parse_confidence_source: None,
                raw_parsed_text: None,
                verified_text: None,
                review_required: false,
                verified: false,
                stale: false,
                grading_status: state.grading_status,
                grading_confidence: None,
                grading_confidence_reason: None,
                question_max_points: question.max_points,
                total_points_awarded: None,
                feedback_text: None,
                criterion_results: Vec::new(),
                highlights: Vec::new(),
                warnings: state.warnings,
            })
        })
        .collect::<HostResult<Vec<_>>>()?;
    sort_answers_by_question_order(&mut answers);
    Ok(answers)
}

fn sort_answers_by_question_order(answers: &mut [StudentWorkflowAnswer]) {
    answers.sort_by(|left, right| {
        left.question_number
            .cmp(&right.question_number)
            .then_with(|| left.question_id.cmp(&right.question_id))
    });
}

fn crop_failed_seed_state(row: &Value) -> HostResult<AnswerSeedState> {
    let mut warnings = parse_warnings(row.get("warnings"))?;
    if warnings.is_empty() {
        warnings.push(WorkspaceWarning {
            code: Some("crop_failed".into()),
            message: "Question crop generation failed, so this answer requires manual grading."
                .into(),
            scope: Some("answer".into()),
        });
    }
    Ok(AnswerSeedState {
        pii_prescreen: None,
        manual_grading_required: true,
        manual_grading_reason: Some("crop_failed".to_string()),
        moderation_eligible: false,
        parse_status: "blocked".to_string(),
        grading_status: "manual_required".to_string(),
        warnings,
    })
}

pub(super) fn parse_review_required(
    parse_status: &str,
    parse_confidence: Option<&str>,
    warnings: &[crate::models::WorkspaceWarning],
) -> bool {
    if matches!(parse_confidence, Some("low")) {
        return true;
    }
    parse_status == "warning"
        || warnings.iter().any(|warning| {
            matches!(
                warning.code.as_deref(),
                Some("handwriting_verify_low_confidence" | "handwriting_verify_error_status")
            )
        })
}

pub(super) fn apply_preliminary_confidence_to_answers(
    submission: &mut StudentWorkflowSubmission,
    preliminary_rows: &[Value],
) -> HostResult<()> {
    let by_question = preliminary_rows_by_question(preliminary_rows)?;
    for answer in &mut submission.answers {
        let Some(rows) = by_question.get(&answer.question_id) else {
            continue;
        };
        let (confidence, reason) = aggregate_preliminary_confidence(rows);
        answer.grading_confidence = confidence;
        answer.grading_confidence_reason = reason;
    }
    Ok(())
}

pub(super) fn aggregate_preliminary_confidence(
    rows: &[&Value],
) -> (Option<String>, Option<String>) {
    let mut best_level = None;
    let mut best_reason = None;
    for row in rows {
        let Some(level) = row.get("confidence").and_then(Value::as_str) else {
            continue;
        };
        if !matches!(level, "low" | "medium" | "high") {
            continue;
        }
        let reason = row
            .get("confidence_reason")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        if preliminary_confidence_rank(level)
            < preliminary_confidence_rank(best_level.unwrap_or(""))
        {
            best_level = Some(level);
            best_reason = reason;
        }
    }
    (best_level.map(str::to_owned), best_reason)
}

fn preliminary_confidence_rank(level: &str) -> usize {
    match level {
        "low" => 0,
        "medium" => 1,
        "high" => 2,
        _ => 3,
    }
}

pub(super) fn build_preliminary_answer_score_requests(
    workspace: &ExamWorkspaceState,
    submission: &StudentWorkflowSubmission,
    question_by_id: &HashMap<String, &crate::models::QuestionRecord>,
) -> HostResult<Vec<Value>> {
    build_preliminary_answer_score_requests_where(workspace, submission, question_by_id, |answer| {
        answer.verified && !answer.stale
    })
}

pub(super) fn build_preliminary_answer_score_requests_for_stale_question(
    workspace: &ExamWorkspaceState,
    submission: &StudentWorkflowSubmission,
    question_by_id: &HashMap<String, &crate::models::QuestionRecord>,
    question_id: &str,
) -> HostResult<Vec<Value>> {
    build_preliminary_answer_score_requests_where(workspace, submission, question_by_id, |answer| {
        answer.question_id == question_id && answer.verified && answer.stale
    })
}

fn build_preliminary_answer_score_requests_where(
    workspace: &ExamWorkspaceState,
    submission: &StudentWorkflowSubmission,
    question_by_id: &HashMap<String, &crate::models::QuestionRecord>,
    include_answer: impl Fn(&crate::models::StudentWorkflowAnswer) -> bool,
) -> HostResult<Vec<Value>> {
    let subject = workspace
        .project
        .subject
        .clone()
        .unwrap_or_else(|| "Exam".into());
    let profile = cli_instructor_profile_json(&workspace.project_config.instructor_profile);
    let mut requests = Vec::new();
    for answer in &submission.answers {
        if !include_answer(answer) {
            continue;
        }
        let question = question_by_id.get(&answer.question_id).ok_or_else(|| {
            HostError::Validation(format!(
                "Question '{}' was missing from the workspace.",
                answer.question_id
            ))
        })?;
        let question_text_clean = question
            .analysis
            .question_text_clean
            .clone()
            .unwrap_or_else(|| question.baseline_pdf_text.clone());
        let context = cli_question_context_value(&question.analysis.question_context);
        let rubric_criteria = question
            .rubric
            .criteria
            .iter()
            .enumerate()
            .map(|(criterion_index, criterion)| {
                json!({
                    "criterion_index": criterion_index,
                    "label": criterion.label,
                    "points": criterion.points,
                    "partial_credit_guidance": criterion.partial_credit_guidance,
                })
            })
            .collect::<Vec<_>>();
        requests.push(json!({
            "student_ref": submission.student_ref,
            "question_id": answer.question_id,
            "subject": subject,
            "student_answer": answer.verified_text.clone().unwrap_or_default(),
            "question_text_clean": question_text_clean,
            "question_context": context,
            "rubric_criteria": rubric_criteria,
            "instructor_profile": profile,
        }));
    }
    Ok(requests)
}

pub(super) fn final_rows_from_preliminary(
    workspace: &ExamWorkspaceState,
    submission: &StudentWorkflowSubmission,
    question_by_id: &HashMap<String, &crate::models::QuestionRecord>,
    preliminary_rows: &[Value],
) -> HostResult<Vec<Value>> {
    let by_question = preliminary_rows_by_question(preliminary_rows)?;
    let mut final_rows = Vec::new();
    for (question_id, rows) in by_question {
        final_rows.push(final_grading_row(
            workspace,
            submission,
            question_by_id,
            &question_id,
            &rows,
        )?);
    }
    Ok(final_rows)
}

fn preliminary_rows_by_question(
    preliminary_rows: &[Value],
) -> HostResult<HashMap<String, Vec<&Value>>> {
    let mut by_question: HashMap<String, Vec<&Value>> = HashMap::new();
    for row in preliminary_rows {
        let question_id = required_string(row, "question_id")?;
        by_question.entry(question_id).or_default().push(row);
    }
    Ok(by_question)
}

fn final_grading_row(
    workspace: &ExamWorkspaceState,
    submission: &StudentWorkflowSubmission,
    question_by_id: &HashMap<String, &crate::models::QuestionRecord>,
    question_id: &str,
    rows: &[&Value],
) -> HostResult<Value> {
    let question = workflow_question(question_by_id, question_id)?;
    let answer = workflow_answer(submission, question_id)?;
    let criterion_results = criterion_results_with_totals(rows, question)?;
    Ok(json!({
        "student_ref": submission.student_ref,
        "question_id": question_id,
        "subject": workflow_subject(workspace),
        "question_text_clean": question_text_clean(question),
        "question_context": cli_question_context_value(&question.analysis.question_context),
        "rubric_criteria": rubric_criteria_json(question),
        "criterion_results": criterion_results.0,
        "total_points_awarded": criterion_results.1,
        "question_max_points": question.max_points.unwrap_or(0),
        "student_answer": answer.verified_text.clone().unwrap_or_default(),
        "question_crop_path": answer.crop_image_path,
    }))
}

fn workflow_question<'a>(
    question_by_id: &'a HashMap<String, &'a crate::models::QuestionRecord>,
    question_id: &str,
) -> HostResult<&'a crate::models::QuestionRecord> {
    question_by_id.get(question_id).copied().ok_or_else(|| {
        HostError::Validation(format!(
            "Question '{}' was missing from the workspace.",
            question_id
        ))
    })
}

fn workflow_answer<'a>(
    submission: &'a StudentWorkflowSubmission,
    question_id: &str,
) -> HostResult<&'a StudentWorkflowAnswer> {
    submission
        .answers
        .iter()
        .find(|answer| answer.question_id == question_id)
        .ok_or_else(|| {
            HostError::Validation(format!(
                "Submission '{}' is missing answer state for '{}'.",
                submission.student_ref, question_id
            ))
        })
}

fn workflow_subject(workspace: &ExamWorkspaceState) -> String {
    workspace
        .project
        .subject
        .clone()
        .unwrap_or_else(|| "Exam".into())
}

fn question_text_clean(question: &crate::models::QuestionRecord) -> String {
    question
        .analysis
        .question_text_clean
        .clone()
        .unwrap_or_else(|| question.baseline_pdf_text.clone())
}

fn rubric_criteria_json(question: &crate::models::QuestionRecord) -> Vec<Value> {
    question
        .rubric
        .criteria
        .iter()
        .enumerate()
        .map(|(criterion_index, criterion)| {
            json!({
                "criterion_index": criterion_index,
                "label": criterion.label,
                "points": criterion.points,
                "partial_credit_guidance": criterion.partial_credit_guidance,
            })
        })
        .collect()
}

fn criterion_results_with_totals(
    rows: &[&Value],
    question: &crate::models::QuestionRecord,
) -> HostResult<(Vec<Value>, i64)> {
    let mut criterion_results = Vec::new();
    let mut total_points_awarded = 0;
    for row in rows {
        let result = criterion_result_row_with_label(row, question)?;
        total_points_awarded += result.1;
        criterion_results.push(result.0);
    }
    Ok((criterion_results, total_points_awarded))
}

fn criterion_result_row_with_label(
    row: &Value,
    question: &crate::models::QuestionRecord,
) -> HostResult<(Value, i64)> {
    let criterion_index = required_i64(row, "criterion_index")?;
    let points_awarded = required_i64(row, "points_awarded")?;
    let rationale = required_string(row, "rationale")?;
    let rubric_criterion = usize::try_from(criterion_index)
        .ok()
        .and_then(|index| question.rubric.criteria.get(index));
    let label = rubric_criterion
        .map(|criterion| criterion.label.as_str())
        .or_else(|| row.get("criterion_label").and_then(Value::as_str))
        .unwrap_or("")
        .to_string();
    let points = rubric_criterion
        .map(|criterion| criterion.points)
        .unwrap_or_default();
    Ok((
        json!({
            "criterion_index": criterion_index,
            "label": label,
            "points": points,
            "points_awarded": points_awarded,
            "rationale": rationale,
        }),
        points_awarded,
    ))
}

fn cli_question_context_value(question_context: &Option<String>) -> Value {
    question_context
        .as_deref()
        .map(|s| Value::String(s.to_string()))
        .unwrap_or(Value::String(String::new()))
}

pub(super) fn feedback_request_json(row: &Value) -> Value {
    json!({
        "student_ref": row.get("student_ref").cloned().unwrap_or_default(),
        "question_id": row.get("question_id").cloned().unwrap_or_default(),
        "subject": row.get("subject").cloned().unwrap_or_default(),
        "total_points_awarded": row.get("total_points_awarded").cloned().unwrap_or_default(),
        "question_max_points": row.get("question_max_points").cloned().unwrap_or_default(),
        "student_answer": row.get("student_answer").cloned().unwrap_or_default(),
        "question_text_clean": row.get("question_text_clean").cloned().unwrap_or_default(),
        "question_context": row.get("question_context").cloned().unwrap_or_default(),
        "rubric_criteria": row.get("rubric_criteria").cloned().unwrap_or_default(),
        "criterion_results": feedback_criterion_results_json(row),
    })
}

fn feedback_criterion_results_json(row: &Value) -> Value {
    let rows = row
        .get("criterion_results")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    Value::Array(
        rows.iter()
            .map(|criterion| {
                json!({
                    "criterion_index": criterion
                        .get("criterion_index")
                        .cloned()
                        .unwrap_or_default(),
                    "points_awarded": criterion
                        .get("points_awarded")
                        .cloned()
                        .unwrap_or_default(),
                    "rationale": criterion.get("rationale").cloned().unwrap_or_default(),
                })
            })
            .collect(),
    )
}

pub(super) fn apply_final_grading_rows(
    submission: &mut StudentWorkflowSubmission,
    final_rows: &[Value],
) -> HostResult<()> {
    let finals_by_qid = final_rows_by_question_ref(final_rows);
    for answer in &mut submission.answers {
        let Some(final_row) = finals_by_qid.get(answer.question_id.as_str()) else {
            continue;
        };
        answer.grading_status = "draft_ready".into();
        answer.stale = false;
        if let Some(question_max_points) =
            final_row.get("question_max_points").and_then(Value::as_i64)
        {
            answer.question_max_points = Some(question_max_points);
        }
        answer.total_points_awarded = final_row
            .get("total_points_awarded")
            .and_then(Value::as_i64);
        answer.criterion_results = criterion_results_from_final_row(final_row);
    }
    Ok(())
}

pub(super) fn apply_feedback_rows(
    submission: &mut StudentWorkflowSubmission,
    feedback_rows: &[Value],
) -> HostResult<()> {
    let feedback_by_qid = feedback_text_by_question(feedback_rows);
    for answer in &mut submission.answers {
        if let Some(feedback_text) = feedback_by_qid.get(&answer.question_id) {
            answer.feedback_text = Some(feedback_text.clone());
        }
    }
    Ok(())
}

pub(super) fn apply_feedback_and_markup(
    submission: &mut StudentWorkflowSubmission,
    final_rows: Vec<Value>,
    feedback_rows: &[Value],
    highlight_rows: &[Value],
) -> HostResult<()> {
    apply_final_grading_rows(submission, &final_rows)?;
    apply_feedback_rows(submission, feedback_rows)?;
    let highlights_by_qid = highlight_rows_by_question(highlight_rows);
    for answer in &mut submission.answers {
        answer.highlights = highlight_spans_from_rows(highlights_by_qid.get(&answer.question_id));
    }
    Ok(())
}

pub(super) fn apply_highlight_rows(
    submission: &mut StudentWorkflowSubmission,
    highlight_rows: &[Value],
) -> HostResult<()> {
    let highlights_by_qid = highlight_rows_by_question(highlight_rows);
    for answer in &mut submission.answers {
        if let Some(rows) = highlights_by_qid.get(&answer.question_id) {
            answer.highlights = highlight_spans_from_rows(Some(rows));
        }
    }
    Ok(())
}

fn feedback_text_by_question(feedback_rows: &[Value]) -> HashMap<String, String> {
    feedback_rows
        .iter()
        .filter_map(|row| {
            Some((
                row.get("question_id")?.as_str()?.to_string(),
                row.get("feedback_text")
                    .and_then(Value::as_str)?
                    .to_string(),
            ))
        })
        .collect()
}

fn highlight_rows_by_question(highlight_rows: &[Value]) -> HashMap<String, Vec<Value>> {
    highlight_rows
        .iter()
        .filter_map(|row| {
            Some((
                row.get("question_id")?.as_str()?.to_string(),
                row.get("highlights")?.as_array()?.clone(),
            ))
        })
        .collect()
}

fn final_rows_by_question_ref(final_rows: &[Value]) -> HashMap<&str, &Value> {
    final_rows
        .iter()
        .filter_map(|row| Some((row.get("question_id")?.as_str()?, row)))
        .collect()
}

fn criterion_results_from_final_row(final_row: &Value) -> Vec<StudentWorkflowCriterionResult> {
    final_row
        .get("criterion_results")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(criterion_result_from_value)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn criterion_result_from_value(row: &Value) -> Option<StudentWorkflowCriterionResult> {
    Some(StudentWorkflowCriterionResult {
        criterion_index: row.get("criterion_index")?.as_i64()?,
        label: row
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        points: row
            .get("points")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        points_awarded: row.get("points_awarded")?.as_i64()?,
        rationale: row.get("rationale")?.as_str()?.to_string(),
    })
}

fn highlight_spans_from_rows(
    highlight_rows: Option<&Vec<Value>>,
) -> Vec<StudentWorkflowHighlightSpan> {
    highlight_rows
        .map(|rows| {
            rows.iter()
                .filter_map(highlight_span_from_value)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn highlight_span_from_value(row: &Value) -> Option<StudentWorkflowHighlightSpan> {
    Some(StudentWorkflowHighlightSpan {
        kind: row.get("kind")?.as_str()?.to_string(),
        start_char: row.get("start_char")?.as_i64()?,
        end_char: row.get("end_char")?.as_i64()?,
        text: row.get("text")?.as_str()?.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::{json, Value};

    use super::*;
    use crate::models::{
        ProjectConfig, ProjectSummary, QuestionAnalysisState, QuestionRecord, RubricCriterion,
        RubricState, StudentWorkflowDetectRegion, TemplatePageArtifactSummary,
        TemplateQuestionRegion, WorkerJobResult,
    };

    fn completed_with_data(data: Value) -> CompletedWorkerJob {
        CompletedWorkerJob {
            job_id: "job-1".into(),
            result: WorkerJobResult {
                terminal_type: "job_finished".into(),
                terminal_payload: json!({}),
                envelope: json!({ "data": data }),
                events: Vec::new(),
            },
        }
    }

    fn workflow_question(question_id: &str, question_number: i64) -> QuestionRecord {
        QuestionRecord {
            question_id: question_id.into(),
            question_number,
            page_number: question_number,
            max_points: Some(5),
            text: format!("Question {question_number}"),
            baseline_pdf_text: format!("Baseline {question_number}"),
            region: Some(TemplateQuestionRegion {
                x: 10 * question_number,
                y: 20 * question_number,
                width: 100,
                height: 50,
            }),
            source_artifact_id: None,
            image_path: Some(format!("/tmp/template-{question_id}.png")),
            analysis: QuestionAnalysisState {
                question_text_clean: Some(format!("Clean question {question_number}")),
                question_context: Some("Show your work".into()),
                ..QuestionAnalysisState::default()
            },
            rubric: RubricState {
                criteria: vec![
                    RubricCriterion {
                        criterion_id: format!("{question_id}-c1"),
                        label: "Accuracy".into(),
                        points: 3,
                        partial_credit_guidance: "Award for correct answer".into(),
                        source: "manual".into(),
                    },
                    RubricCriterion {
                        criterion_id: format!("{question_id}-c2"),
                        label: "Reasoning".into(),
                        points: 2,
                        partial_credit_guidance: "Award for explanation".into(),
                        source: "manual".into(),
                    },
                ],
                ..RubricState::default()
            },
        }
    }

    fn workspace() -> ExamWorkspaceState {
        ExamWorkspaceState {
            project: ProjectSummary {
                project_id: "project-1".into(),
                display_name: "Midterm".into(),
                subject: Some("Chemistry".into()),
                course_code: None,
                lms_course_id: None,
                project_path: "/tmp/project".into(),
                created_at: "0".into(),
                updated_at: "0".into(),
            },
            status: "ready".into(),
            status_label: "Ready".into(),
            failure_message: None,
            template_preview_artifacts: vec![
                TemplatePageArtifactSummary {
                    artifact_id: "page-1".into(),
                    page_number: 1,
                    image_path: "/tmp/template-page-1.png".into(),
                    label: "Page 1".into(),
                },
                TemplatePageArtifactSummary {
                    artifact_id: "page-2".into(),
                    page_number: 2,
                    image_path: "/tmp/template-page-2.png".into(),
                    label: "Page 2".into(),
                },
            ],
            aruco_status: Default::default(),
            questions: vec![workflow_question("q1", 1), workflow_question("q2", 2)],
            redaction_regions: Vec::new(),
            warnings: Vec::new(),
            can_approve: false,
            can_approve_rubric: false,
            project_config: ProjectConfig::default(),
            student_roster: Vec::new(),
            student_intake: Default::default(),
            student_workflow: Default::default(),
            moderation_state: Default::default(),
            results_lms_state: Default::default(),
            results_lms_rows: Vec::new(),
            results_lms_metrics: None,
            results_lms_review_summary: None,
            workflow_stage: String::new(),
            workflow_label: String::new(),
        }
    }

    fn submission() -> StudentWorkflowSubmission {
        StudentWorkflowSubmission {
            student_ref: "student-1".into(),
            canonical_pdf_path: "/tmp/student-1.pdf".into(),
            page_count: 2,
            stage: "detect".into(),
            latest_job_id: None,
            failure_message: None,
            warnings: Vec::new(),
            page_artifacts: vec![
                StudentWorkflowPage {
                    page_number: 1,
                    image_path: "/tmp/student-page-1.png".into(),
                    source_pdf_path: Some("/tmp/student-1.pdf".into()),
                    ocr_metadata_path: None,
                },
                StudentWorkflowPage {
                    page_number: 2,
                    image_path: "/tmp/student-page-2.png".into(),
                    source_pdf_path: Some("/tmp/student-1.pdf".into()),
                    ocr_metadata_path: None,
                },
            ],
            alignment_pages: vec![StudentWorkflowAlignmentPage {
                page_number: 1,
                confidence: Some(0.98),
                low_confidence: false,
                review_exempt: false,
                review_exempt_reason: None,
                question_count: 1,
                transform: StudentWorkflowTransform {
                    rotation: 1.0,
                    scale: 0.9,
                    translate_x: 2.0,
                    translate_y: 3.0,
                },
                warnings: Vec::new(),
            }],
            detect_review: None,
            answers: Vec::new(),
        }
    }

    fn answer(question_id: &str, verified: bool) -> StudentWorkflowAnswer {
        StudentWorkflowAnswer {
            question_id: question_id.into(),
            question_number: if question_id == "q1" { 1 } else { 2 },
            crop_image_path: Some(format!("/tmp/{question_id}.png")),
            pii_prescreen: None,
            manual_grading_required: false,
            manual_grading_reason: None,
            moderation_eligible: false,
            parse_status: "ok".into(),
            parse_confidence: None,
            parse_confidence_source: None,
            raw_parsed_text: None,
            verified_text: Some(format!("answer for {question_id}")),
            review_required: false,
            verified,
            stale: false,
            grading_status: "not_started".into(),
            grading_confidence: None,
            grading_confidence_reason: None,
            question_max_points: Some(5),
            total_points_awarded: None,
            feedback_text: None,
            criterion_results: Vec::new(),
            highlights: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn question_by_id(workspace: &ExamWorkspaceState) -> HashMap<String, &QuestionRecord> {
        workspace
            .questions
            .iter()
            .map(|question| (question.question_id.clone(), question))
            .collect()
    }

    #[test]
    fn parse_alignment_pages_handles_success_failed_and_missing_transform() {
        let completed = completed_with_data(json!({
            "alignment_results": [
                {
                    "page_number": 1,
                    "status": "ok",
                    "confidence": 0.91,
                    "transform": {
                        "rotation": 1.5,
                        "scale": 0.99,
                        "translate_x": 4.0,
                        "translate_y": -2.0
                    }
                },
                {
                    "page_number": 2,
                    "status": "failed"
                }
            ]
        }));

        let pages = parse_alignment_pages(&completed).expect("alignment pages should parse");

        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].transform.rotation, 1.5);
        assert!(pages[1].low_confidence);
        assert_eq!(pages[1].transform.scale, 1.0);

        let missing_transform = completed_with_data(json!({
            "alignment_results": [
                {
                    "page_number": 1,
                    "status": "ok"
                }
            ]
        }));
        assert!(parse_alignment_pages(&missing_transform)
            .expect_err("non-failed rows require transform")
            .to_string()
            .contains("missing transform"));
    }

    #[test]
    fn canonicalize_targets_and_page_results_preserve_pdf_and_ocr_metadata() {
        let workspace = workspace();
        let intake = crate::models::StudentIntakeSummary {
            student_ref: "student-1".into(),
            local_display_name: None,
            canonical_pdf_path: "/tmp/student-1.pdf".into(),
            ingest_status: "ready".into(),
            page_count: 2,
            exam_page_paths: vec!["/tmp/raw-1.png".into(), "/tmp/raw-2.png".into()],
            warnings: Vec::new(),
            binding_token_hex: None,
        };
        let mut submission = submission();

        let targets = build_canonicalize_targets(&workspace, &intake, &submission)
            .expect("canonical targets should build");
        assert_eq!(targets[0]["page"]["image_path"], "/tmp/raw-1.png");
        assert_eq!(
            targets[0]["template_page"]["image_path"],
            "/tmp/template-page-1.png"
        );

        let data = json!({
            "canonicalize_results": [
                {
                    "status": "ok",
                    "output_page": {
                        "page_number": 1,
                        "image_path": "/tmp/canonical-1.png",
                        "source_pdf_path": "/tmp/student-1.pdf"
                    }
                },
                {
                    "status": "failed",
                    "output_page": {
                        "page_number": 2,
                        "image_path": "/tmp/ignored.png"
                    }
                }
            ]
        });
        let pages = parse_canonicalized_pages(data.as_object().unwrap())
            .expect("canonicalized pages should parse");
        assert_eq!(pages.len(), 1);
        assert_eq!(
            pages[0].source_pdf_path.as_deref(),
            Some("/tmp/student-1.pdf")
        );

        submission.page_artifacts = pages;
        apply_detect_page_ocr_results(
            &mut submission,
            json!({
                "page_ocr_results": [
                    {"page_number": 1, "ocr_metadata_path": "/tmp/ocr-1.json"}
                ]
            })
            .as_object()
            .unwrap(),
        )
        .expect("ocr metadata should apply");
        assert_eq!(
            submission.page_artifacts[0].ocr_metadata_path.as_deref(),
            Some("/tmp/ocr-1.json")
        );
    }

    #[test]
    fn detect_targets_crop_targets_and_review_fallbacks_are_built() {
        let workspace = workspace();
        let submission = submission();
        let detect_targets =
            build_detect_targets(&workspace, &submission).expect("detect targets should build");
        assert_eq!(detect_targets.len(), 2);
        assert_eq!(detect_targets[0]["question_hints"][0]["question_id"], "q1");
        assert_eq!(
            detect_targets[0]["question_hints"][0]["question_label"],
            "1"
        );
        assert_eq!(
            detect_targets[0]["page"]["source_pdf_path"],
            "/tmp/student-1.pdf"
        );

        let detect_data = json!({
            "detect_results": [
                {
                    "student_ref": "student-1",
                    "question_id": "q1",
                    "page_number": 1,
                    "status": "ok",
                    "region_source": "ocr_refined",
                    "region": {
                        "x": 1,
                        "y": 2,
                        "width": 30,
                        "height": 40,
                        "units": "rendered_page_pixels"
                    }
                },
                {
                    "question_id": "q2",
                    "page_number": 2,
                    "status": "warning",
                    "warnings": [{"code": "low_confidence", "message": "Review"}]
                }
            ]
        });

        let crop_targets =
            build_crop_targets(detect_data.as_object().unwrap()).expect("crop targets");
        assert_eq!(crop_targets.len(), 1);
        assert_eq!(crop_targets[0]["student_ref"], "student-1");

        let mut review =
            build_detect_review(&workspace, &submission, detect_data.as_object().unwrap())
                .expect("review should build")
                .expect("one row needs review");
        assert_eq!(review.trusted_crop_targets.len(), 1);
        assert_eq!(review.pending_rows[0].template_region.x, 20);
        assert_eq!(
            review.pending_rows[0].warnings[0].code.as_deref(),
            Some("low_confidence")
        );

        assert!(crop_targets_from_detect_review(&review, "student-1")
            .expect_err("missing resolved region should fail")
            .to_string()
            .contains("missing a resolved detect region"));
        review.pending_rows[0].resolved_region = Some(StudentWorkflowDetectRegion {
            x: 4,
            y: 5,
            width: 60,
            height: 70,
            units: "rendered_page_pixels".into(),
        });
        let resolved =
            crop_targets_from_detect_review(&review, "student-1").expect("resolved targets");
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[1]["region"]["width"], 60);

        let invalid = StudentWorkflowDetectRegion {
            x: 0,
            y: 0,
            width: 0,
            height: 1,
            units: "rendered_page_pixels".into(),
        };
        assert!(validate_detect_region(&invalid)
            .expect_err("zero width should fail")
            .to_string()
            .contains("positive size"));
    }

    #[test]
    fn pii_results_seed_answers_and_parse_targets_skip_safe_prescreen() {
        let workspace = workspace();
        let crop_rows = vec![
            json!({
                "question_id": "q2",
                "status": "ok",
                "question_crop_path": "/tmp/q2.png"
            }),
            json!({
                "question_id": "q1",
                "status": "failed",
                "warnings": [{"code": "crop_error", "message": "Could not crop"}]
            }),
        ];
        let pii_data = json!({
            "pii_results": [
                {
                    "question_id": "q2",
                    "status": "ok",
                    "contains_handwriting": "no",
                    "contains_pii": false,
                    "pii_types_detected": []
                }
            ]
        });
        let answers =
            build_answers_from_pii_results(&workspace, &crop_rows, pii_data.as_object().unwrap())
                .expect("answers should seed");

        assert_eq!(answers[0].question_id, "q1");
        assert_eq!(
            answers[0].manual_grading_reason.as_deref(),
            Some("crop_failed")
        );
        assert_eq!(answers[1].question_id, "q2");
        assert!(!answers[1].manual_grading_required);
        assert_eq!(answers[1].parse_status, "not_started");

        let mut submission = submission();
        submission.answers = answers;
        let parse_targets = build_parse_targets(
            &workspace,
            &submission,
            &[json!({
                "question_id": "q2",
                "status": "ok",
                "question_crop_path": "/tmp/q2.png"
            })],
        )
        .expect("parse target should build");
        assert_eq!(parse_targets[0]["student_ref"], "student-1");
        assert_eq!(parse_targets[0]["pii_prescreen"]["status"], "ok");
        assert_eq!(
            parse_targets[0]["parse_question_context"]["question_text_clean"],
            "Clean question 2"
        );
    }

    #[test]
    fn parse_results_update_review_and_verified_state() {
        let mut submission = submission();
        submission.answers = vec![answer("q1", false), answer("q2", false)];
        let parse_data = json!({
            "parse_results": [
                {
                    "question_id": "q1",
                    "status": "warning",
                    "confidence": "low",
                    "confidence_source": "handwriting",
                    "parsed_text": "needs review"
                },
                {
                    "question_id": "q2",
                    "status": "ok",
                    "confidence": "high"
                }
            ]
        });

        merge_parse_results_into_answers(&mut submission, parse_data.as_object().unwrap())
            .expect("parse rows should merge");

        assert!(submission.answers[0].review_required);
        assert!(!submission.answers[0].verified);
        assert_eq!(
            submission.answers[0].verified_text.as_deref(),
            Some("needs review")
        );
        assert!(!submission.answers[1].review_required);
        assert!(submission.answers[1].verified);
        assert_eq!(submission.answers[1].verified_text.as_deref(), Some(""));
    }

    #[test]
    fn preliminary_final_feedback_and_markup_rows_shape_answer_state() {
        let workspace = workspace();
        let question_by_id = question_by_id(&workspace);
        let mut submission = submission();
        submission.answers = vec![answer("q1", true), answer("q2", false)];
        let preliminary_rows = vec![
            json!({
                "question_id": "q1",
                "criterion_index": 0,
                "points_awarded": 2,
                "rationale": "Mostly correct",
                "confidence": "high",
                "confidence_reason": "Clear answer"
            }),
            json!({
                "question_id": "q1",
                "criterion_index": 1,
                "points_awarded": 1,
                "rationale": "Some reasoning",
                "confidence": "low",
                "confidence_reason": "Brief explanation"
            }),
        ];

        apply_preliminary_confidence_to_answers(&mut submission, &preliminary_rows)
            .expect("preliminary confidence should apply");
        assert_eq!(
            submission.answers[0].grading_confidence.as_deref(),
            Some("low")
        );
        assert_eq!(
            submission.answers[0].grading_confidence_reason.as_deref(),
            Some("Brief explanation")
        );

        let score_requests =
            build_preliminary_answer_score_requests(&workspace, &submission, &question_by_id)
                .expect("score requests should build");
        assert_eq!(score_requests.len(), 1);
        assert_eq!(score_requests[0]["subject"], "Chemistry");
        assert_eq!(score_requests[0]["rubric_criteria"][0]["label"], "Accuracy");

        let final_rows = final_rows_from_preliminary(
            &workspace,
            &submission,
            &question_by_id,
            &preliminary_rows,
        )
        .expect("final rows should build");
        assert_eq!(final_rows[0]["total_points_awarded"], 3);
        assert_eq!(final_rows[0]["question_max_points"], 5);

        let feedback_request = feedback_request_json(&final_rows[0]);
        assert_eq!(feedback_request["student_answer"], "answer for q1");
        assert_eq!(
            feedback_request["criterion_results"][0]["rationale"],
            "Mostly correct"
        );

        apply_feedback_and_markup(
            &mut submission,
            final_rows,
            &[json!({
                "question_id": "q1",
                "feedback_text": "Good structure."
            })],
            &[json!({
                "question_id": "q1",
                "highlights": [
                    {
                        "kind": "strength",
                        "start_char": 0,
                        "end_char": 4,
                        "text": "Good"
                    }
                ]
            })],
        )
        .expect("feedback and markup should apply");

        assert_eq!(submission.answers[0].grading_status, "draft_ready");
        assert_eq!(submission.answers[0].total_points_awarded, Some(3));
        assert_eq!(submission.answers[0].question_max_points, Some(5));
        assert_eq!(submission.answers[0].criterion_results.len(), 2);
        assert_eq!(
            submission.answers[0].feedback_text.as_deref(),
            Some("Good structure.")
        );
        assert_eq!(submission.answers[0].highlights[0].kind, "strength");
        assert_eq!(submission.answers[1].grading_status, "not_started");
    }

    #[test]
    fn manual_pii_block_and_aggregate_helpers_cover_edge_cases() {
        let workspace = workspace();
        let warning = WorkspaceWarning {
            code: Some("pii_prescreen_unavailable".into()),
            message: "Prescreen unavailable".into(),
            scope: Some("answer".into()),
        };
        let answers = build_answers_for_manual_pii_block(
            &workspace,
            &[json!({
                "question_id": "q1",
                "status": "ok",
                "question_crop_path": "/tmp/q1.png"
            })],
            warning,
        )
        .expect("manual pii block answers should build");
        assert_eq!(
            answers[0].manual_grading_reason.as_deref(),
            Some("pii_ambiguous")
        );
        assert_eq!(
            answers[0].warnings[0].code.as_deref(),
            Some("pii_prescreen_unavailable")
        );

        let rows = [
            json!({"confidence": "medium", "confidence_reason": "usable"}),
            json!({"confidence": "nonsense", "confidence_reason": "ignored"}),
            json!({"confidence": "low", "confidence_reason": "weak evidence"}),
        ];
        let refs = rows.iter().collect::<Vec<_>>();
        let (confidence, reason) = aggregate_preliminary_confidence(&refs);
        assert_eq!(confidence.as_deref(), Some("low"));
        assert_eq!(reason.as_deref(), Some("weak evidence"));
    }
}
