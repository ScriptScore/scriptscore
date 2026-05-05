// SPDX-License-Identifier: AGPL-3.0-only
use std::sync::Arc;

use crate::errors::{HostError, HostResult};
use crate::models::ScansOcrHintResult;

use super::{workspace_actions, AppStateInner, RuntimeEventSink};

#[derive(Clone, Debug, serde::Deserialize)]
struct CliIntakePreviewPage {
    page_number: i64,
    page_count: i64,
    page_width_pt: f64,
    page_height_pt: f64,
    png_width_px: i64,
    png_height_px: i64,
    png_base64: String,
}

#[derive(Clone, Debug, serde::Deserialize)]
struct CliPdfPointRect {
    page_number: i64,
    x_pt: f64,
    y_pt: f64,
    width_pt: f64,
    height_pt: f64,
}

pub(super) fn transient_scans_ocr_hint(
    state: &Arc<AppStateInner>,
    png_bytes: Vec<u8>,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<ScansOcrHintResult> {
    let data = run_transient_runtime_command_inner(
        state,
        "scans.ocr",
        serde_json::json!({}),
        serde_json::json!({}),
        Some(&png_bytes),
        event_sink,
    )?;
    Ok(ocr_hint_from_data(&data))
}

pub(super) fn transient_render_pdf_page_png(
    state: &Arc<AppStateInner>,
    pdf_path: String,
    page_number: i64,
    zoom: f64,
    max_width_px: Option<i64>,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<workspace_actions::IntakePreviewPage> {
    let request_payload =
        render_pdf_page_request_payload(pdf_path, page_number, zoom, max_width_px);
    let data = run_transient_runtime_command_inner(
        state,
        "scans.pdf-render-page",
        request_payload.clone(),
        request_payload,
        None,
        event_sink,
    )?;
    preview_page_from_data(data)
}

fn render_pdf_page_request_payload(
    pdf_path: String,
    page_number: i64,
    zoom: f64,
    max_width_px: Option<i64>,
) -> serde_json::Value {
    let mut request_payload = serde_json::json!({
        "pdf_path": pdf_path,
        "page_number": page_number,
        "zoom": zoom,
    });
    if let Some(max_width_px) = max_width_px {
        request_payload["max_width_px"] = serde_json::json!(max_width_px);
    }
    request_payload
}

pub(super) fn transient_clip_pdf_rects_png_base64(
    state: &Arc<AppStateInner>,
    pdf_path: String,
    rects: Vec<workspace_actions::PdfPointRect>,
    zoom: f64,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<Vec<String>> {
    let rects_payload = rects
        .into_iter()
        .map(|rect| {
            serde_json::json!({
                "page_number": rect.page_number,
                "x_pt": rect.x_pt,
                "y_pt": rect.y_pt,
                "width_pt": rect.width_pt,
                "height_pt": rect.height_pt,
            })
        })
        .collect::<Vec<serde_json::Value>>();
    let request_payload = serde_json::json!({
        "pdf_path": pdf_path,
        "rects": rects_payload,
        "zoom": zoom,
    });
    let data = run_transient_runtime_command_inner(
        state,
        "scans.pdf-clip-rects",
        request_payload.clone(),
        request_payload,
        None,
        event_sink,
    )?;
    Ok(clipped_pngs_from_data(&data))
}

pub(super) fn transient_pdf_clip_text(
    state: &Arc<AppStateInner>,
    input: workspace_actions::PdfTextClipInput,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<String> {
    let request_payload = serde_json::json!({
        "pdf_path": input.pdf_path,
        "page_number": input.page_number,
        "x_pt": input.x_pt,
        "y_pt": input.y_pt,
        "width_pt": input.width_pt,
        "height_pt": input.height_pt,
    });
    let data = run_transient_runtime_command_inner(
        state,
        "scans.pdf-extract-text",
        request_payload.clone(),
        request_payload,
        None,
        event_sink,
    )?;
    Ok(clipped_text_from_data(&data))
}

pub(super) fn intake_default_pdf_rects_from_template(
    state: &Arc<AppStateInner>,
    pdf_path: String,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<Vec<workspace_actions::PdfPointRect>> {
    let project_path = {
        let app = state.lock();
        app.current_project_path()?
    };
    let Some(payload) = workspace_actions::build_intake_default_pdf_rects_request(&project_path)?
    else {
        return Ok(Vec::new());
    };
    let request_payload = serde_json::json!({
        "pdf_path": pdf_path,
        "regions": payload.get("regions").cloned().unwrap_or_default(),
        "raster_sizes_by_page": payload
            .get("raster_sizes_by_page")
            .cloned()
            .unwrap_or_default(),
    });
    let data = run_transient_runtime_command_inner(
        state,
        "scans.pdf-map-template-regions",
        request_payload.clone(),
        request_payload,
        None,
        event_sink,
    )?;
    mapped_rects_from_data(&data)
}

fn ocr_hint_from_data(data: &serde_json::Map<String, serde_json::Value>) -> ScansOcrHintResult {
    ScansOcrHintResult {
        hint_text: data
            .get("hint_text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string(),
        segment_count: data
            .get("segment_count")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
    }
}

fn preview_page_from_data(
    data: serde_json::Map<String, serde_json::Value>,
) -> HostResult<workspace_actions::IntakePreviewPage> {
    let cli: CliIntakePreviewPage = serde_json::from_value(serde_json::Value::Object(data))?;
    Ok(workspace_actions::IntakePreviewPage {
        page_number: cli.page_number,
        page_count: cli.page_count,
        page_width_pt: cli.page_width_pt,
        page_height_pt: cli.page_height_pt,
        png_width_px: cli.png_width_px,
        png_height_px: cli.png_height_px,
        png_base64: cli.png_base64,
    })
}

fn clipped_pngs_from_data(data: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    data.get("clips")
        .and_then(serde_json::Value::as_array)
        .map(|clips| {
            clips
                .iter()
                .filter_map(|clip| clip.get("png_base64").and_then(serde_json::Value::as_str))
                .map(ToOwned::to_owned)
                .collect::<Vec<String>>()
        })
        .unwrap_or_default()
}

fn clipped_text_from_data(data: &serde_json::Map<String, serde_json::Value>) -> String {
    data.get("text")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn mapped_rects_from_data(
    data: &serde_json::Map<String, serde_json::Value>,
) -> HostResult<Vec<workspace_actions::PdfPointRect>> {
    let rects: Vec<CliPdfPointRect> = serde_json::from_value(
        data.get("rects")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    )?;
    Ok(rects
        .into_iter()
        .map(|rect| workspace_actions::PdfPointRect {
            page_number: rect.page_number,
            x_pt: rect.x_pt,
            y_pt: rect.y_pt,
            width_pt: rect.width_pt,
            height_pt: rect.height_pt,
        })
        .collect())
}

fn run_transient_runtime_command_inner(
    state: &Arc<AppStateInner>,
    command_name: &str,
    worker_request_payload: serde_json::Value,
    persisted_request_payload: serde_json::Value,
    stdin_bytes: Option<&[u8]>,
    event_sink: &dyn RuntimeEventSink,
) -> HostResult<serde_json::Map<String, serde_json::Value>> {
    let reserved = super::runtime::start_runtime_job(state, event_sink, command_name)?;
    let completed = super::runtime::run_reserved_job(
        state,
        event_sink,
        reserved,
        super::runtime::RuntimeJobRequest {
            command_name,
            worker_request_payload,
            persisted_request_payload,
            output_artifacts_dir: None,
            project_path: None,
            stdin_bytes,
        },
    )?;
    completed
        .result
        .envelope
        .get("data")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .ok_or_else(|| HostError::Protocol(format!("{command_name} envelope missing data.")))
}

#[cfg(test)]
mod tests {
    use super::{
        clipped_pngs_from_data, clipped_text_from_data, mapped_rects_from_data, ocr_hint_from_data,
        preview_page_from_data, render_pdf_page_request_payload,
    };

    #[test]
    fn ocr_hint_from_data_defaults_missing_fields() {
        let hint = ocr_hint_from_data(&serde_json::Map::new());
        assert_eq!(hint.hint_text, "");
        assert_eq!(hint.segment_count, 0);
    }

    #[test]
    fn preview_page_from_data_parses_cli_shape() {
        let preview = preview_page_from_data(
            serde_json::json!({
                "page_number": 2,
                "page_count": 5,
                "page_width_pt": 612.0,
                "page_height_pt": 792.0,
                "png_width_px": 1224,
                "png_height_px": 1584,
                "png_base64": "abc123",
            })
            .as_object()
            .expect("object")
            .clone(),
        )
        .expect("preview should parse");

        assert_eq!(preview.page_number, 2);
        assert_eq!(preview.page_count, 5);
        assert_eq!(preview.png_width_px, 1224);
        assert_eq!(preview.png_base64, "abc123");
    }

    #[test]
    fn preview_page_from_data_rejects_missing_cli_fields() {
        let error = preview_page_from_data(
            serde_json::json!({
                "page_number": 2,
                "page_count": 5,
                "png_base64": "abc123"
            })
            .as_object()
            .expect("object")
            .clone(),
        )
        .expect_err("missing dimensions should fail protocol parsing");

        assert!(error.to_string().contains("page_width_pt"));
    }

    #[test]
    fn render_pdf_page_request_payload_includes_optional_width_cap() {
        assert_eq!(
            render_pdf_page_request_payload("/tmp/exam.pdf".into(), 2, 2.0, Some(1600)),
            serde_json::json!({
                "pdf_path": "/tmp/exam.pdf",
                "page_number": 2,
                "zoom": 2.0,
                "max_width_px": 1600,
            })
        );
    }

    #[test]
    fn render_pdf_page_request_payload_omits_missing_width_cap() {
        assert_eq!(
            render_pdf_page_request_payload("/tmp/exam.pdf".into(), 2, 2.0, None),
            serde_json::json!({
                "pdf_path": "/tmp/exam.pdf",
                "page_number": 2,
                "zoom": 2.0,
            })
        );
    }

    #[test]
    fn clipped_pngs_from_data_keeps_only_present_base64_rows() {
        let clips = clipped_pngs_from_data(
            serde_json::json!({
                "clips": [
                    {"png_base64": "one"},
                    {},
                    {"png_base64": "two"},
                ]
            })
            .as_object()
            .expect("object"),
        );
        assert_eq!(clips, vec!["one".to_string(), "two".to_string()]);
    }

    #[test]
    fn clipped_text_from_data_defaults_to_empty_string() {
        assert_eq!(clipped_text_from_data(&serde_json::Map::new()), "");
    }

    #[test]
    fn mapped_rects_from_data_parses_cli_rects() {
        let rects = mapped_rects_from_data(
            serde_json::json!({
                "rects": [
                    {
                        "page_number": 1,
                        "x_pt": 10.0,
                        "y_pt": 20.0,
                        "width_pt": 30.0,
                        "height_pt": 40.0,
                    }
                ]
            })
            .as_object()
            .expect("object"),
        )
        .expect("rects should parse");
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].page_number, 1);
        assert_eq!(rects[0].width_pt, 30.0);
    }

    #[test]
    fn mapped_rects_from_data_rejects_malformed_rects() {
        let error = mapped_rects_from_data(
            serde_json::json!({
                "rects": [
                    {
                        "page_number": 1,
                        "x_pt": 10.0,
                        "y_pt": 20.0,
                        "width_pt": "wide",
                        "height_pt": 40.0,
                    }
                ]
            })
            .as_object()
            .expect("object"),
        )
        .expect_err("malformed rect should fail protocol parsing");

        assert!(error.to_string().contains("invalid type"));
    }
}
