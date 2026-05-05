# SPDX-License-Identifier: AGPL-3.0-only
"""Direct CLI entrypoint for ScriptScore."""

from __future__ import annotations

import argparse
import json
import os
import sys
from datetime import UTC, datetime
from typing import Any, cast

from scriptscore.contracts import ValidationFailedError, ValidationIssue, exit_code_for_category
from scriptscore.engine import ScriptScoreEngine, create_engine
from scriptscore.runtime import make_error_envelope, new_operation_id
from scriptscore.transport import SidecarServer


def _add_stdio_command_flags(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--stdin", action="store_true", dest="use_stdin")
    parser.add_argument("--options", type=str)
    parser.add_argument("--request-id", type=str)
    parser.add_argument("--emit-events", action="store_true")


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="scriptscore")
    subparsers = parser.add_subparsers(dest="command_group")

    sidecar = subparsers.add_parser("sidecar", help="Run the JSON-RPC sidecar transport.")
    sidecar_subparsers = sidecar.add_subparsers(dest="sidecar_command")
    sidecar_subparsers.add_parser("rpc", help="Serve JSON-RPC 2.0 over stdin/stdout.")

    smoke = subparsers.add_parser("_smoke", help=argparse.SUPPRESS)
    smoke_subparsers = smoke.add_subparsers(dest="smoke_command")
    smoke_ping = smoke_subparsers.add_parser("ping", help=argparse.SUPPRESS)
    _add_stdio_command_flags(smoke_ping)
    smoke_ping.set_defaults(command_name="smoke.ping")

    runtime = subparsers.add_parser("runtime", help="Run bounded runtime inspection commands.")
    runtime_subparsers = runtime.add_subparsers(dest="runtime_command")
    runtime_list_llm_models = runtime_subparsers.add_parser(
        "list-llm-models",
        help="List built-in LLM models visible to the selected provider.",
    )
    _add_stdio_command_flags(runtime_list_llm_models)
    runtime_list_llm_models.set_defaults(command_name="runtime.list-llm-models")
    runtime_validate_llm_model = runtime_subparsers.add_parser(
        "validate-llm-model",
        help="Validate one built-in LLM model against the selected provider and capability filter.",
    )
    _add_stdio_command_flags(runtime_validate_llm_model)
    runtime_validate_llm_model.set_defaults(command_name="runtime.validate-llm-model")

    exam = subparsers.add_parser("exam", help="Run exam commands.")
    exam_subparsers = exam.add_subparsers(dest="exam_command")

    exam_setup = exam_subparsers.add_parser(
        "setup", help="Bootstrap template pages and question definitions."
    )
    _add_stdio_command_flags(exam_setup)
    exam_setup.set_defaults(command_name="exam.setup")

    exam_analyze = exam_subparsers.add_parser(
        "analyze", help="Analyze template question text and assets."
    )
    _add_stdio_command_flags(exam_analyze)
    exam_analyze.set_defaults(command_name="exam.analyze")

    exam_generate_rubric = exam_subparsers.add_parser(
        "generate-rubric", help="Generate one rubric draft."
    )
    _add_stdio_command_flags(exam_generate_rubric)
    exam_generate_rubric.set_defaults(command_name="exam.generate-rubric")

    grading = subparsers.add_parser("grading", help="Run grading commands.")
    grading_subparsers = grading.add_subparsers(dest="grading_command")

    grading_score_preliminary = grading_subparsers.add_parser(
        "score-preliminary",
        help="Score criterion-level preliminary grading rows.",
    )
    _add_stdio_command_flags(grading_score_preliminary)
    grading_score_preliminary.set_defaults(command_name="grading.score-preliminary")

    grading_run_consistency = grading_subparsers.add_parser(
        "run-consistency",
        help="Review criterion-level scoring consistency.",
    )
    _add_stdio_command_flags(grading_run_consistency)
    grading_run_consistency.set_defaults(command_name="grading.run-consistency")

    grading_draft_feedback = grading_subparsers.add_parser(
        "draft-feedback",
        help="Draft student-facing question feedback.",
    )
    _add_stdio_command_flags(grading_draft_feedback)
    grading_draft_feedback.set_defaults(command_name="grading.draft-feedback")

    grading_markup = grading_subparsers.add_parser(
        "markup",
        help="Generate highlight spans for graded answers.",
    )
    _add_stdio_command_flags(grading_markup)
    grading_markup.set_defaults(command_name="grading.markup")

    grading_export = grading_subparsers.add_parser(
        "export",
        help="Export self-contained HTML grading reports.",
    )
    _add_stdio_command_flags(grading_export)
    grading_export.set_defaults(command_name="grading.export")

    scans = subparsers.add_parser("scans", help="Run scan commands.")
    scans_subparsers = scans.add_subparsers(dest="scans_command")

    scans_ingest = scans_subparsers.add_parser(
        "ingest", help="Render uploaded PDFs into page artifacts."
    )
    _add_stdio_command_flags(scans_ingest)
    scans_ingest.set_defaults(command_name="scans.ingest")

    scans_canonicalize = scans_subparsers.add_parser(
        "canonicalize", help="Render aligned student pages onto template-sized canvases."
    )
    _add_stdio_command_flags(scans_canonicalize)
    scans_canonicalize.set_defaults(command_name="scans.canonicalize")

    scans_transform = scans_subparsers.add_parser("transform", help="Apply manual page transforms.")
    _add_stdio_command_flags(scans_transform)
    scans_transform.set_defaults(command_name="scans.transform")

    scans_align_auto = scans_subparsers.add_parser(
        "align-auto", help="Compute auto-alignment proposals."
    )
    _add_stdio_command_flags(scans_align_auto)
    scans_align_auto.set_defaults(command_name="scans.align-auto")

    scans_detect = scans_subparsers.add_parser(
        "detect", help="Refine per-student question regions with OCR."
    )
    _add_stdio_command_flags(scans_detect)
    scans_detect.set_defaults(command_name="scans.detect")

    scans_crop = scans_subparsers.add_parser(
        "crop", help="Crop question images from explicit pages."
    )
    _add_stdio_command_flags(scans_crop)
    scans_crop.set_defaults(command_name="scans.crop")

    scans_pii = scans_subparsers.add_parser(
        "pii",
        help="Analyze cropped answers for handwriting and student-specific PII.",
    )
    _add_stdio_command_flags(scans_pii)
    scans_pii.set_defaults(command_name="scans.pii")

    scans_parse = scans_subparsers.add_parser(
        "parse", help="Build parse drafts from cropped answers."
    )
    _add_stdio_command_flags(scans_parse)
    scans_parse.set_defaults(command_name="scans.parse")

    scans_ocr = scans_subparsers.add_parser(
        "ocr",
        help="Extract a private OCR hint from a PNG on stdin (transient; no artifacts).",
    )
    # Reuse stdio flags for parity, but the handler reads the PNG from stdin directly.
    _add_stdio_command_flags(scans_ocr)
    scans_ocr.set_defaults(command_name="scans.ocr")

    scans_pdf_render_page = scans_subparsers.add_parser(
        "pdf-render-page",
        help="Render one PDF page to a transient base64 PNG payload.",
    )
    _add_stdio_command_flags(scans_pdf_render_page)
    scans_pdf_render_page.set_defaults(command_name="scans.pdf-render-page")

    scans_pdf_clip_rects = scans_subparsers.add_parser(
        "pdf-clip-rects",
        help="Clip one or more PDF-space rects to transient base64 PNG payloads.",
    )
    _add_stdio_command_flags(scans_pdf_clip_rects)
    scans_pdf_clip_rects.set_defaults(command_name="scans.pdf-clip-rects")

    scans_pdf_extract_text = scans_subparsers.add_parser(
        "pdf-extract-text",
        help="Extract transient text from one PDF-space rect.",
    )
    _add_stdio_command_flags(scans_pdf_extract_text)
    scans_pdf_extract_text.set_defaults(command_name="scans.pdf-extract-text")

    scans_pdf_map_template_regions = scans_subparsers.add_parser(
        "pdf-map-template-regions",
        help="Map template rendered-page regions to transient PDF-point rects.",
    )
    _add_stdio_command_flags(scans_pdf_map_template_regions)
    scans_pdf_map_template_regions.set_defaults(command_name="scans.pdf-map-template-regions")

    scans_pdf_create_redacted = scans_subparsers.add_parser(
        "pdf-create-redacted",
        help="Burn template redaction rectangles into a PDF copy (desktop intake canonical step).",
    )
    _add_stdio_command_flags(scans_pdf_create_redacted)
    scans_pdf_create_redacted.set_defaults(command_name="scans.pdf-create-redacted")

    scans_pdf_detect_aruco = scans_subparsers.add_parser(
        "pdf-detect-aruco",
        help="Detect ArUco markers on rendered PDF pages.",
    )
    _add_stdio_command_flags(scans_pdf_detect_aruco)
    scans_pdf_detect_aruco.set_defaults(command_name="scans.pdf-detect-aruco")

    scans_pdf_stamp_aruco = scans_subparsers.add_parser(
        "pdf-stamp-aruco",
        help="Stamp four ArUco markers per template PDF page.",
    )
    _add_stdio_command_flags(scans_pdf_stamp_aruco)
    scans_pdf_stamp_aruco.set_defaults(command_name="scans.pdf-stamp-aruco")
    return parser


def _include_builtin_fakes_for_testing() -> bool:
    return os.environ.get("SCRIPTSCORE_INCLUDE_BUILTIN_FAKES") == "1"


def _load_request_payload(*, use_stdin: bool, options: str | None) -> Any:
    if use_stdin and options is not None:
        raise SystemExit("Exactly one of --stdin or --options may be supplied.")
    if not use_stdin and options is None:
        raise SystemExit("One of --stdin or --options is required.")
    if use_stdin:
        return json.load(sys.stdin)
    assert options is not None
    return json.loads(options)


def _emit_direct_validation_error(
    *,
    command_name: str,
    request_id: str | None,
    message: str,
) -> int:
    now = datetime.now(UTC)
    error = ValidationFailedError(
        [
            ValidationIssue(
                path=[],
                code="json_parse_failed",
                message=message,
            )
        ],
        message="Request payload is invalid.",
    )
    envelope = make_error_envelope(
        command=command_name,
        operation_id=new_operation_id(),
        request_id=request_id,
        error=error,
        started=now,
        finished=now,
    )
    print(envelope.model_dump_json(exclude_none=True))
    return exit_code_for_category(error.category)


def _run_registered_command(
    *,
    engine: ScriptScoreEngine,
    command_name: str,
    use_stdin: bool,
    options: str | None,
    request_id: str | None,
    emit_events: bool,
) -> int:
    try:
        request_payload = _load_request_payload(use_stdin=use_stdin, options=options)
    except json.JSONDecodeError as exc:
        return _emit_direct_validation_error(
            command_name=command_name,
            request_id=request_id,
            message=str(exc),
        )
    event_lines: list[str] = []
    event_sink = None
    if emit_events:

        def _sink(event: Any) -> None:
            event_lines.append(event.model_dump_json(exclude_none=True))

        event_sink = _sink
    result = engine.run(
        command_name,
        cast(dict[str, Any], request_payload)
        if isinstance(request_payload, dict)
        else request_payload,
        request_id=request_id,
        event_sink=event_sink,
    )
    if emit_events:
        for line in event_lines:
            print(line)
    print(result.envelope.model_dump_json(exclude_none=True))
    return result.exit_code


def main(argv: list[str] | None = None) -> int:
    parser = _build_parser()
    args = parser.parse_args(argv)

    engine = create_engine(
        include_builtin_fakes=_include_builtin_fakes_for_testing(),
    )

    if args.command_group == "sidecar" and args.sidecar_command == "rpc":
        return SidecarServer(engine=engine).serve()

    command_name = getattr(args, "command_name", None)
    if isinstance(command_name, str):
        return _run_registered_command(
            engine=engine,
            command_name=command_name,
            use_stdin=args.use_stdin,
            options=args.options,
            request_id=args.request_id,
            emit_events=args.emit_events,
        )

    parser.print_help()
    return 0
