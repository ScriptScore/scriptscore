// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::HashMap;

use serde_json::{json, Map, Value};

use crate::errors::{HostError, HostResult};
use crate::models::{
    ExamWorkspaceState, StudentWorkflowAlignmentPage, StudentWorkflowAnswer,
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
    let subject = workspace
        .project
        .subject
        .clone()
        .unwrap_or_else(|| "Exam".into());
    let profile = cli_instructor_profile_json(&workspace.project_config.instructor_profile);
    let mut requests = Vec::new();
    for answer in &submission.answers {
        if !answer.verified || answer.stale {
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
