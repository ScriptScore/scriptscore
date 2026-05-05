# SPDX-License-Identifier: AGPL-3.0-only
"""Coverage smoke tests for command compatibility export modules."""

from __future__ import annotations

from scriptscore.commands import exam, scans
from scriptscore.commands.exam_analyze import exam_analyze_spec, handle_exam_analyze
from scriptscore.commands.exam_generate_rubric import (
    exam_generate_rubric_spec,
    handle_exam_generate_rubric,
)
from scriptscore.commands.exam_setup import exam_setup_spec, handle_exam_setup
from scriptscore.commands.scans_canonicalize import (
    handle_scans_canonicalize,
    scans_canonicalize_spec,
)
from scriptscore.commands.scans_crop import handle_scans_crop, scans_crop_spec
from scriptscore.commands.scans_ingest import handle_scans_ingest, scans_ingest_spec
from scriptscore.commands.scans_parse import handle_scans_parse, scans_parse_spec
from scriptscore.commands.scans_pii import handle_scans_pii, scans_pii_spec
from scriptscore.commands.scans_transform import handle_scans_transform, scans_transform_spec


def test_exam_compat_exports_match_split_modules() -> None:
    assert exam.exam_setup_spec is exam_setup_spec
    assert exam.exam_analyze_spec is exam_analyze_spec
    assert exam.exam_generate_rubric_spec is exam_generate_rubric_spec
    assert exam.handle_exam_setup is handle_exam_setup
    assert exam.handle_exam_analyze is handle_exam_analyze
    assert exam.handle_exam_generate_rubric is handle_exam_generate_rubric


def test_scans_compat_exports_match_split_modules() -> None:
    assert scans.scans_canonicalize_spec is scans_canonicalize_spec
    assert scans.scans_transform_spec is scans_transform_spec
    assert scans.scans_ingest_spec is scans_ingest_spec
    assert scans.scans_crop_spec is scans_crop_spec
    assert scans.scans_pii_spec is scans_pii_spec
    assert scans.scans_parse_spec is scans_parse_spec
    assert scans.handle_scans_canonicalize is handle_scans_canonicalize
    assert scans.handle_scans_transform is handle_scans_transform
    assert scans.handle_scans_ingest is handle_scans_ingest
    assert scans.handle_scans_crop is handle_scans_crop
    assert scans.handle_scans_pii is handle_scans_pii
    assert scans.handle_scans_parse is handle_scans_parse
