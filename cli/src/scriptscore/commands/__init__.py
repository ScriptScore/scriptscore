# SPDX-License-Identifier: AGPL-3.0-only
"""Command registry builder."""

from scriptscore.commands.exam_analyze import exam_analyze_spec
from scriptscore.commands.exam_generate_rubric import exam_generate_rubric_spec
from scriptscore.commands.exam_setup import exam_setup_spec
from scriptscore.commands.grading_draft_feedback import grading_draft_feedback_spec
from scriptscore.commands.grading_export import grading_export_spec
from scriptscore.commands.grading_markup import grading_markup_spec
from scriptscore.commands.grading_run_consistency import grading_run_consistency_spec
from scriptscore.commands.grading_score_preliminary import grading_score_preliminary_spec
from scriptscore.commands.runtime_list_llm_models import runtime_list_llm_models_spec
from scriptscore.commands.runtime_validate_llm_model import runtime_validate_llm_model_spec
from scriptscore.commands.scans_align_auto import scans_align_auto_spec
from scriptscore.commands.scans_canonicalize import scans_canonicalize_spec
from scriptscore.commands.scans_crop import scans_crop_spec
from scriptscore.commands.scans_detect import scans_detect_spec
from scriptscore.commands.scans_ingest import scans_ingest_spec
from scriptscore.commands.scans_ocr import scans_ocr_spec
from scriptscore.commands.scans_parse import scans_parse_spec
from scriptscore.commands.scans_pdf_aruco import (
    scans_pdf_detect_aruco_spec,
    scans_pdf_stamp_aruco_spec,
)
from scriptscore.commands.scans_pdf_create_redacted import scans_pdf_create_redacted_spec
from scriptscore.commands.scans_pdf_transient import (
    scans_pdf_clip_rects_spec,
    scans_pdf_extract_text_spec,
    scans_pdf_map_template_regions_spec,
    scans_pdf_render_page_spec,
)
from scriptscore.commands.scans_pii import scans_pii_spec
from scriptscore.commands.scans_transform import scans_transform_spec
from scriptscore.commands.smoke import smoke_ping_spec
from scriptscore.runtime import CommandRegistry


def build_command_registry() -> CommandRegistry:
    """Build the initial command registry."""

    registry = CommandRegistry()
    registry.register(smoke_ping_spec())
    registry.register(runtime_list_llm_models_spec())
    registry.register(runtime_validate_llm_model_spec())
    registry.register(exam_setup_spec())
    registry.register(exam_analyze_spec())
    registry.register(exam_generate_rubric_spec())
    registry.register(grading_score_preliminary_spec())
    registry.register(grading_run_consistency_spec())
    registry.register(grading_draft_feedback_spec())
    registry.register(grading_markup_spec())
    registry.register(grading_export_spec())
    registry.register(scans_ingest_spec())
    registry.register(scans_canonicalize_spec())
    registry.register(scans_transform_spec())
    registry.register(scans_align_auto_spec())
    registry.register(scans_detect_spec())
    registry.register(scans_crop_spec())
    registry.register(scans_pii_spec())
    registry.register(scans_parse_spec())
    registry.register(scans_ocr_spec())
    registry.register(scans_pdf_render_page_spec())
    registry.register(scans_pdf_clip_rects_spec())
    registry.register(scans_pdf_extract_text_spec())
    registry.register(scans_pdf_map_template_regions_spec())
    registry.register(scans_pdf_create_redacted_spec())
    registry.register(scans_pdf_detect_aruco_spec())
    registry.register(scans_pdf_stamp_aruco_spec())
    return registry
